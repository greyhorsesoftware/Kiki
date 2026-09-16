//! Serves `org.freedesktop.FileManager1` (Show in folder) and the
//! `org.freedesktop.impl.portal.FileChooser` backend on the session bus, forwarding
//! each call to kikid over stdout as a JSON request and waiting for kikid's reply on
//! stdin (the shell answers through the daemon). Framing is API-PLUGIN.md's, reversed.

use kiki_plugin_sdk::json::{self, Value};
use kiki_plugin_sdk::{read_frame, write_json};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use tokio::sync::oneshot;
use zbus::zvariant::{ObjectPath, OwnedValue, Value as ZValue};
use zbus::{connection, interface};

struct Bridge {
    next: AtomicU64,
    pending: Mutex<HashMap<u64, oneshot::Sender<Value>>>,
    out: std::sync::Mutex<std::io::Stdout>,
}

impl Bridge {
    async fn call(&self, kind: &str, fields: Value) -> Value {
        let id = self.next.fetch_add(1, Ordering::Relaxed);
        let (tx, rx) = oneshot::channel();
        self.pending.lock().unwrap().insert(id, tx);
        let mut req = fields;
        if let Value::Obj(m) = &mut req {
            m.insert("id".into(), Value::Uint(id));
            m.insert("type".into(), Value::Str(kind.into()));
        }
        {
            let mut o = self.out.lock().unwrap();
            let _ = write_json(&mut *o, &req);
        }
        rx.await.unwrap_or(Value::Null)
    }
}

struct FileManager {
    bridge: Arc<Bridge>,
}

#[interface(name = "org.freedesktop.FileManager1")]
impl FileManager {
    async fn show_items(&self, uris: Vec<String>, _startup_id: String) {
        let _ = self.bridge.call("ShowItems", Value::obj().v("uris", Value::Arr(uris.into_iter().map(Value::Str).collect())).b("properties", false).done()).await;
    }
    async fn show_folders(&self, uris: Vec<String>, _startup_id: String) {
        let _ = self.bridge.call("ShowFolders", Value::obj().v("uris", Value::Arr(uris.into_iter().map(Value::Str).collect())).done()).await;
    }
    async fn show_item_properties(&self, uris: Vec<String>, _startup_id: String) {
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
            .done();
        let reply = self.bridge.call("ShowChooser", req).await;
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
    let bridge = Arc::new(Bridge { next: AtomicU64::new(1), pending: Mutex::new(HashMap::new()), out: std::sync::Mutex::new(std::io::stdout()) });
    // Replies from kikid arrive on stdin.
    let b2 = Arc::clone(&bridge);
    std::thread::spawn(move || {
        let mut stdin = std::io::stdin().lock();
        while let Ok(Some((kind, payload))) = read_frame(&mut stdin) {
            if kind != 0 {
                continue;
            }
            if let Ok(v) = json::parse(&payload) {
                // Requests from the daemon (Describe, Ping) versus replies to ours.
                match v.str_field("type") {
                    Some("Describe") => { let id = v.u64_field("id").unwrap_or(0); let mut o = b2.out.lock().unwrap(); let _ = write_json(&mut *o, &Value::obj().u("id", id).v("ok", kiki_plugin_sdk::service_describe("dbus", "D-Bus: Show in folder and portal chooser", env!("CARGO_PKG_VERSION"), &[])).done()); continue }
                    Some("Ping") => { let id = v.u64_field("id").unwrap_or(0); let mut o = b2.out.lock().unwrap(); let _ = write_json(&mut *o, &Value::obj().u("id", id).v("ok", Value::obj().done()).done()); continue }
                    _ => {}
                }
                if let Some(id) = v.u64_field("id") {
                    if let Some(tx) = b2.pending.lock().unwrap().remove(&id) {
                        let _ = tx.send(v.get("ok").cloned().unwrap_or(Value::Null));
                    }
                }
            }
        }
        std::process::exit(0);
    });
    let _conn = connection::Builder::session()?
        .name("org.freedesktop.FileManager1")?
        .name("org.freedesktop.impl.portal.desktop.kiki")?
        .serve_at("/org/freedesktop/FileManager1", FileManager { bridge: Arc::clone(&bridge) })?
        .serve_at("/org/freedesktop/portal/desktop", FileChooser { bridge })?
        .build()
        .await?;
    std::future::pending::<()>().await;
    Ok(())
}
