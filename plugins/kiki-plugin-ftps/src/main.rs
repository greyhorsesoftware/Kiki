//! FTPS location plugin: explicit TLS by default (AUTH TLS on 21), implicit on request.
//! Listings use MLSD where the server offers it (machine-readable, with facts), else LIST.

use kiki_plugin_sdk::json::Value;
use kiki_plugin_sdk::{self as sdk, Describe, Entry, Features, Handler, Kind, Meta, Outgoing, PluginError, Result, WriteArgs};
use rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier};
use rustls::pki_types::{CertificateDer, ServerName, UnixTime};
use rustls::{ClientConfig, DigitallySignedStruct, RootCertStore, SignatureScheme};
use std::collections::HashMap;
use std::io::{Read, Write};
use std::sync::{Arc, Mutex};
use suppaftp::list::File as ListFile;
use suppaftp::types::FileType;
use suppaftp::{FtpError, Mode, RustlsConnector, RustlsFtpStream};

struct Session {
    ftp: RustlsFtpStream,
    mlsd: bool,
    fingerprint: Option<String>,
}

struct Ftps {
    sessions: HashMap<String, Session>,
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
/// records the fingerprint it saw so kiki can offer to pin it.
#[derive(Debug)]
struct PinVerifier {
    pinned: Option<String>,
    insecure: bool,
    seen: Arc<Mutex<Option<String>>>,
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
    fn verify_server_cert(&self, end_entity: &CertificateDer<'_>, intermediates: &[CertificateDer<'_>], server_name: &ServerName<'_>, ocsp: &[u8], now: UnixTime) -> std::result::Result<ServerCertVerified, rustls::Error> {
        let fp = sha256_hex(end_entity.as_ref());
        *self.seen.lock().unwrap() = Some(fp.clone());
        if let Some(p) = &self.pinned {
            return if p.eq_ignore_ascii_case(&fp) { Ok(ServerCertVerified::assertion()) } else { Err(rustls::Error::General("certificate fingerprint does not match the pinned one".into())) };
        }
        if self.insecure {
            return Ok(ServerCertVerified::assertion());
        }
        self.inner.verify_server_cert(end_entity, intermediates, server_name, ocsp, now)
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

fn tls_config(pinned: Option<String>, insecure: bool, seen: Arc<Mutex<Option<String>>>) -> Arc<ClientConfig> {
    let mut roots = RootCertStore::empty();
    roots.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
    let inner = rustls::client::WebPkiServerVerifier::builder(Arc::new(roots)).build().expect("webpki verifier");
    let verifier = PinVerifier { pinned, insecure, seen, inner };
    Arc::new(ClientConfig::builder().dangerous().with_custom_certificate_verifier(Arc::new(verifier)).with_no_client_auth())
}

fn entry_from(f: &ListFile) -> Entry {
    let kind = if f.is_directory() { Kind::Dir } else if f.is_symlink() { Kind::Link } else { Kind::File };
    let mtime_ms = f.modified().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0);
    Entry { name: f.name().to_string(), kind, meta: Some(Meta { size: f.size() as u64, mtime_ms, mode: None, owner: f.uid().map(|u| u.to_string()), group: f.gid().map(|g| g.to_string()) }), rel: String::new() }
}

impl Ftps {
    fn session(&mut self, location: &str) -> Result<&mut Session> {
        self.sessions.get_mut(&key(location, "browse")).ok_or_else(|| PluginError::network("not connected"))
    }
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
                sdk::field("remotePath", "Remote path", "path", true, Some("/")),
                sdk::field("localPath", "Local path", "path", false, None),
            ],
            defaults: Value::obj().s("port", "21").s("remotePath", "/").s("encryption", "Explicit TLS (AUTH TLS)").done(),
            secret_fields: vec!["password"],
            detector_upload: "sizeOnly",
            detector_download: "sizeMtime",
            features: Features { set_mtime: false, mode: false, real_dirs: true, meta_in_scan: true, pipelining: false, partial_read: true },
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
        let port: u16 = cfg(config, "port").parse().unwrap_or(21);
        let implicit = cfg(config, "encryption").starts_with("Implicit");
        let seen = Arc::new(Mutex::new(None));
        let tls = tls_config(config.str_field("trustedFingerprint").map(str::to_string), config.get("insecure").and_then(Value::as_bool).unwrap_or(false), Arc::clone(&seen));
        let connector = RustlsConnector::from(tls);
        let addr = format!("{host}:{port}");
        let mut ftp = if implicit {
            RustlsFtpStream::connect_secure_implicit(&addr, connector, &host).map_err(ftp_err)?
        } else {
            let plain = RustlsFtpStream::connect(&addr).map_err(ftp_err)?;
            plain.into_secure(connector, &host).map_err(ftp_err)?
        };
        ftp.login(cfg(config, "username"), secrets.str_field("password").unwrap_or("")).map_err(ftp_err)?;
        ftp.set_mode(Mode::Passive);
        let _ = ftp.transfer_type(FileType::Binary);
        let mlsd = ftp.feat().map(|f| f.iter().any(|(k, _)| k.eq_ignore_ascii_case("MLST"))).unwrap_or(false);
        let fingerprint = seen.lock().unwrap().clone();
        self.sessions.insert(k, Session { ftp, mlsd, fingerprint: fingerprint.clone() });
        Ok(Value::obj().opt_s("fingerprint", fingerprint.as_deref()).v("banner", Value::Null).done())
    }

    fn disconnect(&mut self, location: &str, role: &str) {
        if let Some(mut s) = self.sessions.remove(&key(location, role)) {
            let _ = s.ftp.quit();
        }
    }

    fn capabilities(&mut self, _location: &str) -> Result<Value> {
        Ok(Value::obj().b("trash", false).b("setMtime", false).b("mode", false).b("realDirs", true).v("digestKind", Value::Null).s("separator", "/").v("fastScan", Value::Null).b("partialRead", true).done())
    }

    fn scan(&mut self, location: &str, path: &str, recursive: bool, sink: &mut dyn FnMut(Vec<Entry>)) -> Result<u64> {
        if recursive {
            return Err(PluginError::unsupported());
        }
        let sess = self.session(location)?;
        let lines = if sess.mlsd { sess.ftp.mlsd(Some(path)).map_err(ftp_err)? } else { sess.ftp.list(Some(path)).map_err(ftp_err)? };
        let mut batch = Vec::with_capacity(256);
        let mut n = 0u64;
        for line in &lines {
            let parsed = if sess.mlsd { ListFile::from_mlsx_line(line) } else { ListFile::from_posix_line(line) };
            if let Ok(f) = parsed {
                if f.name() == "." || f.name() == ".." {
                    continue;
                }
                batch.push(entry_from(&f));
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

    fn stat(&mut self, location: &str, path: &str) -> Result<Meta> {
        let sess = self.session(location)?;
        let size = sess.ftp.size(path).map_err(ftp_err)? as u64;
        let mtime_ms = sess.ftp.mdtm(path).ok().map(|t| t.timestamp_millis().max(0) as u64).unwrap_or(0);
        Ok(Meta { size, mtime_ms, mode: None, owner: None, group: None })
    }

    fn read(&mut self, location: &str, path: &str, offset: u64, out: &mut Outgoing) -> Result<()> {
        let sess = self.session(location)?;
        if offset > 0 {
            sess.ftp.resume_transfer(offset as usize).map_err(ftp_err)?;
        }
        let mut stream = sess.ftp.retr_as_stream(path).map_err(ftp_err)?;
        let mut buf = vec![0u8; 256 * 1024];
        loop {
            let n = stream.read(&mut buf).map_err(PluginError::io)?;
            if n == 0 {
                break;
            }
            out.write_all(&buf[..n]).map_err(PluginError::io)?;
        }
        sess.ftp.finalize_retr_stream(stream).map_err(ftp_err)?;
        Ok(())
    }

    fn write(&mut self, location: &str, path: &str, mut args: WriteArgs) -> Result<u64> {
        let sess = self.session(location)?;
        let n = sess.ftp.put_file(path, &mut args.data).map_err(ftp_err)?;
        Ok(n)
    }

    fn mkdir(&mut self, location: &str, path: &str) -> Result<()> {
        self.session(location)?.ftp.mkdir(path).map_err(ftp_err)
    }

    fn rename(&mut self, location: &str, from: &str, to: &str) -> Result<()> {
        self.session(location)?.ftp.rename(from, to).map_err(ftp_err)
    }

    fn delete(&mut self, location: &str, path: &str) -> Result<()> {
        let sess = self.session(location)?;
        match sess.ftp.rm(path) {
            Ok(()) => Ok(()),
            Err(_) => sess.ftp.rmdir(path).map_err(ftp_err),
        }
    }
}

fn main() {
    let mut h = Ftps { sessions: HashMap::new() };
    if let Err(e) = sdk::run(&mut h) {
        eprintln!("kiki-plugin-ftps: {e}");
    }
}
