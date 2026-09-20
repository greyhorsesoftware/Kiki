//! SFTP location plugin. Listing prefers a single `find` over an SSH exec channel when the
//! server allows it (one round trip per directory or per tree): GNU `find -printf` where present,
//! otherwise `find -exec stat` (coreutils or BSD `stat`); everything else is SFTP. Reads keep
//! `READ_IN_FLIGHT` requests outstanding on a dedicated channel.

use kiki_plugin_sdk::json::Value;
use kiki_plugin_sdk::{self as sdk, Describe, Entry, Features, Handler, Kind, Meta, Outgoing, PluginError, Result, WriteArgs};
use russh::client::{self, Handle};
use russh::keys::{HashAlg, PrivateKeyWithHashAlg};
use russh::ChannelMsg;
use russh_sftp::client::{RawSftpSession, SftpSession};
use russh_sftp::protocol::{FileType, OpenFlags};
use std::collections::{HashMap, VecDeque};
use std::io::{Read, Write};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::io::AsyncWriteExt;
use tokio::runtime::Runtime;

/// SFTP throughput is bounded by round trips, not bandwidth, with one outstanding request.
const READ_IN_FLIGHT: usize = 32;
/// What is asked for at a time. A server may give LESS than it is asked for and that is not the
/// end of the file — OpenSSH never gives more than 255 KiB, some servers 32 — so a short read is
/// followed up (see `read_pipelined`). 64 KiB is under every cap met so far, so the follow-up is
/// the exception and the pipeline stays full.
const READ_CHUNK: u32 = 64 * 1024;
/// One chunk request in flight: the task that will hand back its bytes.
type Chunk = tokio::task::JoinHandle<std::result::Result<Vec<u8>, russh_sftp::client::error::Error>>;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum FastScan {
    /// GNU findutils: `find -printf` with NUL-separated fields.
    Gnu,
    /// A `find` without `-printf` plus coreutils-style `stat -c`.
    PosixStatC,
    /// A `find` without `-printf` plus BSD-style `stat -f`.
    PosixStatF,
    None,
}

impl FastScan {
    fn label(self) -> &'static str {
        match self {
            FastScan::Gnu => "gnu",
            FastScan::PosixStatC | FastScan::PosixStatF => "posix",
            FastScan::None => "none",
        }
    }
}

struct Session {
    handle: Handle<ClientHandler>,
    sftp: SftpSession,
    /// Second SFTP channel used only for pipelined reads.
    raw: Arc<RawSftpSession>,
    fast: Mutex<FastScan>,
    fingerprint: Option<String>,
    /// `~/.ssh/known_hosts` already vouches for this server's key.
    known: bool,
}

impl Session {
    fn fast(&self) -> FastScan {
        *self.fast.lock().unwrap()
    }
}

struct ClientHandler {
    pinned: Option<String>,
    seen: Arc<Mutex<Option<String>>>,
    /// Why a key was refused, so the failure can say so rather than "connection closed".
    refused: Arc<Mutex<Option<String>>>,
    /// Set when `~/.ssh/known_hosts` already vouches for this key: the user has verified this
    /// host with ssh, so kiki has no reason to ask them a second time.
    known: Arc<Mutex<bool>>,
    host: String,
    port: u16,
}

/// What `~/.ssh/known_hosts` (or `KIKI_KNOWN_HOSTS`) says about a host key.
enum KnownHosts {
    /// Listed, and this is the key.
    Matches,
    /// Listed with a different key of the same kind — ssh's "identification has changed".
    Changed(usize),
    /// Not listed, unreadable, or listed only with other algorithms.
    Unknown,
}

fn known_hosts_path() -> std::path::PathBuf {
    if let Ok(p) = std::env::var("KIKI_KNOWN_HOSTS") {
        return std::path::PathBuf::from(p);
    }
    std::path::PathBuf::from(std::env::var("HOME").unwrap_or_default()).join(".ssh/known_hosts")
}

fn known_hosts_says(host: &str, port: u16, key: &russh::keys::PublicKey) -> KnownHosts {
    let path = known_hosts_path();
    if !path.exists() {
        return KnownHosts::Unknown;
    }
    match russh::keys::known_hosts::check_known_hosts_path(host, port, key, &path) {
        Ok(true) => KnownHosts::Matches,
        Ok(false) => KnownHosts::Unknown,
        Err(russh::keys::Error::KeyChanged { line }) => KnownHosts::Changed(line),
        Err(_) => KnownHosts::Unknown,
    }
}

impl client::Handler for ClientHandler {
    type Error = russh::Error;

    async fn check_server_key(&mut self, key: &russh::keys::PublicKey) -> std::result::Result<bool, Self::Error> {
        let fp = key.fingerprint(HashAlg::Sha256).to_string();
        *self.seen.lock().unwrap() = Some(fp.clone());
        // A key the user has already accepted for this location: it must be that key and no other.
        if let Some(p) = &self.pinned {
            if p == &fp {
                return Ok(true);
            }
            *self.refused.lock().unwrap() = Some(format!("this location trusts {p}"));
            return Ok(false);
        }
        // Nothing pinned yet, so ask ssh. `known_hosts` is a verification the user has already
        // done once, by hand, and it is the same file every other ssh client on this machine
        // checks — kiki honouring it means no second question for a host ssh already knows, and
        // a hard refusal when ssh itself would refuse.
        match known_hosts_says(&self.host, self.port, key) {
            KnownHosts::Matches => {
                *self.known.lock().unwrap() = true;
                Ok(true)
            }
            KnownHosts::Changed(line) => {
                *self.refused.lock().unwrap() = Some(format!("~/.ssh/known_hosts records a different key for this host (line {line})"));
                Ok(false)
            }
            // Unknown to ssh as well. The key is reported back with the reply; kiki shows it to
            // the user, and once they accept it, saves it as `trustedFingerprint`.
            KnownHosts::Unknown => Ok(true),
        }
    }
}

/// Requests run concurrently (SDK worker threads); sessions are shared behind a mutex and every
/// SFTP call takes `&self`, so a listing and a transfer on different sessions interleave.
struct Sftp {
    rt: Runtime,
    sessions: Mutex<HashMap<String, Arc<Session>>>,
}

/// Answer every prompt of a keyboard-interactive exchange with the password. Bounded: a server
/// that keeps asking is not going to be satisfied by the same answer.
async fn keyboard_interactive(handle: &mut Handle<ClientHandler>, user: &str, password: &str) -> Result<bool> {
    use russh::client::KeyboardInteractiveAuthResponse as R;
    let mut reply = handle.authenticate_keyboard_interactive_start(user, None::<String>).await.map_err(net)?;
    for _ in 0..4 {
        match reply {
            R::Success => return Ok(true),
            R::Failure { .. } => return Ok(false),
            R::InfoRequest { prompts, .. } => {
                let answers = prompts.iter().map(|_| password.to_string()).collect();
                reply = handle.authenticate_keyboard_interactive_respond(answers).await.map_err(net)?;
            }
        }
    }
    Ok(false)
}

/// Say what was offered and what could not be, so "authentication failed" is something to act on.
fn auth_failure(tried: &[String], unusable: &[String], password_tried: bool) -> String {
    let mut offered: Vec<String> = Vec::new();
    match tried.len() {
        0 => {}
        1 => offered.push(format!("the key {}", tried[0])),
        n => offered.push(format!("{n} keys ({})", tried.join(", "))),
    }
    if password_tried {
        offered.push("the password".to_string());
    }
    let mut msg = if offered.is_empty() {
        "nothing to sign in with: no usable key was found and no password was given".to_string()
    } else {
        format!("the server refused {}", offered.join(" and "))
    };
    if !unusable.is_empty() {
        msg.push_str(&format!(" — could not use {}", unusable.join("; ")));
    }
    msg
}

fn key(location: &str, role: &str) -> String {
    format!("{location}\u{0}{role}")
}

// ---------------------------------------------------------------- keys on this machine

/// A private key found in `~/.ssh`.
#[derive(Debug, Clone, PartialEq)]
struct FoundKey {
    path: String,
    /// "ED25519", "RSA", … from the `.pub` beside it when there is one.
    algorithm: String,
    comment: String,
    /// Needs a passphrase before it can be used.
    encrypted: bool,
}

impl FoundKey {
    /// One line for the form: `id_ed25519 · ED25519 · gideon@omarchy · passphrase`.
    fn label(&self) -> String {
        let name = self.path.rsplit('/').next().unwrap_or(&self.path);
        let mut parts = vec![name.to_string()];
        if !self.algorithm.is_empty() {
            parts.push(self.algorithm.clone());
        }
        if !self.comment.is_empty() {
            parts.push(self.comment.clone());
        }
        if self.encrypted {
            parts.push("passphrase".to_string());
        }
        parts.join(" · ")
    }
}

/// `KIKI_SSH_DIR` stands in for `~/.ssh` under test.
fn ssh_dir() -> std::path::PathBuf {
    std::env::var("KIKI_SSH_DIR").map(std::path::PathBuf::from).unwrap_or_else(|_| std::path::PathBuf::from(std::env::var("HOME").unwrap_or_default()).join(".ssh"))
}

/// Is this file a private key? By what is in it, not by its name: people call keys anything.
fn looks_like_private_key(path: &std::path::Path) -> bool {
    let Ok(mut f) = std::fs::File::open(path) else { return false };
    let mut head = [0u8; 64];
    let n = f.read(&mut head).unwrap_or(0);
    let head = String::from_utf8_lossy(&head[..n]);
    head.starts_with("-----BEGIN ") && head.contains("PRIVATE KEY-----")
}

/// Every private key in the ssh directory, the usual names first and in ssh's own order of
/// preference, then the rest by name. Never follows into subdirectories, never reads a file
/// larger than a key could be.
fn discover_keys() -> Vec<FoundKey> {
    let dir = ssh_dir();
    let Ok(entries) = std::fs::read_dir(&dir) else { return vec![] };
    let mut found: Vec<FoundKey> = Vec::new();
    for e in entries.flatten() {
        let path = e.path();
        let name = e.file_name().to_string_lossy().to_string();
        let Ok(meta) = e.metadata() else { continue };
        if !meta.is_file() || meta.len() > 64 * 1024 || name.ends_with(".pub") || !looks_like_private_key(&path) {
            continue;
        }
        // `<algorithm> <base64> <comment…>`, read as text: russh's loader drops the comment, and
        // the comment is how anyone tells their keys apart.
        let (mut algorithm, mut comment) = (String::new(), String::new());
        if let Ok(public) = std::fs::read_to_string(path.with_file_name(format!("{name}.pub"))) {
            let mut parts = public.trim().splitn(3, char::is_whitespace);
            algorithm = parts.next().unwrap_or("").trim_start_matches("ssh-").trim_start_matches("ecdsa-sha2-").to_uppercase();
            let _blob = parts.next();
            comment = parts.next().unwrap_or("").trim().to_string();
        }
        let encrypted = matches!(russh::keys::load_secret_key(&path, None), Err(russh::keys::Error::KeyIsEncrypted));
        found.push(FoundKey { path: path.to_string_lossy().to_string(), algorithm, comment, encrypted });
    }
    const USUAL: [&str; 4] = ["id_ed25519", "id_ecdsa", "id_rsa", "id_dsa"];
    let rank = |k: &FoundKey| { let n = k.path.rsplit('/').next().unwrap_or(""); USUAL.iter().position(|u| *u == n).unwrap_or(USUAL.len()) };
    found.sort_by(|a, b| rank(a).cmp(&rank(b)).then_with(|| a.path.cmp(&b.path)));
    found
}

/// The keys to offer the server: exactly the ones the location names (one path per line, `~`
/// allowed), and none when it names none — that is a password-only location. Which keys to use is
/// the user's choice, made in the form (which ticks the first key found to begin with); the plugin
/// does not go looking for others. A saved location from before this field was a list holds a
/// single path, which is a list of one.
fn keys_to_try(identity: &str) -> Vec<String> {
    let home = std::env::var("HOME").unwrap_or_default();
    identity.lines().map(str::trim).filter(|l| !l.is_empty()).map(|l| if let Some(rest) = l.strip_prefix('~') { format!("{home}{rest}") } else { l.to_string() }).collect()
}

/// sshd hangs up after `MaxAuthTries` failures (6 by default), and every key offered is one. Stop
/// short of that so the password still gets its turn.
const MAX_KEYS_OFFERED: usize = 4;

fn cfg<'a>(config: &'a Value, k: &str) -> &'a str {
    config.str_field(k).unwrap_or("")
}

fn to_meta(m: &russh_sftp::protocol::FileAttributes) -> Meta {
    Meta { hidden: false, size: m.size.unwrap_or(0), mtime_ms: m.mtime.map(|t| t as u64 * 1000).unwrap_or(0), mode: m.permissions.map(|p| p & 0o7777), owner: m.user.clone(), group: m.group.clone() }
}

fn kind_of(ft: FileType) -> Kind {
    match ft {
        FileType::Dir => Kind::Dir,
        FileType::File => Kind::File,
        FileType::Symlink => Kind::Link,
        FileType::Other => Kind::Other,
    }
}

fn net<E: std::fmt::Display>(e: E) -> PluginError {
    PluginError::network(e.to_string())
}

fn sftp_err(e: russh_sftp::client::error::Error) -> PluginError {
    use russh_sftp::protocol::StatusCode;
    match e {
        russh_sftp::client::error::Error::Status(s) => match s.status_code {
            StatusCode::NoSuchFile => PluginError::not_found(),
            StatusCode::PermissionDenied => PluginError::new("Denied", s.error_message),
            StatusCode::OpUnsupported => PluginError::unsupported(),
            _ => PluginError::io(s.error_message),
        },
        other => PluginError::io(other),
    }
}

impl Sftp {
    fn session(&self, location: &str) -> Result<Arc<Session>> {
        // The session the request was made on: the browser's, or a job's own (`sdk::current_role`).
        let k = key(location, &sdk::current_role());
        self.sessions.lock().unwrap().get(&k).cloned().ok_or_else(|| PluginError::network("not connected"))
    }

    /// Run one command over an exec channel; returns (stdout, exit status).
    fn exec(&self, sess: &Session, cmd: &str, on_data: impl FnMut(&[u8])) -> Result<u32> {
        self.rt.block_on(self.exec_async(sess, cmd, on_data))
    }

    async fn exec_async(&self, sess: &Session, cmd: &str, mut on_data: impl FnMut(&[u8])) -> Result<u32> {
        {
            let mut ch = sess.handle.channel_open_session().await.map_err(net)?;
            ch.exec(true, cmd).await.map_err(net)?;
            let mut status = 0u32;
            loop {
                if sdk::cancelled() {
                    let _ = ch.close().await;
                    return Err(sdk::cancel_error());
                }
                match ch.wait().await {
                    Some(ChannelMsg::Data { data }) => on_data(&data),
                    Some(ChannelMsg::ExitStatus { exit_status }) => status = exit_status,
                    Some(ChannelMsg::Failure) => {
                        // exec refused (ForceCommand internal-sftp, restricted shell): the server
                        // leaves the channel open, so close it ourselves.
                        let _ = ch.close().await;
                        return Err(PluginError::unsupported());
                    }
                    Some(ChannelMsg::Eof) | Some(ChannelMsg::Close) => break,
                    Some(_) => {}
                    None => break,
                }
            }
            Ok(status)
        }
    }

    /// The probe must never stall a Connect: 3 s and it is `none`.
    fn exec_timeout(&self, sess: &Session, cmd: &str, on_data: impl FnMut(&[u8])) -> Result<u32> {
        self.rt.block_on(async { tokio::time::timeout(Duration::from_secs(3), self.exec_async(sess, cmd, on_data)).await.unwrap_or_else(|_| Err(PluginError::network("probe timed out"))) })
    }

    fn probe(&self, sess: &Session) -> FastScan {
        let mut out = Vec::new();
        match self.exec_timeout(sess, "command -v find >/dev/null 2>&1 && find --version 2>/dev/null | head -1", |d| out.extend_from_slice(d)) {
            Ok(0) if String::from_utf8_lossy(&out).contains("GNU") => return FastScan::Gnu,
            Ok(_) => {}
            Err(_) => return FastScan::None,
        }
        // No GNU find: a plain find plus a stat we can format still beats READDIR round trips.
        out.clear();
        let cmd = "command -v find stat >/dev/null 2>&1 || exit 3; if stat -c '%F' / >/dev/null 2>&1; then echo statc; elif stat -f '%HT' / >/dev/null 2>&1; then echo statf; fi";
        match self.exec_timeout(sess, cmd, |d| out.extend_from_slice(d)) {
            Ok(0) => match String::from_utf8_lossy(&out).trim() {
                "statc" => FastScan::PosixStatC,
                "statf" => FastScan::PosixStatF,
                _ => FastScan::None,
            },
            _ => FastScan::None,
        }
    }

    /// One `find` over exec. Entries are held back until the command finishes so a stream that
    /// dies half way never leaks a partial listing: the caller falls back to SFTP and sends the
    /// full listing once.
    fn fast_scan(&self, sess: &Session, path: &str, recursive: bool, sink: &mut dyn FnMut(Vec<Entry>)) -> Result<u64> {
        let depth = if recursive { "" } else { "-maxdepth 1 " };
        let q = sdk::shell_quote(path);
        let cmd = match sess.fast() {
            FastScan::Gnu => {
                let fmt = if recursive { "%y\\0%Y\\0%s\\0%T@\\0%m\\0%u\\0%g\\0%P\\0" } else { "%y\\0%Y\\0%s\\0%T@\\0%m\\0%u\\0%g\\0%f\\0" };
                format!("find {q} -mindepth 1 {depth}-printf '{fmt}'")
            }
            // One `stat` line per entry (type|size|mtime|mode|user|group) followed by the NUL-terminated
            // path, so names containing newlines or quotes survive.
            FastScan::PosixStatC => format!("find {q} -mindepth 1 {depth}-exec sh -c 'for f; do stat -c \"%F|%s|%Y|%a|%U|%G\" \"$f\" 2>/dev/null || echo \"?|0|0|0||\"; printf \"%s\\\\0\" \"$f\"; done' sh {{}} +"),
            FastScan::PosixStatF => format!("find {q} -mindepth 1 {depth}-exec sh -c 'for f; do stat -f \"%HT|%z|%m|%OLp|%Su|%Sg\" \"$f\" 2>/dev/null || echo \"?|0|0|0||\"; printf \"%s\\\\0\" \"$f\"; done' sh {{}} +"),
            FastScan::None => return Err(PluginError::unsupported()),
        };
        let mut buf: Vec<u8> = Vec::new();
        let mut entries: Vec<Entry> = Vec::new();
        let gnu = sess.fast() == FastScan::Gnu;
        let status = self.exec(sess, &cmd, |data| {
            buf.extend_from_slice(data);
            while let Some(e) = if gnu { parse_gnu_record(&mut buf, recursive) } else { parse_stat_record(&mut buf, path, recursive) } {
                entries.push(e);
            }
        })?;
        // GNU find exits 1 when some entry could not be read but still prints everything else;
        // that is a listing, not a failure. Anything else (killed, 126, 127) is.
        if status > 1 || (status == 1 && !buf.is_empty()) {
            return Err(PluginError::io(format!("find exited with {status}")));
        }
        let n = entries.len() as u64;
        let mut it = entries.into_iter().peekable();
        while it.peek().is_some() {
            sink(it.by_ref().take(1024).collect());
        }
        Ok(n)
    }

    /// Pipelined download: `READ_IN_FLIGHT` chunk requests outstanding on the raw channel,
    /// delivered in order.
    fn read_pipelined(&self, raw: &Arc<RawSftpSession>, path: &str, offset: u64, out: &mut Outgoing) -> Result<()> {
        self.rt.block_on(async {
            let handle = raw.open(path, OpenFlags::READ, Default::default()).await.map_err(sftp_err)?.handle;
            let size = raw.fstat(handle.clone()).await.ok().and_then(|a| a.attrs.size);
            let mut inflight: VecDeque<(u64, Chunk)> = VecDeque::new();
            let mut next = offset;
            let mut eof = false;
            let issue = |raw: &Arc<RawSftpSession>, handle: &str, off: u64| {
                let raw = Arc::clone(raw);
                let h = handle.to_string();
                tokio::task::spawn(async move { raw.read(h, off, READ_CHUNK).await.map(|d| d.data) })
            };
            let result: Result<()> = async {
                loop {
                    while !eof && inflight.len() < READ_IN_FLIGHT && size.is_none_or(|s| next < s) {
                        inflight.push_back((next, issue(raw, &handle, next)));
                        next += READ_CHUNK as u64;
                    }
                    let Some((at, job)) = inflight.pop_front() else { break };
                    if sdk::cancelled() {
                        return Err(sdk::cancel_error());
                    }
                    match job.await.map_err(PluginError::io)? {
                        Ok(data) if data.is_empty() => eof = true,
                        Ok(data) => {
                            out.write_all(&data).map_err(PluginError::io)?;
                            // Fewer bytes than asked for is not the end of the file: the server
                            // gives what it likes. The chunks after this one are already asked
                            // for at their own offsets, so the rest of THIS one is fetched now,
                            // in order, before they are written. (Treating a short read as the
                            // end cut every file over 255 KiB short against OpenSSH.)
                            let mut got = data.len() as u64;
                            while got < READ_CHUNK as u64 && !eof {
                                if sdk::cancelled() {
                                    return Err(sdk::cancel_error());
                                }
                                match raw.read(handle.clone(), at + got, READ_CHUNK - got as u32).await {
                                    Ok(more) if more.data.is_empty() => eof = true,
                                    Ok(more) => {
                                        out.write_all(&more.data).map_err(PluginError::io)?;
                                        got += more.data.len() as u64;
                                    }
                                    Err(russh_sftp::client::error::Error::Status(st)) if st.status_code == russh_sftp::protocol::StatusCode::Eof => eof = true,
                                    Err(e) => return Err(sftp_err(e)),
                                }
                            }
                        }
                        Err(russh_sftp::client::error::Error::Status(st)) if st.status_code == russh_sftp::protocol::StatusCode::Eof => eof = true,
                        Err(e) => return Err(sftp_err(e)),
                    }
                    if eof {
                        // Drain whatever is still in flight; those chunks are past the end.
                        while let Some((_, j)) = inflight.pop_front() {
                            let _ = j.await;
                        }
                    }
                }
                Ok(())
            }
            .await;
            let _ = raw.close(handle).await;
            result
        })
    }
}

/// GNU `-printf` record: type, link target type, size, mtime, mode, user, group, name/rel path.
fn parse_gnu_record(buf: &mut Vec<u8>, recursive: bool) -> Option<Entry> {
    let mut fields = Vec::with_capacity(8);
    let mut pos = 0;
    for _ in 0..8 {
        let i = buf[pos..].iter().position(|&b| b == 0)?;
        fields.push(buf[pos..pos + i].to_vec());
        pos += i + 1;
    }
    buf.drain(..pos);
    let s = |i: usize| String::from_utf8_lossy(&fields[i]).into_owned();
    let kind = match fields[0].first() {
        Some(b'd') => Kind::Dir,
        Some(b'f') => Kind::File,
        Some(b'l') => Kind::Link,
        _ => Kind::Other,
    };
    let mtime_ms = s(3).parse::<f64>().map(|t| (t * 1000.0) as u64).unwrap_or(0);
    let rel = s(7);
    let name = rel.rsplit('/').next().unwrap_or("").to_string();
    Some(Entry {
        name,
        kind,
        meta: Some(Meta { hidden: false, size: s(2).parse().unwrap_or(0), mtime_ms, mode: u32::from_str_radix(&s(4), 8).ok(), owner: Some(s(5)), group: Some(s(6)) }),
        rel: if recursive { rel } else { String::new() },
    })
}

/// `stat` record: `type|size|mtime|mode|user|group\n<path>\0`.
fn parse_stat_record(buf: &mut Vec<u8>, root: &str, recursive: bool) -> Option<Entry> {
    let nl = buf.iter().position(|&b| b == b'\n')?;
    let nul = buf[nl + 1..].iter().position(|&b| b == 0)?;
    let line = String::from_utf8_lossy(&buf[..nl]).into_owned();
    let full = String::from_utf8_lossy(&buf[nl + 1..nl + 1 + nul]).into_owned();
    buf.drain(..nl + 1 + nul + 1);
    let f: Vec<&str> = line.splitn(6, '|').collect();
    let get = |i: usize| f.get(i).copied().unwrap_or("");
    let t = get(0).to_ascii_lowercase();
    let kind = if t.starts_with("dir") {
        Kind::Dir
    } else if t.starts_with("regular") {
        Kind::File
    } else if t.starts_with("symbolic") {
        Kind::Link
    } else {
        Kind::Other
    };
    let name = full.rsplit('/').next().unwrap_or("").to_string();
    let prefix = format!("{}/", root.trim_end_matches('/'));
    let rel = if recursive { full.strip_prefix(&prefix).unwrap_or(&full).to_string() } else { String::new() };
    Some(Entry {
        name,
        kind,
        meta: Some(Meta {
            hidden: false,
            size: get(1).parse().unwrap_or(0),
            mtime_ms: get(2).parse::<u64>().unwrap_or(0) * 1000,
            mode: u32::from_str_radix(get(3), 8).ok(),
            owner: Some(get(4).to_string()),
            group: Some(get(5).to_string()),
        }),
        rel,
    })
}

impl Handler for Sftp {
    /// The `keys` field: every private key in `~/.ssh`, found afresh each time the form asks,
    /// so a key made a minute ago is there without restarting anything.
    fn browse(&self, field: &str, _config: &Value, _secrets: &Value) -> Result<Vec<(String, String)>> {
        if field != "identityFile" {
            return Err(PluginError::unsupported());
        }
        Ok(discover_keys().into_iter().map(|k| { let label = k.label(); (k.path, label) }).collect())
    }

    fn describe(&self) -> Describe {
        Describe {
            scheme: "sftp",
            display_name: "SFTP",
            version: "1.0",
            form: vec![
                sdk::field("name", "Name", "text", true, None),
                sdk::field("host", "Host", "text", true, None),
                sdk::field("port", "Port", "port", true, Some("22")),
                sdk::field("username", "Username", "text", true, None),
                // `keys`: the form lists what `browse` finds, ticks the first, and lets any
                // number be ticked; none ticked is a password-only location. The password is an
                // equal, not a fallback of last resort.
                // Two tabs, one way in: the form sends only the chosen tab's fields, and says
                // which it was in `auth`.
                sdk::field_in("Password", "password", "Password", "password", false, None),
                sdk::field_in("Key", "identityFile", "Keys", "keys", false, None),
                sdk::field_in("Key", "passphrase", "Key passphrase", "password", false, None),
                // Where it is, apart from how to get in.
                sdk::on_page("Locations", sdk::field("remotePath", "Remote path", "path", true, Some("/"))),
                sdk::on_page("Locations", sdk::field("localPath", "Local path", "path", false, None)),
            ],
            defaults: Value::obj().s("port", "22").s("remotePath", "/").done(),
            secret_fields: vec!["passphrase", "password"],
            detector_upload: "sizeMtime",
            detector_download: "sizeMtime",
            features: Features { set_mtime: true, mode: true, real_dirs: true, meta_in_scan: true, pipelining: true, partial_read: true },
            available: None,
        }
    }

    fn validate(&self, config: &Value) -> Result<()> {
        if cfg(config, "host").is_empty() {
            return Err(PluginError::invalid("host", "host is required"));
        }
        if cfg(config, "username").is_empty() {
            return Err(PluginError::invalid("username", "username is required"));
        }
        let port = cfg(config, "port");
        if !port.is_empty() && port.parse::<u16>().map(|p| p == 0).unwrap_or(true) {
            return Err(PluginError::invalid("port", "port must be 1 to 65535"));
        }
        Ok(())
    }

    fn connect(&self, location: &str, role: &str, config: &Value, secrets: &Value) -> Result<Value> {
        let k = key(location, role);
        if let Some(s) = self.sessions.lock().unwrap().get(&k) {
            return Ok(Value::obj().opt_s("fingerprint", s.fingerprint.as_deref()).b("knownHost", s.known).v("banner", Value::Null).done());
        }
        let host = cfg(config, "host").to_string();
        let port: u16 = cfg(config, "port").parse().unwrap_or(22);
        let user = cfg(config, "username").to_string();
        // `auth` is the tab the location was saved from: "password" offers no key, "key" sends
        // no password. A location from before there were tabs has neither, and gets both.
        let auth = cfg(config, "auth").to_lowercase();
        let candidates = if auth == "password" { Vec::new() } else { keys_to_try(cfg(config, "identityFile")) };
        let pinned = config.str_field("trustedFingerprint").map(str::to_string);
        let seen = Arc::new(Mutex::new(None));
        let refused: Arc<Mutex<Option<String>>> = Arc::new(Mutex::new(None));
        let known = Arc::new(Mutex::new(false));
        let password = if auth == "key" { None } else { secrets.str_field("password").map(str::to_string) };
        let passphrase = secrets.str_field("passphrase").map(str::to_string);
        let result = self.rt.block_on(async {
            let config = Arc::new(client::Config { inactivity_timeout: Some(Duration::from_secs(300)), keepalive_interval: Some(Duration::from_secs(30)), ..Default::default() });
            let handler = ClientHandler { pinned: pinned.clone(), seen: Arc::clone(&seen), refused: Arc::clone(&refused), known: Arc::clone(&known), host: host.clone(), port };
            let mut handle = client::connect(config, (host.as_str(), port), handler).await.map_err(|e| PluginError::network(format!("{host}:{port}: {e}")))?;
            // Keys first, then the password, then the password again as the answer to a
            // keyboard-interactive prompt — which is how many servers ask for it (PAM) while
            // refusing the plain `password` method outright.
            let mut authed = false;
            let mut tried: Vec<String> = Vec::new();
            let mut unusable: Vec<String> = Vec::new();
            for path in candidates.iter() {
                if authed || tried.len() >= MAX_KEYS_OFFERED {
                    break;
                }
                let name = path.rsplit('/').next().unwrap_or(path).to_string();
                if !std::path::Path::new(path).exists() {
                    unusable.push(format!("{name}: no such file"));
                    continue;
                }
                // Unencrypted keys load without the passphrase; only an encrypted one needs it,
                // so one passphrase field serves a mix of both.
                let key = match russh::keys::load_secret_key(path, None) {
                    Ok(k) => k,
                    Err(russh::keys::Error::KeyIsEncrypted) => match russh::keys::load_secret_key(path, passphrase.as_deref()) {
                        Ok(k) => k,
                        Err(_) => {
                            unusable.push(format!("{name}: {}", if passphrase.is_some() { "wrong passphrase" } else { "needs its passphrase" }));
                            continue;
                        }
                    },
                    Err(e) => {
                        unusable.push(format!("{name}: {e}"));
                        continue;
                    }
                };
                let hash = handle.best_supported_rsa_hash().await.map_err(net)?.flatten();
                let r = handle.authenticate_publickey(&user, PrivateKeyWithHashAlg::new(Arc::new(key), hash)).await.map_err(net)?;
                authed = r.success();
                tried.push(name);
            }
            let mut password_tried = false;
            if !authed {
                if let Some(pw) = &password {
                    password_tried = true;
                    authed = handle.authenticate_password(&user, pw).await.map_err(net)?.success();
                    if !authed {
                        authed = keyboard_interactive(&mut handle, &user, pw).await?;
                    }
                }
            }
            if !authed {
                return Err(PluginError::auth(auth_failure(&tried, &unusable, password_tried)));
            }
            let ch = handle.channel_open_session().await.map_err(net)?;
            ch.request_subsystem(true, "sftp").await.map_err(net)?;
            let sftp = SftpSession::new(ch.into_stream()).await.map_err(sftp_err)?;
            let ch2 = handle.channel_open_session().await.map_err(net)?;
            ch2.request_subsystem(true, "sftp").await.map_err(net)?;
            let raw = RawSftpSession::new(ch2.into_stream());
            raw.init().await.map_err(sftp_err)?;
            Ok::<_, PluginError>((handle, sftp, Arc::new(raw)))
        });
        // A refused host key surfaces from russh as a closed connection; say what really happened.
        let (handle, sftp, raw) = match result {
            Ok(v) => v,
            Err(e) => {
                // A refused host key surfaces from russh as a closed connection; say what really happened.
                let why = refused.lock().unwrap().clone();
                return match why {
                    Some(why) => {
                        let got = seen.lock().unwrap().clone().unwrap_or_default();
                        Err(PluginError::auth(format!("the host key of {host} has changed — it offered {got}, and {why}. Verify the server before connecting again.")))
                    }
                    None => Err(e),
                };
            }
        };
        let fingerprint = seen.lock().unwrap().clone();
        let vouched = *known.lock().unwrap();
        let sess = Session { handle, sftp, raw, fast: Mutex::new(FastScan::None), fingerprint: fingerprint.clone(), known: vouched };
        // A job scans too — a mirror, a folder being copied — so its session is probed as well.
        let f = self.probe(&sess);
        *sess.fast.lock().unwrap() = f;
        self.sessions.lock().unwrap().insert(k, Arc::new(sess));
        Ok(Value::obj().opt_s("fingerprint", fingerprint.as_deref()).b("knownHost", vouched).v("banner", Value::Null).done())
    }

    fn disconnect(&self, location: &str, role: &str) {
        let s = self.sessions.lock().unwrap().remove(&key(location, role));
        if let Some(s) = s {
            let _ = self.rt.block_on(async { s.handle.disconnect(russh::Disconnect::ByApplication, "", "en").await });
        }
    }

    fn capabilities(&self, location: &str) -> Result<Value> {
        let fast = self.session(location)?.fast();
        Ok(Value::obj().b("trash", false).b("setMtime", true).b("mode", true).b("realDirs", true).v("digestKind", Value::Null).s("separator", "/").s("fastScan", fast.label()).b("partialRead", true).done())
    }

    fn scan(&self, location: &str, path: &str, recursive: bool, sink: &mut dyn FnMut(Vec<Entry>)) -> Result<u64> {
        let sess = self.session(location)?;
        if sess.fast() != FastScan::None {
            match self.fast_scan(&sess, path, recursive, sink) {
                Ok(n) => return Ok(n),
                Err(e) if e.code == "Cancelled" => return Err(e),
                Err(_) => {
                    // Fall back for the rest of the session; nothing was sent for the failed call.
                    *sess.fast.lock().unwrap() = FastScan::None;
                }
            }
        }
        if recursive {
            return Err(PluginError::unsupported());
        }
        let sftp = &sess.sftp;
        self.rt.block_on(async {
            let rd = sftp.read_dir(path).await.map_err(sftp_err)?;
            let mut batch = Vec::with_capacity(256);
            let mut n = 0u64;
            for e in rd {
                if sdk::cancelled() {
                    return Err(sdk::cancel_error());
                }
                let name = e.file_name();
                if name == "." || name == ".." {
                    continue;
                }
                let md = e.metadata();
                batch.push(Entry { name, kind: kind_of(md.file_type()), meta: Some(to_meta(&md)), rel: String::new() });
                n += 1;
                if batch.len() >= 256 {
                    sink(std::mem::take(&mut batch));
                }
            }
            if !batch.is_empty() {
                sink(batch);
            }
            Ok(n)
        })
    }

    fn stat(&self, location: &str, path: &str) -> Result<Meta> {
        let sess = self.session(location)?;
        let sftp = &sess.sftp;
        self.rt.block_on(async { sftp.symlink_metadata(path).await.map(|m| to_meta(&m)).map_err(sftp_err) })
    }

    fn read(&self, location: &str, path: &str, offset: u64, out: &mut Outgoing) -> Result<()> {
        let raw = Arc::clone(&self.session(location)?.raw);
        self.read_pipelined(&raw, path, offset, out)
    }

    fn write(&self, location: &str, path: &str, args: WriteArgs) -> Result<u64> {
        let sess = self.session(location)?;
        let sftp = &sess.sftp;
        let rt = &self.rt;
        let mtime = args.mtime_ms;
        let mut args = args;
        rt.block_on(async {
            let mut f = sftp.create(path).await.map_err(sftp_err)?;
            let mut buf = vec![0u8; 256 * 1024];
            let mut total = 0u64;
            loop {
                if sdk::cancelled() {
                    return Err(sdk::cancel_error());
                }
                let n = args.data.read(&mut buf).map_err(PluginError::io)?;
                if n == 0 {
                    break;
                }
                f.write_all(&buf[..n]).await.map_err(PluginError::io)?;
                total += n as u64;
            }
            f.shutdown().await.map_err(PluginError::io)?;
            if let Some(t) = mtime {
                let attrs = russh_sftp::protocol::FileAttributes { mtime: Some((t / 1000) as u32), atime: Some((t / 1000) as u32), ..Default::default() };
                let _ = sftp.set_metadata(path, attrs).await;
            }
            Ok(total)
        })
    }

    fn mkdir(&self, location: &str, path: &str) -> Result<()> {
        let sftp = &self.session(location)?.sftp;
        self.rt.block_on(async { sftp.create_dir(path).await.map_err(sftp_err) })
    }

    fn rename(&self, location: &str, from: &str, to: &str) -> Result<()> {
        let sftp = &self.session(location)?.sftp;
        self.rt.block_on(async { sftp.rename(from, to).await.map_err(sftp_err) })
    }

    fn delete(&self, location: &str, path: &str) -> Result<()> {
        let sftp = &self.session(location)?.sftp;
        self.rt.block_on(async {
            let md = sftp.symlink_metadata(path).await.map_err(sftp_err)?;
            if md.file_type() == FileType::Dir {
                sftp.remove_dir(path).await.map_err(sftp_err)
            } else {
                sftp.remove_file(path).await.map_err(sftp_err)
            }
        })
    }

    fn set_mtime(&self, location: &str, path: &str, mtime_ms: u64) -> Result<()> {
        let sftp = &self.session(location)?.sftp;
        self.rt.block_on(async {
            let attrs = russh_sftp::protocol::FileAttributes { mtime: Some((mtime_ms / 1000) as u32), atime: Some((mtime_ms / 1000) as u32), ..Default::default() };
            sftp.set_metadata(path, attrs).await.map_err(sftp_err)
        })
    }

    fn chmod(&self, location: &str, path: &str, mode: u32) -> Result<()> {
        let sftp = &self.session(location)?.sftp;
        self.rt.block_on(async {
            let attrs = russh_sftp::protocol::FileAttributes { permissions: Some(mode), ..Default::default() };
            sftp.set_metadata(path, attrs).await.map_err(sftp_err)
        })
    }
}

fn main() {
    // Multi-thread so concurrent requests can each block_on their own future.
    let rt = tokio::runtime::Builder::new_multi_thread().worker_threads(2).enable_all().build().expect("tokio runtime");
    let h = Sftp { rt, sessions: Mutex::new(HashMap::new()) };
    if let Err(e) = sdk::run(&h) {
        eprintln!("kiki-plugin-sftp: {e}");
    }
}

#[cfg(test)]
mod key_tests {
    use super::*;
    use std::process::Command;

    /// `KIKI_SSH_DIR` and `HOME` are process-wide: one test at a time.
    static ENV: Mutex<()> = Mutex::new(());

    fn keygen(dir: &std::path::Path, name: &str, kind: &str, passphrase: &str, comment: &str) -> bool {
        Command::new("ssh-keygen").args(["-q", "-t", kind, "-N", passphrase, "-C", comment, "-f"]).arg(dir.join(name)).status().map(|s| s.success()).unwrap_or(false)
    }

    fn temp_ssh_dir(tag: &str) -> std::path::PathBuf {
        let d = std::env::temp_dir().join(format!("kiki-sftp-keys-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn finds_every_private_key_and_nothing_else() {
        let _g = ENV.lock().unwrap_or_else(|p| p.into_inner());
        let d = temp_ssh_dir("find");
        if !keygen(&d, "id_ed25519", "ed25519", "", "me@here") {
            eprintln!("ssh-keygen not available; skipped");
            return;
        }
        assert!(keygen(&d, "work-laptop", "ed25519", "hunter2", "work"));
        assert!(keygen(&d, "id_rsa", "rsa", "", ""));
        std::fs::write(d.join("known_hosts"), "example.com ssh-ed25519 AAAA\n").unwrap();
        std::fs::write(d.join("config"), "Host *\n").unwrap();
        std::fs::create_dir(d.join("sockets")).unwrap();
        std::env::set_var("KIKI_SSH_DIR", &d);
        let found = discover_keys();
        std::env::remove_var("KIKI_SSH_DIR");

        let names: Vec<&str> = found.iter().map(|k| k.path.rsplit('/').next().unwrap()).collect();
        // The usual names first, in ssh's order of preference; the rest after, by name. No
        // `.pub`, no known_hosts, no config, no directory.
        assert_eq!(names, vec!["id_ed25519", "id_rsa", "work-laptop"]);
        assert_eq!((found[0].algorithm.as_str(), found[0].comment.as_str(), found[0].encrypted), ("ED25519", "me@here", false));
        assert!(found[2].encrypted, "a key with a passphrase says so");
        assert_eq!(found[2].label(), "work-laptop · ED25519 · work · passphrase");
        assert_eq!(found[1].label(), "id_rsa · RSA");
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn no_ssh_directory_is_no_keys_not_an_error() {
        let _g = ENV.lock().unwrap_or_else(|p| p.into_inner());
        std::env::set_var("KIKI_SSH_DIR", "/nonexistent/kiki/ssh");
        assert!(discover_keys().is_empty());
        std::env::remove_var("KIKI_SSH_DIR");
    }

    #[test]
    fn only_named_keys_are_offered_and_none_named_is_none() {
        let _g = ENV.lock().unwrap_or_else(|p| p.into_inner());
        let home = std::env::var("HOME").unwrap_or_default();
        // One path per line; `~` expands; blank lines are nothing; a saved location from before
        // the field was a list holds one path, which is a list of one.
        assert_eq!(keys_to_try("~/.ssh/a\n\n  /etc/keys/b  \n"), vec![format!("{home}/.ssh/a"), "/etc/keys/b".to_string()]);
        assert_eq!(keys_to_try("~/.ssh/id_ed25519"), vec![format!("{home}/.ssh/id_ed25519")]);

        // None named is none offered — a password-only location — however many keys there are
        // to be found: choosing is the form's job, not something done behind the user's back.
        let d = temp_ssh_dir("none");
        if keygen(&d, "id_ed25519", "ed25519", "", "") {
            std::env::set_var("KIKI_SSH_DIR", &d);
            assert!(keys_to_try("").is_empty());
            std::env::remove_var("KIKI_SSH_DIR");
        }
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn a_failure_says_what_was_offered_and_what_could_not_be() {
        assert_eq!(auth_failure(&[], &[], false), "nothing to sign in with: no usable key was found and no password was given");
        assert_eq!(auth_failure(&["id_ed25519".into()], &[], false), "the server refused the key id_ed25519");
        assert_eq!(auth_failure(&[], &[], true), "the server refused the password");
        assert_eq!(
            auth_failure(&["id_ed25519".into(), "id_rsa".into()], &["work: needs its passphrase".into()], true),
            "the server refused 2 keys (id_ed25519, id_rsa) and the password — could not use work: needs its passphrase"
        );
    }

    #[test]
    fn the_form_offers_keys_and_a_password_as_equals() {
        let sftp = Sftp { rt: tokio::runtime::Builder::new_current_thread().build().unwrap(), sessions: Mutex::new(HashMap::new()) };
        let form = sftp.describe().form;
        let field = |k: &str| form.iter().find(|f| f.str_field("key") == Some(k)).unwrap_or_else(|| panic!("no {k} field")).clone();
        assert_eq!(field("identityFile").str_field("kind"), Some("keys"));
        assert_eq!(field("password").str_field("label"), Some("Password"));
        assert_eq!(field("password").str_field("kind"), Some("password"));
        // Password on one tab, everything about keys on the other; the username on neither — a
        // key needs a username as much as a password does.
        assert_eq!(field("password").str_field("group"), Some("Password"));
        assert_eq!(field("identityFile").str_field("group"), Some("Key"));
        assert_eq!(field("passphrase").str_field("group"), Some("Key"));
        assert_eq!(field("username").str_field("group"), None);
        // The two paths are a page of their own; everything about connecting is on the first.
        assert_eq!(field("remotePath").str_field("page"), Some("Locations"));
        assert_eq!(field("localPath").str_field("page"), Some("Locations"));
        assert_eq!(field("host").str_field("page"), None);
        assert!(sftp.describe().defaults.str_field("identityFile").is_none(), "no key is assumed by the plugin: the form ticks the first one found");
    }
}
