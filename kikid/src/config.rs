//! Config files under ~/.config/kiki and ~/.local/state/kiki, and system volumes.

use crate::json::Value;
use crate::toml;
use std::collections::BTreeMap;
use std::path::PathBuf;

pub fn config_dir() -> PathBuf {
    if let Ok(d) = std::env::var("KIKI_CONFIG_DIR") {
        return PathBuf::from(d);
    }
    let base = std::env::var("XDG_CONFIG_HOME").map(PathBuf::from).unwrap_or_else(|_| home().join(".config"));
    base.join("kiki")
}

pub fn home() -> PathBuf {
    std::env::var("HOME").map(PathBuf::from).unwrap_or_else(|_| PathBuf::from("/"))
}

pub fn read_named(name: &str) -> Value {
    read_toml(name)
}

pub fn write_named(name: &str, v: &Value) -> std::io::Result<()> {
    write_toml(name, v)
}

fn read_toml(name: &str) -> Value {
    let path = config_dir().join(name);
    match std::fs::read_to_string(&path) {
        Ok(text) => match toml::parse(&text) {
            Ok(v) => v,
            Err(e) => {
                eprintln!("{}: {e}", path.display());
                Value::Obj(BTreeMap::new())
            }
        },
        Err(_) => Value::Obj(BTreeMap::new()),
    }
}

fn write_toml(name: &str, v: &Value) -> std::io::Result<()> {
    let dir = config_dir();
    std::fs::create_dir_all(&dir)?;
    let path = dir.join(name);
    let tmp = dir.join(format!(".{name}.tmp"));
    std::fs::write(&tmp, toml::write(v))?;
    std::fs::rename(tmp, path)
}

// ---------------------------------------------------------------- favorites

pub fn favorites() -> Value {
    let v = read_toml("favorites.toml");
    match v.get("favorite") {
        Some(Value::Arr(a)) if !a.is_empty() => Value::Arr(a.clone()),
        _ => default_favorites(),
    }
}

fn default_favorites() -> Value {
    let h = home();
    let mut items = vec![("Home", h.clone())];
    for name in ["Desktop", "Documents", "Downloads", "Pictures", "Projects", "Videos", "Music"] {
        let p = h.join(name);
        if p.is_dir() {
            items.push((name, p));
        }
    }
    Value::Arr(
        items
            .into_iter()
            .map(|(n, p)| Value::obj().s("name", n).s("uri", crate::vfs::uri::Uri::from_path(&p).to_string()).done())
            .collect(),
    )
}

pub fn set_favorites(items: &[Value]) -> std::io::Result<()> {
    let mut m = BTreeMap::new();
    m.insert("favorite".to_string(), Value::Arr(items.to_vec()));
    write_toml("favorites.toml", &Value::Obj(m))
}

// ---------------------------------------------------------------- settings

pub fn settings() -> Value {
    let v = read_toml("settings.toml");
    // Defaults the shell relies on; the file overrides key by key.
    let mut m = BTreeMap::new();
    m.insert("view".into(), Value::obj().s("default", "list").s("sort", "name").s("order", "asc").b("inspector", false).done());
    m.insert("timers".into(), Value::obj().u("toastMs", 8000).u("searchDebounceMs", 150).u("mirrorPollMs", 400).done());
    m.insert("editor".into(), Value::obj().s("terminal", "auto").s("placement", "right").u("tabWidth", 4).done());
    m.insert("git".into(), Value::obj().b("enabled", true).s("showIgnored", "dim").s("folders", "aggregate").done());
    m.insert("project".into(), Value::obj().u("width", 320).b("arrange", true).b("agent", true).done());
    merge(&mut m, &v);
    Value::Obj(m)
}

fn merge(into: &mut BTreeMap<String, Value>, from: &Value) {
    if let Value::Obj(f) = from {
        for (k, v) in f {
            match (into.get_mut(k), v) {
                (Some(Value::Obj(a)), Value::Obj(_)) => merge(a, v),
                _ => {
                    into.insert(k.clone(), v.clone());
                }
            }
        }
    }
}

pub fn set_settings(patch: &Value) -> std::io::Result<()> {
    let mut cur = match read_toml("settings.toml") {
        Value::Obj(m) => m,
        _ => BTreeMap::new(),
    };
    merge(&mut cur, patch);
    write_toml("settings.toml", &Value::Obj(cur))
}

pub fn socket_path_string() -> String {
    std::env::var("KIKI_SOCKET").unwrap_or_else(|_| format!("{}/kiki.sock", std::env::var("XDG_RUNTIME_DIR").unwrap_or_else(|_| "/tmp".into())))
}

pub fn reset_all() -> std::io::Result<()> {
    for f in ["settings.toml", "favorites.toml", "open-in.toml", "filters.toml"] {
        let p = config_dir().join(f);
        if p.exists() {
            std::fs::remove_file(p)?;
        }
    }
    Ok(())
}

/// The keymap, served from one table so the cheat sheet and the Settings page agree.
pub fn keymap() -> Value {
    let rows: &[(&str, &str, &str)] = &[
        ("/", "search", "02"), ("Ctrl+L", "edit path", "02"), ("Ctrl+1 / 2 / 3", "icon / list / columns", "02"),
        ("h j k l, arrows", "move selection; h/l pop and push columns", "02"), ("Enter", "open", "02"), ("Backspace, Alt+Left", "back", "02"),
        ("F5", "refresh", "02"), ("Ctrl+I", "inspector", "03"), ("Ctrl+C / X / V", "copy, cut, paste", "04"), ("F2", "rename", "04"),
        ("Del", "move to trash", "04"), ("Ctrl+Z / Ctrl+Shift+Z", "undo, redo", "04"), ("Ctrl+Shift+N", "new folder", "04"),
        ("Ctrl+Shift+S", "split", "07"), ("Tab", "switch pane (split)", "07"), ("F6", "move across (split)", "07"), ("Ctrl+M", "mirror", "08"),
        ("Tab (in search)", "cycle scope", "12"), ("e", "edit file / project mode on a folder", "13"), ("Alt+Enter", "open in default tool", "14"),
        ("Alt+Shift+Enter", "open in… list", "14"), ("Ctrl+Shift+P", "project mode", "16"), ("Alt+S", "share", "18"), ("Alt+Q", "AI query", "19"),
        ("Ctrl+,", "settings", "20"), ("?", "keybinding cheat sheet", "10"),
    ];
    Value::Arr(rows.iter().map(|(k, a, p)| Value::obj().s("key", *k).s("action", *a).s("plan", *p).done()).collect())
}

// ---------------------------------------------------------------- volumes

pub fn volumes() -> Value {
    let mut out = Vec::new();
    #[cfg(target_os = "linux")]
    {
        if let Ok(text) = std::fs::read_to_string("/proc/self/mounts") {
            for line in text.lines() {
                let f: Vec<&str> = line.split(' ').collect();
                if f.len() < 3 {
                    continue;
                }
                let (dev, mnt, fstype) = (f[0], f[1], f[2]);
                let real = dev.starts_with("/dev/") && !matches!(fstype, "squashfs" | "devtmpfs");
                if !real {
                    continue;
                }
                if mnt.starts_with("/boot") || mnt.starts_with("/proc") || mnt.starts_with("/sys") || mnt.starts_with("/snap") {
                    continue;
                }
                let mnt = mnt.replace("\\040", " ");
                let (free, total) = fs_space(&mnt);
                let removable = mnt.starts_with("/run/media/") || mnt.starts_with("/media/");
                let name = if mnt == "/" { "System".to_string() } else { std::path::Path::new(&mnt).file_name().map(|s| s.to_string_lossy().into_owned()).unwrap_or(mnt.clone()) };
                out.push(
                    Value::obj()
                        .s("name", name)
                        .s("uri", crate::vfs::uri::Uri::local(&mnt).map(|u| u.to_string()).unwrap_or_default())
                        .s("fsType", fstype)
                        .u("free", free)
                        .u("total", total)
                        .b("removable", removable)
                        .done(),
                );
            }
        }
    }
    #[cfg(not(target_os = "linux"))]
    {
        let (free, total) = fs_space("/");
        out.push(Value::obj().s("name", "System").s("uri", "file:///").s("fsType", "apfs").u("free", free).u("total", total).b("removable", false).done());
    }
    Value::Arr(out)
}

pub fn fs_space(path: &str) -> (u64, u64) {
    let c = match std::ffi::CString::new(path) {
        Ok(c) => c,
        Err(_) => return (0, 0),
    };
    let mut st: libc::statvfs = unsafe { std::mem::zeroed() };
    if unsafe { libc::statvfs(c.as_ptr(), &mut st) } != 0 {
        return (0, 0);
    }
    let frsize = st.f_frsize as u64;
    (st.f_bavail as u64 * frsize, st.f_blocks as u64 * frsize)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn favorites_round_trip_in_temp_config() {
        let _guard = crate::ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let dir = std::env::temp_dir().join(format!("kiki-config-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::env::set_var("KIKI_CONFIG_DIR", &dir);
        let defaults = favorites();
        assert!(!defaults.as_arr().unwrap().is_empty());
        let items = vec![Value::obj().s("name", "Work").s("uri", "file:///tmp/work").done()];
        set_favorites(&items).unwrap();
        let back = favorites();
        assert_eq!(back.as_arr().unwrap()[0].str_field("name"), Some("Work"));
        set_settings(&Value::obj().v("view", Value::obj().s("default", "icon").done()).done()).unwrap();
        let s = settings();
        assert_eq!(s.get("view").unwrap().str_field("default"), Some("icon"));
        assert_eq!(s.get("view").unwrap().str_field("sort"), Some("name")); // default kept
        assert!(volumes().as_arr().unwrap().iter().any(|v| v.u64_field("total").unwrap_or(0) > 0));
        std::fs::remove_dir_all(&dir).unwrap();
        std::env::remove_var("KIKI_CONFIG_DIR");
    }
}
