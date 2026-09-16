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
const READ_IN_FLIGHT: usize = 16;
const READ_CHUNK: u32 = 256 * 1024;

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
}

impl Session {
    fn fast(&self) -> FastScan {
        *self.fast.lock().unwrap()
    }
}

struct ClientHandler {
    pinned: Option<String>,
    seen: Arc<Mutex<Option<String>>>,
}

impl client::Handler for ClientHandler {
    type Error = russh::Error;

    async fn check_server_key(&mut self, key: &russh::keys::PublicKey) -> std::result::Result<bool, Self::Error> {
        let fp = key.fingerprint(HashAlg::Sha256).to_string();
        *self.seen.lock().unwrap() = Some(fp.clone());
        Ok(match &self.pinned {
            Some(p) => p == &fp,
            None => true, // trust on first use; the fingerprint is returned to kiki, which pins it in config
        })
    }
}

/// Requests run concurrently (SDK worker threads); sessions are shared behind a mutex and every
/// SFTP call takes `&self`, so a listing and a transfer on different sessions interleave.
struct Sftp {
    rt: Runtime,
    sessions: Mutex<HashMap<String, Arc<Session>>>,
}

fn key(location: &str, role: &str) -> String {
    format!("{location}\u{0}{role}")
}

fn cfg<'a>(config: &'a Value, k: &str) -> &'a str {
    config.str_field(k).unwrap_or("")
}

fn to_meta(m: &russh_sftp::protocol::FileAttributes) -> Meta {
    Meta { size: m.size.unwrap_or(0), mtime_ms: m.mtime.map(|t| t as u64 * 1000).unwrap_or(0), mode: m.permissions.map(|p| p & 0o7777), owner: m.user.clone(), group: m.group.clone() }
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
        // Browsing calls use the browse session; jobs get their own through Connect with role job.
        let k = key(location, "browse");
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
            let mut inflight: VecDeque<tokio::task::JoinHandle<std::result::Result<Vec<u8>, russh_sftp::client::error::Error>>> = VecDeque::new();
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
                        inflight.push_back(issue(raw, &handle, next));
                        next += READ_CHUNK as u64;
                    }
                    let Some(job) = inflight.pop_front() else { break };
                    if sdk::cancelled() {
                        return Err(sdk::cancel_error());
                    }
                    match job.await.map_err(PluginError::io)? {
                        Ok(data) if data.is_empty() => eof = true,
                        Ok(data) => {
                            let short = (data.len() as u32) < READ_CHUNK;
                            out.write_all(&data).map_err(PluginError::io)?;
                            if short {
                                eof = true;
                            }
                        }
                        Err(russh_sftp::client::error::Error::Status(st)) if st.status_code == russh_sftp::protocol::StatusCode::Eof => eof = true,
                        Err(e) => return Err(sftp_err(e)),
                    }
                    if eof {
                        // Drain whatever is still in flight; those chunks are past the end.
                        while let Some(j) = inflight.pop_front() {
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
        meta: Some(Meta { size: s(2).parse().unwrap_or(0), mtime_ms, mode: u32::from_str_radix(&s(4), 8).ok(), owner: Some(s(5)), group: Some(s(6)) }),
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
                sdk::field("identityFile", "Identity file", "file", false, Some("~/.ssh/id_ed25519")),
                sdk::field("passphrase", "Key passphrase", "password", false, None),
                sdk::field("password", "Password (if no key)", "password", false, None),
                sdk::field("remotePath", "Remote path", "path", true, Some("/")),
                sdk::field("localPath", "Local path", "path", false, None),
            ],
            defaults: Value::obj().s("port", "22").s("remotePath", "/").s("identityFile", "~/.ssh/id_ed25519").done(),
            secret_fields: vec!["passphrase", "password"],
            detector_upload: "sizeMtime",
            detector_download: "sizeMtime",
            features: Features { set_mtime: true, mode: true, real_dirs: true, meta_in_scan: true, pipelining: true, partial_read: true },
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
            return Ok(Value::obj().opt_s("fingerprint", s.fingerprint.as_deref()).v("banner", Value::Null).done());
        }
        let host = cfg(config, "host").to_string();
        let port: u16 = cfg(config, "port").parse().unwrap_or(22);
        let user = cfg(config, "username").to_string();
        let identity = cfg(config, "identityFile").replace('~', &std::env::var("HOME").unwrap_or_default());
        let pinned = config.str_field("trustedFingerprint").map(str::to_string);
        let seen = Arc::new(Mutex::new(None));
        let password = secrets.str_field("password").map(str::to_string);
        let passphrase = secrets.str_field("passphrase").map(str::to_string);
        let (handle, sftp, raw) = self.rt.block_on(async {
            let config = Arc::new(client::Config { inactivity_timeout: Some(Duration::from_secs(300)), keepalive_interval: Some(Duration::from_secs(30)), ..Default::default() });
            let handler = ClientHandler { pinned: pinned.clone(), seen: Arc::clone(&seen) };
            let mut handle = client::connect(config, (host.as_str(), port), handler).await.map_err(|e| PluginError::network(format!("{host}:{port}: {e}")))?;
            let mut authed = false;
            if !identity.is_empty() && std::path::Path::new(&identity).exists() {
                match russh::keys::load_secret_key(&identity, passphrase.as_deref()) {
                    Ok(key) => {
                        let hash = handle.best_supported_rsa_hash().await.map_err(net)?.flatten();
                        let r = handle.authenticate_publickey(&user, PrivateKeyWithHashAlg::new(Arc::new(key), hash)).await.map_err(net)?;
                        authed = r.success();
                    }
                    Err(e) => {
                        if password.is_none() {
                            return Err(PluginError::invalid("identityFile", format!("cannot load key: {e}")));
                        }
                    }
                }
            }
            if !authed {
                if let Some(pw) = &password {
                    let r = handle.authenticate_password(&user, pw).await.map_err(net)?;
                    authed = r.success();
                }
            }
            if !authed {
                return Err(PluginError::auth("authentication failed"));
            }
            let ch = handle.channel_open_session().await.map_err(net)?;
            ch.request_subsystem(true, "sftp").await.map_err(net)?;
            let sftp = SftpSession::new(ch.into_stream()).await.map_err(sftp_err)?;
            let ch2 = handle.channel_open_session().await.map_err(net)?;
            ch2.request_subsystem(true, "sftp").await.map_err(net)?;
            let raw = RawSftpSession::new(ch2.into_stream());
            raw.init().await.map_err(sftp_err)?;
            Ok::<_, PluginError>((handle, sftp, Arc::new(raw)))
        })?;
        let fingerprint = seen.lock().unwrap().clone();
        let sess = Session { handle, sftp, raw, fast: Mutex::new(FastScan::None), fingerprint: fingerprint.clone() };
        if role == "browse" {
            let f = self.probe(&sess);
            *sess.fast.lock().unwrap() = f;
        }
        self.sessions.lock().unwrap().insert(k, Arc::new(sess));
        Ok(Value::obj().opt_s("fingerprint", fingerprint.as_deref()).v("banner", Value::Null).done())
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
