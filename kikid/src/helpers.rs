//! Service plugins (highlight, dbus, ai): fixed-name plugins the daemon uses itself. Same directory as every other plugin.

use crate::plugin::Plugin;
use crate::vfs::VfsError;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, OnceLock};

pub fn helper_dirs() -> Vec<PathBuf> {
    crate::plugin::plugin_dirs()
}

pub fn find(name: &str) -> Option<PathBuf> {
    helper_dirs().into_iter().map(|d| d.join(name)).find(|p| p.is_file())
}

fn running() -> &'static Mutex<std::collections::HashMap<String, Arc<Plugin>>> {
    static R: OnceLock<Mutex<std::collections::HashMap<String, Arc<Plugin>>>> = OnceLock::new();
    R.get_or_init(|| Mutex::new(std::collections::HashMap::new()))
}

/// Spawns (or reuses) a helper by binary name, speaking the plugin framing with `Describe` optional.
pub fn get(name: &str) -> Result<Arc<Plugin>, VfsError> {
    if let Some(p) = running().lock().unwrap().get(name) {
        if p.alive() {
            return Ok(Arc::clone(p));
        }
    }
    let bin = find(name).ok_or_else(|| VfsError::Io(format!("{name} is not installed")))?;
    let p = Plugin::spawn_path(&bin, name)?;
    if let Ok(d) = p.request(crate::json::Value::obj().s("type", "Describe").done()) {
        let _ = p.set_describe(d);
    }
    running().lock().unwrap().insert(name.to_string(), Arc::clone(&p));
    Ok(p)
}

pub fn highlight() -> Result<Arc<Plugin>, VfsError> {
    get("kiki-plugin-highlight")
}

pub fn running_named(name: &str) -> bool {
    running().lock().unwrap().get(name).map(|p| p.alive()).unwrap_or(false)
}

pub fn reap_idle() {
    // Service plugins other than dbus (which must stay up to serve the bus) idle out too.
    let idle: Vec<(String, Arc<Plugin>)> =
        running().lock().unwrap().iter().filter(|(k, p)| !k.ends_with("dbus") && p.alive() && !p.busy() && p.idle_for() >= crate::plugin::IDLE_EXIT).map(|(k, p)| (k.clone(), Arc::clone(p))).collect();
    for (k, p) in idle {
        p.shutdown();
        running().lock().unwrap().remove(&k);
    }
}
