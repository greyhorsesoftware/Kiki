//! kiki's face on the session bus: `org.freedesktop.FileManager1` (Show in folder) and the
//! `org.freedesktop.impl.portal.FileChooser` backend.
//!
//! This program is what the bus activates (docs/0.3.0/01-daemon-on-demand.md, decision 4): it is
//! not the engine and it does not connect to one. A call arrives, it is put to a kiki window —
//! started if there is none — through the window's own IPC, exactly as the `kiki` command
//! reaches a running window; the window starts its own daemon if it needs one. The listener
//! holds nothing but the request it is serving.
//!
//! A chooser cannot be answered in one call — the person has not chosen yet — so the window
//! returns a token and this waits, asking for the answer until it has one.

use kiki_plugin_sdk::json::{self, Value};
use std::collections::HashMap;
use std::process::Command;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use zbus::zvariant::{ObjectPath, OwnedValue, Value as ZValue};
use zbus::{connection, interface};

/// How long the listener stays with nothing to do before it leaves. It is not a daemon: the bus
/// starts it for a request and starts it again for the next one, so sitting about afterwards is
/// only a process in somebody's list. `KIKI_DBUS_IDLE_MS` shortens it for the tests.
fn idle_limit() -> Duration {
    Duration::from_millis(std::env::var("KIKI_DBUS_IDLE_MS").ok().and_then(|v| v.parse().ok()).unwrap_or(60_000))
}

/// Requests being served right now. A chooser can stand open for an hour; the idle clock only
/// runs when nothing at all is outstanding.
static BUSY: AtomicUsize = AtomicUsize::new(0);

/// Held for as long as a request is being served, however that request ends.
struct Serving;

impl Serving {
    fn start() -> Serving {
        BUSY.fetch_add(1, Ordering::Relaxed);
        Serving
    }
}

impl Drop for Serving {
    fn drop(&mut self) {
        BUSY.fetch_sub(1, Ordering::Relaxed);
        LAST_MS.store(since_start(), Ordering::Relaxed);
    }
}

/// The moment this process began, so "how long since anything happened" is one subtraction.
fn started() -> Instant {
    static S: std::sync::OnceLock<Instant> = std::sync::OnceLock::new();
    *S.get_or_init(Instant::now)
}

fn since_start() -> u64 {
    started().elapsed().as_millis() as u64
}

/// When the last request ended, so the idle clock runs from the end of the work rather than from
/// the start of the process.
static LAST_MS: AtomicU64 = AtomicU64::new(0);

/// Leaves when there is nothing outstanding and nothing has happened for `idle_limit`. The bus
/// starts another listener for the next request; one that sat about would only be a process
/// nobody asked for (and, in a test, one that holds a private bus open).
async fn leave_when_idle() {
    let limit = idle_limit();
    loop {
        tokio::time::sleep(Duration::from_millis(500)).await;
        if BUSY.load(Ordering::Relaxed) == 0 && since_start().saturating_sub(LAST_MS.load(Ordering::Relaxed)) >= limit.as_millis() as u64 {
            std::process::exit(0);
        }
    }
}

/// How long to keep asking a window that is starting before giving up on it.
const WINDOW_WAIT: Duration = Duration::from_secs(20);
/// How long one question to a window may take. A window answers an IPC call at once; this is
/// only so that a window which never answers cannot hold the application that asked.
const ASK_LIMIT: Duration = Duration::from_secs(10);
/// How often to ask a chooser whether it has been answered, and how long to let one stand. A
/// person may leave a file chooser open; the portal's own caller is the one that gives up.
const POLL_EVERY: Duration = Duration::from_millis(200);
const CHOOSER_LIMIT: Duration = Duration::from_secs(60 * 60);

/// The window: what to run to put something to it, and how to start one that is not there.
struct Bridge;

impl Bridge {
    /// `qs -p <shell.qml> ipc call shell dbus <kind> <json>` — or whatever `KIKI_SHELL_CMD`
    /// names, which is how a test stands in for a window and how a checkout points at its own.
    fn shell_command(kind: &str, payload: &str) -> Command {
        match std::env::var("KIKI_SHELL_CMD") {
            Ok(cmd) => {
                let mut c = Command::new(cmd);
                c.arg(kind).arg(payload);
                c
            }
            Err(_) => {
                let mut c = Command::new("qs");
                c.arg("-p").arg(shell_qml()).arg("ipc").arg("call").arg("shell").arg("dbus").arg(kind).arg(payload);
                c
            }
        }
    }

    /// Put one thing to the window, whatever it answers. `None` when there is no window to ask.
    ///
    /// Asynchronous and on a deadline, both on purpose: the call is made from inside a D-Bus
    /// method, so a blocking wait would hold a runtime thread, and a window that never answers
    /// would hold the application that asked — for ever. It answers or it does not.
    async fn ask(kind: &str, fields: &Value) -> Option<Value> {
        let payload = json::to_string(fields);
        let std_cmd = Bridge::shell_command(kind, &payload);
        let mut cmd = tokio::process::Command::from(std_cmd);
        cmd.kill_on_drop(true);
        let out = match tokio::time::timeout(ASK_LIMIT, cmd.output()).await {
            Ok(Ok(out)) => out,
            Ok(Err(e)) => {
                eprintln!("kiki-dbus: could not run the window's ipc: {e}");
                return None;
            }
            Err(_) => {
                eprintln!("kiki-dbus: the window did not answer {kind} within {ASK_LIMIT:?}");
                return None;
            }
        };
        if !out.status.success() {
            return None;
        }
        let text = String::from_utf8_lossy(&out.stdout);
        // A window that answers nothing at all has still answered: the call was made.
        Some(json::parse(text.trim().as_bytes()).unwrap_or_else(|_| Value::obj().done()))
    }

    /// Start a window, the way the `kiki` command does, and leave it running: it is the person's
    /// window now, not this process's.
    fn start_window() {
        let mut c = match std::env::var("KIKI_SHELL_START") {
            Ok(cmd) => Command::new(cmd),
            Err(_) => {
                let mut c = Command::new("qs");
                c.arg("-n").arg("-p").arg(shell_qml());
                c
            }
        };
        let _ = c.stdin(std::process::Stdio::null()).stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null()).spawn();
    }

    /// Put something to a window, starting one if nothing answers and waiting for it to be ready.
    async fn call(&self, kind: &str, fields: Value) -> Value {
        if let Some(v) = Bridge::ask(kind, &fields).await {
            return v;
        }
        Bridge::start_window();
        let start = Instant::now();
        while start.elapsed() < WINDOW_WAIT {
            tokio::time::sleep(POLL_EVERY).await;
            if let Some(v) = Bridge::ask(kind, &fields).await {
                return v;
            }
        }
        eprintln!("kiki-dbus: no kiki window answered in {WINDOW_WAIT:?}");
        Value::Null
    }

    /// A chooser: ask the window to show one, then collect the answer when the person has chosen.
    async fn choose(&self, fields: Value) -> Value {
        let started = self.call("ShowChooser", fields).await;
        let Some(token) = started.str_field("token").map(str::to_string) else { return Value::Null };
        let ask = Value::obj().s("token", token).done();
        let start = Instant::now();
        while start.elapsed() < CHOOSER_LIMIT {
            tokio::time::sleep(POLL_EVERY).await;
            match Bridge::ask("ChooserPoll", &ask).await {
                // The window is gone: nobody is going to answer this one.
                None => return Value::Null,
                Some(v) if v.get("pending").and_then(Value::as_bool) == Some(true) => continue,
                Some(v) => return v,
            }
        }
        Value::Null
    }
}

/// The shell kiki runs: the package's, or a checkout's when `KIKI_SHELL_DIR` says so.
fn shell_qml() -> String {
    format!("{}/shell.qml", std::env::var("KIKI_SHELL_DIR").unwrap_or_else(|_| "/usr/share/kiki".into()))
}

struct FileManager {
    bridge: Arc<Bridge>,
}

#[interface(name = "org.freedesktop.FileManager1")]
impl FileManager {
    async fn show_items(&self, uris: Vec<String>, _startup_id: String) {
        let _serving = Serving::start();
        let _ = self.bridge.call("ShowItems", Value::obj().v("uris", Value::Arr(uris.into_iter().map(Value::Str).collect())).b("properties", false).done()).await;
    }
    async fn show_folders(&self, uris: Vec<String>, _startup_id: String) {
        let _serving = Serving::start();
        let _ = self.bridge.call("ShowFolders", Value::obj().v("uris", Value::Arr(uris.into_iter().map(Value::Str).collect())).done()).await;
    }
    async fn show_item_properties(&self, uris: Vec<String>, _startup_id: String) {
        let _serving = Serving::start();
        let _ = self.bridge.call("ShowItems", Value::obj().v("uris", Value::Arr(uris.into_iter().map(Value::Str).collect())).b("properties", true).done()).await;
    }
}

struct FileChooser {
    bridge: Arc<Bridge>,
}

type Options = HashMap<String, OwnedValue>;

fn opt_str(o: &Options, k: &str) -> Option<String> {
    o.get(k).and_then(|v| <&str>::try_from(v).ok().map(str::to_string))
}
fn opt_bool(o: &Options, k: &str) -> bool {
    o.get(k).and_then(|v| bool::try_from(v).ok()).unwrap_or(false)
}
fn opt_bytes_path(o: &Options, k: &str) -> Option<String> {
    o.get(k).and_then(|v| <Vec<u8>>::try_from(v.clone()).ok()).map(|b| String::from_utf8_lossy(b.strip_suffix(&[0]).unwrap_or(&b)).into_owned())
}

/// filters: a(sa(us)) → [{ name, patterns: [glob] }]
fn opt_filters(o: &Options) -> Value {
    let mut out = Vec::new();
    if let Some(v) = o.get("filters") {
        if let Ok(list) = <Vec<(String, Vec<(u32, String)>)>>::try_from(v.clone()) {
            for (name, pats) in list {
                out.push(Value::obj().s("name", name).v("patterns", Value::Arr(pats.into_iter().map(|(_, p)| Value::Str(p)).collect())).done());
            }
        }
    }
    Value::Arr(out)
}

impl FileChooser {
    async fn choose(&self, mode: &str, handle: ObjectPath<'_>, parent_window: String, title: String, options: Options) -> (u32, HashMap<String, OwnedValue>) {
        let _serving = Serving::start();
        let req = Value::obj()
            .s("token", handle.to_string())
            .s("mode", mode)
            .s("title", title)
            .b("multiple", opt_bool(&options, "multiple"))
            .b("directory", opt_bool(&options, "directory"))
            .v("filters", opt_filters(&options))
            .opt_s("currentFolder", opt_bytes_path(&options, "current_folder").as_deref())
            .opt_s("currentName", opt_str(&options, "current_name").as_deref())
            .opt_s("parentWindow", if parent_window.is_empty() { None } else { Some(parent_window.as_str()) })
            .v(
                "files",
                Value::Arr(
                    options
                        .get("files")
                        .and_then(|v| <Vec<Vec<u8>>>::try_from(v.clone()).ok())
                        .unwrap_or_default()
                        .into_iter()
                        .map(|b| Value::Str(String::from_utf8_lossy(b.strip_suffix(&[0]).unwrap_or(&b)).into_owned()))
                        .collect(),
                ),
            )
            .done();
        let reply = self.bridge.choose(req).await;
        let mut results: HashMap<String, OwnedValue> = HashMap::new();
        match reply.get("uris").and_then(Value::as_arr) {
            Some(uris) if !uris.is_empty() => {
                let list: Vec<String> = uris.iter().filter_map(|u| u.as_str().map(str::to_string)).collect();
                if let Ok(v) = OwnedValue::try_from(ZValue::from(list)) {
                    results.insert("uris".into(), v);
                }
                (0, results)
            }
            _ => (1, results),
        }
    }
}

#[interface(name = "org.freedesktop.impl.portal.FileChooser")]
impl FileChooser {
    async fn open_file(&self, handle: ObjectPath<'_>, _app_id: String, parent_window: String, title: String, options: Options) -> (u32, HashMap<String, OwnedValue>) {
        self.choose("open", handle, parent_window, title, options).await
    }
    async fn save_file(&self, handle: ObjectPath<'_>, _app_id: String, parent_window: String, title: String, options: Options) -> (u32, HashMap<String, OwnedValue>) {
        self.choose("save", handle, parent_window, title, options).await
    }
    async fn save_files(&self, handle: ObjectPath<'_>, _app_id: String, parent_window: String, title: String, options: Options) -> (u32, HashMap<String, OwnedValue>) {
        self.choose("saveFiles", handle, parent_window, title, options).await
    }
    #[zbus(property)]
    fn version(&self) -> u32 {
        4
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // The clock starts here, so a listener the bus started and nobody used still leaves.
    started();
    LAST_MS.store(0, Ordering::Relaxed);
    let bridge = Arc::new(Bridge);
    let _conn = connection::Builder::session()?
        .name("org.freedesktop.FileManager1")?
        .name("org.freedesktop.impl.portal.desktop.kiki")?
        .serve_at("/org/freedesktop/FileManager1", FileManager { bridge: Arc::clone(&bridge) })?
        .serve_at("/org/freedesktop/portal/desktop", FileChooser { bridge })?
        .build()
        .await?;
    leave_when_idle().await;
    Ok(())
}
