//! Drives the mail plugin binary in SMTP mode against an in-process mock SMTP server: implicit
//! TLS with a pinned self-signed certificate, STARTTLS, AUTH PLAIN, and the message's MIME.

use kiki_plugin_sdk::json::{self, Value};
use kiki_plugin_sdk::{read_frame, write_json};
use rustls::pki_types::pem::PemObject;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpListener;
use std::process::{Command, Stdio};
use std::sync::{mpsc, Arc};

const CERT: &str = "-----BEGIN CERTIFICATE-----
MIIBPTCB5aADAgECAgkAhyCaPlZ0xRswCgYIKoZIzj0EAwIwFDESMBAGA1UEAwwJ
bG9jYWxob3N0MCAXDTI2MDkxNjIxMDc0OVoYDzIxMjYwODIzMjEwNzQ5WjAUMRIw
EAYDVQQDDAlsb2NhbGhvc3QwWTATBgcqhkjOPQIBBggqhkjOPQMBBwNCAASdZj15
XyAHlIQ5MgtJQlMTGE5Zg18uyiKGKIqGhE2Z2IuZXa+zYAaz+V4IHRUAlhv1Md3O
5V05TbK/52+aBrlzox4wHDAaBgNVHREEEzARgglsb2NhbGhvc3SHBH8AAAEwCgYI
KoZIzj0EAwIDRwAwRAIgLwyTbt8bDMz8/jmXzjYhzoBWJdh6+5eS8fR45RxjSuMC
IH0b2GhmXYqI5K0Ep/JSvAs+Ot0GpjTrOJZrAu/Ifep9
-----END CERTIFICATE-----
";
const KEY: &str = "-----BEGIN PRIVATE KEY-----
MIGHAgEAMBMGByqGSM49AgEGCCqGSM49AwEHBG0wawIBAQQgPMSKsLBR5ou4gKhg
LuzntQXvW1+B+DPbNpPuMTiNvhmhRANCAASdZj15XyAHlIQ5MgtJQlMTGE5Zg18u
yiKGKIqGhE2Z2IuZXa+zYAaz+V4IHRUAlhv1Md3O5V05TbK/52+aBrlz
-----END PRIVATE KEY-----
";
const FINGERPRINT: &str = "68615b72a3cfcba8dbddb66fb0ca0d99c2d9172e5715c4b50f70afef5d3e8e44";

enum Stream {
    Plain(std::net::TcpStream),
    Tls(Box<rustls::StreamOwned<rustls::ServerConnection, std::net::TcpStream>>),
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

struct Received {
    from: String,
    to: Vec<String>,
    data: String,
    auth: Option<String>,
    starttls: bool,
}

fn server_config() -> Arc<rustls::ServerConfig> {
    let cert = rustls::pki_types::CertificateDer::from_pem_slice(CERT.as_bytes()).unwrap();
    let key = rustls::pki_types::PrivateKeyDer::from_pem_slice(KEY.as_bytes()).unwrap();
    Arc::new(
        rustls::ServerConfig::builder_with_provider(Arc::new(rustls::crypto::ring::default_provider()))
            .with_safe_default_protocol_versions()
            .unwrap()
            .with_no_client_auth()
            .with_single_cert(vec![cert], key)
            .unwrap(),
    )
}

/// One-shot mock SMTP server; `implicit` = TLS from the first byte, else plaintext offering STARTTLS.
fn start(implicit: bool) -> (u16, mpsc::Receiver<Received>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let (tcp, _) = listener.accept().unwrap();
        let cfg = server_config();
        let mut s = if implicit { Stream::Tls(Box::new(rustls::StreamOwned::new(rustls::ServerConnection::new(cfg.clone()).unwrap(), tcp))) } else { Stream::Plain(tcp) };
        let mut r = Received { from: String::new(), to: vec![], data: String::new(), auth: None, starttls: false };
        s.write_all(b"220 mock ESMTP\r\n").unwrap();
        // One reader for the whole session: a TLS record often carries several lines at once.
        let mut br = BufReader::new(s);
        loop {
            let mut line = String::new();
            if br.read_line(&mut line).unwrap() == 0 {
                break;
            }
            let line = line.trim_end().to_string();
            let up = line.to_ascii_uppercase();
            let s = br.get_mut();
            if up.starts_with("EHLO") {
                let tls_line = if !implicit && !r.starttls { "250-STARTTLS\r\n" } else { "" };
                s.write_all(format!("250-mock\r\n{tls_line}250-AUTH PLAIN LOGIN\r\n250 OK\r\n").as_bytes()).unwrap();
            } else if up == "STARTTLS" {
                s.write_all(b"220 go ahead\r\n").unwrap();
                let plain = br.into_inner();
                let upgraded = match plain {
                    Stream::Plain(t) => Stream::Tls(Box::new(rustls::StreamOwned::new(rustls::ServerConnection::new(cfg.clone()).unwrap(), t))),
                    other => other,
                };
                br = BufReader::new(upgraded);
                r.starttls = true;
            } else if up.starts_with("AUTH PLAIN ") {
                r.auth = Some(line[11..].to_string());
                let ok = line[11..] == *"AGtpa2kAc2VjcmV0"; // base64("\0kiki\0secret")
                s.write_all(if ok { b"235 ok\r\n" } else { b"535 bad credentials\r\n" }).unwrap();
            } else if up.starts_with("MAIL FROM:") {
                r.from = line[10..].to_string();
                s.write_all(b"250 ok\r\n").unwrap();
            } else if up.starts_with("RCPT TO:") {
                r.to.push(line[8..].to_string());
                s.write_all(b"250 ok\r\n").unwrap();
            } else if up == "DATA" {
                s.write_all(b"354 end with .\r\n").unwrap();
                let mut data = Vec::new();
                loop {
                    let mut l = String::new();
                    if br.read_line(&mut l).unwrap() == 0 {
                        break;
                    }
                    if l == ".\r\n" {
                        break;
                    }
                    data.push(l);
                }
                r.data = data.concat();
                br.get_mut().write_all(b"250 queued\r\n").unwrap();
            } else if up == "QUIT" {
                s.write_all(b"221 bye\r\n").unwrap();
                break;
            } else {
                s.write_all(b"502 nope\r\n").unwrap();
            }
        }
        let _ = tx.send(r);
    });
    (port, rx)
}

fn share(port: u16, security: &str, fingerprint: &str, password: &str) -> Value {
    let dir = std::env::temp_dir().join(format!("kiki-mail-{}-{port}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let f = dir.join("my report.txt");
    std::fs::write(&f, b"line one\n.dot line\n").unwrap();
    let mut child = Command::new(env!("CARGO_BIN_EXE_kiki-plugin-share-mail")).stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::inherit()).spawn().unwrap();
    let mut stdin = child.stdin.take().unwrap();
    let mut stdout = BufReader::new(child.stdout.take().unwrap());
    let config = Value::obj().s("mode", "smtp").s("host", "localhost").s("port", port.to_string()).s("security", security).s("username", "kiki").s("from", "kiki@example.com").s("trustFingerprint", fingerprint).done();
    let req = Value::obj()
        .u("id", 1)
        .s("type", "Share")
        .v("config", config)
        .v("secrets", Value::obj().s("password", password).done())
        .v("uris", Value::Arr(vec![Value::Str(format!("file://{}", f.to_string_lossy().replace(' ', "%20")))]))
        .v("compose", Value::obj().s("to", "a@example.com, b@example.com").s("subject", "Report").s("body", "see attached").done())
        .done();
    write_json(&mut stdin, &req).unwrap();
    let mut reply = Value::Null;
    while let Some((_, payload)) = read_frame(&mut stdout).unwrap() {
        let v = json::parse(&payload).unwrap();
        if v.get("ok").is_some() || v.get("err").is_some() {
            reply = v;
            break;
        }
    }
    let _ = write_json(&mut stdin, &Value::obj().u("id", 2).s("type", "Shutdown").done());
    let _ = child.wait();
    let _ = std::fs::remove_dir_all(&dir);
    reply
}

#[test]
fn implicit_tls_with_pinned_certificate_sends_a_multipart_message() {
    let (port, rx) = start(true);
    let r = share(port, "tls", FINGERPRINT, "secret");
    assert_eq!(r.get("ok").map(|o| o.str_field("result")), Some(Some("sent")), "{}", json::to_string(&r));
    let m = rx.recv().unwrap();
    assert_eq!(m.from, "<kiki@example.com>");
    assert_eq!(m.to, vec!["<a@example.com>", "<b@example.com>"]);
    assert_eq!(m.auth.as_deref(), Some("AGtpa2kAc2VjcmV0"));
    assert!(m.data.contains("Subject: Report\r\n"));
    assert!(m.data.contains("Content-Type: multipart/mixed; boundary="));
    assert!(m.data.contains("filename=\"my report.txt\""), "filename with a space intact");
    // attachment bytes round-trip through base64: "line one\n.dot line\n"
    assert!(m.data.contains("bGluZSBvbmUKLmRvdCBsaW5lCg=="), "{}", m.data);
    assert!(!m.starttls);
}

#[test]
fn starttls_upgrade_and_wrong_password() {
    let (port, rx) = start(false);
    let r = share(port, "starttls", FINGERPRINT, "secret");
    assert_eq!(r.get("ok").map(|o| o.str_field("result")), Some(Some("sent")), "{}", json::to_string(&r));
    let m = rx.recv().unwrap();
    assert!(m.starttls, "session upgraded with STARTTLS before AUTH");
    assert!(m.auth.is_some());

    let (port, _rx) = start(false);
    let r = share(port, "starttls", FINGERPRINT, "wrong");
    assert_eq!(r.get("err").map(|e| e.str_field("code")), Some(Some("Auth")), "{}", json::to_string(&r));
}

#[test]
fn unpinned_self_signed_certificate_is_refused() {
    let (port, _rx) = start(true);
    let r = share(port, "tls", "", "secret");
    let e = r.get("err").expect("error");
    assert_eq!(e.str_field("code"), Some("Network"), "{}", json::to_string(&r));
    let (port, _rx) = start(true);
    let r = share(port, "tls", "00".repeat(32).as_str(), "secret");
    let msg = r.get("err").unwrap().str_field("message").unwrap_or("").to_string();
    assert!(msg.contains(FINGERPRINT), "the real fingerprint is reported so the user can pin it: {msg}");
}
