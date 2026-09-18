//! Plugin host: spawns `kiki-plugin-<scheme>` processes and speaks API-PLUGIN.md over their pipes.

use crate::json::Value;
use crate::proto::{self, Frame, Framing, Reader};
use crate::vfs::VfsError;
use std::collections::HashMap;
use std::path::PathBuf;
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;

pub const REQUEST_TIMEOUT: Duration = Duration::from_secs(120);

pub fn plugin_dirs() -> Vec<PathBuf> {
    let mut v = Vec::new();
    if let Ok(d) = std::env::var("KIKI_PLUGIN_DIR") {
        v.push(PathBuf::from(d));
    }
    v.push(crate::config::home().join(".local/lib/kiki/plugins"));
    v.push(PathBuf::from("/usr/lib/kiki/plugins"));
    v
}

pub fn find_binary(scheme: &str) -> Option<PathBuf> {
    let name = format!("kiki-plugin-{scheme}");
    plugin_dirs().into_iter().map(|d| d.join(&name)).find(|p| p.is_file())
}

const SERVICES: &[&str] = &["dbus", "highlight"];

/// The location kinds this build ships. Discovery, `Describe`, the Add-location dialog and
/// `AddLocation` never see anything else, so this list and the workspace's default members are
/// together the whole answer to "which protocols does this kiki speak". The stub is the contract
/// test's plugin, so it counts only under the `stub` feature.
pub const LOCATION_KINDS: &[&str] = if cfg!(feature = "stub") { &["ftps", "sftp", "stub"] } else { &["ftps", "sftp"] };

/// Whether this build can connect to a location kind at all.
pub fn ships(scheme: &str) -> bool {
    LOCATION_KINDS.contains(&scheme)
}

/// Whether a directory entry is something we could actually run. A development plugin directory
/// is usually `target/release`, which is full of `kiki-plugin-<name>.d` dependency files.
#[cfg(unix)]
pub fn runnable(path: &std::path::Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    std::fs::metadata(path).map(|m| m.is_file() && m.permissions().mode() & 0o111 != 0).unwrap_or(false)
}

#[cfg(not(unix))]
pub fn runnable(path: &std::path::Path) -> bool {
    path.is_file()
}

/// Every plugin binary present: (name suffix, path). User directory wins, first seen wins.
pub fn inventory() -> Vec<(String, PathBuf)> {
    let mut out: Vec<(String, PathBuf)> = Vec::new();
    for d in plugin_dirs() {
        if let Ok(rd) = std::fs::read_dir(&d) {
            for e in rd.flatten() {
                let name = e.file_name().to_string_lossy().into_owned();
                if let Some(s) = name.strip_prefix("kiki-plugin-") {
                    if !runnable(&e.path()) {
                        continue;
                    }
                    if !out.iter().any(|(x, _)| x == s) {
                        out.push((s.to_string(), e.path()));
                    }
                }
            }
        }
    }
    out.sort();
    out
}

/// Status of every plugin for the Settings page: kind, running, version, last Describe.
pub fn status_json() -> Value {
    let r = registry().lock().unwrap();
    Value::Arr(
        inventory()
            .into_iter()
            .map(|(name, path)| {
                let kind = if name.starts_with("share-") {
                    "share"
                } else if SERVICES.contains(&name.as_str()) {
                    "service"
                } else {
                    "location"
                };
                let running =
                    r.running.get(&name).map(|p| p.alive()).unwrap_or(false) || crate::helpers::running_named(&format!("kiki-plugin-{name}")) || crate::share::running_named(name.strip_prefix("share-").unwrap_or(""));
                let described = r.described.get(&name).cloned();
                Value::obj().s("name", name.clone()).s("kind", kind).s("path", path.to_string_lossy()).b("running", running).v("describe", described.unwrap_or(Value::Null)).done()
            })
            .collect(),
    )
}

/// Location plugin schemes present: binaries in the plugin directories, minus everything this
/// build does not ship (share and service plugins are never location kinds).
pub fn available() -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for d in plugin_dirs() {
        if let Ok(rd) = std::fs::read_dir(&d) {
            for e in rd.flatten() {
                let name = e.file_name().to_string_lossy().into_owned();
                if let Some(s) = name.strip_prefix("kiki-plugin-") {
                    if !ships(s) || !runnable(&e.path()) {
                        continue;
                    }
                    if !out.iter().any(|x| x == s) {
                        out.push(s.to_string());
                    }
                }
            }
        }
    }
    out.sort();
    out
}

pub enum Msg {
    Json(Value),
    Binary(Vec<u8>),
}

pub struct Plugin {
    pub scheme: String,
    child: Mutex<Child>,
    stdin: Mutex<ChildStdin>,
    pending: Mutex<HashMap<u64, Sender<Msg>>>,
    stream: Mutex<Option<u64>>,
    next_id: AtomicU64,
    describe: OnceLock<Value>,
    last_used: Mutex<std::time::Instant>,
}

pub const IDLE_EXIT: Duration = Duration::from_secs(300);

impl Plugin {
    pub fn spawn(scheme: &str) -> Result<Arc<Plugin>, VfsError> {
        if !ships(scheme) {
            return Err(VfsError::Io(format!("{scheme} locations are not part of this build")));
        }
        let bin = find_binary(scheme).ok_or_else(|| VfsError::Io(format!("no plugin for scheme {scheme}")))?;
        let p = Self::spawn_path(&bin, scheme)?;
        let d = p.request(Value::obj().s("type", "Describe").done())?;
        let _ = p.describe.set(d);
        Ok(p)
    }

    /// Spawns any binary speaking the plugin framing; `Describe` is not called.
    pub fn spawn_path(bin: &std::path::Path, scheme: &str) -> Result<Arc<Plugin>, VfsError> {
        let mut child = Command::new(bin)
            .env("KIKI_PLUGIN_PROTOCOL", "1")
            .env("KIKI_PLUGIN_SCHEME", scheme)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .map_err(|e| VfsError::Io(format!("spawn {}: {e}", bin.display())))?;
        let stdin = child.stdin.take().unwrap();
        let stdout = child.stdout.take().unwrap();
        let plugin = Arc::new(Plugin {
            scheme: scheme.to_string(),
            child: Mutex::new(child),
            stdin: Mutex::new(stdin),
            pending: Mutex::new(HashMap::new()),
            stream: Mutex::new(None),
            next_id: AtomicU64::new(1),
            describe: OnceLock::new(),
            last_used: Mutex::new(std::time::Instant::now()),
        });
        let p2 = Arc::clone(&plugin);
        std::thread::Builder::new()
            .name(format!("plugin-{scheme}"))
            .spawn(move || {
                let mut reader = Reader::new(stdout);
                loop {
                    match reader.next() {
                        Ok(Some(Frame::Json(v))) => {
                            let id = v.u64_field("id").unwrap_or(0);
                            let is_reply = v.get("ok").is_some() || v.get("err").is_some();
                            let tx = {
                                let mut pend = p2.pending.lock().unwrap();
                                if is_reply {
                                    pend.remove(&id)
                                } else {
                                    pend.get(&id).cloned()
                                }
                            };
                            if let Some(tx) = tx {
                                let _ = tx.send(Msg::Json(v));
                            }
                        }
                        Ok(Some(Frame::Binary(b))) => {
                            let id = *p2.stream.lock().unwrap();
                            if let Some(id) = id {
                                let tx = p2.pending.lock().unwrap().get(&id).cloned();
                                if let Some(tx) = tx {
                                    let _ = tx.send(Msg::Binary(b));
                                }
                            }
                        }
                        Ok(None) | Err(_) => break,
                    }
                }
                p2.pending.lock().unwrap().clear(); // process ended: outstanding requests fail
            })
            .expect("spawn plugin reader");
        Ok(plugin)
    }

    pub fn describe(&self) -> &Value {
        self.describe.get().expect("describe set at spawn")
    }

    pub fn set_describe(&self, d: Value) -> Result<(), Value> {
        self.describe.set(d)
    }

    fn send(&self, v: &Value) -> Result<(), VfsError> {
        use std::io::Write;
        let mut w = self.stdin.lock().unwrap();
        proto::write_json(&mut *w, Framing::Binary, v).map_err(|e| VfsError::Io(format!("plugin write: {e}")))?;
        w.flush().map_err(|e| VfsError::Io(e.to_string()))
    }

    fn send_binary(&self, b: &[u8]) -> Result<(), VfsError> {
        use std::io::Write;
        let mut w = self.stdin.lock().unwrap();
        proto::write_binary(&mut *w, b).map_err(|e| VfsError::Io(format!("plugin write: {e}")))?;
        w.flush().map_err(|e| VfsError::Io(e.to_string()))
    }

    fn begin(&self, mut req: Value, streaming: bool) -> Result<(u64, Receiver<Msg>), VfsError> {
        *self.last_used.lock().unwrap() = std::time::Instant::now();
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        if let Value::Obj(m) = &mut req {
            m.insert("id".into(), Value::Uint(id));
        }
        let (tx, rx) = mpsc::channel();
        self.pending.lock().unwrap().insert(id, tx);
        if streaming {
            *self.stream.lock().unwrap() = Some(id);
        }
        if let Err(e) = self.send(&req) {
            self.pending.lock().unwrap().remove(&id);
            return Err(e);
        }
        Ok((id, rx))
    }

    fn finish(&self, id: u64, v: Value) -> Result<Value, VfsError> {
        self.pending.lock().unwrap().remove(&id);
        if let Some(e) = v.get("err") {
            let code = e.str_field("code").unwrap_or("Io");
            let msg = e.str_field("message").unwrap_or("").to_string();
            return Err(match code {
                "NotFound" => VfsError::NotFound,
                "Denied" | "Auth" => VfsError::Denied,
                "Exists" => VfsError::Exists,
                "NotEmpty" => VfsError::NotEmpty,
                "Unsupported" => VfsError::Unsupported,
                _ => VfsError::Io(format!("{code}: {msg}")),
            });
        }
        Ok(v.get("ok").cloned().unwrap_or(Value::Null))
    }

    fn wait_reply(&self, id: u64, rx: &Receiver<Msg>, on_frame: Option<&mut dyn FnMut(Msg)>) -> Result<Value, VfsError> {
        self.wait_reply_with(id, rx, None, on_frame)
    }

    /// Waits for the reply; while waiting, `cancel` is polled and, once set, a `Cancel` is sent
    /// to the plugin and the request ends with an error as soon as the plugin acknowledges it.
    fn wait_reply_with(&self, id: u64, rx: &Receiver<Msg>, cancel: Option<&AtomicBool>, mut on_frame: Option<&mut dyn FnMut(Msg)>) -> Result<Value, VfsError> {
        let started = std::time::Instant::now();
        let mut last_frame = std::time::Instant::now();
        let mut sent_cancel = false;
        loop {
            match rx.recv_timeout(Duration::from_millis(100)) {
                Ok(Msg::Json(v)) if v.get("ok").is_some() || v.get("err").is_some() => {
                    let r = self.finish(id, v);
                    return if sent_cancel { Err(VfsError::Io("cancelled".into())) } else { r };
                }
                Ok(m) => {
                    last_frame = std::time::Instant::now();
                    if !sent_cancel {
                        if let Some(f) = on_frame.as_mut() {
                            f(m);
                        }
                    }
                }
                Err(mpsc::RecvTimeoutError::Timeout) => {
                    if let Some(c) = cancel {
                        if c.load(Ordering::Relaxed) && !sent_cancel {
                            sent_cancel = true;
                            self.cancel(id);
                            last_frame = std::time::Instant::now();
                        }
                    }
                    let limit = if sent_cancel { Duration::from_secs(5) } else { REQUEST_TIMEOUT };
                    if last_frame.elapsed() > limit && started.elapsed() > limit {
                        self.pending.lock().unwrap().remove(&id);
                        return Err(VfsError::Io(if sent_cancel { "cancelled".into() } else { "plugin request timed out or plugin exited".into() }));
                    }
                }
                Err(mpsc::RecvTimeoutError::Disconnected) => {
                    self.pending.lock().unwrap().remove(&id);
                    return Err(VfsError::Io("plugin exited".into()));
                }
            }
        }
    }

    /// Fire-and-forget `Cancel { target }`; the reply is discarded when it arrives.
    pub fn cancel(&self, target: u64) {
        let _ = self.begin(Value::obj().s("type", "Cancel").u("target", target).done(), false);
    }

    /// A plain request: one reply.
    pub fn request(&self, req: Value) -> Result<Value, VfsError> {
        let (id, rx) = self.begin(req, false)?;
        self.wait_reply(id, &rx, None)
    }

    /// A streaming request: `on_frame` gets every intermediate JSON or binary frame; returns the final reply.
    pub fn request_stream(&self, req: Value, on_frame: impl FnMut(Msg)) -> Result<Value, VfsError> {
        self.request_stream_with(req, None, on_frame)
    }

    /// A streaming request that stops early when `cancel` is set (a closed listing, a cancelled job).
    pub fn request_stream_with(&self, req: Value, cancel: Option<&AtomicBool>, mut on_frame: impl FnMut(Msg)) -> Result<Value, VfsError> {
        let (id, rx) = self.begin(req, true)?;
        let r = self.wait_reply_with(id, &rx, cancel, Some(&mut on_frame));
        *self.stream.lock().unwrap() = None;
        r
    }

    /// Write: send the request, then binary frames, then the end marker; returns the reply.
    pub fn write_stream(&self, req: Value, chunks: impl FnMut() -> Option<Vec<u8>>) -> Result<Value, VfsError> {
        self.write_stream_with(req, None, chunks)
    }

    pub fn write_stream_with(&self, req: Value, cancel: Option<&AtomicBool>, mut chunks: impl FnMut() -> Option<Vec<u8>>) -> Result<Value, VfsError> {
        let (id, rx) = self.begin(req, false)?;
        while let Some(c) = chunks() {
            if cancel.map(|c| c.load(Ordering::Relaxed)).unwrap_or(false) {
                break;
            }
            self.send_binary(&c)?;
        }
        self.send_binary(&[])?;
        self.wait_reply_with(id, &rx, cancel, None)
    }

    pub fn shutdown(&self) {
        let _ = self.request(Value::obj().s("type", "Shutdown").done());
        let mut c = self.child.lock().unwrap();
        let _ = c.wait();
    }

    pub fn idle_for(&self) -> Duration {
        self.last_used.lock().unwrap().elapsed()
    }

    pub fn busy(&self) -> bool {
        !self.pending.lock().unwrap().is_empty()
    }

    pub fn alive(&self) -> bool {
        matches!(self.child.lock().unwrap().try_wait(), Ok(None))
    }
}

impl Drop for Plugin {
    fn drop(&mut self) {
        if let Ok(c) = self.child.get_mut() {
            let _ = c.kill();
        }
    }
}

// ---------------------------------------------------------------- registry

struct Registry {
    running: HashMap<String, Arc<Plugin>>,
    described: HashMap<String, Value>,
}

fn registry() -> &'static Mutex<Registry> {
    static R: OnceLock<Mutex<Registry>> = OnceLock::new();
    R.get_or_init(|| Mutex::new(Registry { running: HashMap::new(), described: HashMap::new() }))
}

/// Reaper: every minute, shut down plugins idle for longer than `IDLE_EXIT` (they respawn on use).
pub fn start_reaper() {
    std::thread::Builder::new()
        .name("plugin-reaper".into())
        .spawn(|| loop {
            std::thread::sleep(Duration::from_secs(60));
            let idle: Vec<(String, Arc<Plugin>)> = registry().lock().unwrap().running.iter().filter(|(_, p)| p.alive() && !p.busy() && p.idle_for() >= IDLE_EXIT).map(|(k, p)| (k.clone(), Arc::clone(p))).collect();
            for (k, p) in idle {
                p.shutdown();
                registry().lock().unwrap().running.remove(&k);
            }
            crate::share::reap_idle();
            crate::helpers::reap_idle();
        })
        .expect("spawn reaper");
}

/// The running plugin for a scheme, spawning it on first use or after a crash.
pub fn get(scheme: &str) -> Result<Arc<Plugin>, VfsError> {
    {
        let r = registry().lock().unwrap();
        if let Some(p) = r.running.get(scheme) {
            if p.alive() {
                return Ok(Arc::clone(p));
            }
        }
    }
    let p = Plugin::spawn(scheme)?;
    let mut r = registry().lock().unwrap();
    r.described.insert(scheme.to_string(), p.describe().clone());
    r.running.insert(scheme.to_string(), Arc::clone(&p));
    Ok(p)
}

/// Describe results for every installed plugin (spawning briefly the ones not yet seen).
pub fn describe_all() -> Vec<Value> {
    let mut out = Vec::new();
    for scheme in available() {
        let cached = registry().lock().unwrap().described.get(&scheme).cloned();
        let d = match cached {
            Some(d) => d,
            None => match Plugin::spawn(&scheme) {
                Ok(p) => {
                    let d = p.describe().clone();
                    registry().lock().unwrap().described.insert(scheme.clone(), d.clone());
                    p.shutdown();
                    d
                }
                Err(e) => {
                    eprintln!("plugin {scheme}: {}", e.message());
                    continue;
                }
            },
        };
        out.push(d);
    }
    out
}

pub fn describe(scheme: &str) -> Option<Value> {
    if let Some(d) = registry().lock().unwrap().described.get(scheme).cloned() {
        return Some(d);
    }
    describe_all().into_iter().find(|d| d.str_field("scheme") == Some(scheme))
}
