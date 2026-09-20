//! FTPS location plugin: explicit TLS by default (AUTH TLS on 21), implicit on request.
//! Listings use MLSD where the server offers it (machine-readable, with facts), else LIST.

use kiki_plugin_sdk::json::Value;
use kiki_plugin_sdk::{self as sdk, Describe, Entry, Features, Handler, Kind, Meta, Outgoing, PluginError, Result, WriteArgs};
use rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier};
use rustls::pki_types::{CertificateDer, ServerName, UnixTime};
use rustls::{ClientConfig, DigitallySignedStruct, RootCertStore, SignatureScheme};
use std::collections::HashMap;
use std::io::{Read, Write};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use suppaftp::list::File as ListFile;
use suppaftp::types::FileType;
use suppaftp::{FtpError, Mode, RustlsConnector, RustlsFtpStream, Status};

struct Session {
    ftp: RustlsFtpStream,
    mlsd: bool,
    fingerprint: Option<String>,
}

/// Requests run concurrently (SDK worker threads); an FTP session is one control connection with
/// at most one transfer in flight, so each session sits behind its own mutex.
struct Ftps {
    sessions: Mutex<HashMap<String, Arc<Mutex<Session>>>>,
}

fn key(location: &str, role: &str) -> String {
    format!("{location}\u{0}{role}")
}

fn cfg<'a>(config: &'a Value, k: &str) -> &'a str {
    config.str_field(k).unwrap_or("")
}

fn ftp_err(e: FtpError) -> PluginError {
    match e {
        FtpError::ConnectionError(io) => PluginError::network(io.to_string()),
        FtpError::SecureError(s) => PluginError::network(s),
        FtpError::UnexpectedResponse(r) => match r.status as u32 {
            530 => PluginError::auth(String::from_utf8_lossy(&r.body).trim().to_string()),
            550 => PluginError::not_found(),
            553 => PluginError::new("Denied", String::from_utf8_lossy(&r.body).trim().to_string()),
            _ => PluginError::io(String::from_utf8_lossy(&r.body).trim().to_string()),
        },
        FtpError::BadResponse => PluginError::io("bad response"),
        FtpError::InvalidAddress(e) => PluginError::network(e.to_string()),
    }
}

/// Accepts a certificate whose SHA-256 fingerprint matches the pinned one, or any when `insecure`;
/// records the fingerprint it saw so kiki can offer to pin it, and whether it refused it.
#[derive(Debug)]
struct PinVerifier {
    pinned: Option<String>,
    insecure: bool,
    seen: Arc<Mutex<Option<String>>>,
    rejected: Arc<AtomicBool>,
    inner: Arc<rustls::client::WebPkiServerVerifier>,
}

fn sha256_hex(data: &[u8]) -> String {
    let out = ring::digest::digest(&ring::digest::SHA256, data);
    let mut s = String::with_capacity(95);
    for (i, b) in out.as_ref().iter().enumerate() {
        if i > 0 {
            s.push(':');
        }
        s.push_str(&format!("{b:02X}"));
    }
    s
}

impl ServerCertVerifier for PinVerifier {
    fn verify_server_cert(
        &self,
        end_entity: &CertificateDer<'_>,
        intermediates: &[CertificateDer<'_>],
        server_name: &ServerName<'_>,
        ocsp: &[u8],
        now: UnixTime,
    ) -> std::result::Result<ServerCertVerified, rustls::Error> {
        let fp = sha256_hex(end_entity.as_ref());
        *self.seen.lock().unwrap() = Some(fp.clone());
        let r = if let Some(p) = &self.pinned {
            if p.eq_ignore_ascii_case(&fp) {
                Ok(ServerCertVerified::assertion())
            } else {
                Err(rustls::Error::General("certificate fingerprint does not match the pinned one".into()))
            }
        } else if self.insecure {
            Ok(ServerCertVerified::assertion())
        } else {
            self.inner.verify_server_cert(end_entity, intermediates, server_name, ocsp, now)
        };
        if r.is_err() {
            self.rejected.store(true, Ordering::SeqCst);
        }
        r
    }
    fn verify_tls12_signature(&self, message: &[u8], cert: &CertificateDer<'_>, dss: &DigitallySignedStruct) -> std::result::Result<HandshakeSignatureValid, rustls::Error> {
        self.inner.verify_tls12_signature(message, cert, dss)
    }
    fn verify_tls13_signature(&self, message: &[u8], cert: &CertificateDer<'_>, dss: &DigitallySignedStruct) -> std::result::Result<HandshakeSignatureValid, rustls::Error> {
        self.inner.verify_tls13_signature(message, cert, dss)
    }
    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        self.inner.supported_verify_schemes()
    }
}

fn tls_config(pinned: Option<String>, insecure: bool, seen: Arc<Mutex<Option<String>>>, rejected: Arc<AtomicBool>) -> Arc<ClientConfig> {
    let mut roots = RootCertStore::empty();
    roots.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
    let inner = rustls::client::WebPkiServerVerifier::builder(Arc::new(roots)).build().expect("webpki verifier");
    let verifier = PinVerifier { pinned, insecure, seen, rejected, inner };
    Arc::new(ClientConfig::builder().dangerous().with_custom_certificate_verifier(Arc::new(verifier)).with_no_client_auth())
}

fn entry_from(f: &ListFile) -> Entry {
    let kind = if f.is_directory() {
        Kind::Dir
    } else if f.is_symlink() {
        Kind::Link
    } else {
        Kind::File
    };
    let mtime_ms = f.modified().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0);
    Entry {
        name: f.name().to_string(),
        kind,
        meta: Some(Meta { hidden: false, size: f.size() as u64, mtime_ms, mode: None, owner: f.uid().map(|u| u.to_string()), group: f.gid().map(|g| g.to_string()) }),
        rel: String::new(),
    }
}

/// One MLSD line (RFC 3659): `fact=value;...; name`. Parsed here rather than by suppaftp, whose
/// parser rejects the facts real servers emit (ProFTPD/pure-ftpd's `UNIX.mode=0644`, `type=cdir`),
/// which would silently drop every entry. `None` for the `.`/`..` entries.
fn mlsx_entry(line: &str) -> Option<Entry> {
    let (facts, name) = line.split_once(' ')?;
    if name.is_empty() || name == "." || name == ".." {
        return None;
    }
    let mut kind = Kind::File;
    let mut meta = Meta::default();
    for fact in facts.split(';') {
        let Some((k, v)) = fact.split_once('=') else { continue };
        match k.to_ascii_lowercase().as_str() {
            "type" => {
                kind = match v.to_ascii_lowercase().as_str() {
                    "file" => Kind::File,
                    "dir" => Kind::Dir,
                    "cdir" | "pdir" => return None,
                    t if t == "link" || t.starts_with("os.unix=s") => Kind::Link,
                    _ => Kind::Other,
                }
            }
            "size" => meta.size = v.parse().unwrap_or(0),
            "modify" => meta.mtime_ms = mlsx_time_ms(v),
            "unix.mode" => meta.mode = u32::from_str_radix(v, 8).ok().map(|m| m & 0o7777),
            "unix.uid" | "unix.owner" => meta.owner = Some(v.to_string()),
            "unix.gid" | "unix.group" => meta.group = Some(v.to_string()),
            _ => {}
        }
    }
    Some(Entry { name: name.to_string(), kind, meta: Some(meta), rel: String::new() })
}

/// `YYYYMMDDHHMMSS[.sss]` (UTC) to epoch milliseconds; 0 when malformed.
fn mlsx_time_ms(v: &str) -> u64 {
    let whole = v.split_once('.').map(|(w, _)| w).unwrap_or(v);
    if whole.len() != 14 || !whole.bytes().all(|b| b.is_ascii_digit()) {
        return 0;
    }
    let n = |a: usize, b: usize| whole[a..b].parse::<i64>().unwrap();
    let (y, m, d) = (n(0, 4), n(4, 6), n(6, 8));
    // days from civil, Howard Hinnant's algorithm
    let (y, m) = if m <= 2 { (y - 1, m + 9) } else { (y, m - 3) };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let doy = (153 * m + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146097 + doe - 719468;
    let secs = days * 86400 + n(8, 10) * 3600 + n(10, 12) * 60 + n(12, 14);
    secs.max(0) as u64 * 1000
}

impl Ftps {
    fn session(&self, location: &str) -> Result<Arc<Mutex<Session>>> {
        self.sessions.lock().unwrap().get(&key(location, &sdk::current_role())).cloned().ok_or_else(|| PluginError::network("not connected"))
    }
}

/// End an upload's data connection so that the server keeps all of it.
///
/// An upload only ever writes, but a TLS 1.3 server writes too: session tickets, right after the
/// handshake. Nobody reads them, and a socket closed with unread data in it is closed with a
/// reset, not a FIN — on which the server throws away what it had received and not yet read. The
/// file arrived short by a different amount every time (seen with pyftpdlib: 0.8–2.5 MB of 3 MB),
/// and the server still said 226. So: say goodbye in TLS, send the FIN ourselves, and read until
/// the server has closed its side. Only then is the socket dropped. (The goodbye in TLS is the
/// library's, when its stream is dropped; its types are private, so this gets the socket alone.)
fn finish_upload(socket: std::net::TcpStream) {
    let mut socket = socket;
    let _ = socket.shutdown(std::net::Shutdown::Write);
    let _ = socket.set_read_timeout(Some(std::time::Duration::from_secs(10)));
    let mut sink = [0u8; 4096];
    while matches!(socket.read(&mut sink), Ok(n) if n > 0) {}
}

impl Handler for Ftps {
    fn describe(&self) -> Describe {
        Describe {
            scheme: "ftps",
            display_name: "FTPS",
            version: "1.0",
            form: vec![
                sdk::field("name", "Name", "text", true, None),
                sdk::field("host", "Host", "text", true, None),
                sdk::field("port", "Port", "port", true, Some("21")),
                sdk::field("username", "Username", "text", true, None),
                sdk::field("password", "Password", "password", true, None),
                sdk::select_field("encryption", "Encryption", &["Explicit TLS (AUTH TLS)", "Implicit TLS"], "Explicit TLS (AUTH TLS)"),
                sdk::on_page("Locations", sdk::field("remotePath", "Remote path", "path", true, Some("/"))),
                sdk::on_page("Locations", sdk::field("localPath", "Local path", "path", false, None)),
            ],
            defaults: Value::obj().s("port", "21").s("remotePath", "/").s("encryption", "Explicit TLS (AUTH TLS)").done(),
            secret_fields: vec!["password"],
            detector_upload: "sizeOnly",
            detector_download: "sizeMtime",
            features: Features { set_mtime: false, mode: false, real_dirs: true, meta_in_scan: true, pipelining: false, partial_read: true },
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
            return Ok(Value::obj().opt_s("fingerprint", s.lock().unwrap().fingerprint.as_deref()).v("banner", Value::Null).done());
        }
        let host = cfg(config, "host").to_string();
        let port: u16 = cfg(config, "port").parse().unwrap_or(21);
        let implicit = cfg(config, "encryption").starts_with("Implicit");
        let seen = Arc::new(Mutex::new(None));
        let rejected = Arc::new(AtomicBool::new(false));
        let tls = tls_config(config.str_field("trustedFingerprint").map(str::to_string), config.get("insecure").and_then(Value::as_bool).unwrap_or(false), Arc::clone(&seen), Arc::clone(&rejected));
        let connector = RustlsConnector::from(tls);
        let addr = format!("{host}:{port}");
        // A handshake that failed because the verifier refused the certificate is reported as
        // Invalid/fingerprint with the fingerprint in the message, so kiki can offer to pin it.
        let cert_err = |e: FtpError| if rejected.load(Ordering::SeqCst) { PluginError::invalid("fingerprint", seen.lock().unwrap().clone().unwrap_or_default()) } else { ftp_err(e) };
        let mut ftp = if implicit {
            let mut ftp = RustlsFtpStream::connect_secure_implicit(&addr, connector, &host).map_err(&cert_err)?;
            // The data channel starts out unprotected (RFC 4217); into_secure does this for explicit mode.
            ftp.custom_command("PBSZ 0", &[Status::CommandOk]).map_err(ftp_err)?;
            ftp.custom_command("PROT P", &[Status::CommandOk]).map_err(ftp_err)?;
            ftp
        } else {
            let plain = RustlsFtpStream::connect(&addr).map_err(ftp_err)?;
            plain.into_secure(connector, &host).map_err(&cert_err)?
        };
        ftp.login(cfg(config, "username"), secrets.str_field("password").unwrap_or("")).map_err(ftp_err)?;
        ftp.set_mode(Mode::Passive);
        let _ = ftp.transfer_type(FileType::Binary);
        let mlsd = ftp.feat().map(|f| f.iter().any(|(k, _)| k.eq_ignore_ascii_case("MLST"))).unwrap_or(false);
        let fingerprint = seen.lock().unwrap().clone();
        self.sessions.lock().unwrap().insert(k, Arc::new(Mutex::new(Session { ftp, mlsd, fingerprint: fingerprint.clone() })));
        Ok(Value::obj().opt_s("fingerprint", fingerprint.as_deref()).v("banner", Value::Null).done())
    }

    fn disconnect(&self, location: &str, role: &str) {
        let s = self.sessions.lock().unwrap().remove(&key(location, role));
        if let Some(s) = s {
            let _ = s.lock().unwrap().ftp.quit();
        }
    }

    fn capabilities(&self, _location: &str) -> Result<Value> {
        Ok(Value::obj().b("trash", false).b("setMtime", false).b("mode", false).b("realDirs", true).v("digestKind", Value::Null).s("separator", "/").v("fastScan", Value::Null).b("partialRead", true).done())
    }

    fn scan(&self, location: &str, path: &str, recursive: bool, sink: &mut dyn FnMut(Vec<Entry>)) -> Result<u64> {
        if recursive {
            return Err(PluginError::unsupported());
        }
        let s = self.session(location)?;
        let mut sess = s.lock().unwrap();
        let lines = if sess.mlsd { sess.ftp.mlsd(Some(path)).map_err(ftp_err)? } else { sess.ftp.list(Some(path)).map_err(ftp_err)? };
        let mut batch = Vec::with_capacity(256);
        let mut n = 0u64;
        for line in &lines {
            if sdk::cancelled() {
                return Err(sdk::cancel_error());
            }
            let entry = if sess.mlsd { mlsx_entry(line) } else { ListFile::from_posix_line(line).ok().filter(|f| f.name() != "." && f.name() != "..").map(|f| entry_from(&f)) };
            if let Some(e) = entry {
                batch.push(e);
                n += 1;
                if batch.len() >= 256 {
                    sink(std::mem::take(&mut batch));
                }
            }
        }
        if !batch.is_empty() {
            sink(batch);
        }
        Ok(n)
    }

    fn stat(&self, location: &str, path: &str) -> Result<Meta> {
        let s = self.session(location)?;
        let mut sess = s.lock().unwrap();
        let size = sess.ftp.size(path).map_err(ftp_err)? as u64;
        let mtime_ms = sess.ftp.mdtm(path).ok().map(|t| t.and_utc().timestamp_millis().max(0) as u64).unwrap_or(0);
        Ok(Meta { hidden: false, size, mtime_ms, mode: None, owner: None, group: None })
    }

    fn read(&self, location: &str, path: &str, offset: u64, out: &mut Outgoing) -> Result<()> {
        let s = self.session(location)?;
        let mut sess = s.lock().unwrap();
        if offset > 0 {
            sess.ftp.resume_transfer(offset as usize).map_err(ftp_err)?;
        }
        let mut stream = sess.ftp.retr_as_stream(path).map_err(ftp_err)?;
        let mut buf = vec![0u8; 256 * 1024];
        loop {
            if sdk::cancelled() {
                return Err(sdk::cancel_error());
            }
            let n = stream.read(&mut buf).map_err(PluginError::io)?;
            if n == 0 {
                break;
            }
            out.write_all(&buf[..n]).map_err(PluginError::io)?;
        }
        sess.ftp.finalize_retr_stream(stream).map_err(ftp_err)?;
        Ok(())
    }

    fn write(&self, location: &str, path: &str, mut args: WriteArgs) -> Result<u64> {
        let s = self.session(location)?;
        let mut sess = s.lock().unwrap();
        let mut data = sess.ftp.put_with_stream(path).map_err(ftp_err)?;
        let n = std::io::copy(&mut args.data, &mut data).map_err(PluginError::io)?;
        // A second handle on the socket keeps it open while the library's stream is dropped —
        // which is where it flushes and sends TLS's close_notify — so that the close itself can
        // be done properly (see `finish_upload`).
        let socket = data.get_ref().try_clone().map_err(PluginError::io)?;
        drop(data);
        finish_upload(socket);
        sess.ftp.finalize_put_stream(std::io::sink()).map_err(ftp_err)?;
        Ok(n)
    }

    fn mkdir(&self, location: &str, path: &str) -> Result<()> {
        self.session(location)?.lock().unwrap().ftp.mkdir(path).map_err(ftp_err)
    }

    fn rename(&self, location: &str, from: &str, to: &str) -> Result<()> {
        self.session(location)?.lock().unwrap().ftp.rename(from, to).map_err(ftp_err)
    }

    fn delete(&self, location: &str, path: &str) -> Result<()> {
        let s = self.session(location)?;
        let mut sess = s.lock().unwrap();
        match sess.ftp.rm(path) {
            Ok(()) => Ok(()),
            Err(_) => sess.ftp.rmdir(path).map_err(ftp_err),
        }
    }
}

fn main() {
    let h = Ftps { sessions: Mutex::new(HashMap::new()) };
    if let Err(e) = sdk::run(&h) {
        eprintln!("kiki-plugin-ftps: {e}");
    }
}
