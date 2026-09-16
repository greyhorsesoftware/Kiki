//! A minimal HTTP/1.1 client for the LocalSend protocol: one request per connection, plain or
//! TLS. LocalSend receivers use self-signed certificates identified by fingerprint, so the TLS
//! client accepts any certificate (the protocol's own trust model).

use kiki_plugin_sdk::{PluginError, Result};
use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpStream;
use std::sync::Arc;
use std::time::Duration;

#[derive(Debug)]
struct AcceptAll;

impl rustls::client::danger::ServerCertVerifier for AcceptAll {
    fn verify_server_cert(
        &self,
        _: &rustls::pki_types::CertificateDer<'_>,
        _: &[rustls::pki_types::CertificateDer<'_>],
        _: &rustls::pki_types::ServerName<'_>,
        _: &[u8],
        _: rustls::pki_types::UnixTime,
    ) -> std::result::Result<rustls::client::danger::ServerCertVerified, rustls::Error> {
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

fn connect(host: &str, port: u16, https: bool) -> Result<Stream> {
    let tcp = TcpStream::connect((host, port)).map_err(|e| PluginError::network(format!("{host}:{port}: {e}")))?;
    let _ = tcp.set_read_timeout(Some(Duration::from_secs(120)));
    let _ = tcp.set_write_timeout(Some(Duration::from_secs(120)));
    if !https {
        return Ok(Stream::Plain(tcp));
    }
    let cfg = rustls::ClientConfig::builder_with_provider(Arc::new(rustls::crypto::ring::default_provider()))
        .with_safe_default_protocol_versions()
        .map_err(|e| PluginError::network(e.to_string()))?
        .dangerous()
        .with_custom_certificate_verifier(Arc::new(AcceptAll))
        .with_no_client_auth();
    let name = rustls::pki_types::ServerName::try_from(host.to_string()).map_err(|_| PluginError::network("bad host"))?;
    let conn = rustls::ClientConnection::new(Arc::new(cfg), name).map_err(|e| PluginError::network(e.to_string()))?;
    Ok(Stream::Tls(Box::new(rustls::StreamOwned::new(conn, tcp))))
}

/// POST with a body from a reader of known length; `on_sent` gets each chunk's size.
#[allow(clippy::too_many_arguments)]
pub fn post(host: &str, port: u16, https: bool, path: &str, content_type: &str, body: &mut dyn Read, len: u64, mut on_sent: impl FnMut(u64)) -> Result<Response> {
    let mut s = connect(host, port, https)?;
    let head = format!("POST {path} HTTP/1.1\r\nHost: {host}:{port}\r\nUser-Agent: kiki\r\nAccept: */*\r\nConnection: close\r\nContent-Type: {content_type}\r\nContent-Length: {len}\r\n\r\n");
    s.write_all(head.as_bytes()).map_err(PluginError::io)?;
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
