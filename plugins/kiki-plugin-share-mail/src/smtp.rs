//! A small SMTP client: implicit TLS, STARTTLS or plain; AUTH PLAIN; one multipart/mixed
//! message with base64 attachments. No crate beyond rustls.

use kiki_plugin_sdk::{PluginError, Result};
use rustls::pki_types::ServerName;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpStream;
use std::sync::Arc;
use std::time::Duration;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Security {
    /// TLS from the first byte (port 465)
    Tls,
    /// plain greeting, then STARTTLS (port 587)
    StartTls,
    None,
}

pub struct Account {
    pub host: String,
    pub port: u16,
    pub security: Security,
    pub username: String,
    pub password: String,
    pub from: String,
    /// sha256 hex of the server certificate, accepted instead of the public roots
    pub trust_fingerprint: Option<String>,
}

pub struct Message<'a> {
    pub to: Vec<String>,
    pub subject: &'a str,
    pub body: &'a str,
    /// (file name, bytes)
    pub attachments: Vec<(String, Vec<u8>)>,
}

enum Stream {
    Plain(TcpStream),
    Tls(Box<rustls::StreamOwned<rustls::ClientConnection, TcpStream>>),
}

impl Read for Stream {
    fn read(&mut self, b: &mut [u8]) -> std::io::Result<usize> {
        match self {
            Stream::Plain(s) => s.read(b),
            Stream::Tls(s) => s.read(b),
        }
    }
}
impl Write for Stream {
    fn write(&mut self, b: &[u8]) -> std::io::Result<usize> {
        match self {
            Stream::Plain(s) => s.write(b),
            Stream::Tls(s) => s.write(b),
        }
    }
    fn flush(&mut self) -> std::io::Result<()> {
        match self {
            Stream::Plain(s) => s.flush(),
            Stream::Tls(s) => s.flush(),
        }
    }
}

/// Accepts exactly one certificate, identified by its SHA-256 fingerprint (self-signed servers).
#[derive(Debug)]
struct Pinned(Vec<u8>);

impl rustls::client::danger::ServerCertVerifier for Pinned {
    fn verify_server_cert(
        &self,
        end_entity: &rustls::pki_types::CertificateDer<'_>,
        _: &[rustls::pki_types::CertificateDer<'_>],
        _: &ServerName<'_>,
        _: &[u8],
        _: rustls::pki_types::UnixTime,
    ) -> std::result::Result<rustls::client::danger::ServerCertVerified, rustls::Error> {
        let d = ring::digest::digest(&ring::digest::SHA256, end_entity.as_ref());
        if d.as_ref() == self.0.as_slice() {
            Ok(rustls::client::danger::ServerCertVerified::assertion())
        } else {
            Err(rustls::Error::General(format!("certificate fingerprint {} is not the pinned one", hex(d.as_ref()))))
        }
    }
    fn verify_tls12_signature(&self, m: &[u8], c: &rustls::pki_types::CertificateDer<'_>, d: &rustls::DigitallySignedStruct) -> std::result::Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls12_signature(m, c, d, &rustls::crypto::ring::default_provider().signature_verification_algorithms)
    }
    fn verify_tls13_signature(&self, m: &[u8], c: &rustls::pki_types::CertificateDer<'_>, d: &rustls::DigitallySignedStruct) -> std::result::Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls13_signature(m, c, d, &rustls::crypto::ring::default_provider().signature_verification_algorithms)
    }
    fn supported_verify_schemes(&self) -> Vec<rustls::SignatureScheme> {
        rustls::crypto::ring::default_provider().signature_verification_algorithms.supported_schemes()
    }
}

pub fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

fn tls_config(acc: &Account) -> Result<Arc<rustls::ClientConfig>> {
    let provider = Arc::new(rustls::crypto::ring::default_provider());
    let builder = rustls::ClientConfig::builder_with_provider(provider).with_safe_default_protocol_versions().map_err(|e| PluginError::network(e.to_string()))?;
    let cfg = match &acc.trust_fingerprint {
        Some(fp) if !fp.is_empty() => {
            let bytes: Vec<u8> = (0..fp.len() / 2).filter_map(|i| u8::from_str_radix(&fp[i * 2..i * 2 + 2], 16).ok()).collect();
            builder.dangerous().with_custom_certificate_verifier(Arc::new(Pinned(bytes))).with_no_client_auth()
        }
        _ => {
            let mut roots = rustls::RootCertStore::empty();
            roots.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
            builder.with_root_certificates(roots).with_no_client_auth()
        }
    };
    Ok(Arc::new(cfg))
}

fn wrap_tls(acc: &Account, tcp: TcpStream) -> Result<Stream> {
    let name = ServerName::try_from(acc.host.clone()).map_err(|_| PluginError::invalid("host", "not a valid server name"))?;
    let conn = rustls::ClientConnection::new(tls_config(acc)?, name).map_err(|e| PluginError::network(e.to_string()))?;
    Ok(Stream::Tls(Box::new(rustls::StreamOwned::new(conn, tcp))))
}

struct Smtp {
    r: BufReader<Stream>,
}

impl Smtp {
    /// Reads one reply (all continuation lines); returns (code, lines).
    fn reply(&mut self) -> Result<(u16, Vec<String>)> {
        let mut lines = Vec::new();
        loop {
            let mut line = String::new();
            let n = self.r.read_line(&mut line).map_err(|e| PluginError::network(e.to_string()))?;
            if n == 0 {
                return Err(PluginError::network("server closed the connection"));
            }
            let line = line.trim_end_matches(['\r', '\n']).to_string();
            if line.len() < 4 {
                return Err(PluginError::network(format!("bad reply: {line}")));
            }
            let code: u16 = line[..3].parse().map_err(|_| PluginError::network(format!("bad reply: {line}")))?;
            let last = &line[3..4] == " ";
            lines.push(line[4..].to_string());
            if last {
                return Ok((code, lines));
            }
        }
    }

    fn cmd(&mut self, c: &str, expect: &[u16]) -> Result<(u16, Vec<String>)> {
        let s = self.r.get_mut();
        s.write_all(c.as_bytes()).and_then(|_| s.write_all(b"\r\n")).and_then(|_| s.flush()).map_err(|e| PluginError::network(e.to_string()))?;
        let (code, lines) = self.reply()?;
        if !expect.contains(&code) {
            let what = c.split_whitespace().next().unwrap_or(c);
            let msg = lines.join(" ");
            return Err(match code {
                535 | 534 | 530 => PluginError::auth(format!("{what}: {code} {msg}")),
                550..=554 if what == "RCPT" => PluginError::invalid("to", format!("{code} {msg}")),
                _ => PluginError::network(format!("{what}: {code} {msg}")),
            });
        }
        Ok((code, lines))
    }
}

pub fn send(acc: &Account, msg: &Message, mut progress: impl FnMut(&str)) -> Result<()> {
    if msg.to.is_empty() {
        return Err(PluginError::invalid("to", "at least one recipient"));
    }
    progress("connecting");
    let addr = format!("{}:{}", acc.host, acc.port);
    let tcp = TcpStream::connect(&addr).map_err(|e| PluginError::network(format!("{addr}: {e}")))?;
    let _ = tcp.set_read_timeout(Some(Duration::from_secs(60)));
    let _ = tcp.set_write_timeout(Some(Duration::from_secs(60)));
    let stream = if acc.security == Security::Tls { wrap_tls(acc, tcp)? } else { Stream::Plain(tcp) };
    let mut s = Smtp { r: BufReader::new(stream) };
    let (code, _) = s.reply()?;
    if code != 220 {
        return Err(PluginError::network(format!("greeting {code}")));
    }
    let (_, ehlo) = s.cmd("EHLO kiki", &[250])?;
    let mut caps: Vec<String> = ehlo.iter().skip(1).map(|l| l.to_ascii_uppercase()).collect();
    if acc.security == Security::StartTls {
        if !caps.iter().any(|c| c.starts_with("STARTTLS")) {
            return Err(PluginError::network("server does not offer STARTTLS"));
        }
        s.cmd("STARTTLS", &[220])?;
        let plain = match s.r.into_inner() {
            Stream::Plain(t) => t,
            Stream::Tls(_) => unreachable!(),
        };
        s = Smtp { r: BufReader::new(wrap_tls(acc, plain)?) };
        let (_, ehlo) = s.cmd("EHLO kiki", &[250])?;
        caps = ehlo.iter().skip(1).map(|l| l.to_ascii_uppercase()).collect();
    }
    if !acc.username.is_empty() {
        let auth = caps.iter().find(|c| c.starts_with("AUTH")).cloned().unwrap_or_default();
        if auth.contains("PLAIN") || auth.is_empty() {
            let token = base64(format!("\0{}\0{}", acc.username, acc.password).as_bytes());
            s.cmd(&format!("AUTH PLAIN {token}"), &[235])?;
        } else if auth.contains("LOGIN") {
            s.cmd("AUTH LOGIN", &[334])?;
            s.cmd(&base64(acc.username.as_bytes()), &[334])?;
            s.cmd(&base64(acc.password.as_bytes()), &[235])?;
        } else {
            return Err(PluginError::auth(format!("no supported auth mechanism in {auth}")));
        }
    }
    progress("sending");
    s.cmd(&format!("MAIL FROM:<{}>", angle(&acc.from)), &[250])?;
    for to in &msg.to {
        s.cmd(&format!("RCPT TO:<{}>", angle(to)), &[250, 251])?;
    }
    s.cmd("DATA", &[354])?;
    let data = build(acc, msg);
    s.cmd(&data, &[250])?;
    let _ = s.cmd("QUIT", &[221]);
    Ok(())
}

fn angle(a: &str) -> String {
    // "Name <addr>" -> addr
    match (a.find('<'), a.find('>')) {
        (Some(i), Some(j)) if j > i => a[i + 1..j].to_string(),
        _ => a.trim().to_string(),
    }
}

/// The RFC 5322 message with the terminating "." (dot-stuffed), without the final CRLF.
pub fn build(acc: &Account, msg: &Message) -> String {
    let boundary = format!("=_kiki_{}_{}", std::process::id(), std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0));
    let mut m = String::new();
    m.push_str(&format!("From: {}\r\n", acc.from));
    m.push_str(&format!("To: {}\r\n", msg.to.join(", ")));
    m.push_str(&format!("Subject: {}\r\n", encode_header(msg.subject)));
    m.push_str(&format!("Date: {}\r\n", rfc5322_date(std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0))));
    m.push_str("MIME-Version: 1.0\r\nX-Mailer: kiki\r\n");
    m.push_str(&format!("Content-Type: multipart/mixed; boundary=\"{boundary}\"\r\n\r\n"));
    m.push_str(&format!("--{boundary}\r\nContent-Type: text/plain; charset=utf-8\r\nContent-Transfer-Encoding: base64\r\n\r\n"));
    m.push_str(&base64_lines(msg.body.as_bytes()));
    for (name, bytes) in &msg.attachments {
        m.push_str(&format!(
            "--{boundary}\r\nContent-Type: {}; name=\"{}\"\r\nContent-Transfer-Encoding: base64\r\nContent-Disposition: attachment; filename=\"{}\"; filename*=UTF-8''{}\r\n\r\n",
            mime_for(name),
            quote(name),
            quote(name),
            percent(name)
        ));
        m.push_str(&base64_lines(bytes));
    }
    m.push_str(&format!("--{boundary}--\r\n"));
    // dot-stuffing and the terminator (m ends with CRLF, so the join ends with CRLF too)
    let stuffed: Vec<String> = m.split("\r\n").map(|l| if l.starts_with('.') { format!(".{l}") } else { l.to_string() }).collect();
    stuffed.join("\r\n") + "."
}

fn quote(s: &str) -> String {
    s.replace('\\', "\\\\").replace('"', "\\\"").replace(['\r', '\n'], " ")
}

fn percent(s: &str) -> String {
    let mut o = String::new();
    for b in s.bytes() {
        if b.is_ascii_alphanumeric() || b"-._~".contains(&b) {
            o.push(b as char);
        } else {
            o.push_str(&format!("%{b:02X}"));
        }
    }
    o
}

fn encode_header(s: &str) -> String {
    if s.is_ascii() && !s.contains(['\r', '\n']) {
        s.to_string()
    } else {
        format!("=?UTF-8?B?{}?=", base64(s.as_bytes()))
    }
}

fn mime_for(name: &str) -> &'static str {
    match name.rsplit('.').next().unwrap_or("").to_ascii_lowercase().as_str() {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "pdf" => "application/pdf",
        "txt" | "md" => "text/plain",
        "zip" => "application/zip",
        "mp4" => "video/mp4",
        _ => "application/octet-stream",
    }
}

pub fn base64(input: &[u8]) -> String {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(input.len().div_ceil(3) * 4);
    for chunk in input.chunks(3) {
        let n = chunk.len();
        let b = [chunk[0], if n > 1 { chunk[1] } else { 0 }, if n > 2 { chunk[2] } else { 0 }];
        let v = ((b[0] as u32) << 16) | ((b[1] as u32) << 8) | b[2] as u32;
        out.push(T[(v >> 18) as usize & 63] as char);
        out.push(T[(v >> 12) as usize & 63] as char);
        out.push(if n > 1 { T[(v >> 6) as usize & 63] as char } else { '=' });
        out.push(if n > 2 { T[v as usize & 63] as char } else { '=' });
    }
    out
}

fn base64_lines(input: &[u8]) -> String {
    let s = base64(input);
    let mut out = String::with_capacity(s.len() + s.len() / 38 + 2);
    for chunk in s.as_bytes().chunks(76) {
        out.push_str(std::str::from_utf8(chunk).unwrap());
        out.push_str("\r\n");
    }
    out
}

pub fn rfc5322_date(secs: u64) -> String {
    let days = (secs / 86400) as i64;
    let (h, mi, s) = ((secs % 86400) / 3600, (secs % 3600) / 60, secs % 60);
    // civil from days (Howard Hinnant)
    let z = days + 719468;
    let era = z.div_euclid(146097);
    let doe = z.rem_euclid(146097);
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    let wd = ["Thu", "Fri", "Sat", "Sun", "Mon", "Tue", "Wed"][(days.rem_euclid(7)) as usize];
    let mon = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"][(m - 1) as usize];
    format!("{wd}, {d:02} {mon} {y} {h:02}:{mi:02}:{s:02} +0000")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base64_and_date() {
        assert_eq!(base64(b""), "");
        assert_eq!(base64(b"f"), "Zg==");
        assert_eq!(base64(b"fo"), "Zm8=");
        assert_eq!(base64(b"foobar"), "Zm9vYmFy");
        assert_eq!(rfc5322_date(0), "Thu, 01 Jan 1970 00:00:00 +0000");
        assert_eq!(rfc5322_date(1_700_000_000), "Tue, 14 Nov 2023 22:13:20 +0000");
    }

    #[test]
    fn message_is_dot_stuffed_and_multipart() {
        let acc = Account { host: "h".into(), port: 25, security: Security::None, username: String::new(), password: String::new(), from: "kiki@example.com".into(), trust_fingerprint: None };
        let m = Message { to: vec!["a@example.com".into()], subject: "Grüße", body: "hi", attachments: vec![("my file.txt".into(), b"hello".to_vec())] };
        let s = build(&acc, &m);
        assert!(s.starts_with("From: kiki@example.com\r\nTo: a@example.com\r\nSubject: =?UTF-8?B?R3LDvMOfZQ==?=\r\n"));
        assert!(s.contains("Content-Disposition: attachment; filename=\"my file.txt\"; filename*=UTF-8''my%20file.txt\r\n"));
        assert!(s.contains("\r\naGVsbG8=\r\n"));
        assert!(s.ends_with("--\r\n."));
    }
}
