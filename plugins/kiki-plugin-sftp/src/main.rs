//! SFTP location plugin. Listing prefers a single `find -printf` over an SSH exec channel when
//! the server allows it (one round trip per directory or per tree); everything else is SFTP.

use kiki_plugin_sdk::json::Value;
use kiki_plugin_sdk::{self as sdk, Describe, Entry, Features, Handler, Kind, Meta, Outgoing, PluginError, Result, WriteArgs};
use russh::client::{self, Handle};
use russh::keys::{HashAlg, PrivateKeyWithHashAlg};
use russh::ChannelMsg;
use russh_sftp::client::SftpSession;
use russh_sftp::protocol::FileType;
use std::collections::HashMap;
use std::io::{Read, Write};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::runtime::Runtime;

#[derive(Clone, Copy, PartialEq, Eq)]
enum FastScan {
    Gnu,
    None,
}

struct Session {
    handle: Handle<ClientHandler>,
    sftp: SftpSession,
    fast: FastScan,
    fingerprint: Option<String>,
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

struct Sftp {
    rt: Runtime,
    sessions: HashMap<String, Session>,
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
    fn session(&self, location: &str) -> Result<&Session> {
        // Browsing calls use the browse session; jobs get their own through Connect with role job.
        let k = key(location, "browse");
        self.sessions.get(&k).ok_or_else(|| PluginError::network("not connected"))
    }

    /// Run one command over an exec channel; returns (stdout, exit status).
    fn exec(&self, sess: &Session, cmd: &str, mut on_data: impl FnMut(&[u8])) -> Result<u32> {
        self.rt.block_on(async {
            let mut ch = sess.handle.channel_open_session().await.map_err(net)?;
            ch.exec(true, cmd).await.map_err(net)?;
            let mut status = 0u32;
            loop {
                match ch.wait().await {
                    Some(ChannelMsg::Data { data }) => on_data(&data),
                    Some(ChannelMsg::ExitStatus { exit_status }) => status = exit_status,
                    Some(ChannelMsg::Eof) | Some(ChannelMsg::Close) => break,
                    Some(_) => {}
                    None => break,
                }
            }
            Ok(status)
        })
    }

    fn probe(&self, sess: &Session) -> FastScan {
        let mut out = Vec::new();
        match self.exec(sess, "command -v find >/dev/null 2>&1 && find --version 2>/dev/null | head -1", |d| out.extend_from_slice(d)) {
            Ok(0) if String::from_utf8_lossy(&out).contains("GNU") => FastScan::Gnu,
            _ => FastScan::None,
        }
    }

    /// GNU find with NUL-separated records: type, link target type, size, mtime, mode, user, group, name.
    fn fast_scan(&self, sess: &Session, path: &str, recursive: bool, sink: &mut dyn FnMut(Vec<Entry>)) -> Result<u64> {
        let depth = if recursive { "" } else { "-maxdepth 1 " };
        let fmt = if recursive { "%y\\0%Y\\0%s\\0%T@\\0%m\\0%u\\0%g\\0%P\\0" } else { "%y\\0%Y\\0%s\\0%T@\\0%m\\0%u\\0%g\\0%f\\0" };
        let cmd = format!("find {} -mindepth 1 {}-printf '{}'", sdk::shell_quote(path), depth, fmt);
        let mut buf: Vec<u8> = Vec::new();
        let mut count = 0u64;
        let mut batch: Vec<Entry> = Vec::with_capacity(1024);
        let mut flush = |batch: &mut Vec<Entry>, sink: &mut dyn FnMut(Vec<Entry>)| {
            if !batch.is_empty() {
                sink(std::mem::take(batch));
            }
        };
        let status = self.exec(sess, &cmd, |data| {
            buf.extend_from_slice(data);
            // Parse complete 8-field records.
            loop {
                let mut fields = Vec::with_capacity(8);
                let mut pos = 0;
                let mut ok = true;
                for _ in 0..8 {
                    match buf[pos..].iter().position(|&b| b == 0) {
                        Some(i) => {
                            fields.push(buf[pos..pos + i].to_vec());
                            pos += i + 1;
                        }
                        None => {
                            ok = false;
                            break;
                        }
                    }
                }
                if !ok {
                    break;
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
                batch.push(Entry {
                    name,
                    kind,
                    meta: Some(Meta { size: s(2).parse().unwrap_or(0), mtime_ms, mode: u32::from_str_radix(&s(4), 8).ok(), owner: Some(s(5)), group: Some(s(6)) }),
                    rel: if recursive { rel } else { String::new() },
                });
                count += 1;
                if batch.len() >= 1024 {
                    flush(&mut batch, sink);
                }
            }
        })?;
        flush(&mut batch, sink);
        if status != 0 {
            return Err(PluginError::io(format!("find exited with {status}")));
        }
        Ok(count)
    }
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

    fn validate(&mut self, config: &Value) -> Result<()> {
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

    fn connect(&mut self, location: &str, role: &str, config: &Value, secrets: &Value) -> Result<Value> {
        let k = key(location, role);
        if let Some(s) = self.sessions.get(&k) {
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
        let (handle, sftp) = self.rt.block_on(async {
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
            Ok::<_, PluginError>((handle, sftp))
        })?;
        let fingerprint = seen.lock().unwrap().clone();
        let mut sess = Session { handle, sftp, fast: FastScan::None, fingerprint: fingerprint.clone() };
        if role == "browse" {
            sess.fast = self.probe(&sess);
        }
        self.sessions.insert(k, sess);
        Ok(Value::obj().opt_s("fingerprint", fingerprint.as_deref()).v("banner", Value::Null).done())
    }

    fn disconnect(&mut self, location: &str, role: &str) {
        if let Some(s) = self.sessions.remove(&key(location, role)) {
            let _ = self.rt.block_on(async { s.handle.disconnect(russh::Disconnect::ByApplication, "", "en").await });
        }
    }

    fn capabilities(&mut self, location: &str) -> Result<Value> {
        let fast = self.session(location)?.fast;
        Ok(Value::obj().b("trash", false).b("setMtime", true).b("mode", true).b("realDirs", true).v("digestKind", Value::Null).s("separator", "/").opt_s("fastScan", if fast == FastScan::Gnu { Some("gnu") } else { None }).b("partialRead", true).done())
    }

    fn scan(&mut self, location: &str, path: &str, recursive: bool, sink: &mut dyn FnMut(Vec<Entry>)) -> Result<u64> {
        let fast = self.session(location)?.fast;
        if fast == FastScan::Gnu {
            let r = {
                let sess = self.session(location)?;
                self.fast_scan(sess, path, recursive, sink)
            };
            match r {
                Ok(n) => return Ok(n),
                Err(_) => {
                    // Fall back for the rest of the session; anything already sent is discarded by the daemon.
                    if let Some(s) = self.sessions.get_mut(&key(location, "browse")) {
                        s.fast = FastScan::None;
                    }
                }
            }
        }
        if recursive {
            return Err(PluginError::unsupported());
        }
        let sess = self.session(location)?;
        let sftp = &sess.sftp;
        self.rt.block_on(async {
            let rd = sftp.read_dir(path).await.map_err(sftp_err)?;
            let mut batch = Vec::with_capacity(256);
            let mut n = 0u64;
            for e in rd {
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

    fn stat(&mut self, location: &str, path: &str) -> Result<Meta> {
        let sess = self.session(location)?;
        let sftp = &sess.sftp;
        self.rt.block_on(async { sftp.symlink_metadata(path).await.map(|m| to_meta(&m)).map_err(sftp_err) })
    }

    fn read(&mut self, location: &str, path: &str, offset: u64, out: &mut Outgoing) -> Result<()> {
        let sess = self.session(location)?;
        let sftp = &sess.sftp;
        self.rt.block_on(async {
            let mut f = sftp.open(path).await.map_err(sftp_err)?;
            if offset > 0 {
                use tokio::io::AsyncSeekExt;
                f.seek(std::io::SeekFrom::Start(offset)).await.map_err(PluginError::io)?;
            }
            let mut buf = vec![0u8; 256 * 1024];
            loop {
                let n = f.read(&mut buf).await.map_err(PluginError::io)?;
                if n == 0 {
                    break;
                }
                out.write_all(&buf[..n]).map_err(PluginError::io)?;
            }
            Ok(())
        })
    }

    fn write(&mut self, location: &str, path: &str, mut args: WriteArgs) -> Result<u64> {
        let sess = self.session(location)?;
        let sftp = &sess.sftp;
        let rt = &self.rt;
        let mtime = args.mtime_ms;
        rt.block_on(async {
            let mut f = sftp.create(path).await.map_err(sftp_err)?;
            let mut buf = vec![0u8; 256 * 1024];
            let mut total = 0u64;
            loop {
                let n = args.data.read(&mut buf).map_err(PluginError::io)?;
                if n == 0 {
                    break;
                }
                f.write_all(&buf[..n]).await.map_err(PluginError::io)?;
                total += n as u64;
            }
            f.shutdown().await.map_err(PluginError::io)?;
            if let Some(t) = mtime {
                let mut attrs = russh_sftp::protocol::FileAttributes::default();
                attrs.mtime = Some((t / 1000) as u32);
                attrs.atime = Some((t / 1000) as u32);
                let _ = sftp.set_metadata(path, attrs).await;
            }
            Ok(total)
        })
    }

    fn mkdir(&mut self, location: &str, path: &str) -> Result<()> {
        let sftp = &self.session(location)?.sftp;
        self.rt.block_on(async { sftp.create_dir(path).await.map_err(sftp_err) })
    }

    fn rename(&mut self, location: &str, from: &str, to: &str) -> Result<()> {
        let sftp = &self.session(location)?.sftp;
        self.rt.block_on(async { sftp.rename(from, to).await.map_err(sftp_err) })
    }

    fn delete(&mut self, location: &str, path: &str) -> Result<()> {
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

    fn set_mtime(&mut self, location: &str, path: &str, mtime_ms: u64) -> Result<()> {
        let sftp = &self.session(location)?.sftp;
        self.rt.block_on(async {
            let mut attrs = russh_sftp::protocol::FileAttributes::default();
            attrs.mtime = Some((mtime_ms / 1000) as u32);
            attrs.atime = Some((mtime_ms / 1000) as u32);
            sftp.set_metadata(path, attrs).await.map_err(sftp_err)
        })
    }

    fn chmod(&mut self, location: &str, path: &str, mode: u32) -> Result<()> {
        let sftp = &self.session(location)?.sftp;
        self.rt.block_on(async {
            let mut attrs = russh_sftp::protocol::FileAttributes::default();
            attrs.permissions = Some(mode);
            sftp.set_metadata(path, attrs).await.map_err(sftp_err)
        })
    }
}

fn main() {
    let rt = tokio::runtime::Builder::new_current_thread().enable_all().build().expect("tokio runtime");
    let mut h = Sftp { rt, sessions: HashMap::new() };
    if let Err(e) = sdk::run(&mut h) {
        eprintln!("kiki-plugin-sftp: {e}");
    }
}
