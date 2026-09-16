//! Drives the `kiki-plugin-sftp` binary over its pipe protocol against an in-process mock SSH
//! server (russh server + russh-sftp server over an in-memory tree) whose exec channel emulates
//! the `find`/`stat` flavours the plugin probes for. Covers: GNU fast scan, the POSIX `stat -c`
//! and `stat -f` fallbacks, a refused exec channel, a killed exec stream falling back to READDIR
//! without duplicates, GNU find's exit status 1, pipelined reads, and the write/stat/mkdir/rename/
//! delete/setmtime/chmod round trip.

use kiki_plugin_sdk::json::{self, Value};
use kiki_plugin_sdk::{read_frame, write_binary, write_json};
use russh::keys::{Algorithm, PrivateKey};
use russh::server::{Auth, Msg, Session};
use russh::{Channel, ChannelId, CryptoVec};
use russh_sftp::protocol::{Attrs, Data, File, FileAttributes, Handle, Name, OpenFlags, Status, StatusCode};
use std::collections::{BTreeMap, HashMap};
use std::io::{BufReader, Write};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

// ---------------------------------------------------------------- in-memory tree

#[derive(Clone)]
enum NodeKind {
    Dir,
    File(Vec<u8>),
    Link(String),
}

#[derive(Clone)]
struct Node {
    kind: NodeKind,
    mode: u32,
    mtime: u32,
}

type Fs = Arc<Mutex<BTreeMap<String, Node>>>;

fn attrs(n: &Node) -> FileAttributes {
    let mut a = FileAttributes { size: Some(0), uid: Some(1000), user: Some("kiki".into()), gid: Some(1000), group: Some("kiki".into()), permissions: Some(n.mode), atime: Some(n.mtime), mtime: Some(n.mtime) };
    match &n.kind {
        NodeKind::Dir => a.set_dir(true),
        NodeKind::File(b) => {
            a.size = Some(b.len() as u64);
            a.set_regular(true);
        }
        NodeKind::Link(t) => {
            a.size = Some(t.len() as u64);
            a.set_symlink(true);
        }
    }
    a
}

fn children(fs: &BTreeMap<String, Node>, dir: &str, recursive: bool) -> Vec<(String, Node)> {
    let prefix = if dir == "/" { "/".to_string() } else { format!("{dir}/") };
    fs.iter()
        .filter(|(p, _)| p.starts_with(&prefix) && p.len() > prefix.len() && (recursive || !p[prefix.len()..].contains('/')))
        .map(|(p, n)| (p.clone(), n.clone()))
        .collect()
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

fn fixture() -> Fs {
    let mut m = BTreeMap::new();
    let d = |m: &mut BTreeMap<String, Node>, p: &str| {
        m.insert(p.to_string(), Node { kind: NodeKind::Dir, mode: 0o755, mtime: 1_700_000_000 });
    };
    let f = |m: &mut BTreeMap<String, Node>, p: &str, b: &[u8], mt: u32| {
        m.insert(p.to_string(), Node { kind: NodeKind::File(b.to_vec()), mode: 0o644, mtime: mt });
    };
    d(&mut m, "/");
    d(&mut m, "/docs");
    f(&mut m, "/docs/readme.md", b"hello", 1_700_000_001);
    d(&mut m, "/docs/sub");
    f(&mut m, "/docs/sub/deep.txt", b"deep", 1_700_000_002);
    d(&mut m, "/empty");
    f(&mut m, "/data.bin", &pseudo_random(4 * 1024 * 1024 + 12_345), 1_700_000_003);
    f(&mut m, "/we ird'na\nme.txt", b"odd", 1_700_000_004);
    m.insert("/link".into(), Node { kind: NodeKind::Link("/docs/readme.md".into()), mode: 0o777, mtime: 1_700_000_005 });
    Arc::new(Mutex::new(m))
}

// ---------------------------------------------------------------- mock server

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum ExecMode {
    /// GNU findutils present.
    Gnu,
    /// find without -printf, coreutils-style `stat -c`.
    StatC,
    /// find without -printf, BSD-style `stat -f`.
    StatF,
    /// exec allowed but no find on the box.
    NoFind,
    /// exec channel requests are refused (ForceCommand internal-sftp).
    Refused,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum ExecFail {
    None,
    /// The listing command dies after this many bytes of output (status 137).
    KillAfter(usize),
    /// The listing command prints everything and exits 1 (an unreadable entry).
    ExitOne,
}

#[derive(Default)]
struct Counters {
    readdir: AtomicUsize,
    reads: AtomicUsize,
    inflight: AtomicUsize,
    max_inflight: AtomicUsize,
    execs: AtomicUsize,
}

struct MockCfg {
    exec: ExecMode,
    fail: ExecFail,
    /// entries per READDIR reply
    page: usize,
}

struct Mock {
    port: u16,
    fs: Fs,
    counters: Arc<Counters>,
    _thread: std::thread::JoinHandle<()>,
}

#[derive(Clone)]
struct MockServer {
    fs: Fs,
    cfg: Arc<MockCfg>,
    counters: Arc<Counters>,
}

impl russh::server::Server for MockServer {
    type Handler = SshSession;
    fn new_client(&mut self, _: Option<std::net::SocketAddr>) -> SshSession {
        SshSession { fs: self.fs.clone(), cfg: self.cfg.clone(), counters: self.counters.clone(), channels: HashMap::new(), inbound: HashMap::new() }
    }
}

struct SshSession {
    fs: Fs,
    cfg: Arc<MockCfg>,
    counters: Arc<Counters>,
    channels: HashMap<ChannelId, Channel<Msg>>,
    /// per channel: bytes of SFTP packets not yet complete (for counting READ arrivals)
    inbound: HashMap<ChannelId, Vec<u8>>,
}

fn unquote(s: &str) -> String {
    // Undo kiki_plugin_sdk::shell_quote: 'a'\''b' -> a'b
    let inner = s.trim().trim_start_matches('\'').trim_end_matches('\'');
    inner.replace("'\\''", "'")
}

fn type_words(n: &Node, bsd: bool) -> &'static str {
    match (&n.kind, bsd) {
        (NodeKind::Dir, false) => "directory",
        (NodeKind::File(b), false) if b.is_empty() => "regular empty file",
        (NodeKind::File(_), false) => "regular file",
        (NodeKind::Link(_), false) => "symbolic link",
        (NodeKind::Dir, true) => "Directory",
        (NodeKind::File(_), true) => "Regular File",
        (NodeKind::Link(_), true) => "Symbolic Link",
    }
}

impl SshSession {
    /// Emulates the shell side of the exec channel for the commands the plugin issues.
    fn emulate(&self, cmd: &str) -> (Vec<u8>, u32) {
        let mode = self.cfg.exec;
        if cmd.contains("find --version") {
            return match mode {
                ExecMode::Gnu => (b"find (GNU findutils) 4.9.0\n".to_vec(), 0),
                ExecMode::StatC | ExecMode::StatF => (Vec::new(), 0), // `find --version` fails, `| head -1` exits 0
                _ => (Vec::new(), 1),                                   // command -v find fails
            };
        }
        if cmd.contains("stat -c '%F' /") {
            return match mode {
                ExecMode::StatC => (b"statc\n".to_vec(), 0),
                ExecMode::StatF => (b"statf\n".to_vec(), 0),
                _ => (Vec::new(), 3),
            };
        }
        if let Some(rest) = cmd.strip_prefix("find ") {
            let end = rest.find(" -mindepth").expect("find command shape");
            let path = unquote(&rest[..end]);
            let recursive = !cmd.contains("-maxdepth 1");
            let fs = self.fs.lock().unwrap();
            let list = children(&fs, &path, recursive);
            let mut out = Vec::new();
            if cmd.contains("-printf") {
                assert_eq!(mode, ExecMode::Gnu, "-printf used without GNU find");
                for (p, n) in &list {
                    let a = attrs(n);
                    let ty = match n.kind {
                        NodeKind::Dir => "d",
                        NodeKind::File(_) => "f",
                        NodeKind::Link(_) => "l",
                    };
                    let rel = &p[if path == "/" { 1 } else { path.len() + 1 }..];
                    let shown = if recursive { rel } else { rel.rsplit('/').next().unwrap() };
                    write!(out, "{ty}\0{}\0{}\0{}.0000000000\0{:o}\0kiki\0kiki\0{shown}\0", if ty == "l" { "f" } else { ty }, a.size.unwrap(), n.mtime, n.mode).unwrap();
                }
            } else if cmd.contains("-exec sh -c") {
                let bsd = cmd.contains("stat -f");
                assert_eq!(bsd, mode == ExecMode::StatF, "stat flavour does not match the probe answer");
                for (p, n) in &list {
                    let a = attrs(n);
                    write!(out, "{}|{}|{}|{:o}|kiki|kiki\n{p}\0", type_words(n, bsd), a.size.unwrap(), n.mtime, n.mode).unwrap();
                }
            } else {
                return (b"find: unknown predicate\n".to_vec(), 1);
            }
            return match self.cfg.fail {
                ExecFail::None => (out, 0),
                ExecFail::ExitOne => (out, 1),
                ExecFail::KillAfter(n) => {
                    out.truncate(n);
                    (out, 137)
                }
            };
        }
        (b"sh: command not found\n".to_vec(), 127)
    }
}

impl russh::server::Handler for SshSession {
    type Error = russh::Error;

    async fn auth_password(&mut self, user: &str, password: &str) -> Result<Auth, Self::Error> {
        Ok(if user == "kiki" && password == "secret" { Auth::Accept } else { Auth::Reject { proceed_with_methods: None, partial_success: false } })
    }

    async fn channel_open_session(&mut self, channel: Channel<Msg>, _session: &mut Session) -> Result<bool, Self::Error> {
        self.channels.insert(channel.id(), channel);
        Ok(true)
    }

    async fn subsystem_request(&mut self, channel_id: ChannelId, name: &str, session: &mut Session) -> Result<(), Self::Error> {
        if name != "sftp" {
            return session.channel_failure(channel_id);
        }
        let channel = self.channels.remove(&channel_id).expect("channel");
        session.channel_success(channel_id)?;
        let h = SftpFs { fs: self.fs.clone(), cfg: self.cfg.clone(), counters: self.counters.clone(), cursors: HashMap::new() };
        let stream = Counting { inner: channel.into_stream(), counters: self.counters.clone(), wbuf: Vec::new() };
        tokio::spawn(russh_sftp::server::run(stream, h));
        Ok(())
    }

    /// Called for every channel data packet as it comes off the socket, before the SFTP task
    /// gets to it: READ requests counted here minus replies written = requests in flight.
    async fn data(&mut self, channel_id: ChannelId, data: &[u8], _session: &mut Session) -> Result<(), Self::Error> {
        let buf = self.inbound.entry(channel_id).or_default();
        buf.extend_from_slice(data);
        let counters = &self.counters;
        drain_packets(buf, |t| {
            if t == FXP_READ {
                let now = counters.inflight.fetch_add(1, Ordering::SeqCst) + 1;
                counters.max_inflight.fetch_max(now, Ordering::SeqCst);
            }
        });
        Ok(())
    }

    async fn exec_request(&mut self, channel_id: ChannelId, data: &[u8], session: &mut Session) -> Result<(), Self::Error> {
        self.counters.execs.fetch_add(1, Ordering::SeqCst);
        if self.cfg.exec == ExecMode::Refused {
            return session.channel_failure(channel_id);
        }
        let cmd = String::from_utf8_lossy(data).into_owned();
        let (out, status) = self.emulate(&cmd);
        session.channel_success(channel_id)?;
        for chunk in out.chunks(32 * 1024) {
            session.data(channel_id, CryptoVec::from(chunk.to_vec()))?;
        }
        session.exit_status_request(channel_id, status)?;
        session.eof(channel_id)?;
        session.close(channel_id)
    }
}

/// Wraps the SFTP channel's write side and counts DATA/STATUS replies, closing the in-flight
/// count opened by `SshSession::data`.
struct Counting {
    inner: russh::ChannelStream<Msg>,
    counters: Arc<Counters>,
    wbuf: Vec<u8>,
}

const FXP_READ: u8 = 5;
const FXP_STATUS: u8 = 101;
const FXP_DATA: u8 = 103;

fn drain_packets(buf: &mut Vec<u8>, mut on_type: impl FnMut(u8)) {
    loop {
        if buf.len() < 5 {
            return;
        }
        let len = u32::from_be_bytes([buf[0], buf[1], buf[2], buf[3]]) as usize;
        if buf.len() < 4 + len {
            return;
        }
        on_type(buf[4]);
        buf.drain(..4 + len);
    }
}

impl tokio::io::AsyncRead for Counting {
    fn poll_read(mut self: std::pin::Pin<&mut Self>, cx: &mut std::task::Context<'_>, buf: &mut tokio::io::ReadBuf<'_>) -> std::task::Poll<std::io::Result<()>> {
        std::pin::Pin::new(&mut self.inner).poll_read(cx, buf)
    }
}

impl tokio::io::AsyncWrite for Counting {
    fn poll_write(mut self: std::pin::Pin<&mut Self>, cx: &mut std::task::Context<'_>, data: &[u8]) -> std::task::Poll<std::io::Result<usize>> {
        let r = std::pin::Pin::new(&mut self.inner).poll_write(cx, data);
        if let std::task::Poll::Ready(Ok(n)) = &r {
            self.wbuf.extend_from_slice(&data[..*n]);
            let counters = self.counters.clone();
            drain_packets(&mut self.wbuf, |t| {
                if t == FXP_DATA || t == FXP_STATUS {
                    let _ = counters.inflight.fetch_update(Ordering::SeqCst, Ordering::SeqCst, |v| Some(v.saturating_sub(1)));
                }
            });
        }
        r
    }
    fn poll_flush(mut self: std::pin::Pin<&mut Self>, cx: &mut std::task::Context<'_>) -> std::task::Poll<std::io::Result<()>> {
        std::pin::Pin::new(&mut self.inner).poll_flush(cx)
    }
    fn poll_shutdown(mut self: std::pin::Pin<&mut Self>, cx: &mut std::task::Context<'_>) -> std::task::Poll<std::io::Result<()>> {
        std::pin::Pin::new(&mut self.inner).poll_shutdown(cx)
    }
}

struct SftpFs {
    fs: Fs,
    cfg: Arc<MockCfg>,
    counters: Arc<Counters>,
    /// opendir handle -> next index
    cursors: HashMap<String, usize>,
}

fn status(id: u32, code: StatusCode) -> Status {
    Status { id, status_code: code, error_message: String::new(), language_tag: "en".into() }
}

impl russh_sftp::server::Handler for SftpFs {
    type Error = StatusCode;

    fn unimplemented(&self) -> Self::Error {
        StatusCode::OpUnsupported
    }

    async fn open(&mut self, id: u32, filename: String, pflags: OpenFlags, _attrs: FileAttributes) -> Result<Handle, Self::Error> {
        let mut fs = self.fs.lock().unwrap();
        if pflags.contains(OpenFlags::CREATE) {
            fs.insert(filename.clone(), Node { kind: NodeKind::File(Vec::new()), mode: 0o644, mtime: 1_700_000_100 });
        } else if !matches!(fs.get(&filename).map(|n| &n.kind), Some(NodeKind::File(_))) {
            return Err(StatusCode::NoSuchFile);
        }
        Ok(Handle { id, handle: format!("f:{filename}") })
    }

    async fn close(&mut self, id: u32, handle: String) -> Result<Status, Self::Error> {
        self.cursors.remove(&handle);
        Ok(status(id, StatusCode::Ok))
    }

    async fn read(&mut self, id: u32, handle: String, offset: u64, len: u32) -> Result<Data, Self::Error> {
        self.counters.reads.fetch_add(1, Ordering::SeqCst);
        let fs = self.fs.lock().unwrap();
        let Some(Node { kind: NodeKind::File(b), .. }) = fs.get(&handle[2..]) else { return Err(StatusCode::NoSuchFile) };
        if offset as usize >= b.len() {
            return Err(StatusCode::Eof);
        }
        let end = (offset as usize + len as usize).min(b.len());
        Ok(Data { id, data: b[offset as usize..end].to_vec() })
    }

    async fn write(&mut self, id: u32, handle: String, offset: u64, data: Vec<u8>) -> Result<Status, Self::Error> {
        let mut fs = self.fs.lock().unwrap();
        let Some(Node { kind: NodeKind::File(b), .. }) = fs.get_mut(&handle[2..]) else { return Err(StatusCode::NoSuchFile) };
        let end = offset as usize + data.len();
        if b.len() < end {
            b.resize(end, 0);
        }
        b[offset as usize..end].copy_from_slice(&data);
        Ok(status(id, StatusCode::Ok))
    }

    async fn lstat(&mut self, id: u32, path: String) -> Result<Attrs, Self::Error> {
        let fs = self.fs.lock().unwrap();
        fs.get(&path).map(|n| Attrs { id, attrs: attrs(n) }).ok_or(StatusCode::NoSuchFile)
    }

    async fn stat(&mut self, id: u32, path: String) -> Result<Attrs, Self::Error> {
        self.lstat(id, path).await
    }

    async fn fstat(&mut self, id: u32, handle: String) -> Result<Attrs, Self::Error> {
        self.lstat(id, handle[2..].to_string()).await
    }

    async fn setstat(&mut self, id: u32, path: String, a: FileAttributes) -> Result<Status, Self::Error> {
        let mut fs = self.fs.lock().unwrap();
        let n = fs.get_mut(&path).ok_or(StatusCode::NoSuchFile)?;
        if let Some(t) = a.mtime {
            n.mtime = t;
        }
        if let Some(p) = a.permissions {
            n.mode = p & 0o7777;
        }
        Ok(status(id, StatusCode::Ok))
    }

    async fn fsetstat(&mut self, id: u32, handle: String, a: FileAttributes) -> Result<Status, Self::Error> {
        self.setstat(id, handle[2..].to_string(), a).await
    }

    async fn opendir(&mut self, id: u32, path: String) -> Result<Handle, Self::Error> {
        let fs = self.fs.lock().unwrap();
        if !matches!(fs.get(&path).map(|n| &n.kind), Some(NodeKind::Dir)) {
            return Err(StatusCode::NoSuchFile);
        }
        let h = format!("d:{path}");
        self.cursors.insert(h.clone(), 0);
        Ok(Handle { id, handle: h })
    }

    async fn readdir(&mut self, id: u32, handle: String) -> Result<Name, Self::Error> {
        self.counters.readdir.fetch_add(1, Ordering::SeqCst);
        let fs = self.fs.lock().unwrap();
        let list = children(&fs, &handle[2..], false);
        let pos = *self.cursors.get(&handle).ok_or(StatusCode::BadMessage)?;
        if pos >= list.len() {
            return Err(StatusCode::Eof);
        }
        let page: Vec<File> = list[pos..(pos + self.cfg.page).min(list.len())].iter().map(|(p, n)| File::new(p.rsplit('/').next().unwrap(), attrs(n))).collect();
        self.cursors.insert(handle, pos + page.len());
        Ok(Name { id, files: page })
    }

    async fn remove(&mut self, id: u32, filename: String) -> Result<Status, Self::Error> {
        let mut fs = self.fs.lock().unwrap();
        match fs.get(&filename).map(|n| &n.kind) {
            Some(NodeKind::Dir) | None => Err(StatusCode::NoSuchFile),
            _ => {
                fs.remove(&filename);
                Ok(status(id, StatusCode::Ok))
            }
        }
    }

    async fn mkdir(&mut self, id: u32, path: String, _attrs: FileAttributes) -> Result<Status, Self::Error> {
        let mut fs = self.fs.lock().unwrap();
        if fs.contains_key(&path) {
            return Err(StatusCode::Failure);
        }
        fs.insert(path, Node { kind: NodeKind::Dir, mode: 0o755, mtime: 1_700_000_200 });
        Ok(status(id, StatusCode::Ok))
    }

    async fn rmdir(&mut self, id: u32, path: String) -> Result<Status, Self::Error> {
        let mut fs = self.fs.lock().unwrap();
        if !children(&fs, &path, false).is_empty() {
            return Err(StatusCode::Failure);
        }
        fs.remove(&path).map(|_| status(id, StatusCode::Ok)).ok_or(StatusCode::NoSuchFile)
    }

    async fn realpath(&mut self, id: u32, path: String) -> Result<Name, Self::Error> {
        Ok(Name { id, files: vec![File::dummy(if path.is_empty() || path == "." { "/" } else { &path })] })
    }

    async fn rename(&mut self, id: u32, oldpath: String, newpath: String) -> Result<Status, Self::Error> {
        let mut fs = self.fs.lock().unwrap();
        let n = fs.remove(&oldpath).ok_or(StatusCode::NoSuchFile)?;
        fs.insert(newpath, n);
        Ok(status(id, StatusCode::Ok))
    }
}

fn start(exec: ExecMode, fail: ExecFail, page: usize) -> Mock {
    let fs = fixture();
    let counters = Arc::new(Counters::default());
    let std_listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = std_listener.local_addr().unwrap().port();
    std_listener.set_nonblocking(true).unwrap();
    let server = MockServer { fs: fs.clone(), cfg: Arc::new(MockCfg { exec, fail, page }), counters: counters.clone() };
    let thread = std::thread::spawn(move || {
        let rt = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
        rt.block_on(async move {
            use russh::server::Server as _;
            let config = Arc::new(russh::server::Config {
                auth_rejection_time: Duration::from_millis(1),
                auth_rejection_time_initial: Some(Duration::ZERO),
                keys: vec![PrivateKey::random(&mut rand::thread_rng(), Algorithm::Ed25519).unwrap()],
                ..Default::default()
            });
            let listener = tokio::net::TcpListener::from_std(std_listener).unwrap();
            let mut server = server;
            let _ = server.run_on_socket(config, &listener).await;
        });
    });
    Mock { port, fs, counters, _thread: thread }
}

// ---------------------------------------------------------------- plugin driver

struct Plugin {
    child: Child,
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
    next: u64,
}

impl Plugin {
    fn spawn() -> Plugin {
        let mut child = Command::new(env!("CARGO_BIN_EXE_kiki-plugin-sftp")).stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::inherit()).spawn().expect("spawn plugin");
        let stdin = child.stdin.take().unwrap();
        let stdout = BufReader::new(child.stdout.take().unwrap());
        Plugin { child, stdin, stdout, next: 1 }
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

    fn connect(&mut self, port: u16, password: &str) -> Value {
        let cfg = Value::obj().s("host", "127.0.0.1").s("port", port.to_string()).s("username", "kiki").done();
        let (_, r) = self.req(Value::obj().s("type", "Connect").s("location", "lab").s("role", "browse").v("config", cfg).v("secrets", Value::obj().s("password", password).done()).done());
        r
    }

    fn scan(&mut self, path: &str, recursive: bool) -> Vec<Value> {
        let (frames, r) = self.req(Value::obj().s("type", "Scan").s("location", "lab").s("path", path).b("recursive", recursive).done());
        assert!(r.get("ok").is_some(), "scan failed: {}", json::to_string(&r));
        let entries: Vec<Value> = frames.iter().flat_map(|f| f.get("entries").and_then(Value::as_arr).unwrap_or(&[]).to_vec()).collect();
        assert_eq!(r.get("ok").unwrap().u64_field("n"), Some(entries.len() as u64), "n must equal the streamed entry count");
        entries
    }

    fn read(&mut self, path: &str, offset: u64) -> Vec<u8> {
        let id = self.next;
        self.next += 1;
        write_json(&mut self.stdin, &Value::obj().u("id", id).s("type", "Read").s("location", "lab").s("path", path).u("offset", offset).done()).unwrap();
        let mut bytes = Vec::new();
        loop {
            let (kind, payload) = read_frame(&mut self.stdout).unwrap().expect("eof");
            assert_eq!(kind, 1);
            if payload.is_empty() {
                break;
            }
            bytes.extend_from_slice(&payload);
        }
        let (kind, payload) = read_frame(&mut self.stdout).unwrap().expect("eof");
        assert_eq!(kind, 0);
        let r = json::parse(&payload).unwrap();
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

/// Listing shape that both scan paths must agree on.
fn normalise(entries: &[Value]) -> Vec<(String, String, u64, u64, u64, String)> {
    let mut v: Vec<_> = entries
        .iter()
        .map(|e| {
            let m = e.get("meta").expect("meta inline");
            (e.str_field("name").unwrap().to_string(), e.str_field("kind").unwrap().to_string(), m.u64_field("size").unwrap(), m.u64_field("mtime").unwrap(), m.u64_field("mode").unwrap(), e.str_field("rel").unwrap_or("").to_string())
        })
        .collect();
    v.sort();
    v
}

fn fast_scan(p: &mut Plugin) -> String {
    p.ok(Value::obj().s("type", "Capabilities").s("location", "lab").done()).str_field("fastScan").unwrap().to_string()
}

fn connected(exec: ExecMode, fail: ExecFail, page: usize) -> (Mock, Plugin) {
    let m = start(exec, fail, page);
    let mut p = Plugin::spawn();
    let r = p.connect(m.port, "secret");
    assert!(r.get("ok").is_some(), "connect: {}", json::to_string(&r));
    assert!(r.get("ok").unwrap().str_field("fingerprint").is_some(), "fingerprint reported on first use");
    (m, p)
}

// ---------------------------------------------------------------- tests

#[test]
fn gnu_fast_scan_lists_in_one_round_trip_and_matches_readdir() {
    let (m, mut p) = connected(ExecMode::Gnu, ExecFail::None, 2);
    assert_eq!(fast_scan(&mut p), "gnu");
    let root_fast = p.scan("/", false);
    assert_eq!(m.counters.readdir.load(Ordering::SeqCst), 0, "GNU mode must not touch READDIR");
    let names: Vec<&str> = root_fast.iter().map(|e| e.str_field("name").unwrap()).collect();
    assert!(names.contains(&"we ird'na\nme.txt"), "newline, quote and space survive: {names:?}");
    let link = root_fast.iter().find(|e| e.str_field("name") == Some("link")).unwrap();
    assert_eq!(link.str_field("kind"), Some("link"), "symlinks are listed as links, never followed");

    // Whole tree in one exec: rel paths, no name collisions.
    let tree = p.scan("/docs", true);
    let rels: Vec<&str> = tree.iter().map(|e| e.str_field("rel").unwrap()).collect();
    assert_eq!(rels, vec!["readme.md", "sub", "sub/deep.txt"]);

    // Same server, exec refused: READDIR gives an identical listing, paged 2 at a time.
    let (m2, mut q) = connected(ExecMode::Refused, ExecFail::None, 2);
    assert_eq!(fast_scan(&mut q), "none");
    let root_slow = q.scan("/", false);
    assert_eq!(normalise(&root_fast), normalise(&root_slow));
    assert!(m2.counters.readdir.load(Ordering::SeqCst) >= 3, "5 entries at 2 per page");
    let (_, r) = q.req(Value::obj().s("type", "Scan").s("location", "lab").s("path", "/docs").b("recursive", true).done());
    assert_eq!(r.get("err").unwrap().str_field("code"), Some("Unsupported"), "recursive scan needs exec; the daemon walks per directory instead");
}

#[test]
fn posix_stat_fallbacks_match_readdir() {
    let (_m0, mut q) = connected(ExecMode::Refused, ExecFail::None, 100);
    let reference = normalise(&q.scan("/", false));
    let reference_tree = normalise(&q_tree(&mut q));
    for mode in [ExecMode::StatC, ExecMode::StatF] {
        let (m, mut p) = connected(mode, ExecFail::None, 100);
        assert_eq!(fast_scan(&mut p), "posix", "{mode:?}");
        assert_eq!(normalise(&p.scan("/", false)), reference, "{mode:?}");
        assert_eq!(m.counters.readdir.load(Ordering::SeqCst), 0, "{mode:?} must not touch READDIR");
        assert_eq!(normalise(&p.scan("/docs", true)), reference_tree, "{mode:?} recursive");
    }
    // exec works but there is no find at all
    let (_m, mut p) = connected(ExecMode::NoFind, ExecFail::None, 100);
    assert_eq!(fast_scan(&mut p), "none");
    assert_eq!(normalise(&p.scan("/", false)), reference);
}

/// The reference tree for a READDIR-only server: walk it by hand, one Scan per directory.
fn q_tree(q: &mut Plugin) -> Vec<Value> {
    let mut out = Vec::new();
    for (dir, prefix) in [("/docs", ""), ("/docs/sub", "sub/")] {
        for mut e in q.scan(dir, false) {
            let rel = format!("{prefix}{}", e.str_field("name").unwrap());
            if let Value::Obj(m) = &mut e {
                m.insert("rel".into(), Value::Str(rel));
            }
            out.push(e);
        }
    }
    out
}

#[test]
fn killed_exec_stream_falls_back_without_duplicates() {
    let (m, mut p) = connected(ExecMode::Gnu, ExecFail::KillAfter(40), 100);
    assert_eq!(fast_scan(&mut p), "gnu");
    let listing = p.scan("/", false);
    let names: Vec<&str> = listing.iter().map(|e| e.str_field("name").unwrap()).collect();
    let mut dedup = names.clone();
    dedup.sort();
    dedup.dedup();
    assert_eq!(dedup.len(), names.len(), "no duplicate entries after fallback: {names:?}");
    assert_eq!(names.len(), 5, "complete listing over READDIR: {names:?}");
    assert!(m.counters.readdir.load(Ordering::SeqCst) >= 1);
    // Fast scan is off for the rest of the session: the next listing goes straight to READDIR.
    assert_eq!(fast_scan(&mut p), "none");
    let execs = m.counters.execs.load(Ordering::SeqCst);
    p.scan("/docs", false);
    assert_eq!(m.counters.execs.load(Ordering::SeqCst), execs, "no further exec attempts");
}

#[test]
fn find_exit_status_one_is_still_a_listing() {
    let (m, mut p) = connected(ExecMode::Gnu, ExecFail::ExitOne, 100);
    let listing = p.scan("/", false);
    assert_eq!(listing.len(), 5);
    assert_eq!(m.counters.readdir.load(Ordering::SeqCst), 0);
    assert_eq!(fast_scan(&mut p), "gnu", "an unreadable entry does not disable fast scan");
}

#[test]
fn reads_are_pipelined_and_exact() {
    let (m, mut p) = connected(ExecMode::Refused, ExecFail::None, 100);
    let expect = pseudo_random(4 * 1024 * 1024 + 12_345);
    let got = p.read("/data.bin", 0);
    assert_eq!(got.len(), expect.len());
    assert!(got == expect, "bytes differ");
    let reads = m.counters.reads.load(Ordering::SeqCst);
    assert_eq!(reads, 17, "256 KiB chunks: 16 full + 1 short");
    assert!(m.counters.max_inflight.load(Ordering::SeqCst) >= 8, "requests overlap on the wire: max in flight {}", m.counters.max_inflight.load(Ordering::SeqCst));
    // Resume from an offset (partialRead)
    let tail = p.read("/data.bin", 4 * 1024 * 1024);
    assert_eq!(tail, &expect[4 * 1024 * 1024..]);
    // Small file, and a missing one is a typed error
    assert_eq!(p.read("/docs/readme.md", 0), b"hello");
    let id = p.next;
    p.next += 1;
    write_json(&mut p.stdin, &Value::obj().u("id", id).s("type", "Read").s("location", "lab").s("path", "/nope").done()).unwrap();
    let (kind, payload) = read_frame(&mut p.stdout).unwrap().unwrap();
    assert_eq!((kind, payload.len()), (1, 0), "end marker even on failure");
    let (_, payload) = read_frame(&mut p.stdout).unwrap().unwrap();
    assert_eq!(json::parse(&payload).unwrap().get("err").unwrap().str_field("code"), Some("NotFound"));
}

#[test]
fn write_and_metadata_operations_round_trip() {
    let (m, mut p) = connected(ExecMode::Gnu, ExecFail::None, 100);
    let data = pseudo_random(1_000_003);
    let r = p.write("/upload.bin", &data, 1_600_000_000_000);
    assert_eq!(r.get("ok").unwrap().u64_field("bytes"), Some(data.len() as u64), "{}", json::to_string(&r));
    {
        let fs = m.fs.lock().unwrap();
        let n = fs.get("/upload.bin").unwrap();
        assert!(matches!(&n.kind, NodeKind::File(b) if *b == data));
        assert_eq!(n.mtime, 1_600_000_000, "mtime applied after the write");
    }
    let st = p.ok(Value::obj().s("type", "Stat").s("location", "lab").s("path", "/upload.bin").done());
    assert_eq!(st.u64_field("size"), Some(data.len() as u64));
    assert_eq!(st.u64_field("mtime"), Some(1_600_000_000_000));

    p.ok(Value::obj().s("type", "Mkdir").s("location", "lab").s("path", "/made").done());
    p.ok(Value::obj().s("type", "Rename").s("location", "lab").s("from", "/upload.bin").s("to", "/made/upload.bin").done());
    p.ok(Value::obj().s("type", "Chmod").s("location", "lab").s("path", "/made/upload.bin").u("mode", 0o600).done());
    p.ok(Value::obj().s("type", "SetMtime").s("location", "lab").s("path", "/made/upload.bin").u("mtime", 1_500_000_000_000).done());
    let st = p.ok(Value::obj().s("type", "Stat").s("location", "lab").s("path", "/made/upload.bin").done());
    assert_eq!(st.u64_field("mode"), Some(0o600));
    assert_eq!(st.u64_field("mtime"), Some(1_500_000_000_000));
    // The fast listing sees the same
    let made = p.scan("/made", false);
    assert_eq!(made.len(), 1);
    assert_eq!(made[0].get("meta").unwrap().u64_field("mode"), Some(0o600));

    p.ok(Value::obj().s("type", "Delete").s("location", "lab").s("path", "/made/upload.bin").done());
    p.ok(Value::obj().s("type", "Delete").s("location", "lab").s("path", "/made").done());
    assert!(!m.fs.lock().unwrap().contains_key("/made"));
    let (_, r) = p.req(Value::obj().s("type", "Stat").s("location", "lab").s("path", "/made").done());
    assert_eq!(r.get("err").unwrap().str_field("code"), Some("NotFound"));
}

#[test]
fn wrong_password_is_an_auth_error_and_validate_checks_fields() {
    let m = start(ExecMode::Gnu, ExecFail::None, 100);
    let mut p = Plugin::spawn();
    let r = p.connect(m.port, "nope");
    assert_eq!(r.get("err").unwrap().str_field("code"), Some("Auth"), "{}", json::to_string(&r));
    let (_, r) = p.req(Value::obj().s("type", "Validate").v("config", Value::obj().s("host", "h").s("username", "u").s("port", "70000").done()).done());
    assert_eq!(r.get("err").unwrap().str_field("field"), Some("port"));
    let (_, r) = p.req(Value::obj().s("type", "Validate").v("config", Value::obj().s("host", "").s("username", "u").done()).done());
    assert_eq!(r.get("err").unwrap().str_field("field"), Some("host"));
    let d = p.ok(Value::obj().s("type", "Describe").done());
    assert_eq!(d.str_field("scheme"), Some("sftp"));
    assert_eq!(d.get("features").unwrap().get("pipelining").and_then(Value::as_bool), Some(true));
}
