//! A minimal HTTP/1.1 client for the LocalSend protocol: one request per connection, plain or
//! TLS. LocalSend receivers use self-signed certificates identified by fingerprint: the TLS
//! client accepts exactly the certificate the device announced (see `Pinned`).

use kiki_plugin_sdk::{PluginError, Result};
use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpStream;
use std::sync::Arc;
use std::time::Duration;

/// SHA-256 of a certificate, which is what a LocalSend device announces as its `fingerprint`.
pub type Fingerprint = [u8; 32];

/// 64 hex digits, either case; anything else is not a fingerprint.
pub fn parse_fingerprint(hex: &str) -> Option<Fingerprint> {
    let h = hex.trim().as_bytes();
    if h.len() != 64 {
        return None;
    }
    let mut out = [0u8; 32];
    for (i, pair) in h.chunks(2).enumerate() {
        out[i] = u8::from_str_radix(std::str::from_utf8(pair).ok()?, 16).ok()?;
    }
    Some(out)
}

/// LocalSend has no CA: a device IS its certificate, named by that certificate's SHA-256, which
/// it announces when it is discovered. With a fingerprint to hold it to, the certificate the
/// server presents must be that one — or whoever answered is not the device that was picked.
/// Without one (an address typed by hand, which announced nothing) any certificate is accepted:
/// there is nothing to compare it with, and the user named the address themselves.
#[derive(Debug)]
struct Pinned(Option<Fingerprint>);

impl rustls::client::danger::ServerCertVerifier for Pinned {
    fn verify_server_cert(
        &self,
        cert: &rustls::pki_types::CertificateDer<'_>,
        _: &[rustls::pki_types::CertificateDer<'_>],
        _: &rustls::pki_types::ServerName<'_>,
        _: &[u8],
        _: rustls::pki_types::UnixTime,
    ) -> std::result::Result<rustls::client::danger::ServerCertVerified, rustls::Error> {
        if let Some(want) = &self.0 {
            let got = ring::digest::digest(&ring::digest::SHA256, cert.as_ref());
            // Both sides are public (a hash of a public certificate): nothing to time.
            if got.as_ref() != want {
                return Err(rustls::Error::General(NOT_THE_DEVICE.into()));
            }
        }
        Ok(rustls::client::danger::ServerCertVerified::assertion())
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

pub struct Response {
    pub status: u16,
    pub body: Vec<u8>,
}

pub const NOT_THE_DEVICE: &str = "the device that answered is not the one that announced itself (its certificate does not match)";

/// `quick` is for asking an address whether a LocalSend device is there at all: it gives up on
/// the connection, and on a silent peer, in that long. A send waits as long as a person takes to
/// press Accept.
fn connect(host: &str, port: u16, https: bool, pin: Option<&Fingerprint>, quick: Option<Duration>) -> Result<Stream> {
    let unreachable = |e: std::io::Error| PluginError::network(format!("{host}:{port}: {e}"));
    let tcp = match quick {
        None => TcpStream::connect((host, port)).map_err(unreachable)?,
        Some(t) => {
            use std::net::ToSocketAddrs;
            let addr = (host, port).to_socket_addrs().map_err(unreachable)?.next().ok_or_else(|| PluginError::network(format!("{host}: no address")))?;
            TcpStream::connect_timeout(&addr, t).map_err(unreachable)?
        }
    };
    let wait = quick.unwrap_or(Duration::from_secs(120));
    let _ = tcp.set_read_timeout(Some(wait));
    let _ = tcp.set_write_timeout(Some(wait));
    if !https {
        return Ok(Stream::Plain(tcp));
    }
    let me = crate::identity::get()?;
    let cfg = rustls::ClientConfig::builder_with_provider(Arc::new(rustls::crypto::ring::default_provider()))
        .with_safe_default_protocol_versions()
        .map_err(|e| PluginError::network(e.to_string()))?
        .dangerous()
        .with_custom_certificate_verifier(Arc::new(Pinned(pin.copied())))
        // Receivers ask for the sender's certificate and hang up without one.
        .with_client_auth_cert(
            vec![rustls::pki_types::CertificateDer::from(me.cert_der.clone())],
            rustls::pki_types::PrivateKeyDer::Pkcs8(rustls::pki_types::PrivatePkcs8KeyDer::from(me.key_der.clone())),
        )
        .map_err(|e| PluginError::network(format!("this machine's LocalSend certificate: {e}")))?;
    let name = rustls::pki_types::ServerName::try_from(host.to_string()).map_err(|_| PluginError::network("bad host"))?;
    let conn = rustls::ClientConnection::new(Arc::new(cfg), name).map_err(|e| PluginError::network(e.to_string()))?;
    Ok(Stream::Tls(Box::new(rustls::StreamOwned::new(conn, tcp))))
}

fn handshake_error(e: std::io::Error) -> PluginError {
    let text = e.to_string();
    if text.contains(NOT_THE_DEVICE) {
        PluginError::new("Auth", NOT_THE_DEVICE)
    } else if text.to_ascii_lowercase().contains("certificaterequired") || text.to_ascii_lowercase().contains("certificate required") {
        PluginError::network("the receiver would not accept this machine's certificate")
    } else {
        PluginError::io(text)
    }
}

/// POST with a body from a reader of known length; `on_sent` gets each chunk's size.
#[allow(clippy::too_many_arguments)]
pub fn post(host: &str, port: u16, https: bool, pin: Option<&Fingerprint>, path: &str, content_type: &str, body: &mut dyn Read, len: u64, on_sent: impl FnMut(u64)) -> Result<Response> {
    post_within(None, host, port, https, pin, path, content_type, body, len, on_sent)
}

/// A small JSON POST that gives up quickly: discovery's question to one address.
pub fn ask(host: &str, port: u16, https: bool, path: &str, json: &str, within: Duration) -> Result<Response> {
    post_within(Some(within), host, port, https, None, path, "application/json", &mut json.as_bytes(), json.len() as u64, |_| {})
}

#[allow(clippy::too_many_arguments)]
fn post_within(quick: Option<Duration>, host: &str, port: u16, https: bool, pin: Option<&Fingerprint>, path: &str, content_type: &str, body: &mut dyn Read, len: u64, mut on_sent: impl FnMut(u64)) -> Result<Response> {
    let mut s = connect(host, port, https, pin, quick)?;
    let head = format!("POST {path} HTTP/1.1\r\nHost: {host}:{port}\r\nUser-Agent: kiki\r\nAccept: */*\r\nConnection: close\r\nContent-Type: {content_type}\r\nContent-Length: {len}\r\n\r\n");
    // The handshake happens on the first write, so this is where a refused certificate — theirs
    // or ours — comes back. Say which, in words, rather than rustls's.
    s.write_all(head.as_bytes()).map_err(handshake_error)?;
    let mut buf = vec![0u8; 256 * 1024];
    let mut sent = 0u64;
    while sent < len {
        if kiki_plugin_sdk::cancelled() {
            return Err(kiki_plugin_sdk::cancel_error());
        }
        let n = body.read(&mut buf).map_err(PluginError::io)?;
        if n == 0 {
            break;
        }
        s.write_all(&buf[..n]).map_err(PluginError::io)?;
        sent += n as u64;
        on_sent(n as u64);
    }
    s.flush().map_err(PluginError::io)?;
    read_response(s)
}

fn read_response(s: Stream) -> Result<Response> {
    let mut r = BufReader::new(s);
    let mut line = String::new();
    r.read_line(&mut line).map_err(PluginError::io)?;
    let status: u16 = line.split_whitespace().nth(1).and_then(|c| c.parse().ok()).ok_or_else(|| PluginError::network(format!("bad status line: {line:?}")))?;
    let mut content_length: Option<usize> = None;
    let mut chunked = false;
    loop {
        line.clear();
        if r.read_line(&mut line).map_err(PluginError::io)? == 0 {
            break;
        }
        let l = line.trim_end();
        if l.is_empty() {
            break;
        }
        if let Some((k, v)) = l.split_once(':') {
            match k.trim().to_ascii_lowercase().as_str() {
                "content-length" => content_length = v.trim().parse().ok(),
                "transfer-encoding" if v.to_ascii_lowercase().contains("chunked") => chunked = true,
                _ => {}
            }
        }
    }
    let mut body = Vec::new();
    if chunked {
        loop {
            line.clear();
            r.read_line(&mut line).map_err(PluginError::io)?;
            let n = usize::from_str_radix(line.trim().split(';').next().unwrap_or("0"), 16).unwrap_or(0);
            if n == 0 {
                break;
            }
            let mut chunk = vec![0u8; n];
            r.read_exact(&mut chunk).map_err(PluginError::io)?;
            body.extend_from_slice(&chunk);
            let mut crlf = [0u8; 2];
            let _ = r.read_exact(&mut crlf);
        }
    } else if let Some(n) = content_length {
        body.resize(n, 0);
        r.read_exact(&mut body).map_err(PluginError::io)?;
    } else {
        let _ = r.read_to_end(&mut body);
    }
    Ok(Response { status, body })
}
