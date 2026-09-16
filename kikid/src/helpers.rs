//! Helper processes (highlight, dbus, ai): plugin framing, found under the helper directories.

use crate::plugin::Plugin;
use crate::vfs::VfsError;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, OnceLock};

pub fn helper_dirs() -> Vec<PathBuf> {
    let mut v = Vec::new();
    if let Ok(d) = std::env::var("KIKI_HELPER_DIR") {
        v.push(PathBuf::from(d));
    }
    v.push(crate::config::home().join(".local/lib/kiki/helpers"));
    v.push(PathBuf::from("/usr/lib/kiki/helpers"));
    v
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
    running().lock().unwrap().insert(name.to_string(), Arc::clone(&p));
    Ok(p)
}

pub fn highlight() -> Result<Arc<Plugin>, VfsError> {
    get("kiki-helper-highlight")
}
