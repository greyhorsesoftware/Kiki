//! Share plugin host (plan 18): `kiki-share-<id>` processes, their config and secrets, and
//! Share as a job that fetches remote files and zips folders before handing local paths over.

use crate::json::Value;
use crate::plugin::{Msg, Plugin};
use crate::vfs::uri::Uri;
use crate::vfs::VfsError;
use std::collections::{BTreeMap, HashMap};
use std::path::PathBuf;
use std::sync::{Arc, Mutex, OnceLock};

pub fn share_dirs() -> Vec<PathBuf> {
    crate::plugin::plugin_dirs()
}

pub fn available() -> Vec<(String, PathBuf)> {
    let mut out: Vec<(String, PathBuf)> = Vec::new();
    for d in share_dirs() {
        if let Ok(rd) = std::fs::read_dir(&d) {
            for e in rd.flatten() {
                let name = e.file_name().to_string_lossy().into_owned();
                if let Some(id) = name.strip_prefix("kiki-plugin-share-") {
                    if !crate::plugin::runnable(&e.path()) {
                        continue;
                    }
                    if !out.iter().any(|(i, _)| i == id) {
                        out.push((id.to_string(), e.path()));
                    }
                }
            }
        }
    }
    out.sort();
    out
}

struct Registry {
    running: HashMap<String, Arc<Plugin>>,
    described: HashMap<String, Value>,
}

fn registry() -> &'static Mutex<Registry> {
    static R: OnceLock<Mutex<Registry>> = OnceLock::new();
    R.get_or_init(|| Mutex::new(Registry { running: HashMap::new(), described: HashMap::new() }))
}

fn get(id: &str) -> Result<Arc<Plugin>, VfsError> {
    if let Some(p) = registry().lock().unwrap().running.get(id) {
        if p.alive() {
            return Ok(Arc::clone(p));
        }
    }
    let (_, bin) = available().into_iter().find(|(i, _)| i == id).ok_or_else(|| VfsError::Io(format!("no share plugin {id}")))?;
    let p = Plugin::spawn_path(&bin, id)?;
    let d = p.request(Value::obj().s("type", "Describe").done())?;
    let mut r = registry().lock().unwrap();
    r.described.insert(id.to_string(), d);
    r.running.insert(id.to_string(), Arc::clone(&p));
    Ok(p)
}

/// What the plugin says about itself when the user has said nothing: on, unless it ships off.
fn default_enabled(id: &str) -> bool {
    registry().lock().unwrap().described.get(id).and_then(|d| d.get("defaultEnabled")).and_then(Value::as_bool).unwrap_or(true)
}

pub fn config_for(id: &str) -> (Value, Value) {
    let all = crate::config::read_named("share.toml");
    let cfg = all.get(id).cloned().unwrap_or(Value::Obj(BTreeMap::new()));
    let enabled = cfg.get("enabled").and_then(Value::as_bool).unwrap_or_else(|| default_enabled(id));
    let mut secrets = BTreeMap::new();
    if let Some(d) = registry().lock().unwrap().described.get(id) {
        for k in d.get("secretFields").and_then(Value::as_arr).map(|a| a.iter().filter_map(Value::as_str).map(str::to_string).collect::<Vec<_>>()).unwrap_or_default() {
            if let Some(v) = crate::locations::keyring::lookup(&format!("share:{id}"), &k) {
                secrets.insert(k, Value::Str(v));
            }
        }
    }
    let mut c = cfg.clone();
    if let Value::Obj(m) = &mut c {
        m.insert("enabled".into(), Value::Bool(enabled));
    }
    (c, Value::Obj(secrets))
}

fn on_path(bin: &str) -> bool {
    std::env::var_os("PATH").map(|p| std::env::split_paths(&p).any(|d| d.join(bin).is_file())).unwrap_or(false)
}

pub fn list_json() -> Value {
    let mut out = Vec::new();
    for (id, _) in available() {
        let d = match get(&id) {
            Ok(p) => registry().lock().unwrap().described.get(&id).cloned().unwrap_or_else(|| p.describe().clone()),
            Err(e) => {
                eprintln!("share plugin {id}: {}", e.message());
                continue;
            }
        };
        let (cfg, _) = config_for(&id);
        let mut o = d.clone();
        if let Value::Obj(m) = &mut o {
            m.insert("enabled".into(), Value::Bool(cfg.get("enabled").and_then(Value::as_bool).unwrap_or_else(|| d.get("defaultEnabled").and_then(Value::as_bool).unwrap_or(true))));
            m.insert("configured".into(), Value::Bool(true));
            // Looked for now, not when the plugin described itself: installing the program
            // brings the entry to life without restarting anything.
            let missing: Vec<&str> = d.get("requires").and_then(Value::as_arr).map(|a| a.iter().filter_map(Value::as_str).filter(|b| !on_path(b)).collect()).unwrap_or_default();
            if !missing.is_empty() {
                m.insert("unavailable".into(), Value::Str(format!("{} is not installed", missing.join(", "))));
            }
            m.insert("config".into(), cfg);
        }
        out.push(o);
    }
    Value::Arr(out)
}

pub fn targets(id: &str, query: Option<&str>) -> Result<Value, VfsError> {
    let p = get(id)?;
    let (cfg, secrets) = config_for(id);
    let mut req = Value::obj().s("type", "Targets").v("config", cfg).v("secrets", secrets);
    if let Some(q) = query {
        req = req.s("query", q);
    }
    let r = p.request(req.done())?;
    Ok(r.get("targets").cloned().unwrap_or(Value::Arr(vec![])))
}

pub fn configure(id: &str, config: &Value, secrets: &Value) -> Result<(), VfsError> {
    let p = get(id)?;
    p.request(Value::obj().s("type", "Configure").v("config", config.clone()).v("secrets", secrets.clone()).done())?;
    if let Value::Obj(m) = secrets {
        for (k, v) in m {
            if let Some(s) = v.as_str() {
                crate::locations::keyring::store(&format!("share:{id}"), k, s)?;
            }
        }
    }
    let mut all = match crate::config::read_named("share.toml") {
        Value::Obj(m) => m,
        _ => BTreeMap::new(),
    };
    all.insert(id.to_string(), config.clone());
    crate::config::write_named("share.toml", &Value::Obj(all)).map_err(|e| VfsError::Io(e.to_string()))
}

/// Runs a share inside a job: materialise local files, then stream the plugin's progress.
pub fn run(job: &crate::jobs::Job, id: &str, uris: &[Uri], target: Option<&str>, compose: &Value, cancel: &std::sync::atomic::AtomicBool) -> Result<Value, VfsError> {
    let p = get(id)?;
    let d = registry().lock().unwrap().described.get(id).cloned().unwrap_or(Value::Null);
    let accepts_folders = d.get("accepts").and_then(|a| a.get("folders")).and_then(Value::as_bool).unwrap_or(false);
    let tmp = std::env::temp_dir().join(format!("kiki-share-{}", job.id));
    std::fs::create_dir_all(&tmp)?;
    let mut files: Vec<String> = Vec::new();
    for u in uris {
        if cancel.load(std::sync::atomic::Ordering::Relaxed) {
            return Err(VfsError::Io("cancelled".into()));
        }
        let local = if u.is_local() {
            u.to_path()
        } else {
            // Fetch through the location's plugin into the temp dir.
            let (session, rpath) = crate::locations::resolve(u)?;
            let dst = tmp.join(u.name());
            let mut f = std::fs::File::create(&dst)?;
            session.plugin.read_stream(Value::obj().s("type", "Read").s("location", session.location.clone()).s("path", rpath).done(), |m| {
                if let Msg::Binary(b) = m {
                    use std::io::Write;
                    let _ = f.write_all(&b);
                }
            })?;
            dst
        };
        if local.is_dir() && !accepts_folders {
            let zip = tmp.join(format!("{}.zip", local.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| "folder".into())));
            crate::archive::compress(std::slice::from_ref(&local), &zip, "zip", cancel, &mut |_| {})?;
            files.push(zip.to_string_lossy().into_owned());
        } else {
            files.push(local.to_string_lossy().into_owned());
        }
    }
    let (cfg, secrets) = config_for(id);
    let mut req = Value::obj()
        .s("type", "Share")
        .v("config", cfg)
        .v("secrets", secrets)
        .v("uris", Value::Arr(files.iter().map(|f| Value::Str(Uri::from_path(std::path::Path::new(f)).to_string())).collect()))
        .v("compose", compose.clone());
    if let Some(t) = target {
        req = req.s("target", t);
    }
    let r = p.request_stream(req.done(), |m| {
        if let Msg::Json(v) = m {
            if v.str_field("event") == Some("Progress") {
                job.set_totals(v.u64_field("total").unwrap_or(0), v.u64_field("bytesTotal").unwrap_or(0));
                job.set_progress(v.u64_field("done").unwrap_or(0), v.u64_field("bytes").unwrap_or(0));
            }
        }
    });
    let _ = std::fs::remove_dir_all(&tmp);
    r
}

pub fn running_named(id: &str) -> bool {
    registry().lock().unwrap().running.get(id).map(|p| p.alive()).unwrap_or(false)
}

pub fn reap_idle() {
    let idle: Vec<(String, Arc<Plugin>)> = registry().lock().unwrap().running.iter().filter(|(_, p)| p.alive() && !p.busy() && p.idle_for() >= crate::plugin::IDLE_EXIT).map(|(k, p)| (k.clone(), Arc::clone(p))).collect();
    for (k, p) in idle {
        p.shutdown();
        registry().lock().unwrap().running.remove(&k);
    }
}
