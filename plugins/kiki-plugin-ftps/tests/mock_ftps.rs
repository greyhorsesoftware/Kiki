//! Drives the `kiki-plugin-ftps` binary over its pipe protocol against an in-process mock FTPS
//! server (std::net + rustls with a self-signed certificate, over an in-memory tree). Covers explicit
//! (AUTH TLS) and implicit TLS, the certificate contract (unknown certificate and a wrong pin are
//! `Invalid`/`fingerprint` naming the fingerprint), MLSD listings with inline meta, byte-exact and
//! resumed (REST) reads, the write/stat/mkdir/rename/delete round trip, a wrong password, and
//! Validate. The mock refuses PORT/EPRT and records any data command not preceded by PASV/EPSV.

use kiki_plugin_sdk::json::{self, Value};
use kiki_plugin_sdk::{read_frame, write_binary, write_json};
use rustls::pki_types::pem::PemObject;
use rustls::pki_types::{CertificateDer, PrivateKeyDer};
use rustls::{ServerConfig, ServerConnection, StreamOwned};
use std::collections::BTreeMap;
use std::io::{BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

// Self-signed P-256 certificate for CN=localhost (X.509 v3 with subjectAltName; rustls rejects v1),
// generated once with `openssl ecparam -name prime256v1 -genkey -param_enc named_curve | openssl pkcs8
// -topk8 -nocrypt` and `openssl req -new -x509 -key key.pem -days 36500` over a v3 extensions section.
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

/// The fingerprint the plugin must report: SHA-256 of the DER certificate, colon-separated upper hex.
fn fingerprint() -> String {
    let der = CertificateDer::from_pem_slice(CERT_PEM.as_bytes()).unwrap();
    let out = ring::digest::digest(&ring::digest::SHA256, der.as_ref());
    out.as_ref().iter().map(|b| format!("{b:02X}")).collect::<Vec<_>>().join(":")
}

// ---------------------------------------------------------------- in-memory tree

#[derive(Clone)]
enum NodeKind {
    Dir,
    File(Vec<u8>),
}

#[derive(Clone)]
struct Node {
    kind: NodeKind,
    mode: u32,
    /// epoch seconds
    mtime: u64,
}

type Fs = Arc<Mutex<BTreeMap<String, Node>>>;

fn children(fs: &BTreeMap<String, Node>, dir: &str) -> Vec<(String, Node)> {
    let prefix = if dir == "/" { "/".to_string() } else { format!("{dir}/") };
    fs.iter().filter(|(p, _)| p.starts_with(&prefix) && p.len() > prefix.len() && !p[prefix.len()..].contains('/')).map(|(p, n)| (p[prefix.len()..].to_string(), n.clone())).collect()
}

fn pseudo_random(len: usize) -> Vec<u8> {
    let mut x: u32 = 0x1234_5678;
    (0..len)
        .map(|_| {
            x ^= x << 13;
            x ^= x >> 17;
            x ^= x << 5;
            x as u8
        })
        .collect()
}

const BIG: usize = 1024 * 1024 + 12_345;

fn fixture() -> Fs {
    let mut m = BTreeMap::new();
    let d = |m: &mut BTreeMap<String, Node>, p: &str| {
        m.insert(p.to_string(), Node { kind: NodeKind::Dir, mode: 0o755, mtime: 1_700_000_000 });
    };
    let f = |m: &mut BTreeMap<String, Node>, p: &str, b: &[u8], mt: u64| {
        m.insert(p.to_string(), Node { kind: NodeKind::File(b.to_vec()), mode: 0o644, mtime: mt });
    };
    d(&mut m, "/");
    d(&mut m, "/docs");
    f(&mut m, "/docs/readme.md", b"hello", 1_700_000_001);
    d(&mut m, "/docs/sub");
    f(&mut m, "/docs/sub/deep.txt", b"deep", 1_700_000_002);
    d(&mut m, "/empty");
    f(&mut m, "/data.bin", &pseudo_random(BIG), 1_700_000_003);
    f(&mut m, "/we ird'na me.txt", b"odd", 1_700_000_004);
    Arc::new(Mutex::new(m))
}

/// Epoch seconds as the `YYYYMMDDHHMMSS` (UTC) of MDTM and the MLSx `modify` fact.
fn ftp_time(secs: u64) -> String {
    let (days, rem) = (secs / 86400, secs % 86400);
    // civil from days, Howard Hinnant's algorithm
    let z = days as i64 + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + if m <= 2 { 1 } else { 0 };
    format!("{y:04}{m:02}{d:02}{:02}{:02}{:02}", rem / 3600, rem % 3600 / 60, rem % 60)
}

fn parent_and_name(path: &str) -> (String, String) {
    let p = path.trim_end_matches('/');
    match p.rfind('/') {
        Some(0) => ("/".into(), p[1..].into()),
        Some(i) => (p[..i].into(), p[i + 1..].into()),
        None => ("/".into(), p.into()),
    }
}

// ---------------------------------------------------------------- mock server

/// A control or data channel: plain TCP, or TLS on the same socket after AUTH TLS / PROT P.
#[allow(clippy::large_enum_variant)]
enum Chan {
    Plain(TcpStream),
    Tls(StreamOwned<ServerConnection, TcpStream>),
}

impl Read for Chan {
    fn read(&mut self, b: &mut [u8]) -> std::io::Result<usize> {
        match self {
            Chan::Plain(s) => s.read(b),
            Chan::Tls(s) => s.read(b),
        }
    }
}

impl Write for Chan {
    fn write(&mut self, b: &[u8]) -> std::io::Result<usize> {
        match self {
            Chan::Plain(s) => s.write(b),
            Chan::Tls(s) => s.write(b),
        }
    }
    fn flush(&mut self) -> std::io::Result<()> {
        match self {
            Chan::Plain(s) => s.flush(),
            Chan::Tls(s) => s.flush(),
        }
    }
}

impl Chan {
    fn upgrade(self, tls: &Arc<ServerConfig>) -> Chan {
        match self {
            Chan::Plain(s) => Chan::Tls(StreamOwned::new(ServerConnection::new(Arc::clone(tls)).unwrap(), s)),
            t => t,
        }
    }

    /// Closes cleanly: rustls clients treat a bare TCP close as UnexpectedEof, so send close_notify.
    fn close(self) {
        if let Chan::Tls(mut s) = self {
            let _ = s.flush(); // completes the handshake even when nothing was written
            s.conn.send_close_notify();
            while s.conn.wants_write() {
                if s.conn.write_tls(&mut s.sock).is_err() {
                    break;
                }
            }
        }
    }

    /// One CRLF-terminated command; None at EOF or on a TLS error (a refused certificate).
    fn read_line(&mut self) -> Option<String> {
        let mut buf = Vec::new();
        let mut b = [0u8; 1];
        loop {
            match self.read(&mut b) {
                Ok(0) | Err(_) => return None,
                Ok(_) if b[0] == b'\n' => break,
                Ok(_) => buf.push(b[0]),
            }
        }
        Some(String::from_utf8_lossy(&buf).trim_end_matches('\r').to_string())
    }
}

#[derive(Default)]
struct Counters {
    /// PASV/EPSV replies handed out
    passives: AtomicUsize,
    /// REST offsets received, in order
    rests: Mutex<Vec<u64>>,
    /// PORT/EPRT, or a data command without a preceding PASV/EPSV
    violations: Mutex<Vec<String>>,
    /// Successful sign-ins: how many control connections this server has really been given.
    logins: AtomicUsize,
}

struct Mock {
    port: u16,
    fs: Fs,
    counters: Arc<Counters>,
}

impl Mock {
    fn logins(&self) -> usize {
        self.counters.logins.load(Ordering::SeqCst)
    }

    fn assert_passive_only(&self) {
        let v = self.counters.violations.lock().unwrap();
        assert!(v.is_empty(), "active mode or data command without PASV/EPSV: {v:?}");
        assert!(self.counters.passives.load(Ordering::SeqCst) > 0, "no passive data connection was ever requested");
    }
}

struct Conn {
    fs: Fs,
    counters: Arc<Counters>,
    tls: Arc<ServerConfig>,
    chan: Chan,
    logged_in: bool,
    user: String,
    /// PROT P seen: data connections are TLS (the initial level is Clear, RFC 4217)
    prot_p: bool,
    /// listener from the last PASV/EPSV, consumed by the next data command
    pending: Option<TcpListener>,
    rest: u64,
    rnfr: Option<String>,
}

/// Set MOCK_FTPS_TRACE=1 to see the control-channel exchange on stderr.
fn trace(s: &str) {
    if std::env::var_os("MOCK_FTPS_TRACE").is_some() {
        eprintln!("mock ftps: {s}");
    }
}

fn reply(chan: &mut Chan, s: &str) {
    trace(&format!("<- {s}"));
    let _ = chan.write_all(format!("{s}\r\n").as_bytes());
}

impl Conn {
    fn mlsx_line(&self, name: &str, n: &Node) -> String {
        let (ty, size, perm) = match &n.kind {
            NodeKind::Dir => ("dir", 0, "cdeflmp"),
            NodeKind::File(b) => ("file", b.len(), "adfrw"),
        };
        format!("type={ty};size={size};modify={};perm={perm};UNIX.mode=0{:o};UNIX.uid=1000;UNIX.gid=1000; {name}", ftp_time(n.mtime), n.mode)
    }

    /// Accepts the passive data connection for a transfer. `None` (after a 425) when the client
    /// never asked for PASV/EPSV: the plugin must never use PORT.
    fn data_chan(&mut self, cmd: &str) -> Option<Chan> {
        let Some(l) = self.pending.take() else {
            self.counters.violations.lock().unwrap().push(format!("{cmd} without PASV/EPSV"));
            reply(&mut self.chan, "425 Use PASV or EPSV first");
            return None;
        };
        reply(&mut self.chan, "150 Opening data connection");
        let (sock, _) = l.accept().unwrap();
        let c = Chan::Plain(sock);
        Some(if self.prot_p { c.upgrade(&self.tls) } else { c })
    }

    /// Refuses a data command. The client connects to the passive port right after sending the
    /// command, before reading the reply, so accept that connection and drop it: closing the
    /// listener first would race the connect and turn a 550 into a connection error.
    fn refuse_data(&mut self, msg: &str) {
        if let Some(l) = self.pending.take() {
            let _ = l.accept();
        }
        reply(&mut self.chan, msg);
    }

    fn send_data(&mut self, cmd: &str, bytes: &[u8]) {
        let Some(mut d) = self.data_chan(cmd) else { return };
        let r = d.write_all(bytes);
        d.close();
        match r {
            Ok(()) => reply(&mut self.chan, "226 Transfer complete"),
            Err(_) => reply(&mut self.chan, "426 Connection closed; transfer aborted"),
        }
    }

    fn handle(&mut self, cmd: &str, arg: &str) -> bool {
        if !self.logged_in && !matches!(cmd, "AUTH" | "PBSZ" | "PROT" | "USER" | "PASS" | "FEAT" | "QUIT") {
            reply(&mut self.chan, "530 Please login with USER and PASS");
            return true;
        }
        match cmd {
            "AUTH" => {
                if arg != "TLS" {
                    reply(&mut self.chan, "504 Only AUTH TLS");
                    return true;
                }
                reply(&mut self.chan, "234 AUTH TLS successful");
                if let Chan::Plain(s) = &self.chan {
                    let s = s.try_clone().unwrap();
                    self.chan = Chan::Tls(StreamOwned::new(ServerConnection::new(Arc::clone(&self.tls)).unwrap(), s));
                }
            }
            "PBSZ" => reply(&mut self.chan, "200 PBSZ 0 successful"),
            "PROT" => {
                self.prot_p = arg == "P";
                reply(&mut self.chan, "200 Protection level set");
            }
            "USER" => {
                self.user = arg.to_string();
                reply(&mut self.chan, "331 Password required");
            }
            "PASS" => {
                if self.user == "kiki" && arg == "secret" {
                    self.logged_in = true;
                    self.counters.logins.fetch_add(1, Ordering::SeqCst);
                    reply(&mut self.chan, "230 Login successful");
                } else {
                    reply(&mut self.chan, "530 Login incorrect");
                }
            }
            "FEAT" => {
                for l in ["211-Features:", " MLST type*;size*;modify*;perm;UNIX.mode;UNIX.uid;UNIX.gid;", " MLSD", " REST STREAM", " SIZE", " MDTM", " UTF8", " MFMT", " AUTH TLS", " PBSZ", " PROT", "211 End"] {
                    reply(&mut self.chan, l);
                }
            }
            "PWD" => reply(&mut self.chan, "257 \"/\" is the current directory"),
            "CWD" => {
                if matches!(self.fs.lock().unwrap().get(arg).map(|n| &n.kind), Some(NodeKind::Dir)) {
                    reply(&mut self.chan, "250 Directory changed");
                } else {
                    reply(&mut self.chan, "550 No such directory");
                }
            }
            "TYPE" => reply(&mut self.chan, "200 Type set"),
            "PASV" | "EPSV" => {
                let l = TcpListener::bind("127.0.0.1:0").unwrap();
                let port = l.local_addr().unwrap().port();
                self.pending = Some(l);
                self.counters.passives.fetch_add(1, Ordering::SeqCst);
                if cmd == "PASV" {
                    reply(&mut self.chan, &format!("227 Entering Passive Mode (127,0,0,1,{},{})", port / 256, port % 256));
                } else {
                    reply(&mut self.chan, &format!("229 Entering Extended Passive Mode (|||{port}|)"));
                }
            }
            "PORT" | "EPRT" => {
                self.counters.violations.lock().unwrap().push(format!("{cmd} {arg}"));
                reply(&mut self.chan, "502 Active mode not supported");
            }
            "MLSD" | "LIST" => {
                let path = if arg.is_empty() { "/" } else { arg };
                let listing = {
                    let fs = self.fs.lock().unwrap();
                    if !matches!(fs.get(path).map(|n| &n.kind), Some(NodeKind::Dir)) {
                        None
                    } else if cmd == "LIST" {
                        Some(
                            children(&fs, path)
                                .iter()
                                .map(|(name, n)| {
                                    format!(
                                        "{} 1 kiki kiki {} Jan  1  2020 {name}\r\n",
                                        if matches!(n.kind, NodeKind::Dir) { "drwxr-xr-x" } else { "-rw-r--r--" },
                                        match &n.kind {
                                            NodeKind::File(b) => b.len(),
                                            _ => 0,
                                        }
                                    )
                                })
                                .collect::<String>(),
                        )
                    } else {
                        let this = fs.get(path).unwrap();
                        let mut out = format!("{}\r\n{}\r\n", self.mlsx_line(".", this).replace("type=dir", "type=cdir"), self.mlsx_line("..", this).replace("type=dir", "type=pdir"));
                        for (name, n) in children(&fs, path) {
                            out.push_str(&self.mlsx_line(&name, &n));
                            out.push_str("\r\n");
                        }
                        Some(out)
                    }
                };
                match listing {
                    Some(l) => self.send_data(cmd, l.as_bytes()),
                    None => self.refuse_data("550 No such directory"),
                }
            }
            "MLST" => {
                let path = if arg.is_empty() { "/" } else { arg };
                match self.fs.lock().unwrap().get(path).cloned() {
                    Some(n) => {
                        let line = self.mlsx_line(path, &n);
                        reply(&mut self.chan, &format!("250-Listing {path}\r\n {line}\r\n250 End"));
                    }
                    None => reply(&mut self.chan, "550 No such file"),
                }
            }
            "SIZE" => match self.fs.lock().unwrap().get(arg).map(|n| n.kind.clone()) {
                Some(NodeKind::File(b)) => reply(&mut self.chan, &format!("213 {}", b.len())),
                _ => reply(&mut self.chan, "550 Could not get file size"),
            },
            "MDTM" => match self.fs.lock().unwrap().get(arg).map(|n| n.mtime) {
                Some(t) => reply(&mut self.chan, &format!("213 {}", ftp_time(t))),
                None => reply(&mut self.chan, "550 No such file"),
            },
            "REST" => match arg.parse::<u64>() {
                Ok(o) => {
                    self.rest = o;
                    self.counters.rests.lock().unwrap().push(o);
                    reply(&mut self.chan, &format!("350 Restarting at {o}"));
                }
                Err(_) => reply(&mut self.chan, "501 Bad offset"),
            },
            "RETR" => {
                let data = match self.fs.lock().unwrap().get(arg).map(|n| n.kind.clone()) {
                    Some(NodeKind::File(b)) => Some(b),
                    _ => None,
                };
                let offset = std::mem::take(&mut self.rest) as usize;
                match data {
                    Some(b) => self.send_data(cmd, &b[offset.min(b.len())..]),
                    None => self.refuse_data("550 No such file"),
                }
            }
            "STOR" => {
                let (parent, name) = parent_and_name(arg);
                if name.is_empty() || !matches!(self.fs.lock().unwrap().get(&parent).map(|n| &n.kind), Some(NodeKind::Dir)) {
                    self.refuse_data("553 Bad path");
                    return true;
                }
                let Some(mut d) = self.data_chan(cmd) else { return true };
                let mut buf = Vec::new();
                match d.read_to_end(&mut buf) {
                    Ok(_) => {}
                    Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => {} // client closed without a TLS close_notify
                    Err(e) => panic!("STOR read: {e}"),
                }
                drop(d);
                self.fs.lock().unwrap().insert(arg.to_string(), Node { kind: NodeKind::File(buf), mode: 0o644, mtime: 1_700_000_100 });
                reply(&mut self.chan, "226 Transfer complete");
            }
            "MKD" => {
                let (parent, name) = parent_and_name(arg);
                let mut fs = self.fs.lock().unwrap();
                if fs.contains_key(arg) || name.is_empty() || !matches!(fs.get(&parent).map(|n| &n.kind), Some(NodeKind::Dir)) {
                    reply(&mut self.chan, "550 Cannot create directory");
                } else {
                    fs.insert(arg.to_string(), Node { kind: NodeKind::Dir, mode: 0o755, mtime: 1_700_000_200 });
                    reply(&mut self.chan, &format!("257 \"{arg}\" created"));
                }
            }
            "RMD" => {
                let mut fs = self.fs.lock().unwrap();
                match fs.get(arg).map(|n| &n.kind) {
                    Some(NodeKind::Dir) if children(&fs, arg).is_empty() => {
                        fs.remove(arg);
                        reply(&mut self.chan, "250 Directory removed");
                    }
                    _ => reply(&mut self.chan, "550 Remove directory operation failed"),
                }
            }
            "DELE" => {
                let mut fs = self.fs.lock().unwrap();
                match fs.get(arg).map(|n| &n.kind) {
                    Some(NodeKind::File(_)) => {
                        fs.remove(arg);
                        reply(&mut self.chan, "250 Delete operation successful");
                    }
                    _ => reply(&mut self.chan, "550 Delete operation failed"),
                }
            }
            "RNFR" => {
                if self.fs.lock().unwrap().contains_key(arg) {
                    self.rnfr = Some(arg.to_string());
                    reply(&mut self.chan, "350 Ready for RNTO");
                } else {
                    reply(&mut self.chan, "550 RNFR command failed");
                }
            }
            "RNTO" => match self.rnfr.take() {
                Some(from) => {
                    let mut fs = self.fs.lock().unwrap();
                    let moved: Vec<(String, Node)> = fs.iter().filter(|(p, _)| *p == &from || p.starts_with(&format!("{from}/"))).map(|(p, n)| (format!("{arg}{}", &p[from.len()..]), n.clone())).collect();
                    fs.retain(|p, _| p != &from && !p.starts_with(&format!("{from}/")));
                    fs.extend(moved);
                    reply(&mut self.chan, "250 Rename successful");
                }
                None => reply(&mut self.chan, "503 Bad sequence of commands"),
            },
            "MFMT" => {
                let Some((ts, path)) = arg.split_once(' ') else {
                    reply(&mut self.chan, "501 MFMT needs a time and a path");
                    return true;
                };
                let mut fs = self.fs.lock().unwrap();
                match (fs.get_mut(path), ts.len() == 14) {
                    (Some(n), true) => {
                        n.mtime = ftp_time_parse(ts);
                        reply(&mut self.chan, &format!("213 Modify={ts}; {path}"));
                    }
                    _ => reply(&mut self.chan, "550 MFMT failed"),
                }
            }
            "QUIT" => {
                reply(&mut self.chan, "221 Goodbye");
                return false;
            }
            _ => {
                eprintln!("mock ftps: unknown command {cmd} {arg}");
                reply(&mut self.chan, "502 Command not implemented");
            }
        }
        true
    }
}

/// Inverse of `ftp_time` for MFMT; UTC, no validation beyond the length check done by the caller.
fn ftp_time_parse(ts: &str) -> u64 {
    let n = |a: usize, b: usize| ts[a..b].parse::<i64>().unwrap();
    let (y, m, d) = (n(0, 4), n(4, 6), n(6, 8));
    let (y, m) = if m <= 2 { (y - 1, m + 9) } else { (y, m - 3) };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let doy = (153 * m + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146_097 + doe - 719_468;
    (days * 86400 + n(8, 10) * 3600 + n(10, 12) * 60 + n(12, 14)) as u64
}

fn serve(sock: TcpStream, implicit: bool, fs: Fs, counters: Arc<Counters>, tls: Arc<ServerConfig>) {
    let chan = Chan::Plain(sock);
    let chan = if implicit { chan.upgrade(&tls) } else { chan };
    let mut c = Conn { fs, counters, tls, chan, logged_in: false, user: String::new(), prot_p: false, pending: None, rest: 0, rnfr: None };
    if c.chan.write_all(b"220 mock ftps ready\r\n").is_err() {
        return; // implicit mode: the greeting drove the handshake and the client refused the certificate; drop the socket like a real server
    }
    while let Some(line) = c.chan.read_line() {
        trace(&format!("-> {line}"));
        let (cmd, arg) = line.split_once(' ').unwrap_or((&line, ""));
        if !c.handle(&cmd.to_ascii_uppercase(), arg.trim()) {
            break;
        }
    }
    // No TLS close here: after a refused certificate both sides would block in the handshake.
}

fn start(implicit: bool) -> Mock {
    let cert = CertificateDer::from_pem_slice(CERT_PEM.as_bytes()).unwrap();
    let key = PrivateKeyDer::from_pem_slice(KEY_PEM.as_bytes()).unwrap();
    let mut cfg = ServerConfig::builder().with_no_client_auth().with_single_cert(vec![cert], key).unwrap();
    // No TLS 1.3 session tickets: a data connection is one-directional (client STORs, server only
    // reads), so the server must never want to write mid-transfer. Tickets are the one post-handshake
    // server write, and with both peers doing blocking IO they would deadlock a large upload.
    cfg.send_tls13_tickets = 0;
    let tls = Arc::new(cfg);
    let fs = fixture();
    let counters = Arc::new(Counters::default());
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let (fs2, counters2) = (fs.clone(), counters.clone());
    std::thread::spawn(move || {
        for sock in listener.incoming().flatten() {
            let (fs, counters, tls) = (fs2.clone(), counters2.clone(), tls.clone());
            std::thread::spawn(move || serve(sock, implicit, fs, counters, tls));
        }
    });
    Mock { port, fs, counters }
}

// ---------------------------------------------------------------- plugin driver

struct Plugin {
    child: Child,
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
    next: u64,
    /// The wire lines the plugin logged (`KIKI_PLUGIN_LOG`), kept out of `req`'s frames.
    log: Vec<String>,
}

impl Plugin {
    fn spawn() -> Plugin {
        Plugin::spawn_all(false)
    }

    /// With the library log asked for, as the daemon asks: `Log` events between the frames.
    fn spawn_logging() -> Plugin {
        Plugin::spawn_all(true)
    }

    fn spawn_all(log: bool) -> Plugin {
        let mut cmd = Command::new(env!("CARGO_BIN_EXE_kiki-plugin-ftps"));
        if log {
            cmd.env("KIKI_PLUGIN_LOG", "1");
        }
        let mut child = cmd.stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::inherit()).spawn().expect("spawn plugin");
        let stdin = child.stdin.take().unwrap();
        let stdout = BufReader::new(child.stdout.take().unwrap());
        Plugin { child, stdin, stdout, next: 1, log: Vec::new() }
    }

    /// Sends a request; returns (streamed JSON frames, final reply).
    fn req(&mut self, mut v: Value) -> (Vec<Value>, Value) {
        let id = self.next;
        self.next += 1;
        if let Value::Obj(m) = &mut v {
            m.insert("id".into(), Value::Uint(id));
        }
        write_json(&mut self.stdin, &v).unwrap();
        let mut frames = Vec::new();
        loop {
            let (kind, payload) = read_frame(&mut self.stdout).unwrap().expect("plugin closed its stdout");
            assert_eq!(kind, 0, "unexpected binary frame");
            let f = json::parse(&payload).unwrap();
            if f.str_field("event") == Some("Log") {
                if f.str_field("target") == Some("wire") {
                    self.log.push(f.str_field("message").unwrap_or("").to_string());
                }
                continue;
            }
            if f.get("ok").is_some() || f.get("err").is_some() {
                assert_eq!(f.u64_field("id"), Some(id));
                return (frames, f);
            }
            frames.push(f);
        }
    }

    fn ok(&mut self, v: Value) -> Value {
        let (_, r) = self.req(v);
        r.get("ok").unwrap_or_else(|| panic!("expected ok, got {}", json::to_string(&r))).clone()
    }

    fn connect(&mut self, port: u16, password: &str, implicit: bool, pin: Option<&str>) -> Value {
        let cfg = Value::obj()
            .s("host", "127.0.0.1")
            .s("port", port.to_string())
            .s("username", "kiki")
            .s("remotePath", "/")
            .s("encryption", if implicit { "Implicit TLS" } else { "Explicit TLS (AUTH TLS)" })
            .opt_s("trustedFingerprint", pin)
            .done();
        let (_, r) = self.req(Value::obj().s("type", "Connect").s("location", "lab").s("role", "browse").v("config", cfg).v("secrets", Value::obj().s("password", password).done()).done());
        r
    }

    fn scan(&mut self, path: &str) -> Vec<Value> {
        let (frames, r) = self.req(Value::obj().s("type", "Scan").s("location", "lab").s("path", path).b("recursive", false).done());
        assert!(r.get("ok").is_some(), "scan failed: {}", json::to_string(&r));
        let entries: Vec<Value> = frames.iter().flat_map(|f| f.get("entries").and_then(Value::as_arr).unwrap_or(&[]).to_vec()).collect();
        assert_eq!(r.get("ok").unwrap().u64_field("n"), Some(entries.len() as u64), "n must equal the streamed entry count");
        entries
    }

    /// Streams a Read; returns the bytes and the final reply.
    fn read_raw(&mut self, path: &str, offset: u64) -> (Vec<u8>, Value) {
        let id = self.next;
        self.next += 1;
        write_json(&mut self.stdin, &Value::obj().u("id", id).s("type", "Read").s("location", "lab").s("path", path).u("offset", offset).done()).unwrap();
        let mut bytes = Vec::new();
        loop {
            let (kind, payload) = read_frame(&mut self.stdout).unwrap().expect("eof");
            assert_eq!(kind, 1, "binary frames until the end marker");
            if payload.is_empty() {
                break;
            }
            bytes.extend_from_slice(&payload);
        }
        let (kind, payload) = read_frame(&mut self.stdout).unwrap().expect("eof");
        assert_eq!(kind, 0);
        (bytes, json::parse(&payload).unwrap())
    }

    fn read(&mut self, path: &str, offset: u64) -> Vec<u8> {
        let (bytes, r) = self.read_raw(path, offset);
        assert!(r.get("ok").is_some(), "read failed: {}", json::to_string(&r));
        assert_eq!(r.get("ok").unwrap().u64_field("bytes"), Some(bytes.len() as u64));
        bytes
    }

    fn write(&mut self, path: &str, data: &[u8], mtime: u64) -> Value {
        let id = self.next;
        self.next += 1;
        write_json(&mut self.stdin, &Value::obj().u("id", id).s("type", "Write").s("location", "lab").s("path", path).u("size", data.len() as u64).u("mtime", mtime).done()).unwrap();
        for c in data.chunks(200_000) {
            write_binary(&mut self.stdin, c).unwrap();
        }
        write_binary(&mut self.stdin, &[]).unwrap();
        self.stdin.flush().unwrap();
        let (kind, payload) = read_frame(&mut self.stdout).unwrap().expect("eof");
        assert_eq!(kind, 0);
        json::parse(&payload).unwrap()
    }
}

impl Drop for Plugin {
    fn drop(&mut self) {
        let _ = write_json(&mut self.stdin, &Value::obj().u("id", 999_999).s("type", "Shutdown").done());
        let _ = self.child.wait();
    }
}

/// (name, kind, size, mtime ms, mode) of a listing, sorted.
fn normalise(entries: &[Value]) -> Vec<(String, String, u64, u64, Option<u64>)> {
    let mut v: Vec<_> = entries
        .iter()
        .map(|e| {
            let m = e.get("meta").expect("meta inline");
            assert!(m.get("size").is_some(), "meta must be an object: {}", json::to_string(e));
            (e.str_field("name").unwrap().to_string(), e.str_field("kind").unwrap().to_string(), m.u64_field("size").unwrap(), m.u64_field("mtime").unwrap(), m.u64_field("mode"))
        })
        .collect();
    v.sort();
    v
}

/// What the fixture says a directory listing must contain.
fn expected(fs: &Fs, dir: &str) -> Vec<(String, String, u64, u64, Option<u64>)> {
    let fs = fs.lock().unwrap();
    let mut v: Vec<_> = children(&fs, dir)
        .into_iter()
        .map(|(name, n)| match &n.kind {
            NodeKind::Dir => (name, "dir".to_string(), 0, n.mtime * 1000, Some(n.mode as u64)),
            NodeKind::File(b) => (name, "file".to_string(), b.len() as u64, n.mtime * 1000, Some(n.mode as u64)),
        })
        .collect();
    v.sort();
    v
}

fn err_of(r: &Value) -> &Value {
    r.get("err").unwrap_or_else(|| panic!("expected err, got {}", json::to_string(r)))
}

/// Connects with the fingerprint pinned; asserts the reply reports it.
fn connected(implicit: bool) -> (Mock, Plugin) {
    let m = start(implicit);
    let mut p = Plugin::spawn();
    let fp = fingerprint();
    let r = p.connect(m.port, "secret", implicit, Some(&fp));
    assert!(r.get("ok").is_some(), "connect: {}", json::to_string(&r));
    assert_eq!(r.get("ok").unwrap().str_field("fingerprint"), Some(fp.as_str()));
    (m, p)
}

fn assert_listing(m: &Mock, p: &mut Plugin) {
    let root = p.scan("/");
    assert_eq!(normalise(&root), expected(&m.fs, "/"));
    let names: Vec<&str> = root.iter().map(|e| e.str_field("name").unwrap()).collect();
    assert!(names.contains(&"we ird'na me.txt"), "space and quote survive: {names:?}");
    assert!(!names.contains(&".") && !names.contains(&".."), "cdir/pdir entries are dropped: {names:?}");
    let odd = root.iter().find(|e| e.str_field("name") == Some("we ird'na me.txt")).unwrap();
    assert_eq!(odd.get("meta").unwrap().str_field("owner"), Some("1000"));
    assert_eq!(normalise(&p.scan("/docs")), expected(&m.fs, "/docs"));
    assert!(p.scan("/empty").is_empty());
    let (_, r) = p.req(Value::obj().s("type", "Scan").s("location", "lab").s("path", "/nope").b("recursive", false).done());
    assert_eq!(err_of(&r).str_field("code"), Some("NotFound"));
    let (_, r) = p.req(Value::obj().s("type", "Scan").s("location", "lab").s("path", "/docs").b("recursive", true).done());
    assert_eq!(err_of(&r).str_field("code"), Some("Unsupported"), "no recursive listing over FTP");
}

// ---------------------------------------------------------------- tests

#[test]
fn explicit_tls_trust_on_first_use_then_listing() {
    let m = start(false);
    let mut p = Plugin::spawn();
    let fp = fingerprint();
    // Unknown certificate: refused, with the fingerprint so kiki can ask the user to pin it.
    let r = p.connect(m.port, "secret", false, None);
    let e = err_of(&r);
    assert_eq!(e.str_field("code"), Some("Invalid"), "{}", json::to_string(&r));
    assert_eq!(e.str_field("field"), Some("fingerprint"));
    assert_eq!(e.str_field("message"), Some(fp.as_str()));
    // Pinned: connects and reports the same fingerprint; Connect is idempotent.
    let r = p.connect(m.port, "secret", false, Some(&fp));
    assert_eq!(r.get("ok").and_then(|o| o.str_field("fingerprint")), Some(fp.as_str()), "{}", json::to_string(&r));
    let again = p.connect(m.port, "secret", false, Some(&fp));
    assert_eq!(json::to_string(again.get("ok").unwrap()), json::to_string(r.get("ok").unwrap()), "Connect is idempotent");
    // The pin is case-insensitive.
    let mut q = Plugin::spawn();
    assert!(q.connect(m.port, "secret", false, Some(&fp.to_lowercase())).get("ok").is_some());

    assert_listing(&m, &mut p);
    m.assert_passive_only();
}

/// A location's name can be given away: remove one and add another under the same name. The
/// plugin keeps a session per name and role, so unless it compares the config it hands the new
/// location the connection to the machine that was removed — and FTP answers a `LIST` of a path
/// that is not there with an empty listing and a 226, so what the user sees is an empty folder
/// and no error at all. (The daemon disconnects on remove; this is the plugin's own guard, and
/// it holds even if something else ever forgets to.)
#[test]
fn a_name_reused_for_another_server_does_not_inherit_the_old_connection() {
    let old = start(false);
    let new = start(false);
    new.fs.lock().unwrap().insert("/only-on-the-new-one.txt".into(), Node { kind: NodeKind::File(b"new".to_vec()), mode: 0o644, mtime: 1_700_000_000 });
    let fp = fingerprint();
    let mut p = Plugin::spawn();

    assert!(p.connect(old.port, "secret", false, Some(&fp)).get("ok").is_some());
    let first = normalise(&p.scan("/"));
    assert_eq!(old.logins(), 1);

    // The same details again: the session stands, and nobody signs in twice for nothing.
    assert!(p.connect(old.port, "secret", false, Some(&fp)).get("ok").is_some());
    assert_eq!(old.logins(), 1, "the same server and sign-in reuses the session it has");
    assert_eq!(normalise(&p.scan("/")), first);

    // The same location name and role, another machine behind it.
    assert!(p.connect(new.port, "secret", false, Some(&fp)).get("ok").is_some());
    assert_eq!(new.logins(), 1, "the new server was really connected to");
    let second = normalise(&p.scan("/"));
    assert!(second.iter().any(|e| e.0 == "only-on-the-new-one.txt"), "the listing is the new server's: {second:?}");
    assert_ne!(first, second);

    // And a changed password on the same server is a new sign-in too, not the old connection.
    assert_eq!(err_of(&p.connect(new.port, "wrong", false, Some(&fp))).str_field("code"), Some("Auth"));
}

#[test]
fn implicit_tls_connect_and_listing() {
    let (m, mut p) = connected(true);
    assert_listing(&m, &mut p);
    m.assert_passive_only();
}

#[test]
fn mismatched_pin_is_invalid_fingerprint() {
    let fp = fingerprint();
    for implicit in [false, true] {
        let m = start(implicit);
        let mut p = Plugin::spawn();
        let r = p.connect(m.port, "secret", implicit, Some("AA:BB:CC:DD:EE:FF:00:11:22:33:44:55:66:77:88:99:AA:BB:CC:DD:EE:FF:00:11:22:33:44:55:66:77:88:99"));
        let e = err_of(&r);
        assert_eq!(e.str_field("code"), Some("Invalid"), "implicit={implicit}: {}", json::to_string(&r));
        assert_eq!(e.str_field("field"), Some("fingerprint"));
        assert_eq!(e.str_field("message"), Some(fp.as_str()), "the server's actual fingerprint is named");
        // The session is not left half-open: a correct pin now succeeds.
        assert!(p.connect(m.port, "secret", implicit, Some(&fp)).get("ok").is_some());
    }
}

#[test]
fn reads_are_exact_and_resume_with_rest() {
    let (m, mut p) = connected(false);
    let expect = pseudo_random(BIG);
    let got = p.read("/data.bin", 0);
    assert_eq!(got.len(), expect.len());
    assert!(got == expect, "bytes differ");
    assert!(m.counters.rests.lock().unwrap().is_empty(), "no REST for a read from 0");
    // partialRead: the tail comes back via REST
    let tail = p.read("/data.bin", 700_000);
    assert_eq!(tail, &expect[700_000..]);
    assert_eq!(*m.counters.rests.lock().unwrap(), vec![700_000]);
    // A later full read is not affected by the earlier REST.
    assert_eq!(p.read("/docs/readme.md", 0), b"hello");
    assert_eq!(p.read("/data.bin", 0).len(), BIG);
    // Missing file: end marker, then a typed error.
    let (bytes, r) = p.read_raw("/nope", 0);
    assert!(bytes.is_empty());
    assert_eq!(err_of(&r).str_field("code"), Some("NotFound"));
    // The control connection survives all of that.
    assert_eq!(p.scan("/docs").len(), 2);
    m.assert_passive_only();
}

// The connection log carries the control channel (owner, 2026-09-24): every command sent and
// every reply, as suppaftp traces them — with the password redacted before it leaves the plugin.
#[test]
fn the_connection_log_carries_the_control_channel() {
    let m = start(false);
    let mut p = Plugin::spawn_logging();
    let fp = fingerprint();
    let r = p.connect(m.port, "secret", false, Some(&fp));
    assert!(r.get("ok").is_some(), "connect: {}", json::to_string(&r));
    p.scan("/docs");
    let log = p.log.clone();
    assert!(log.iter().any(|l| l.starts_with("→ USER ")), "{log:?}");
    assert!(log.contains(&"→ PASS [redacted]".to_string()), "the password is redacted, not sent on: {log:?}");
    assert!(!log.iter().any(|l| l.contains("secret")), "no secret on the wire: {log:?}");
    assert!(log.iter().any(|l| l.starts_with("← 2")), "the server's replies, code first: {log:?}");
    assert!(log.iter().any(|l| l.starts_with("→ PASV") || l.starts_with("→ EPSV")), "{log:?}");
    assert!(log.iter().any(|l| l.starts_with("→ MLSD") || l.starts_with("→ LIST")), "{log:?}");
    let firsts: Vec<&String> = log.iter().filter(|l| l.starts_with("← ")).collect();
    assert!(firsts.windows(2).all(|w| w[0] != w[1]), "a reply line is logged once, not twice: {log:?}");
    drop(m);
}

#[test]
fn write_and_metadata_operations_round_trip() {
    let (m, mut p) = connected(false);
    let data = pseudo_random(300_007);
    let r = p.write("/upload.bin", &data, 1_600_000_000_000);
    assert_eq!(r.get("ok").and_then(|o| o.u64_field("bytes")), Some(data.len() as u64), "{}", json::to_string(&r));
    assert!(matches!(&m.fs.lock().unwrap().get("/upload.bin").expect("stored").kind, NodeKind::File(b) if *b == data));
    // Stat = SIZE + MDTM
    let st = p.ok(Value::obj().s("type", "Stat").s("location", "lab").s("path", "/upload.bin").done());
    assert_eq!(st.u64_field("size"), Some(data.len() as u64));
    assert_eq!(st.u64_field("mtime"), Some(1_700_000_100_000));
    // An empty file round-trips too.
    assert_eq!(p.write("/zero", &[], 0).get("ok").and_then(|o| o.u64_field("bytes")), Some(0));
    assert_eq!(p.read("/zero", 0), b"");

    p.ok(Value::obj().s("type", "Mkdir").s("location", "lab").s("path", "/made").done());
    let (_, r) = p.req(Value::obj().s("type", "Mkdir").s("location", "lab").s("path", "/made").done());
    assert!(r.get("err").is_some(), "mkdir of an existing directory fails: {}", json::to_string(&r));
    p.ok(Value::obj().s("type", "Rename").s("location", "lab").s("from", "/upload.bin").s("to", "/made/upload.bin").done());
    let (_, r) = p.req(Value::obj().s("type", "Rename").s("location", "lab").s("from", "/upload.bin").s("to", "/x").done());
    assert_eq!(err_of(&r).str_field("code"), Some("NotFound"));
    // The listing sees the moved file with its facts.
    let made = p.scan("/made");
    assert_eq!(normalise(&made), vec![("upload.bin".to_string(), "file".to_string(), data.len() as u64, 1_700_000_100_000, Some(0o644))]);
    // SetMtime is not offered (features.setMtime false): typed Unsupported.
    let (_, r) = p.req(Value::obj().s("type", "SetMtime").s("location", "lab").s("path", "/made/upload.bin").u("mtime", 1_500_000_000_000).done());
    assert_eq!(err_of(&r).str_field("code"), Some("Unsupported"));

    // Delete: a non-empty directory is refused, then file, then directory.
    let (_, r) = p.req(Value::obj().s("type", "Delete").s("location", "lab").s("path", "/made").done());
    assert!(r.get("err").is_some(), "non-empty directory: {}", json::to_string(&r));
    p.ok(Value::obj().s("type", "Delete").s("location", "lab").s("path", "/made/upload.bin").done());
    p.ok(Value::obj().s("type", "Delete").s("location", "lab").s("path", "/made").done());
    p.ok(Value::obj().s("type", "Delete").s("location", "lab").s("path", "/zero").done());
    {
        let fs = m.fs.lock().unwrap();
        assert!(!fs.contains_key("/made") && !fs.contains_key("/made/upload.bin") && !fs.contains_key("/zero"));
    }
    let (_, r) = p.req(Value::obj().s("type", "Stat").s("location", "lab").s("path", "/made/upload.bin").done());
    assert_eq!(err_of(&r).str_field("code"), Some("NotFound"));
    let (_, r) = p.req(Value::obj().s("type", "Delete").s("location", "lab").s("path", "/made").done());
    assert_eq!(err_of(&r).str_field("code"), Some("NotFound"));
    m.assert_passive_only();
}

#[test]
fn wrong_password_is_an_auth_error_and_validate_checks_fields() {
    let m = start(false);
    let mut p = Plugin::spawn();
    let r = p.connect(m.port, "nope", false, Some(&fingerprint()));
    assert_eq!(err_of(&r).str_field("code"), Some("Auth"), "{}", json::to_string(&r));
    let (_, r) = p.req(Value::obj().s("type", "Validate").v("config", Value::obj().s("host", "h").s("username", "u").s("port", "70000").done()).done());
    assert_eq!(err_of(&r).str_field("field"), Some("port"));
    let (_, r) = p.req(Value::obj().s("type", "Validate").v("config", Value::obj().s("host", "").s("username", "u").done()).done());
    assert_eq!(err_of(&r).str_field("field"), Some("host"));
    p.ok(Value::obj().s("type", "Validate").v("config", Value::obj().s("host", "h").s("username", "u").s("port", "21").done()).done());
    let d = p.ok(Value::obj().s("type", "Describe").done());
    assert_eq!(d.str_field("scheme"), Some("ftps"));
    let f = d.get("features").unwrap();
    assert_eq!(f.get("partialRead").and_then(Value::as_bool), Some(true));
    assert_eq!(f.get("metaInScan").and_then(Value::as_bool), Some(true));
    // Nothing touched a data connection, so only the "no PASV" half of the passive check applies.
    assert!(m.counters.violations.lock().unwrap().is_empty());
}

/// FTP is a text protocol and a command is a line: a name with a line break in it is two commands.
/// Found by a fixture file called "line\nbreak.txt" — which also threw every reply after it out of
/// step, failing fifteen files that had nothing wrong with them.
#[test]
fn a_line_break_in_a_name_never_reaches_the_wire() {
    let (m, mut p) = connected(false);
    assert!(p.write("/victim.txt", b"keep me", 0).get("ok").is_some());

    // The write that would have been `STOR /a` and then `DELE /victim.txt`.
    let r = p.write("/a\r\nDELE /victim.txt", b"x", 0);
    let e = err_of(&r);
    assert_eq!(e.str_field("code"), Some("Invalid"), "{}", json::to_string(&r));
    assert!(m.fs.lock().unwrap().get("/victim.txt").is_some(), "nothing was deleted");
    assert!(m.fs.lock().unwrap().get("/a").is_none(), "and nothing was stored");

    // Every way in that takes a name, a bare LF and a NUL included.
    for (ty, field, value) in [("Mkdir", "path", "/d\nRMD /x"), ("Delete", "path", "/victim.txt\r\nNOOP"), ("Stat", "path", "/s\n"), ("Scan", "path", "/l\r\n")] {
        let (_, r) = p.req(Value::obj().s("type", ty).s("location", "lab").s(field, value).done());
        assert_eq!(err_of(&r).str_field("code"), Some("Invalid"), "{ty}: {}", json::to_string(&r));
    }
    for (from, to) in [("/victim.txt\r\nDELE /victim.txt", "/b"), ("/victim.txt", "/b\r\nDELE /victim.txt")] {
        let (_, r) = p.req(Value::obj().s("type", "Rename").s("location", "lab").s("from", from).s("to", to).done());
        assert_eq!(err_of(&r).str_field("code"), Some("Invalid"));
    }
    // A read ends its (empty) stream before it answers, so it is asked the way reads are.
    let (bytes, r) = p.read_raw("/r\0", 0);
    assert!(bytes.is_empty());
    assert_eq!(err_of(&r).str_field("code"), Some("Invalid"));
    assert!(m.fs.lock().unwrap().get("/victim.txt").is_some());

    // And the conversation is still in step: what comes next is answered for what it is.
    assert_eq!(p.read("/victim.txt", 0), b"keep me");
    assert!(p.write("/after.txt", b"fine", 0).get("ok").is_some());
}
