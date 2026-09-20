//! The LocalSend plugin over HTTPS, against an in-process TLS receiver: it sends only to the
//! device whose certificate matches the fingerprint that device announced. LocalSend has no CA —
//! a device is its certificate — so without this check the files go to whoever answers first.

use kiki_plugin_sdk::json::{self, Value};
use kiki_plugin_sdk::{read_frame, write_json};
use rustls::pki_types::pem::PemObject;
use rustls::pki_types::{CertificateDer, PrivateKeyDer};
use std::io::{BufReader, Read, Write};
use std::net::TcpListener;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

// Self-signed P-256 certificate for CN=localhost, the one the FTPS tests use.
const CERT_PEM: &str = "-----BEGIN CERTIFICATE-----
MIIBSDCB8KADAgECAgkA/9do5M9pgogwCgYIKoZIzj0EAwIwFDESMBAGA1UEAwwJ
bG9jYWxob3N0MCAXDTI2MDkxNjIxMTMwOVoYDzIxMjYwODIzMjExMzA5WjAUMRIw
EAYDVQQDDAlsb2NhbGhvc3QwWTATBgcqhkjOPQIBBggqhkjOPQMBBwNCAASdZj15
XyAHlIQ5MgtJQlMTGE5Zg18uyiKGKIqGhE2Z2IuZXa+zYAaz+V4IHRUAlhv1Md3O
5V05TbK/52+aBrlzoykwJzAaBgNVHREEEzARgglsb2NhbGhvc3SHBH8AAAEwCQYD
VR0TBAIwADAKBggqhkjOPQQDAgNHADBEAiAYlwRbE3j2y4ArTbjcDybVSVWNJ6w9
4NIS67AFa1zPdQIgHxVVgrspgwvb39diXudmfWw3Wh6tfCwIHPN4Ko6LNE8=
-----END CERTIFICATE-----
";
const KEY_PEM: &str = "-----BEGIN PRIVATE KEY-----
MIGHAgEAMBMGByqGSM49AgEGCCqGSM49AwEHBG0wawIBAQQgPMSKsLBR5ou4gKhg
LuzntQXvW1+B+DPbNpPuMTiNvhmhRANCAASdZj15XyAHlIQ5MgtJQlMTGE5Zg18u
yiKGKIqGhE2Z2IuZXa+zYAaz+V4IHRUAlhv1Md3O5V05TbK/52+aBrlz
-----END PRIVATE KEY-----
";

/// What the device would announce: SHA-256 of its DER certificate, in hex.
fn fingerprint() -> String {
    let der = CertificateDer::from_pem_slice(CERT_PEM.as_bytes()).unwrap();
    ring::digest::digest(&ring::digest::SHA256, der.as_ref()).as_ref().iter().map(|b| format!("{b:02x}")).collect()
}

#[derive(Debug)]
struct AnyClientCert;

impl rustls::server::danger::ClientCertVerifier for AnyClientCert {
    fn root_hint_subjects(&self) -> &[rustls::DistinguishedName] {
        &[]
    }
    fn verify_client_cert(&self, _: &CertificateDer<'_>, _: &[CertificateDer<'_>], _: rustls::pki_types::UnixTime) -> Result<rustls::server::danger::ClientCertVerified, rustls::Error> {
        Ok(rustls::server::danger::ClientCertVerified::assertion())
    }
    fn verify_tls12_signature(&self, m: &[u8], c: &CertificateDer<'_>, d: &rustls::DigitallySignedStruct) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls12_signature(m, c, d, &rustls::crypto::ring::default_provider().signature_verification_algorithms)
    }
    fn verify_tls13_signature(&self, m: &[u8], c: &CertificateDer<'_>, d: &rustls::DigitallySignedStruct) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls13_signature(m, c, d, &rustls::crypto::ring::default_provider().signature_verification_algorithms)
    }
    fn supported_verify_schemes(&self) -> Vec<rustls::SignatureScheme> {
        rustls::crypto::ring::default_provider().signature_verification_algorithms.supported_schemes()
    }
}

/// A TLS receiver that answers every request with 204 ("accepted nothing"). Returns its port and
/// the number of bytes of HTTP it was ever sent — the thing that must stay 0 for an impostor.
fn receiver() -> (u16, Arc<AtomicUsize>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let seen = Arc::new(AtomicUsize::new(0));
    let seen_t = Arc::clone(&seen);
    let cert = CertificateDer::from_pem_slice(CERT_PEM.as_bytes()).unwrap();
    let key = PrivateKeyDer::from_pem_slice(KEY_PEM.as_bytes()).unwrap();
    let cfg = Arc::new(
        rustls::ServerConfig::builder_with_provider(Arc::new(rustls::crypto::ring::default_provider()))
            .with_safe_default_protocol_versions()
            .unwrap()
            // What the real app does: the sender must present a certificate (any — there is no
            // CA), and a connection without one ends with "certificate required". The plugin
            // sent none until 2026-09-19, so no HTTPS send to a real LocalSend ever worked, and
            // nothing here noticed: the receivers in these tests did not ask.
            .with_client_cert_verifier(Arc::new(AnyClientCert))
            .with_single_cert(vec![cert], key)
            .unwrap(),
    );
    std::thread::spawn(move || {
        for tcp in listener.incoming().flatten() {
            let _ = tcp.set_read_timeout(Some(std::time::Duration::from_secs(5)));
            let conn = rustls::ServerConnection::new(Arc::clone(&cfg)).unwrap();
            let mut tls = rustls::StreamOwned::new(conn, tcp);
            let mut buf = [0u8; 8192];
            let mut head = Vec::new();
            // A refused certificate ends the handshake: read() errors and nothing was seen.
            while !head.windows(4).any(|w| w == b"\r\n\r\n") {
                match tls.read(&mut buf) {
                    Ok(n) if n > 0 => {
                        seen_t.fetch_add(n, Ordering::SeqCst);
                        head.extend_from_slice(&buf[..n]);
                    }
                    _ => break,
                }
            }
            if !head.is_empty() {
                let _ = tls.write_all(b"HTTP/1.1 204 No Content\r\nContent-Length: 0\r\nConnection: close\r\n\r\n");
                let _ = tls.flush();
            }
        }
    });
    (port, seen)
}

fn share(target: &str) -> Value {
    let dir = std::env::temp_dir().join(format!("kiki-localsend-tls-{}-{}", std::process::id(), target.len()));
    std::fs::create_dir_all(&dir).unwrap();
    let f = dir.join("secret.txt");
    std::fs::write(&f, b"for the right device only").unwrap();
    let mut child = Command::new(env!("CARGO_BIN_EXE_kiki-plugin-share-localsend")).env("XDG_CACHE_HOME", dir.join("cache")).stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::inherit()).spawn().unwrap();
    let mut stdin = child.stdin.take().unwrap();
    let mut stdout = BufReader::new(child.stdout.take().unwrap());
    let req = Value::obj()
        .u("id", 1)
        .s("type", "Share")
        .v("config", Value::obj().done())
        .v("secrets", Value::obj().done())
        .v("uris", Value::Arr(vec![Value::Str(format!("file://{}", f.to_string_lossy()))]))
        .s("target", target)
        .v("compose", Value::obj().done())
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
fn the_device_that_announced_itself_is_sent_to() {
    let (port, seen) = receiver();
    let reply = share(&format!("https://127.0.0.1:{port}#{}", fingerprint()));
    assert!(reply.get("ok").is_some(), "{reply:?}");
    assert!(seen.load(Ordering::SeqCst) > 0, "the request arrived");
}

#[test]
fn an_impostor_is_refused_before_anything_is_sent() {
    let (port, seen) = receiver();
    // Somebody else's fingerprint: the device that was picked is not the one answering here.
    let reply = share(&format!("https://127.0.0.1:{port}#{}", "ab".repeat(32)));
    let err = reply.get("err").unwrap_or_else(|| panic!("should have been refused: {reply:?}"));
    assert!(err.str_field("message").unwrap_or("").contains("not the one that announced itself"), "{err:?}");
    assert_eq!(seen.load(Ordering::SeqCst), 0, "not one byte of the request reached it");
}

#[test]
fn an_address_typed_by_hand_has_nothing_to_be_held_to() {
    let (port, seen) = receiver();
    let reply = share(&format!("https://127.0.0.1:{port}"));
    assert!(reply.get("ok").is_some(), "{reply:?}");
    assert!(seen.load(Ordering::SeqCst) > 0);
}

#[test]
fn a_mangled_fingerprint_is_an_error_not_no_fingerprint() {
    let (port, seen) = receiver();
    let reply = share(&format!("https://127.0.0.1:{port}#abc123"));
    assert_eq!(reply.get("err").and_then(|e| e.str_field("code")), Some("Invalid"), "{reply:?}");
    assert_eq!(seen.load(Ordering::SeqCst), 0);
}

#[test]
fn this_machine_keeps_one_identity() {
    let cache = std::env::temp_dir().join(format!("kiki-localsend-id-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&cache);
    let fingerprint_of = || {
        let (port, _) = receiver();
        let dir = cache.join("files");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("a.txt"), b"x").unwrap();
        let mut child = Command::new(env!("CARGO_BIN_EXE_kiki-plugin-share-localsend")).env("XDG_CACHE_HOME", &cache).stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::inherit()).spawn().unwrap();
        let mut stdin = child.stdin.take().unwrap();
        let mut stdout = BufReader::new(child.stdout.take().unwrap());
        let req = Value::obj().u("id", 1).s("type", "Share").v("config", Value::obj().done()).v("secrets", Value::obj().done())
            .v("uris", Value::Arr(vec![Value::Str(format!("file://{}", dir.join("a.txt").to_string_lossy()))])).s("target", format!("https://127.0.0.1:{port}")).v("compose", Value::obj().done()).done();
        write_json(&mut stdin, &req).unwrap();
        while let Some((_, payload)) = read_frame(&mut stdout).unwrap() {
            let v = json::parse(&payload).unwrap();
            if v.get("ok").is_some() || v.get("err").is_some() {
                assert!(v.get("ok").is_some(), "{v:?}");
                break;
            }
        }
        let _ = write_json(&mut stdin, &Value::obj().u("id", 2).s("type", "Shutdown").done());
        let _ = child.wait();
        std::fs::read(cache.join("kiki-share-localsend/identity.crt.der")).expect("the certificate was kept")
    };
    let first = fingerprint_of();
    let second = fingerprint_of();
    assert_eq!(first, second, "a receiver that has seen this machine before sees the same one");
    use std::os::unix::fs::PermissionsExt;
    let mode = std::fs::metadata(cache.join("kiki-share-localsend/identity.key.der")).unwrap().permissions().mode() & 0o777;
    assert_eq!(mode, 0o600, "the key is this user's alone");
    let _ = std::fs::remove_dir_all(&cache);
}
