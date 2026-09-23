//! The kiki location plugin SDK. Implement [`Handler`], call [`run`], done.
//! Wire format and message semantics: docs/0.1.0/API-PLUGIN.md.

pub use kiki_json as json;
use kiki_json::Value;
use std::collections::HashMap;
use std::io::{self, Read, Write};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

#[derive(Debug)]
pub struct PluginError {
    pub code: &'static str,
    pub message: String,
    pub field: Option<String>,
}

impl PluginError {
    pub fn new(code: &'static str, message: impl Into<String>) -> Self {
        PluginError { code, message: message.into(), field: None }
    }
    pub fn invalid(field: &str, message: impl Into<String>) -> Self {
        PluginError { code: "Invalid", message: message.into(), field: Some(field.to_string()) }
    }
    pub fn not_found() -> Self {
        Self::new("NotFound", "not found")
    }
    pub fn unsupported() -> Self {
        Self::new("Unsupported", "not supported")
    }
    pub fn network(m: impl Into<String>) -> Self {
        Self::new("Network", m)
    }
    pub fn auth(m: impl Into<String>) -> Self {
        Self::new("Auth", m)
    }
    pub fn io(m: impl std::fmt::Display) -> Self {
        Self::new("Io", m.to_string())
    }
}

impl From<io::Error> for PluginError {
    fn from(e: io::Error) -> Self {
        match e.kind() {
            io::ErrorKind::NotFound => Self::not_found(),
            io::ErrorKind::PermissionDenied => Self::new("Denied", e.to_string()),
            io::ErrorKind::AlreadyExists => Self::new("Exists", e.to_string()),
            _ => Self::io(e),
        }
    }
}

pub type Result<T> = std::result::Result<T, PluginError>;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Dir,
    File,
    Link,
    Other,
}

#[derive(Clone, Debug, Default)]
pub struct Meta {
    /// The backend's own hidden flag (DOS Hidden on SMB); the daemon filters it like a dot-file.
    pub hidden: bool,
    pub size: u64,
    pub mtime_ms: u64,
    pub mode: Option<u32>,
    pub owner: Option<String>,
    pub group: Option<String>,
}

impl Meta {
    pub fn to_json(&self) -> Value {
        let mut o = Value::obj()
            .u("size", self.size)
            .u("mtime", self.mtime_ms)
            .v("mode", self.mode.map(|m| Value::Uint(m as u64)).unwrap_or(Value::Null))
            .opt_s("owner", self.owner.as_deref())
            .opt_s("group", self.group.as_deref())
            .v("digest", Value::Null);
        if self.hidden {
            o = o.b("hidden", true);
        }
        o.done()
    }
}

#[derive(Clone, Debug)]
pub struct Entry {
    pub name: String,
    pub kind: Kind,
    pub meta: Option<Meta>,
    /// Path relative to the scanned root, for recursive scans; empty otherwise.
    pub rel: String,
}

/// A form field as the Add-location dialog renders it.
pub fn field(key: &str, label: &str, kind: &str, required: bool, default: Option<&str>) -> Value {
    Value::obj().s("key", key).s("label", label).s("kind", kind).b("required", required).opt_s("default", default).v("options", Value::Null).v("group", Value::Null).done()
}

/// A field that belongs to one of the form's tabs. Fields sharing a `group` are shown together
/// under a tab of that name, and only the chosen tab's fields are sent; which tab was chosen
/// arrives in the config as `auth` (the group's name, lower-cased).
pub fn field_in(group: &str, key: &str, label: &str, kind: &str, required: bool, default: Option<&str>) -> Value {
    Value::obj().s("key", key).s("label", label).s("kind", kind).b("required", required).opt_s("default", default).v("options", Value::Null).s("group", group).done()
}

/// Put a field on one of the form's pages. Pages are sections of one form — "Connection",
/// "Locations" — shown one at a time but ALL sent; they are not alternatives the way a `group`'s
/// tabs are. A field with no page is on the first one.
pub fn on_page(page: &str, field: Value) -> Value {
    match field {
        Value::Obj(mut m) => {
            m.insert("page".to_string(), Value::Str(page.to_string()));
            Value::Obj(m)
        }
        other => other,
    }
}

pub fn select_field(key: &str, label: &str, options: &[&str], default: &str) -> Value {
    Value::obj()
        .s("key", key)
        .s("label", label)
        .s("kind", "select")
        .b("required", true)
        .s("default", default)
        .v("options", Value::Arr(options.iter().map(|o| Value::Str(o.to_string())).collect()))
        .v("group", Value::Null)
        .done()
}

pub struct Features {
    pub set_mtime: bool,
    pub mode: bool,
    pub real_dirs: bool,
    pub meta_in_scan: bool,
    pub pipelining: bool,
    pub partial_read: bool,
}

pub struct Describe {
    pub scheme: &'static str,
    pub display_name: &'static str,
    pub version: &'static str,
    pub form: Vec<Value>,
    pub defaults: Value,
    pub secret_fields: Vec<&'static str>,
    pub detector_upload: &'static str,
    pub detector_download: &'static str,
    pub features: Features,
    /// `Some((false, reason))` when the plugin cannot work on this machine (a missing daemon or
    /// library); the dialog shows the reason instead of the form. `None` means available.
    pub available: Option<(bool, String)>,
}

impl Describe {
    fn to_json(&self) -> Value {
        Value::obj()
            .s("kind", "location")
            .s("scheme", self.scheme)
            .s("displayName", self.display_name)
            .s("version", self.version)
            .v("form", Value::Arr(self.form.clone()))
            .v("defaults", self.defaults.clone())
            .v("secretFields", Value::Arr(self.secret_fields.iter().map(|s| Value::Str(s.to_string())).collect()))
            .v("detector", Value::obj().s("upload", self.detector_upload).s("download", self.detector_download).done())
            .b("available", self.available.as_ref().map(|(a, _)| *a).unwrap_or(true))
            .s("unavailableReason", self.available.as_ref().map(|(_, r)| r.clone()).unwrap_or_default())
            .v(
                "features",
                Value::obj()
                    .b("setMtime", self.features.set_mtime)
                    .b("mode", self.features.mode)
                    .b("realDirs", self.features.real_dirs)
                    .v("digestKind", Value::Null)
                    .s("separator", "/")
                    .b("metaInScan", self.features.meta_in_scan)
                    .b("pipelining", self.features.pipelining)
                    .b("partialRead", self.features.partial_read)
                    .done(),
            )
            .done()
    }
}

/// Incoming bytes of a `Write`: binary frames routed from the reader thread until the empty
/// end frame.
pub struct Incoming {
    rx: std::sync::mpsc::Receiver<Vec<u8>>,
    buf: Vec<u8>,
    pos: usize,
    done: bool,
}

impl Read for Incoming {
    fn read(&mut self, out: &mut [u8]) -> io::Result<usize> {
        if self.pos >= self.buf.len() {
            if self.done {
                return Ok(0);
            }
            match self.rx.recv() {
                Ok(b) if b.is_empty() => {
                    self.done = true;
                    return Ok(0);
                }
                Ok(b) => {
                    self.buf = b;
                    self.pos = 0;
                }
                Err(_) => {
                    self.done = true;
                    return Ok(0);
                }
            }
        }
        let n = (self.buf.len() - self.pos).min(out.len());
        out[..n].copy_from_slice(&self.buf[self.pos..self.pos + n]);
        self.pos += n;
        Ok(n)
    }
}

/// The plugin's shared output pipe: every frame is written whole under the lock, so concurrent
/// requests interleave at frame granularity only.
pub type Shared = Mutex<Box<dyn Write + Send>>;

/// Outgoing bytes of a `Read`: `write` sends binary frames; the SDK sends the end frame. Writes
/// fail with `Interrupted` once the request has been cancelled.
pub struct Outgoing<'a> {
    out: &'a Shared,
    cancel: Arc<AtomicBool>,
    pub bytes: u64,
}

impl<'a> Write for Outgoing<'a> {
    fn write(&mut self, b: &[u8]) -> io::Result<usize> {
        if self.cancel.load(Ordering::Relaxed) {
            return Err(io::Error::new(io::ErrorKind::Interrupted, "cancelled"));
        }
        if !b.is_empty() {
            let mut o = self.out.lock().unwrap();
            for chunk in b.chunks(1024 * 1024) {
                write_binary(&mut **o, chunk)?;
            }
            self.bytes += b.len() as u64;
        }
        Ok(b.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        self.out.lock().unwrap().flush()
    }
}

pub struct WriteArgs {
    pub size: Option<u64>,
    pub mode: Option<u32>,
    pub mtime_ms: Option<u64>,
    pub data: Incoming,
}

thread_local! {
    static CANCEL: std::cell::RefCell<Option<Arc<AtomicBool>>> = const { std::cell::RefCell::new(None) };
    static ROLE: std::cell::RefCell<String> = const { std::cell::RefCell::new(String::new()) };
}

/// The role of the session the request this thread is serving was made on: `browse`, or a job's
/// own (`job-<id>`). A location can have several sessions open at once — the one the user is
/// browsing with, and one per transfer or mirror run — and every request names the one it means,
/// so a long upload neither queues behind a listing nor takes the browser's connection down with
/// it when it is cancelled. A plugin with one connection per location can ignore it.
pub fn current_role() -> String {
    ROLE.with(|r| {
        let r = r.borrow();
        if r.is_empty() {
            "browse".to_string()
        } else {
            r.clone()
        }
    })
}

/// For a plugin's own tests, which call its handler without going through `serve`.
pub fn set_current_role(role: &str) {
    ROLE.with(|r| *r.borrow_mut() = role.to_string());
}

/// True once the daemon has sent `Cancel` for the request this thread is serving. Long loops
/// (listings, transfers) check it and return early with any error; the SDK reports `Cancelled`.
pub fn cancelled() -> bool {
    CANCEL.with(|c| c.borrow().as_ref().map(|f| f.load(Ordering::Relaxed)).unwrap_or(false))
}

/// The plugin's own error for an interrupted request.
pub fn cancel_error() -> PluginError {
    PluginError::new("Cancelled", "cancelled")
}

/// Requests run concurrently on worker threads (up to `MAX_CONCURRENT`); handlers take `&self`
/// and keep their sessions behind their own locks. `Read` and `Thumb` streams are serialised so
/// binary frames never interleave.
#[allow(unused_variables)]
pub trait Handler: Send + Sync {
    fn describe(&self) -> Describe;
    fn validate(&self, config: &Value) -> Result<()>;
    /// Returns `{ fingerprint, banner }` fields as JSON (may be empty object).
    fn connect(&self, location: &str, role: &str, config: &Value, secrets: &Value) -> Result<Value>;
    fn disconnect(&self, location: &str, role: &str) {}
    fn capabilities(&self, location: &str) -> Result<Value>;
    fn scan(&self, location: &str, path: &str, recursive: bool, sink: &mut dyn FnMut(Vec<Entry>)) -> Result<u64>;
    fn stat(&self, location: &str, path: &str) -> Result<Meta>;
    fn read(&self, location: &str, path: &str, offset: u64, out: &mut Outgoing) -> Result<()>;
    fn write(&self, location: &str, path: &str, args: WriteArgs) -> Result<u64>;
    fn mkdir(&self, location: &str, path: &str) -> Result<()>;
    fn rename(&self, location: &str, from: &str, to: &str) -> Result<()>;
    fn delete(&self, location: &str, path: &str) -> Result<()>;
    fn set_mtime(&self, location: &str, path: &str, mtime_ms: u64) -> Result<()> {
        Err(PluginError::unsupported())
    }
    fn chmod(&self, location: &str, path: &str, mode: u32) -> Result<()> {
        Err(PluginError::unsupported())
    }
    /// Optional native thumbnail (plan 17); write JPEG or PNG bytes to `out`.
    fn thumb(&self, location: &str, path: &str, out: &mut Outgoing) -> Result<()> {
        Err(PluginError::unsupported())
    }
    /// Optional pick-list for a `browse` form field (plan 25): (value, label) pairs, e.g. the
    /// hosts on the network or the shares on a server, given what the user has typed so far.
    fn browse(&self, field: &str, config: &Value, secrets: &Value) -> Result<Vec<(String, String)>> {
        Err(PluginError::unsupported())
    }
}

// ---------------------------------------------------------------- framing

pub fn read_frame(stdin: &mut dyn Read) -> io::Result<Option<(u8, Vec<u8>)>> {
    let mut head = [0u8; 5];
    match stdin.read_exact(&mut head) {
        Ok(()) => {}
        Err(e) if e.kind() == io::ErrorKind::UnexpectedEof => return Ok(None),
        Err(e) => return Err(e),
    }
    let len = u32::from_le_bytes([head[0], head[1], head[2], head[3]]) as usize;
    if len > 16 * 1024 * 1024 {
        return Err(io::Error::new(io::ErrorKind::InvalidData, "frame too large"));
    }
    let mut payload = vec![0u8; len];
    stdin.read_exact(&mut payload)?;
    Ok(Some((head[4], payload)))
}

pub fn write_json(out: &mut dyn Write, v: &Value) -> io::Result<()> {
    let s = kiki_json::to_string(v);
    out.write_all(&(s.len() as u32).to_le_bytes())?;
    out.write_all(&[0u8])?;
    out.write_all(s.as_bytes())?;
    out.flush()
}

pub fn write_binary(out: &mut dyn Write, b: &[u8]) -> io::Result<()> {
    out.write_all(&(b.len() as u32).to_le_bytes())?;
    out.write_all(&[1u8])?;
    out.write_all(b)?;
    Ok(())
}

fn ok(id: u64, v: Value) -> Value {
    Value::obj().u("id", id).v("ok", v).done()
}

fn err(id: u64, e: &PluginError) -> Value {
    let mut o = Value::obj().s("code", e.code).s("message", e.message.clone());
    if let Some(f) = &e.field {
        o = o.s("field", f.clone());
    }
    Value::obj().u("id", id).v("err", o.done()).done()
}

fn entry_json(e: &Entry) -> Value {
    let mut o = Value::obj().s("name", e.name.clone()).s(
        "kind",
        match e.kind {
            Kind::Dir => "dir",
            Kind::File => "file",
            Kind::Link => "link",
            Kind::Other => "other",
        },
    );
    o = match &e.meta {
        Some(m) => o.v("meta", m.to_json()),
        None => o.v("meta", Value::Null),
    };
    if !e.rel.is_empty() {
        o = o.s("rel", e.rel.clone());
    }
    o.done()
}

pub const MAX_CONCURRENT: usize = 8;

fn emit(out: &Shared, v: &Value) -> io::Result<()> {
    let mut o = out.lock().unwrap();
    write_json(&mut **o, v)
}

// ---------------------------------------------------------------- the library log

/// For a plugin's own lines in a job's log: `sdk::log::info!(target: "kiki", …)`.
pub use log;

/// What the libraries under a plugin say as they work — russh, suppaftp, rustls, all through the
/// `log` facade — sent to the daemon as `Log` events, so that a job's log can show what the SSH or
/// FTP library was doing when a transfer went wrong. Nobody was listening before: every line was
/// dropped.
///
/// - **Level**: `Debug` while a job's session is open on this plugin, `Info` otherwise, and never
///   `Trace` — russh's trace level is packet dumps, enough to slow a transfer and drown the rest.
/// - **Whose line**: the role of the session the calling thread is serving (`current_role`). A
///   library's own background threads serve nobody; their lines carry no role and the daemon
///   files them by which jobs were using the plugin at the time.
/// - **Secrets** are taken out HERE, before the line leaves the process (`redact`): a log is
///   something people paste into bug reports.
pub mod liblog {
    use super::{emit, Shared, Value};
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::{Arc, OnceLock};

    static SINK: OnceLock<Arc<Shared>> = OnceLock::new();
    static JOB_SESSIONS: AtomicUsize = AtomicUsize::new(0);
    struct Forward;
    static FORWARD: Forward = Forward;

    /// Installed once per process, by `run_on`; a second call (tests run several loops) is a no-op.
    /// Only for a host that asked (`KIKI_PLUGIN_LOG`): `Log` events arrive between other frames,
    /// a binary stream's included, and a host that did not ask has no reason to expect them.
    pub(super) fn install(out: &Arc<Shared>) {
        if std::env::var_os("KIKI_PLUGIN_LOG").is_none() {
            return;
        }
        if SINK.set(Arc::clone(out)).is_ok() && log::set_logger(&FORWARD).is_ok() {
            log::set_max_level(log::LevelFilter::Debug);
        }
    }

    pub(super) fn job_session(opened: bool) {
        if opened {
            JOB_SESSIONS.fetch_add(1, Ordering::Relaxed);
        } else {
            let _ = JOB_SESSIONS.fetch_update(Ordering::Relaxed, Ordering::Relaxed, |n| Some(n.saturating_sub(1)));
        }
    }

    /// Libraries whose `Debug` is the wire, not the story. Measured on a real transfer: russh
    /// wrote 380 lines for six small files ("> msg type 94, len 128", "packet type 101") — a line
    /// or three per 32 KiB of a large file — and rustls six per FTP data connection. They are
    /// heard from `Info` up, which is where their warnings and errors are. suppaftp's `Debug` is
    /// the opposite: "Put file …", "Renaming … to …", a few lines a file, and stays.
    const WIRE_LEVEL: [&str; 5] = ["russh", "russh_sftp", "rustls", "tokio", "mio"];

    fn wanted(level: log::Level, target: &str) -> bool {
        let crate_name = target.split("::").next().unwrap_or(target);
        if WIRE_LEVEL.contains(&crate_name) {
            return level <= log::Level::Info;
        }
        level <= if JOB_SESSIONS.load(Ordering::Relaxed) > 0 { log::Level::Debug } else { log::Level::Info }
    }

    /// A secret is what follows `PASS ` (the FTP command, as the protocol spells it) or what
    /// follows a word that names one and then a `:` or `=` — `password=hunter2`,
    /// `passphrase: "x"`, `Authorization: Basic abc`. From there to the end of the line goes.
    /// A sentence that merely mentions a password ("password authentication failed") is left
    /// alone: that is exactly the line somebody needs to read.
    pub fn redact(line: &str) -> String {
        if let Some(at) = line.find("PASS ") {
            return format!("{}[redacted]", &line[..at + 5]);
        }
        const KEYS: [&str; 5] = ["password", "passphrase", "authorization", "secret", "token"];
        let lower = line.to_ascii_lowercase();
        for key in KEYS {
            let mut from = 0;
            while let Some(i) = lower[from..].find(key) {
                let after = from + i + key.len();
                let rest = &line[after..];
                let lead = rest.len() - rest.trim_start_matches(|c: char| c == '"' || c == '\'' || c.is_whitespace()).len();
                if rest[lead..].starts_with([':', '=']) {
                    let sep = after + lead + 1;
                    let gap = line[sep..].len() - line[sep..].trim_start_matches(|c: char| c == '"' || c == '\'' || c.is_whitespace()).len();
                    return format!("{}[redacted]", &line[..sep + gap]);
                }
                from = after;
            }
        }
        line.to_string()
    }

    impl log::Log for Forward {
        fn enabled(&self, m: &log::Metadata) -> bool {
            wanted(m.level(), m.target())
        }
        fn log(&self, r: &log::Record) {
            if !wanted(r.level(), r.target()) {
                return;
            }
            let Some(out) = SINK.get() else { return };
            let role = super::ROLE.with(|x| x.borrow().clone());
            let line = Value::obj().s("event", "Log").s("level", r.level().as_str().to_ascii_lowercase()).s("target", r.target()).s("message", redact(&r.args().to_string())).s("role", role).done();
            let _ = emit(out, &line);
        }
        fn flush(&self) {}
    }

    #[cfg(test)]
    mod tests {
        use super::redact;
        #[test]
        fn secrets_do_not_leave_the_plugin() {
            assert_eq!(redact("CMD PASS hunter2"), "CMD PASS [redacted]");
            assert_eq!(redact("sending password=hunter2 to host"), "sending password=[redacted]");
            assert_eq!(redact("key needs passphrase: \"open sesame\""), "key needs passphrase: \"[redacted]");
            assert_eq!(redact("Authorization: Basic dXNlcjpwYXNz"), "Authorization: [redacted]");
            assert_eq!(redact("USER kiki"), "USER kiki");
            // Mentioning a password is not holding one: this is the line someone needs to read.
            assert_eq!(redact("password authentication failed for gideon"), "password authentication failed for gideon");
            assert_eq!(redact("offering methods: publickey,password"), "offering methods: publickey,password");
            assert_eq!(redact(r#"{"user":"x","token" : "abc"}"#), r#"{"user":"x","token" : "[redacted]"#);
            assert_eq!(redact("226 Transfer complete"), "226 Transfer complete");
            // "PASSIVE" is not PASS-space, and must survive: it is most of an FTP conversation.
            assert_eq!(redact("227 Entering Passive Mode (127,0,0,1,4,1)"), "227 Entering Passive Mode (127,0,0,1,4,1)");
        }
    }
}

/// The dispatch loop over stdin/stdout. Runs until stdin closes or `Shutdown` arrives.
pub fn run(handler: &dyn Handler) -> io::Result<()> {
    let out: Arc<Shared> = Arc::new(Mutex::new(Box::new(io::stdout())));
    run_on(handler, Box::new(io::stdin()), &out)
}

/// The dispatch loop over any pipe pair (tests drive it with in-memory streams).
///
/// The calling thread reads frames; each request runs on its own scoped thread (at most
/// `MAX_CONCURRENT`), `Cancel { target }` flips that request's flag, and the binary frames of the
/// one `Write` in progress are routed to its `Incoming`.
pub fn run_on(handler: &dyn Handler, mut input: Box<dyn Read + Send>, out: &Arc<Shared>) -> io::Result<()> {
    liblog::install(out);
    let inflight: Mutex<HashMap<u64, Arc<AtomicBool>>> = Mutex::new(HashMap::new());
    let stream_lock: Mutex<()> = Mutex::new(());
    let slots = (Mutex::new(0usize), std::sync::Condvar::new());
    let mut active_write: Option<std::sync::mpsc::SyncSender<Vec<u8>>> = None;
    std::thread::scope(|scope| -> io::Result<()> {
        while let Some((kind, payload)) = read_frame(&mut *input)? {
            if kind != 0 {
                if let Some(tx) = &active_write {
                    let end = payload.is_empty();
                    let _ = tx.send(payload);
                    if end {
                        active_write = None;
                    }
                }
                continue;
            }
            let v = match kiki_json::parse(&payload) {
                Ok(v) => v,
                Err(_) => continue,
            };
            let id = v.u64_field("id").unwrap_or(0);
            let t = v.str_field("type").unwrap_or("").to_string();
            match t.as_str() {
                "Describe" => emit(out, &ok(id, handler.describe().to_json()))?,
                "Ping" => emit(out, &ok(id, Value::obj().done()))?,
                "Cancel" => {
                    if let Some(target) = v.u64_field("target") {
                        if let Some(f) = inflight.lock().unwrap().get(&target) {
                            f.store(true, Ordering::Relaxed);
                        }
                    }
                    emit(out, &ok(id, Value::obj().done()))?;
                }
                "Shutdown" => {
                    for f in inflight.lock().unwrap().values() {
                        f.store(true, Ordering::Relaxed);
                    }
                    emit(out, &ok(id, Value::obj().done()))?;
                    return Ok(());
                }
                _ => {
                    let cancel = Arc::new(AtomicBool::new(false));
                    inflight.lock().unwrap().insert(id, Arc::clone(&cancel));
                    let write_rx = if t == "Write" {
                        let (tx, rx) = std::sync::mpsc::sync_channel(8);
                        active_write = Some(tx);
                        Some(rx)
                    } else {
                        None
                    };
                    {
                        let (m, cv) = &slots;
                        let mut n = m.lock().unwrap();
                        while *n >= MAX_CONCURRENT {
                            n = cv.wait(n).unwrap();
                        }
                        *n += 1;
                    }
                    let (out, inflight, stream_lock, slots) = (Arc::clone(out), &inflight, &stream_lock, &slots);
                    scope.spawn(move || {
                        CANCEL.with(|c| *c.borrow_mut() = Some(Arc::clone(&cancel)));
                        set_current_role(v.str_field("role").unwrap_or("browse"));
                        let reply = dispatch(handler, &v, id, &t, write_rx, &out, stream_lock, &cancel);
                        let _ = emit(&out, &reply);
                        inflight.lock().unwrap().remove(&id);
                        let (m, cv) = slots;
                        *m.lock().unwrap() -= 1;
                        cv.notify_one();
                    });
                }
            }
        }
        Ok(())
    })
}

#[allow(clippy::too_many_arguments)]
fn dispatch(handler: &dyn Handler, v: &Value, id: u64, t: &str, write_rx: Option<std::sync::mpsc::Receiver<Vec<u8>>>, out: &Shared, stream_lock: &Mutex<()>, cancel: &Arc<AtomicBool>) -> Value {
    let loc = v.str_field("location").unwrap_or("").to_string();
    let path = v.str_field("path").unwrap_or("/").to_string();
    // What is being asked of the server, in words, for the job's log — the same for every plugin,
    // whatever its library does or does not say for itself. (Listing and stat are left out: a
    // browser does thousands, and they are not what anyone opens a transfer's log to find.)
    match t {
        "Connect" => log::info!(target: "kiki", "connect {loc} ({})", v.str_field("role").unwrap_or("browse")),
        "Disconnect" => log::info!(target: "kiki", "disconnect {loc} ({})", v.str_field("role").unwrap_or("browse")),
        "Write" => log::debug!(target: "kiki", "write {path} ({} bytes)", v.u64_field("size").unwrap_or(0)),
        "Read" => log::debug!(target: "kiki", "read {path}"),
        "Mkdir" => log::debug!(target: "kiki", "mkdir {path}"),
        "Delete" => log::debug!(target: "kiki", "delete {path}"),
        "Rename" => log::debug!(target: "kiki", "rename {} -> {}", v.str_field("from").unwrap_or(""), v.str_field("to").unwrap_or("")),
        _ => {}
    }
    let reply = match t {
        "Validate" => result(id, handler.validate(v.get("config").unwrap_or(&Value::Null))),
        "Browse" => match handler.browse(v.str_field("field").unwrap_or(""), v.get("config").unwrap_or(&Value::Null), v.get("secrets").unwrap_or(&Value::Null)) {
            Ok(opts) => ok(id, Value::obj().v("options", Value::Arr(opts.into_iter().map(|(value, label)| Value::obj().s("value", value).s("label", label).done()).collect())).done()),
            Err(e) => err(id, &e),
        },
        "Connect" => match handler.connect(&loc, v.str_field("role").unwrap_or("browse"), v.get("config").unwrap_or(&Value::Null), v.get("secrets").unwrap_or(&Value::Null)) {
            Ok(r) => {
                if v.str_field("role").is_some_and(|r| r.starts_with("job-")) {
                    liblog::job_session(true);
                }
                ok(id, r)
            }
            Err(e) => err(id, &e),
        },
        "Disconnect" => {
            if v.str_field("role").is_some_and(|r| r.starts_with("job-")) {
                liblog::job_session(false);
            }
            handler.disconnect(&loc, v.str_field("role").unwrap_or("browse"));
            ok(id, Value::obj().done())
        }
        "Capabilities" => match handler.capabilities(&loc) {
            Ok(c) => ok(id, c),
            Err(e) => err(id, &e),
        },
        "Scan" => {
            let recursive = v.get("recursive").and_then(Value::as_bool).unwrap_or(false);
            let r = handler.scan(&loc, &path, recursive, &mut |entries: Vec<Entry>| {
                if cancel.load(Ordering::Relaxed) {
                    return;
                }
                let frame = Value::obj().u("id", id).v("entries", Value::Arr(entries.iter().map(entry_json).collect())).done();
                let _ = emit(out, &frame);
            });
            match r {
                Ok(n) => ok(id, Value::obj().u("n", n).done()),
                Err(e) => err(id, &e),
            }
        }
        "Stat" => match handler.stat(&loc, &path) {
            Ok(m) => ok(id, m.to_json()),
            Err(e) => err(id, &e),
        },
        "Read" | "Thumb" => {
            let _stream = stream_lock.lock().unwrap();
            let offset = v.u64_field("offset").unwrap_or(0);
            let mut o = Outgoing { out, cancel: Arc::clone(cancel), bytes: 0 };
            let r = if t == "Read" { handler.read(&loc, &path, offset, &mut o) } else { handler.thumb(&loc, &path, &mut o) };
            let bytes = o.bytes;
            {
                let mut w = out.lock().unwrap();
                let _ = write_binary(&mut **w, &[]);
                let _ = w.flush();
            }
            match r {
                Ok(()) => ok(id, Value::obj().u("bytes", bytes).done()),
                Err(e) => err(id, &e),
            }
        }
        "Write" => {
            let rx = write_rx.expect("write channel");
            let args = WriteArgs { size: v.u64_field("size"), mode: v.u64_field("mode").map(|m| m as u32), mtime_ms: v.u64_field("mtime"), data: Incoming { rx, buf: Vec::new(), pos: 0, done: false } };
            match handler.write(&loc, &path, args) {
                Ok(n) => ok(id, Value::obj().u("bytes", n).done()),
                Err(e) => err(id, &e),
            }
        }
        "Mkdir" => result(id, handler.mkdir(&loc, &path)),
        "Rename" => result(id, handler.rename(&loc, v.str_field("from").unwrap_or(""), v.str_field("to").unwrap_or(""))),
        "Delete" => result(id, handler.delete(&loc, &path)),
        "SetMtime" => result(id, handler.set_mtime(&loc, &path, v.u64_field("mtime").unwrap_or(0))),
        "Chmod" => result(id, handler.chmod(&loc, &path, v.u64_field("mode").unwrap_or(0) as u32)),
        _ => err(id, &PluginError::unsupported()),
    };
    if cancel.load(Ordering::Relaxed) && reply.get("ok").is_some() && !matches!(t, "Validate" | "Connect" | "Disconnect" | "Capabilities" | "Stat") {
        return err(id, &cancel_error());
    }
    if cancel.load(Ordering::Relaxed) && reply.get("err").is_some() {
        return err(id, &cancel_error());
    }
    // A refusal is the line somebody is looking for.
    if let Some(e) = reply.get("err") {
        if e.str_field("code") != Some("Cancelled") {
            log::warn!(target: "kiki", "{t} {path}: {} {}", e.str_field("code").unwrap_or(""), e.str_field("message").unwrap_or(""));
        }
    }
    reply
}

fn result(id: u64, r: Result<()>) -> Value {
    match r {
        Ok(()) => ok(id, Value::obj().done()),
        Err(e) => err(id, &e),
    }
}

/// Single-quote a string for a POSIX shell (used by plugins that run remote commands).
pub fn shell_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', "'\\''"))
}

// ================================================================ share plugins (plan 18)

pub struct ShareDescribe {
    pub id: &'static str,
    pub name: &'static str,
    pub icon: &'static str,
    pub version: &'static str,
    pub accepts_files: bool,
    pub accepts_folders: bool,
    pub accepts_multiple: bool,
    pub max_bytes: Option<u64>,
    /// "list" | "search" | "none"
    pub targets: &'static str,
    pub form: Vec<Value>,
    pub secret_fields: Vec<&'static str>,
    pub compose: Vec<Value>,
    /// Programs this plugin cannot work without (`tailscale`). The daemon looks for them each
    /// time it lists the plugins, so the menu can show the entry dimmed, saying what is missing,
    /// rather than offer something that fails — and it comes alive when the program is installed,
    /// without a restart.
    pub requires: Vec<&'static str>,
}

impl ShareDescribe {
    fn to_json(&self) -> Value {
        Value::obj()
            .s("kind", "share")
            .s("id", self.id)
            .s("name", self.name)
            .s("icon", self.icon)
            .s("version", self.version)
            .v(
                "accepts",
                Value::obj().b("files", self.accepts_files).b("folders", self.accepts_folders).b("multiple", self.accepts_multiple).v("maxBytes", self.max_bytes.map(Value::Uint).unwrap_or(Value::Null)).done(),
            )
            .s("targets", self.targets)
            .v("form", Value::Arr(self.form.clone()))
            .v("secretFields", Value::Arr(self.secret_fields.iter().map(|s| Value::Str(s.to_string())).collect()))
            .v("compose", Value::Arr(self.compose.clone()))
            .v("requires", Value::Arr(self.requires.iter().map(|s| Value::Str(s.to_string())).collect()))
            .done()
    }
}

pub struct Target {
    pub id: String,
    pub name: String,
    pub detail: String,
    pub online: bool,
    pub icon: String,
}

pub struct ShareProgress<'a> {
    pub id: u64,
    out: &'a mut dyn Write,
}

impl<'a> ShareProgress<'a> {
    pub fn report(&mut self, done: u64, total: u64, bytes: u64, bytes_total: u64, status: &str) {
        let _ = write_json(self.out, &Value::obj().u("id", self.id).s("event", "Progress").u("done", done).u("total", total).u("bytes", bytes).u("bytesTotal", bytes_total).s("status", status).done());
    }
}

pub struct ShareResult {
    /// "sent" | "opened" | "queued"
    pub result: &'static str,
    pub detail: Option<String>,
}

#[allow(unused_variables)]
pub trait ShareHandler {
    fn describe(&self) -> ShareDescribe;
    fn configure(&mut self, config: &Value, secrets: &Value) -> Result<()> {
        Ok(())
    }
    fn targets(&mut self, config: &Value, secrets: &Value, query: Option<&str>) -> Result<Vec<Target>> {
        Ok(Vec::new())
    }
    /// `files` are local paths (the daemon has already fetched remote files and zipped folders unless accepted).
    fn share(&mut self, config: &Value, secrets: &Value, files: &[String], target: Option<&str>, compose: &Value, progress: &mut ShareProgress) -> Result<ShareResult>;
}

pub fn detected(bin: &str) -> bool {
    std::env::var_os("PATH").map(|p| std::env::split_paths(&p).any(|d| d.join(bin).is_file())).unwrap_or(false)
}

/// The dispatch loop for a share plugin.
pub fn run_share(handler: &mut dyn ShareHandler) -> io::Result<()> {
    let stdin = io::stdin();
    let stdout = io::stdout();
    let mut stdin = stdin.lock();
    let mut stdout = stdout.lock();
    while let Some((kind, payload)) = read_frame(&mut stdin)? {
        if kind != 0 {
            continue;
        }
        let v = match kiki_json::parse(&payload) {
            Ok(v) => v,
            Err(_) => continue,
        };
        let id = v.u64_field("id").unwrap_or(0);
        let config = v.get("config").cloned().unwrap_or(Value::Null);
        let secrets = v.get("secrets").cloned().unwrap_or(Value::Null);
        let reply = match v.str_field("type").unwrap_or("") {
            "Describe" => ok(id, handler.describe().to_json()),
            "Ping" => ok(id, Value::obj().done()),
            "Shutdown" => {
                write_json(&mut stdout, &ok(id, Value::obj().done()))?;
                return Ok(());
            }
            "Configure" => result(id, handler.configure(&config, &secrets)),
            "Targets" => match handler.targets(&config, &secrets, v.str_field("query")) {
                Ok(t) => {
                    ok(id, Value::obj().v("targets", Value::Arr(t.into_iter().map(|t| Value::obj().s("id", t.id).s("name", t.name).s("detail", t.detail).b("online", t.online).s("icon", t.icon).done()).collect())).done())
                }
                Err(e) => err(id, &e),
            },
            "Share" => {
                let files: Vec<String> = v
                    .get("uris")
                    .and_then(Value::as_arr)
                    .map(|a| a.iter().filter_map(|u| u.as_str()).map(|u| if let Some(p) = u.strip_prefix("file://") { percent_decode(p) } else { u.to_string() }).collect())
                    .unwrap_or_default();
                let compose = v.get("compose").cloned().unwrap_or(Value::Null);
                let r = {
                    let mut p = ShareProgress { id, out: &mut stdout };
                    handler.share(&config, &secrets, &files, v.str_field("target"), &compose, &mut p)
                };
                match r {
                    Ok(r) => ok(id, Value::obj().s("result", r.result).opt_s("detail", r.detail.as_deref()).done()),
                    Err(e) => err(id, &e),
                }
            }
            "Cancel" => ok(id, Value::obj().done()),
            _ => err(id, &PluginError::unsupported()),
        };
        write_json(&mut stdout, &reply)?;
    }
    Ok(())
}

pub fn percent_decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' && i + 2 < b.len() {
            if let Ok(v) = u8::from_str_radix(&s[i + 1..i + 3], 16) {
                out.push(v);
                i += 3;
                continue;
            }
        }
        out.push(b[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// A service plugin's Describe (dbus, highlight, ai): fixed-name plugins the daemon uses itself.
pub fn service_describe(id: &str, name: &str, version: &str, requests: &[&str]) -> Value {
    Value::obj().s("kind", "service").s("id", id).s("name", name).s("version", version).v("requests", Value::Arr(requests.iter().map(|r| Value::Str(r.to_string())).collect())).done()
}

#[cfg(test)]
mod loop_tests {
    use super::*;
    use std::sync::mpsc;
    use std::time::Duration;

    struct Slow;
    impl Handler for Slow {
        fn describe(&self) -> Describe {
            Describe {
                scheme: "slow",
                display_name: "Slow",
                version: "1",
                form: vec![],
                defaults: Value::obj().done(),
                secret_fields: vec![],
                detector_upload: "sizeMtime",
                detector_download: "sizeMtime",
                features: Features { set_mtime: false, mode: false, real_dirs: true, meta_in_scan: true, pipelining: false, partial_read: true },
                available: None,
            }
        }
        fn validate(&self, _: &Value) -> Result<()> {
            Ok(())
        }
        fn connect(&self, _: &str, _: &str, _: &Value, _: &Value) -> Result<Value> {
            Ok(Value::obj().done())
        }
        fn capabilities(&self, _: &str) -> Result<Value> {
            Ok(Value::obj().done())
        }
        fn scan(&self, _: &str, path: &str, _: bool, sink: &mut dyn FnMut(Vec<Entry>)) -> Result<u64> {
            // "/slow" lists forever until cancelled; anything else lists three entries at once.
            if path == "/slow" {
                let mut n = 0;
                loop {
                    if cancelled() {
                        return Err(cancel_error());
                    }
                    sink(vec![Entry { name: format!("f{n}"), kind: Kind::File, meta: None, rel: String::new() }]);
                    n += 1;
                    std::thread::sleep(Duration::from_millis(5));
                }
            }
            sink((0..3).map(|i| Entry { name: format!("e{i}"), kind: Kind::File, meta: None, rel: String::new() }).collect());
            Ok(3)
        }
        fn stat(&self, _: &str, _: &str) -> Result<Meta> {
            Ok(Meta { hidden: false, size: 1, mtime_ms: 0, mode: None, owner: None, group: None })
        }
        fn read(&self, _: &str, _: &str, _: u64, out: &mut Outgoing) -> Result<()> {
            for _ in 0..20 {
                std::thread::sleep(Duration::from_millis(5));
                out.write_all(b"x").map_err(PluginError::io)?;
            }
            Ok(())
        }
        fn write(&self, _: &str, _: &str, mut args: WriteArgs) -> Result<u64> {
            let mut v = Vec::new();
            args.data.read_to_end(&mut v).map_err(PluginError::io)?;
            Ok(v.len() as u64)
        }
        fn mkdir(&self, _: &str, _: &str) -> Result<()> {
            Ok(())
        }
        fn rename(&self, _: &str, _: &str, _: &str) -> Result<()> {
            Ok(())
        }
        fn delete(&self, _: &str, _: &str) -> Result<()> {
            Ok(())
        }
    }

    /// A pipe pair: the test writes requests into `req_w`, the loop writes replies into a Vec we can poll.
    struct Pipe(mpsc::Receiver<Vec<u8>>, Vec<u8>);
    impl Read for Pipe {
        fn read(&mut self, out: &mut [u8]) -> io::Result<usize> {
            if self.1.is_empty() {
                match self.0.recv() {
                    Ok(b) => self.1 = b,
                    Err(_) => return Ok(0),
                }
            }
            let n = self.1.len().min(out.len());
            out[..n].copy_from_slice(&self.1[..n]);
            self.1.drain(..n);
            Ok(n)
        }
    }
    #[derive(Clone)]
    struct Sink(Arc<Mutex<Vec<u8>>>);
    impl Write for Sink {
        fn write(&mut self, b: &[u8]) -> io::Result<usize> {
            self.0.lock().unwrap().extend_from_slice(b);
            Ok(b.len())
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    fn frames(sink: &Sink) -> Vec<(u8, Vec<u8>)> {
        let data = sink.0.lock().unwrap().clone();
        let mut cur = &data[..];
        let mut out = Vec::new();
        while let Ok(Some(f)) = read_frame(&mut cur) {
            out.push(f);
        }
        out
    }

    fn send(tx: &mpsc::Sender<Vec<u8>>, v: &Value) {
        let mut b = Vec::new();
        write_json(&mut b, v).unwrap();
        tx.send(b).unwrap();
    }

    fn wait_for(sink: &Sink, pred: impl Fn(&[(u8, Vec<u8>)]) -> bool) -> Vec<(u8, Vec<u8>)> {
        for _ in 0..400 {
            let f = frames(sink);
            if pred(&f) {
                return f;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        panic!("timed out: {:?}", frames(sink).iter().map(|(k, p)| (*k, String::from_utf8_lossy(p).into_owned())).collect::<Vec<_>>());
    }

    fn json_replies(f: &[(u8, Vec<u8>)]) -> Vec<Value> {
        f.iter().filter(|(k, _)| *k == 0).filter_map(|(_, p)| kiki_json::parse(p).ok()).collect()
    }

    #[test]
    fn requests_run_concurrently_and_cancel_stops_a_scan() {
        let (tx, rx) = mpsc::channel();
        let sink = Sink(Arc::new(Mutex::new(Vec::new())));
        let out: Arc<Shared> = Arc::new(Mutex::new(Box::new(sink.clone())));
        let handler = Slow;
        let loop_thread = std::thread::spawn(move || run_on(&handler, Box::new(Pipe(rx, Vec::new())), &out));

        // A slow Read (100 ms) then a Scan: the Scan reply must arrive before the Read finishes.
        send(&tx, &Value::obj().u("id", 1).s("type", "Read").s("location", "l").s("path", "/f").done());
        send(&tx, &Value::obj().u("id", 2).s("type", "Scan").s("location", "l").s("path", "/").done());
        let f = wait_for(&sink, |f| json_replies(f).iter().any(|r| r.u64_field("id") == Some(2) && r.get("ok").is_some()));
        assert!(!json_replies(&f).iter().any(|r| r.u64_field("id") == Some(1)), "scan answered while the read was still streaming");
        wait_for(&sink, |f| json_replies(f).iter().any(|r| r.u64_field("id") == Some(1) && r.get("ok").is_some()));

        // An endless scan is stopped by Cancel and reports Cancelled.
        send(&tx, &Value::obj().u("id", 3).s("type", "Scan").s("location", "l").s("path", "/slow").done());
        wait_for(&sink, |f| json_replies(f).iter().filter(|r| r.u64_field("id") == Some(3)).count() >= 3);
        send(&tx, &Value::obj().u("id", 4).s("type", "Cancel").u("target", 3).done());
        let f = wait_for(&sink, |f| json_replies(f).iter().any(|r| r.u64_field("id") == Some(3) && r.get("err").is_some()));
        let e = json_replies(&f).into_iter().find(|r| r.u64_field("id") == Some(3) && r.get("err").is_some()).unwrap();
        assert_eq!(e.get("err").unwrap().str_field("code"), Some("Cancelled"));
        assert!(json_replies(&f).iter().any(|r| r.u64_field("id") == Some(4) && r.get("ok").is_some()), "Cancel itself is acknowledged");

        // Write: binary frames route to the handler while another request runs.
        send(&tx, &Value::obj().u("id", 5).s("type", "Write").s("location", "l").s("path", "/w").done());
        let mut b = Vec::new();
        write_binary(&mut b, b"hello ").unwrap();
        write_binary(&mut b, b"world").unwrap();
        tx.send(b).unwrap();
        send(&tx, &Value::obj().u("id", 6).s("type", "Stat").s("location", "l").s("path", "/x").done());
        let mut end = Vec::new();
        write_binary(&mut end, &[]).unwrap();
        tx.send(end).unwrap();
        let f = wait_for(&sink, |f| json_replies(f).iter().any(|r| r.u64_field("id") == Some(5)));
        let w = json_replies(&f).into_iter().find(|r| r.u64_field("id") == Some(5)).unwrap();
        assert_eq!(w.get("ok").unwrap().u64_field("bytes"), Some(11));
        assert!(json_replies(&f).iter().any(|r| r.u64_field("id") == Some(6) && r.get("ok").is_some()));

        send(&tx, &Value::obj().u("id", 7).s("type", "Shutdown").done());
        loop_thread.join().unwrap().unwrap();
    }
}
