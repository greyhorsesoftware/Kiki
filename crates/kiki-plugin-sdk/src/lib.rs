//! The kiki location plugin SDK. Implement [`Handler`], call [`run`], done.
//! Wire format and message semantics: docs/0.1.0/API-PLUGIN.md.

pub use kiki_json as json;
use kiki_json::Value;
use std::io::{self, Read, Write};

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
    pub size: u64,
    pub mtime_ms: u64,
    pub mode: Option<u32>,
    pub owner: Option<String>,
    pub group: Option<String>,
}

impl Meta {
    pub fn to_json(&self) -> Value {
        Value::obj()
            .u("size", self.size)
            .u("mtime", self.mtime_ms)
            .v("mode", self.mode.map(|m| Value::Uint(m as u64)).unwrap_or(Value::Null))
            .opt_s("owner", self.owner.as_deref())
            .opt_s("group", self.group.as_deref())
            .v("digest", Value::Null)
            .done()
    }
}

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

pub fn select_field(key: &str, label: &str, options: &[&str], default: &str) -> Value {
    Value::obj().s("key", key).s("label", label).s("kind", "select").b("required", true).s("default", default).v("options", Value::Arr(options.iter().map(|o| Value::Str(o.to_string())).collect())).v("group", Value::Null).done()
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
}

impl Describe {
    fn to_json(&self) -> Value {
        Value::obj()
            .s("scheme", self.scheme)
            .s("displayName", self.display_name)
            .s("version", self.version)
            .v("form", Value::Arr(self.form.clone()))
            .v("defaults", self.defaults.clone())
            .v("secretFields", Value::Arr(self.secret_fields.iter().map(|s| Value::Str(s.to_string())).collect()))
            .v("detector", Value::obj().s("upload", self.detector_upload).s("download", self.detector_download).done())
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

/// Incoming bytes of a `Write`: reads binary frames from stdin until the empty end frame.
pub struct Incoming<'a> {
    stdin: &'a mut dyn Read,
    buf: Vec<u8>,
    pos: usize,
    done: bool,
}

impl<'a> Read for Incoming<'a> {
    fn read(&mut self, out: &mut [u8]) -> io::Result<usize> {
        if self.pos >= self.buf.len() {
            if self.done {
                return Ok(0);
            }
            match read_frame(self.stdin)? {
                Some((1, payload)) => {
                    if payload.is_empty() {
                        self.done = true;
                        return Ok(0);
                    }
                    self.buf = payload;
                    self.pos = 0;
                }
                Some((_, _)) => return Err(io::Error::new(io::ErrorKind::InvalidData, "expected binary frame")),
                None => {
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

/// Outgoing bytes of a `Read`: `write` sends binary frames; the SDK sends the end frame.
pub struct Outgoing<'a> {
    stdout: &'a mut dyn Write,
    pub bytes: u64,
}

impl<'a> Write for Outgoing<'a> {
    fn write(&mut self, b: &[u8]) -> io::Result<usize> {
        if !b.is_empty() {
            for chunk in b.chunks(1024 * 1024) {
                write_binary(self.stdout, chunk)?;
            }
            self.bytes += b.len() as u64;
        }
        Ok(b.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        self.stdout.flush()
    }
}

pub struct WriteArgs<'a> {
    pub size: Option<u64>,
    pub mode: Option<u32>,
    pub mtime_ms: Option<u64>,
    pub data: Incoming<'a>,
}

#[allow(unused_variables)]
pub trait Handler {
    fn describe(&self) -> Describe;
    fn validate(&mut self, config: &Value) -> Result<()>;
    /// Returns `{ fingerprint, banner }` fields as JSON (may be empty object).
    fn connect(&mut self, location: &str, role: &str, config: &Value, secrets: &Value) -> Result<Value>;
    fn disconnect(&mut self, location: &str, role: &str) {}
    fn capabilities(&mut self, location: &str) -> Result<Value>;
    fn scan(&mut self, location: &str, path: &str, recursive: bool, sink: &mut dyn FnMut(Vec<Entry>)) -> Result<u64>;
    fn stat(&mut self, location: &str, path: &str) -> Result<Meta>;
    fn read(&mut self, location: &str, path: &str, offset: u64, out: &mut Outgoing) -> Result<()>;
    fn write(&mut self, location: &str, path: &str, args: WriteArgs) -> Result<u64>;
    fn mkdir(&mut self, location: &str, path: &str) -> Result<()>;
    fn rename(&mut self, location: &str, from: &str, to: &str) -> Result<()>;
    fn delete(&mut self, location: &str, path: &str) -> Result<()>;
    fn set_mtime(&mut self, location: &str, path: &str, mtime_ms: u64) -> Result<()> {
        Err(PluginError::unsupported())
    }
    fn chmod(&mut self, location: &str, path: &str, mode: u32) -> Result<()> {
        Err(PluginError::unsupported())
    }
    /// Optional native thumbnail (plan 17); write JPEG or PNG bytes to `out`.
    fn thumb(&mut self, location: &str, path: &str, out: &mut Outgoing) -> Result<()> {
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

/// The dispatch loop. Runs until stdin closes or `Shutdown` arrives.
pub fn run(handler: &mut dyn Handler) -> io::Result<()> {
    let stdin = io::stdin();
    let stdout = io::stdout();
    let mut stdin = stdin.lock();
    let mut stdout = stdout.lock();
    while let Some((kind, payload)) = read_frame(&mut stdin)? {
        if kind != 0 {
            continue; // stray binary frame outside a Write
        }
        let v = match kiki_json::parse(&payload) {
            Ok(v) => v,
            Err(_) => continue,
        };
        let id = v.u64_field("id").unwrap_or(0);
        let t = v.str_field("type").unwrap_or("").to_string();
        let loc = v.str_field("location").unwrap_or("").to_string();
        let path = v.str_field("path").unwrap_or("/").to_string();
        let reply = match t.as_str() {
            "Describe" => ok(id, handler.describe().to_json()),
            "Ping" => ok(id, Value::obj().done()),
            "Shutdown" => {
                write_json(&mut stdout, &ok(id, Value::obj().done()))?;
                return Ok(());
            }
            "Validate" => match handler.validate(v.get("config").unwrap_or(&Value::Null)) {
                Ok(()) => ok(id, Value::obj().done()),
                Err(e) => err(id, &e),
            },
            "Connect" => match handler.connect(&loc, v.str_field("role").unwrap_or("browse"), v.get("config").unwrap_or(&Value::Null), v.get("secrets").unwrap_or(&Value::Null)) {
                Ok(r) => ok(id, r),
                Err(e) => err(id, &e),
            },
            "Disconnect" => {
                handler.disconnect(&loc, v.str_field("role").unwrap_or("browse"));
                ok(id, Value::obj().done())
            }
            "Capabilities" => match handler.capabilities(&loc) {
                Ok(c) => ok(id, c),
                Err(e) => err(id, &e),
            },
            "Scan" => {
                let recursive = v.get("recursive").and_then(Value::as_bool).unwrap_or(false);
                let mut sink_err: Option<io::Error> = None;
                let r = {
                    let out = &mut stdout;
                    handler.scan(&loc, &path, recursive, &mut |entries: Vec<Entry>| {
                        if sink_err.is_some() {
                            return;
                        }
                        let frame = Value::obj().u("id", id).v("entries", Value::Arr(entries.iter().map(entry_json).collect())).done();
                        if let Err(e) = write_json(out, &frame) {
                            sink_err = Some(e);
                        }
                    })
                };
                if let Some(e) = sink_err {
                    return Err(e);
                }
                match r {
                    Ok(n) => ok(id, Value::obj().u("n", n).done()),
                    Err(e) => err(id, &e),
                }
            }
            "Stat" => match handler.stat(&loc, &path) {
                Ok(m) => ok(id, m.to_json()),
                Err(e) => err(id, &e),
            },
            "Read" => {
                let offset = v.u64_field("offset").unwrap_or(0);
                let (r, bytes) = {
                    let mut out = Outgoing { stdout: &mut stdout, bytes: 0 };
                    let r = handler.read(&loc, &path, offset, &mut out);
                    (r, out.bytes)
                };
                write_binary(&mut stdout, &[])?;
                stdout.flush()?;
                match r {
                    Ok(()) => ok(id, Value::obj().u("bytes", bytes).done()),
                    Err(e) => err(id, &e),
                }
            }
            "Thumb" => {
                let (r, bytes) = {
                    let mut out = Outgoing { stdout: &mut stdout, bytes: 0 };
                    let r = handler.thumb(&loc, &path, &mut out);
                    (r, out.bytes)
                };
                write_binary(&mut stdout, &[])?;
                stdout.flush()?;
                match r {
                    Ok(()) => ok(id, Value::obj().u("bytes", bytes).done()),
                    Err(e) => err(id, &e),
                }
            }
            "Write" => {
                let args = WriteArgs { size: v.u64_field("size"), mode: v.u64_field("mode").map(|m| m as u32), mtime_ms: v.u64_field("mtime"), data: Incoming { stdin: &mut stdin, buf: Vec::new(), pos: 0, done: false } };
                match handler.write(&loc, &path, args) {
                    Ok(n) => ok(id, Value::obj().u("bytes", n).done()),
                    Err(e) => {
                        // Drain the rest of the stream so the pipe stays in sync.
                        let mut drain = Incoming { stdin: &mut stdin, buf: Vec::new(), pos: 0, done: false };
                        let _ = io::copy(&mut drain, &mut io::sink());
                        err(id, &e)
                    }
                }
            }
            "Mkdir" => result(id, handler.mkdir(&loc, &path)),
            "Rename" => result(id, handler.rename(&loc, v.str_field("from").unwrap_or(""), v.str_field("to").unwrap_or(""))),
            "Delete" => result(id, handler.delete(&loc, &path)),
            "SetMtime" => result(id, handler.set_mtime(&loc, &path, v.u64_field("mtime").unwrap_or(0))),
            "Chmod" => result(id, handler.chmod(&loc, &path, v.u64_field("mode").unwrap_or(0) as u32)),
            _ => err(id, &PluginError::unsupported()),
        };
        write_json(&mut stdout, &reply)?;
    }
    Ok(())
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
}

impl ShareDescribe {
    fn to_json(&self) -> Value {
        Value::obj()
            .s("id", self.id)
            .s("name", self.name)
            .s("icon", self.icon)
            .s("version", self.version)
            .v("accepts", Value::obj().b("files", self.accepts_files).b("folders", self.accepts_folders).b("multiple", self.accepts_multiple).v("maxBytes", self.max_bytes.map(Value::Uint).unwrap_or(Value::Null)).done())
            .s("targets", self.targets)
            .v("form", Value::Arr(self.form.clone()))
            .v("secretFields", Value::Arr(self.secret_fields.iter().map(|s| Value::Str(s.to_string())).collect()))
            .v("compose", Value::Arr(self.compose.clone()))
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
                Ok(t) => ok(id, Value::obj().v("targets", Value::Arr(t.into_iter().map(|t| Value::obj().s("id", t.id).s("name", t.name).s("detail", t.detail).b("online", t.online).s("icon", t.icon).done()).collect())).done()),
                Err(e) => err(id, &e),
            },
            "Share" => {
                let files: Vec<String> = v.get("uris").and_then(Value::as_arr).map(|a| a.iter().filter_map(|u| u.as_str()).map(|u| if let Some(p) = u.strip_prefix("file://") { percent_decode(p) } else { u.to_string() }).collect()).unwrap_or_default();
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
