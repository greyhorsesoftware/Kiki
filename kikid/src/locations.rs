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
    if let Some(d) = crate::devices::location_for(scheme, authority) {
        return Some(d);
    }
    let locs = all();
    if let Some(l) = locs.iter().find(|l| l.str_field("plugin") == Some(scheme) && l.str_field("name") == Some(authority)) {
        return Some(l.clone());
    }
    let (user, host) = match authority.split_once('@') {
        Some((u, h)) => (Some(u), h),
        None => (None, authority),
    };
    locs.into_iter()
        .find(|l| l.str_field("plugin") == Some(scheme) && l.get("config").map(|c| c.str_field("host") == Some(host) && user.map(|u| c.str_field("username") == Some(u)).unwrap_or(true)).unwrap_or(false))
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

/// The picture a location wears in the sidebar: a path to an image file, or `None` to go back
/// to the plain glyph. Changed in place — it is not part of how the location connects, so the
/// live session is left alone and the location keeps its place in the list.
pub fn set_image(name: &str, image: Option<&str>) -> std::io::Result<()> {
    let mut items = all();
    let Some(Value::Obj(m)) = items.iter_mut().find(|l| l.str_field("name") == Some(name)) else {
        return Err(std::io::Error::new(std::io::ErrorKind::NotFound, format!("no location called {name}")));
    };
    match image.filter(|i| !i.is_empty()) {
        Some(i) => m.insert("image".to_string(), Value::Str(i.to_string())),
        None => m.remove("image"),
    };
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
    plugin::describe(scheme).and_then(|d| d.get("secretFields").and_then(Value::as_arr).map(|a| a.iter().filter_map(|v| v.as_str().map(str::to_string)).collect())).unwrap_or_default()
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
        let mut child = tool().args(["store", "--label", &format!("kiki: {location} {field}"), "app", "kiki", "location", location, "field", field]).spawn().map_err(|e| VfsError::Io(format!("secret-tool: {e}")))?;
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

/// A plugin that verifies certificates itself refuses one nobody has accepted, and the refusal
/// IS the fingerprint (`Invalid`, field `fingerprint`). SFTP answers the same question with a
/// reply instead. Either way it is "here is the key; has anyone said yes to it?".
pub(crate) fn unaccepted_fingerprint(e: &VfsError) -> Option<String> {
    match e {
        VfsError::Io(m) => m.strip_prefix("Invalid/fingerprint: ").filter(|f| !f.is_empty()).map(str::to_string),
        _ => None,
    }
}

/// How a refusal to connect to a never-verified server starts; the key's fingerprint follows.
/// The shell recognises it and offers the verification instead of showing a bare error.
pub const UNVERIFIED_PREFIX: &str = "this server's key has not been verified yet: ";

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
    let pinned = config.str_field("trustedFingerprint").map(str::to_string);
    let reply = plugin.request(Value::obj().s("type", "Connect").s("location", name.clone()).s("role", role).v("config", config).v("secrets", secrets).done()).map_err(|e| match unaccepted_fingerprint(&e) {
        Some(fp) => VfsError::Io(format!("{UNVERIFIED_PREFIX}{fp}")),
        None => e,
    })?;
    // A location added without being checked has never had its server's key looked at.
    match key_verdict(pinned.as_deref(), reply.str_field("fingerprint"), reply.get("knownHost").and_then(Value::as_bool).unwrap_or(false)) {
        KeyVerdict::Proceed => {}
        KeyVerdict::Pin(fp) => {
            let mut pinned_location = location.clone();
            if let Value::Obj(m) = &mut pinned_location {
                let mut c = match m.get("config").cloned() { Some(Value::Obj(c)) => c, _ => BTreeMap::new() };
                c.insert("trustedFingerprint".into(), Value::Str(fp));
                m.insert("config".into(), Value::Obj(c));
            }
            let _ = upsert(pinned_location);
        }
        KeyVerdict::Refuse(fp) => {
            let _ = plugin.request(Value::obj().s("type", "Disconnect").s("location", name.clone()).s("role", role).done());
            return Err(VfsError::Io(format!("{UNVERIFIED_PREFIX}{fp}")));
        }
    }
    let s = Arc::new(Session { plugin, location: name, role: role.to_string() });
    sessions().lock().unwrap().insert(key, Arc::clone(&s));
    Ok(s)
}

/// Location names with a live browse session (devices show these as connected).
pub fn connected_names() -> Vec<String> {
    sessions().lock().unwrap().iter().filter(|(_, s)| s.plugin.alive()).map(|(_, s)| s.location.clone()).collect()
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

/// Validate + connect a candidate location with the given secrets, without saving. Returns the
/// server's key fingerprint when the plugin reports one (SFTP does), and whether the plugin
/// found that key in a record the user already keeps — `~/.ssh/known_hosts`.
pub fn test(location: &Value, secrets: &Value) -> Result<(Option<String>, bool), VfsError> {
    let scheme = location.str_field("plugin").ok_or(VfsError::Io("missing plugin".into()))?;
    let plugin = plugin::get(scheme)?;
    let config = location.get("config").cloned().unwrap_or(Value::Obj(BTreeMap::new()));
    plugin.request(Value::obj().s("type", "Validate").v("config", config.clone()).done())?;
    let name = format!("_test_{}", location.str_field("name").unwrap_or("x"));
    let reply = match plugin.request(Value::obj().s("type", "Connect").s("location", name.clone()).s("role", "browse").v("config", config).v("secrets", secrets.clone()).done()) {
        Ok(r) => r,
        Err(e) => match unaccepted_fingerprint(&e) {
            Some(fp) => return Ok((Some(fp), false)),
            None => return Err(e),
        },
    };
    let _ = plugin.request(Value::obj().s("type", "Disconnect").s("location", name).s("role", "browse").done());
    Ok((reply.str_field("fingerprint").map(str::to_string), reply.get("knownHost").and_then(Value::as_bool).unwrap_or(false)))
}

/// Add or update: validate, connect, store secrets, then write the file.
///
/// A server that identifies itself with a key — SFTP — is not saved until the user has seen that
/// key and said yes. The first attempt comes back `Ok(Some(fingerprint))`, which is the shell's
/// cue to show it; the shell asks again with `trust` set to the fingerprint it displayed, and
/// that is what gets written as `trustedFingerprint`. Every later connection is checked against
/// it, and a server whose key has changed is refused with a message that says so.
pub fn save(location: Value, secrets: &Value, trust: Option<&str>, check: bool) -> Result<Option<String>, VfsError> {
    let mut location = location;
    // "Add": take what was typed. The fields are validated (that needs no network), the secrets
    // are stored, the file is written — and nothing is looked up, resolved or connected to. The
    // server's key is therefore NOT verified yet; `connect` below refuses an unknown one until
    // it has been (through "Save and Connect"), so skipping the check here never becomes
    // trusting whoever answers first.
    if !check {
        let scheme = location.str_field("plugin").ok_or(VfsError::Io("missing plugin".into()))?;
        let config = location.get("config").cloned().unwrap_or(Value::Obj(BTreeMap::new()));
        plugin::get(scheme)?.request(Value::obj().s("type", "Validate").v("config", config).done())?;
        return store(location, secrets);
    }
    if let Some(fp) = trust {
        let mut config = match location.get("config").cloned() {
            Some(Value::Obj(m)) => m,
            _ => BTreeMap::new(),
        };
        config.insert("trustedFingerprint".into(), Value::Str(fp.to_string()));
        if let Value::Obj(m) = &mut location {
            m.insert("config".into(), Value::Obj(config));
        }
    }
    let pinned = location.get("config").and_then(|c| c.str_field("trustedFingerprint")).map(str::to_string);
    // Check with what was typed, over what is already in the keyring: editing a saved location
    // (or verifying one that was added unchecked) does not mean typing its password again.
    let mut for_test = match secrets_for(&location) { Value::Obj(m) => m, _ => BTreeMap::new() };
    if let Value::Obj(given) = secrets {
        for (k, v) in given {
            for_test.insert(k.clone(), v.clone());
        }
    }
    let (seen, known_host) = test(&location, &Value::Obj(for_test))?;
    if let Some(fp) = &seen {
        if pinned.is_none() {
            if !known_host {
                return Ok(Some(fp.clone()));   // the user has not accepted this key yet
            }
            // ssh already knows this host and this key. That is the same verification, done
            // once, by hand; pin it rather than asking the question a second time.
            if let Value::Obj(m) = &mut location {
                let mut config = match m.get("config").cloned() {
                    Some(Value::Obj(c)) => c,
                    _ => BTreeMap::new(),
                };
                config.insert("trustedFingerprint".into(), Value::Str(fp.clone()));
                m.insert("config".into(), Value::Obj(config));
            }
        }
    }
    store(location, secrets)
}

/// Secrets to the keyring, the location to its file, and any live session on the old details
/// dropped.
fn store(location: Value, secrets: &Value) -> Result<Option<String>, VfsError> {
    let name = location.str_field("name").ok_or(VfsError::Io("missing name".into()))?.to_string();
    if let Value::Obj(m) = secrets {
        for (k, v) in m {
            if let Some(s) = v.as_str() {
                keyring::store(&name, k, s)?;
            }
        }
    }
    disconnect(&name);
    upsert(location).map_err(|e| VfsError::Io(e.to_string()))?;
    Ok(None)
}

/// What to do about the key a server offered to a SAVED location, given what the location
/// already holds. Pure, so it can be tested without a server.
#[derive(Debug, PartialEq)]
pub(crate) enum KeyVerdict {
    /// Nothing to decide: the plugin reports no key, or this one is the pinned one (the plugin
    /// itself refuses a pinned key that changed).
    Proceed,
    /// Never verified, but `~/.ssh/known_hosts` vouches for it: pin it and carry on.
    Pin(String),
    /// Never verified and unknown to ssh too: refuse. Connecting would be trusting whoever
    /// answered first, without ever having shown the key to anybody.
    Refuse(String),
}

pub(crate) fn key_verdict(pinned: Option<&str>, offered: Option<&str>, known_host: bool) -> KeyVerdict {
    match (pinned, offered) {
        (Some(_), _) | (None, None) => KeyVerdict::Proceed,
        (None, Some(fp)) if known_host => KeyVerdict::Pin(fp.to_string()),
        (None, Some(fp)) => KeyVerdict::Refuse(fp.to_string()),
    }
}

pub fn json_list() -> Value {
    Value::Arr(all())
}

pub fn scheme_set() -> HashSet<String> {
    plugin::available().into_iter().collect()
}

#[cfg(test)]
mod key_verdict_tests {
    use super::*;

    /// A location added without being checked has never had its server's key looked at. What
    /// the first connect does about that decides whether "Add" is safe.
    #[test]
    fn a_refusal_that_is_a_fingerprint_is_a_question_not_a_failure() {
        let fp = "1B:9D:2F:F6";
        assert_eq!(unaccepted_fingerprint(&VfsError::Io(format!("Invalid/fingerprint: {fp}"))), Some(fp.to_string()));
        assert_eq!(unaccepted_fingerprint(&VfsError::Io("Invalid/host: no such host".into())), None);
        assert_eq!(unaccepted_fingerprint(&VfsError::Io("Invalid/fingerprint: ".into())), None, "nothing seen is nothing to show");
        assert_eq!(unaccepted_fingerprint(&VfsError::Denied), None);
    }

    #[test]
    fn an_unverified_unknown_key_is_refused_not_trusted() {
        assert_eq!(key_verdict(None, Some("SHA256:abc"), false), KeyVerdict::Refuse("SHA256:abc".into()));
    }

    #[test]
    fn a_key_ssh_already_vouches_for_is_pinned() {
        assert_eq!(key_verdict(None, Some("SHA256:abc"), true), KeyVerdict::Pin("SHA256:abc".into()));
    }

    #[test]
    fn a_pinned_location_and_a_plugin_with_no_keys_just_connect() {
        assert_eq!(key_verdict(Some("SHA256:abc"), Some("SHA256:abc"), false), KeyVerdict::Proceed);
        assert_eq!(key_verdict(None, None, false), KeyVerdict::Proceed);      // FTPS: no fingerprint reported
    }
}

#[cfg(test)]
mod image_tests {
    use super::*;

    #[test]
    fn an_image_is_set_and_cleared_in_place() {
        let _guard = crate::ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let dir = std::env::temp_dir().join(format!("kiki-locimg-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::env::set_var("KIKI_CONFIG_DIR", &dir);
        upsert(Value::obj().s("name", "first").s("plugin", "sftp").done()).unwrap();
        upsert(Value::obj().s("name", "second").s("plugin", "sftp").done()).unwrap();

        set_image("first", Some("/pics/nas.png")).unwrap();
        let names: Vec<String> = all().iter().map(|l| l.str_field("name").unwrap().to_string()).collect();
        assert_eq!(names, ["first", "second"], "the location keeps its place");
        assert_eq!(find("first").unwrap().str_field("image"), Some("/pics/nas.png"));
        assert_eq!(find("second").unwrap().str_field("image"), None);

        set_image("first", Some("")).unwrap();
        assert_eq!(find("first").unwrap().str_field("image"), None, "empty clears it");
        assert_eq!(find("first").unwrap().str_field("plugin"), Some("sftp"));
        assert!(set_image("nobody", Some("/x.png")).is_err());

        std::fs::remove_dir_all(&dir).unwrap();
        std::env::remove_var("KIKI_CONFIG_DIR");
    }
}
