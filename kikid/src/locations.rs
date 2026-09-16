//! Saved locations, their secrets (through `secret-tool`, libsecret's CLI), and the
//! resolver from a URI's scheme and authority to a connected plugin session.

use crate::json::Value;
use crate::plugin::{self, Plugin};
use crate::vfs::uri::Uri;
use crate::vfs::VfsError;
use std::collections::{BTreeMap, HashSet};
use std::io::Write;
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex, OnceLock};

// ---------------------------------------------------------------- config file

pub fn all() -> Vec<Value> {
    let v = crate::config::read_named("locations.toml");
    match v.get("location") {
        Some(Value::Arr(a)) => a.clone(),
        _ => Vec::new(),
    }
}

pub fn find(name: &str) -> Option<Value> {
    all().into_iter().find(|l| l.str_field("name") == Some(name))
}

/// Finds by authority: the location name, else a `user@host` match against config.
pub fn resolve_authority(scheme: &str, authority: &str) -> Option<Value> {
    let locs = all();
    if let Some(l) = locs.iter().find(|l| l.str_field("plugin") == Some(scheme) && l.str_field("name") == Some(authority)) {
        return Some(l.clone());
    }
    let (user, host) = match authority.split_once('@') {
        Some((u, h)) => (Some(u), h),
        None => (None, authority),
    };
    locs.into_iter().find(|l| {
        l.str_field("plugin") == Some(scheme)
            && l.get("config").map(|c| c.str_field("host") == Some(host) && user.map(|u| c.str_field("username") == Some(u)).unwrap_or(true)).unwrap_or(false)
    })
}

fn write_all(items: &[Value]) -> std::io::Result<()> {
    let mut m = BTreeMap::new();
    m.insert("location".to_string(), Value::Arr(items.to_vec()));
    crate::config::write_named("locations.toml", &Value::Obj(m))
}

pub fn upsert(location: Value) -> std::io::Result<()> {
    let name = location.str_field("name").unwrap_or("").to_string();
    let mut items: Vec<Value> = all().into_iter().filter(|l| l.str_field("name") != Some(&name)).collect();
    items.push(location);
    write_all(&items)
}

pub fn remove(name: &str) -> std::io::Result<()> {
    let items: Vec<Value> = all().into_iter().filter(|l| l.str_field("name") != Some(name)).collect();
    write_all(&items)?;
    for key in secret_keys(name) {
        keyring::clear(name, &key);
    }
    sessions().lock().unwrap().remove(&format!("{name}\u{0}browse"));
    Ok(())
}

fn secret_keys(name: &str) -> Vec<String> {
    let Some(l) = find(name) else { return Vec::new() };
    let scheme = l.str_field("plugin").unwrap_or("");
    plugin::describe(scheme)
        .and_then(|d| d.get("secretFields").and_then(Value::as_arr).map(|a| a.iter().filter_map(|v| v.as_str().map(str::to_string)).collect()))
        .unwrap_or_default()
}

// ---------------------------------------------------------------- keyring

pub mod keyring {
    use super::*;

    fn tool() -> Command {
        let mut c = Command::new(std::env::var("KIKI_SECRET_TOOL").unwrap_or_else(|_| "secret-tool".into()));
        c.stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::null());
        c
    }

    pub fn store(location: &str, field: &str, secret: &str) -> Result<(), VfsError> {
        let mut child = tool()
            .args(["store", "--label", &format!("kiki: {location} {field}"), "app", "kiki", "location", location, "field", field])
            .spawn()
            .map_err(|e| VfsError::Io(format!("secret-tool: {e}")))?;
        child.stdin.take().unwrap().write_all(secret.as_bytes()).map_err(|e| VfsError::Io(e.to_string()))?;
        let st = child.wait().map_err(|e| VfsError::Io(e.to_string()))?;
        if st.success() {
            Ok(())
        } else {
            Err(VfsError::Io("keyring refused the secret (is a Secret Service running?)".into()))
        }
    }

    pub fn lookup(location: &str, field: &str) -> Option<String> {
        let out = tool().args(["lookup", "app", "kiki", "location", location, "field", field]).output().ok()?;
        if !out.status.success() {
            return None;
        }
        let s = String::from_utf8_lossy(&out.stdout).trim_end_matches('\n').to_string();
        if s.is_empty() {
            None
        } else {
            Some(s)
        }
    }

    pub fn clear(location: &str, field: &str) {
        let _ = tool().args(["clear", "app", "kiki", "location", location, "field", field]).status();
    }
}

// ---------------------------------------------------------------- sessions

pub struct Session {
    pub plugin: Arc<Plugin>,
    pub location: String,
    pub role: String,
}

fn sessions() -> &'static Mutex<BTreeMap<String, Arc<Session>>> {
    static S: OnceLock<Mutex<BTreeMap<String, Arc<Session>>>> = OnceLock::new();
    S.get_or_init(|| Mutex::new(BTreeMap::new()))
}

fn secrets_for(location: &Value) -> Value {
    let name = location.str_field("name").unwrap_or("");
    let mut m = BTreeMap::new();
    for key in secret_keys(name) {
        if let Some(v) = keyring::lookup(name, &key) {
            m.insert(key, Value::Str(v));
        }
    }
    Value::Obj(m)
}

/// Connects (or reuses) the plugin session for a saved location and role.
pub fn connect(location: &Value, role: &str, secrets: Option<Value>) -> Result<Arc<Session>, VfsError> {
    let name = location.str_field("name").ok_or(VfsError::Io("location without name".into()))?.to_string();
    let scheme = location.str_field("plugin").ok_or(VfsError::Io("location without plugin".into()))?.to_string();
    let key = format!("{name}\u{0}{role}");
    if let Some(s) = sessions().lock().unwrap().get(&key) {
        if s.plugin.alive() {
            return Ok(Arc::clone(s));
        }
    }
    let plugin = plugin::get(&scheme)?;
    let config = location.get("config").cloned().unwrap_or(Value::Obj(BTreeMap::new()));
    let secrets = secrets.unwrap_or_else(|| secrets_for(location));
    plugin.request(Value::obj().s("type", "Connect").s("location", name.clone()).s("role", role).v("config", config).v("secrets", secrets).done())?;
    let s = Arc::new(Session { plugin, location: name, role: role.to_string() });
    sessions().lock().unwrap().insert(key, Arc::clone(&s));
    Ok(s)
}

pub fn disconnect(name: &str) {
    let mut s = sessions().lock().unwrap();
    let keys: Vec<String> = s.keys().filter(|k| k.starts_with(&format!("{name}\u{0}"))).cloned().collect();
    for k in keys {
        if let Some(sess) = s.remove(&k) {
            let _ = sess.plugin.request(Value::obj().s("type", "Disconnect").s("location", name).s("role", sess.role.clone()).done());
        }
    }
}

/// Resolves a remote URI to a browsing session plus the path inside the location.
pub fn resolve(uri: &Uri) -> Result<(Arc<Session>, String), VfsError> {
    let loc = resolve_authority(&uri.scheme, &uri.authority).ok_or_else(|| VfsError::Io(format!("no location for {}://{}", uri.scheme, uri.authority)))?;
    let s = connect(&loc, "browse", None)?;
    Ok((s, uri.path.clone()))
}

/// Validate + connect a candidate location with the given secrets, without saving.
pub fn test(location: &Value, secrets: &Value) -> Result<(), VfsError> {
    let scheme = location.str_field("plugin").ok_or(VfsError::Io("missing plugin".into()))?;
    let plugin = plugin::get(scheme)?;
    let config = location.get("config").cloned().unwrap_or(Value::Obj(BTreeMap::new()));
    plugin.request(Value::obj().s("type", "Validate").v("config", config.clone()).done())?;
    let name = format!("_test_{}", location.str_field("name").unwrap_or("x"));
    plugin.request(Value::obj().s("type", "Connect").s("location", name.clone()).s("role", "browse").v("config", config).v("secrets", secrets.clone()).done())?;
    let _ = plugin.request(Value::obj().s("type", "Disconnect").s("location", name).s("role", "browse").done());
    Ok(())
}

/// Add or update: validate, connect, store secrets, then write the file.
pub fn save(location: Value, secrets: &Value) -> Result<(), VfsError> {
    test(&location, secrets)?;
    let name = location.str_field("name").ok_or(VfsError::Io("missing name".into()))?.to_string();
    if let Value::Obj(m) = secrets {
        for (k, v) in m {
            if let Some(s) = v.as_str() {
                keyring::store(&name, k, s)?;
            }
        }
    }
    disconnect(&name);
    upsert(location).map_err(|e| VfsError::Io(e.to_string()))
}

pub fn json_list() -> Value {
    Value::Arr(all())
}

pub fn scheme_set() -> HashSet<String> {
    plugin::available().into_iter().collect()
}
