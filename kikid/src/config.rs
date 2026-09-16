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
    let mut v: Vec<Value> = items.into_iter().map(|(n, p)| Value::obj().s("name", n).s("uri", crate::vfs::uri::Uri::from_path(&p).to_string()).done()).collect();
    v.push(Value::obj().s("name", "Trash").s("uri", "trash:///").done());
    Value::Arr(v)
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
    m.insert(
        "view".into(),
        Value::obj()
            .s("default", "list")
            .s("sort", "name")
            .s("order", "asc")
            .b("inspector", false)
            .b("rememberPerFolder", true)
            .v("columns", Value::Arr(vec![Value::Str("mtime".into()), Value::Str("size".into()), Value::Str("kind".into())]))
            .done(),
    );
    m.insert("timers".into(), Value::obj().u("toastMs", 8000).u("searchDebounceMs", 150).u("mirrorPollMs", 400).done());
    m.insert("editor".into(), Value::obj().s("terminal", "auto").s("placement", "right").u("tabWidth", 4).done());
    m.insert("git".into(), Value::obj().b("enabled", true).s("showIgnored", "dim").s("folders", "aggregate").done());
    m.insert("project".into(), Value::obj().u("width", 320).b("arrange", true).b("agent", true).done());
    m.insert("jarvis".into(), Value::obj().s("provider", "omarchy").s("cliCommand", "").done());
    m.insert("index".into(), Value::obj().v("roots", Value::Arr(vec![])).v("excludes", Value::Arr(vec![])).done());
    m.insert("integration".into(), Value::obj().b("asked", false).done());
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
        ("/", "search", "02"),
        ("Ctrl+L", "edit path", "02"),
        ("Ctrl+1 / 2 / 3", "icon / list / columns", "02"),
        ("h j k l, arrows", "move selection; h/l pop and push columns", "02"),
        ("Enter", "open", "02"),
        ("Backspace, Alt+Left", "back", "02"),
        ("F5", "refresh", "02"),
        ("Ctrl+I", "inspector", "03"),
        ("Ctrl+C / X / V", "copy, cut, paste", "04"),
        ("F2", "rename", "04"),
        ("Del", "move to trash", "04"),
        ("Ctrl+Z / Ctrl+Shift+Z", "undo, redo", "04"),
        ("Ctrl+Shift+N", "new folder", "04"),
        ("Ctrl+Shift+S", "split", "07"),
        ("Tab", "switch pane (split)", "07"),
        ("F6", "move across (split)", "07"),
        ("Ctrl+M", "mirror", "08"),
        ("Tab (in search)", "cycle scope", "12"),
        ("e", "edit file / project mode on a folder", "13"),
        ("Alt+Enter", "open in default tool", "14"),
        ("Alt+Shift+Enter", "open in… list", "14"),
        ("Ctrl+Shift+P", "project mode", "16"),
        ("Alt+S", "share", "18"),
        ("Alt+Q", "AI query", "19"),
        ("Ctrl+,", "settings", "20"),
        ("?", "keybinding cheat sheet", "10"),
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
                        .s("device", dev)
                        .s("fsType", fstype)
                        .u("free", free)
                        .u("total", total)
                        .b("removable", removable)
                        .b("mounted", true)
                        .done(),
                );
            }
        }
        // Block devices with a filesystem that are not mounted (a plugged-in USB stick): the
        // sidebar lists them dimmed and mounts on click through udisks.
        if let Ok(o) = std::process::Command::new("lsblk").args(["-J", "-o", "PATH,LABEL,FSTYPE,MOUNTPOINT,RM,SIZE,TYPE,HOTPLUG"]).output() {
            if o.status.success() {
                let mounted: Vec<String> = out.iter().filter_map(|v| v.str_field("device").map(str::to_string)).collect();
                for v in unmounted_from_lsblk(&String::from_utf8_lossy(&o.stdout)) {
                    if !mounted.iter().any(|m| Some(m.as_str()) == v.str_field("device")) {
                        out.push(v);
                    }
                }
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

/// Unmounted filesystems from `lsblk -J` output: partitions with a filesystem and no mountpoint.
pub fn unmounted_from_lsblk(json: &str) -> Vec<Value> {
    let mut out = Vec::new();
    let Ok(v) = crate::json::parse(json.as_bytes()) else { return out };
    fn walk(v: &Value, out: &mut Vec<Value>) {
        let Some(devs) = v.get("blockdevices").or_else(|| v.get("children")).and_then(Value::as_arr) else { return };
        for d in devs {
            let fstype = d.str_field("fstype").unwrap_or("");
            let mnt = d.str_field("mountpoint").unwrap_or("");
            let ty = d.str_field("type").unwrap_or("");
            let usable = !fstype.is_empty() && !matches!(fstype, "swap" | "crypto_LUKS" | "LVM2_member" | "linux_raid_member") && matches!(ty, "part" | "disk" | "rom");
            if usable && mnt.is_empty() {
                let path = d.str_field("path").unwrap_or("").to_string();
                let label = d.str_field("label").filter(|l| !l.is_empty()).map(str::to_string).unwrap_or_else(|| path.rsplit('/').next().unwrap_or("").to_string());
                let rm = matches!(d.get("rm"), Some(Value::Bool(true))) || matches!(d.get("hotplug"), Some(Value::Bool(true)));
                out.push(
                    Value::obj().s("name", label).s("uri", "").s("device", path).s("fsType", fstype).u("free", 0).u("total", 0).b("removable", rm).b("mounted", false).s("size", d.str_field("size").unwrap_or("")).done(),
                );
            }
            walk(d, out);
        }
    }
    walk(&v, &mut out);
    out
}

fn udisks(args: &[&str]) -> Result<String, String> {
    let o = std::process::Command::new("udisksctl").args(args).arg("--no-user-interaction").output().map_err(|e| format!("udisksctl: {e}"))?;
    if o.status.success() {
        Ok(String::from_utf8_lossy(&o.stdout).trim().to_string())
    } else {
        let err = String::from_utf8_lossy(&o.stderr).trim().to_string();
        Err(if err.is_empty() { format!("udisksctl exited with {}", o.status) } else { err })
    }
}

/// Mount a block device through udisks; returns the mount point.
pub fn mount(device: &str) -> Result<String, String> {
    let out = udisks(&["mount", "-b", device])?;
    // "Mounted /dev/sdb1 at /run/media/david/STICK"
    Ok(out.split(" at ").nth(1).map(|s| s.trim_end_matches('.').to_string()).unwrap_or(out))
}

pub fn unmount(device: &str) -> Result<(), String> {
    udisks(&["unmount", "-b", device]).map(|_| ())
}

/// Unmount and power the drive off (safe to unplug). A non-removable device is only unmounted.
pub fn eject(device: &str) -> Result<(), String> {
    let _ = udisks(&["unmount", "-b", device]);
    let disk = device.trim_end_matches(|c: char| c.is_ascii_digit()).trim_end_matches('p');
    let _ = udisks(&["power-off", "-b", disk]);
    Ok(())
}

// ---------------------------------------------------------------- per-folder view memory (plan 02)

/// How many folders keep their own view and sort; the least recently set fall off the end.
pub const VIEW_PREFS_CAP: usize = 1000;

/// `views.toml`: `[[folder]] uri, view, sort, order, at`.
pub fn view_prefs() -> Value {
    let v = read_named("views.toml");
    let mut out = std::collections::BTreeMap::new();
    if let Some(Value::Arr(a)) = v.get("folder") {
        for f in a {
            if let Some(uri) = f.str_field("uri") {
                out.insert(uri.to_string(), Value::obj().s("view", f.str_field("view").unwrap_or("list")).s("sort", f.str_field("sort").unwrap_or("name")).s("order", f.str_field("order").unwrap_or("asc")).done());
            }
        }
    }
    Value::Obj(out)
}

pub fn set_view_pref(uri: &str, view: &str, sort: &str, order: &str) -> std::io::Result<()> {
    let v = read_named("views.toml");
    let mut list: Vec<Value> = match v.get("folder") {
        Some(Value::Arr(a)) => a.iter().filter(|f| f.str_field("uri") != Some(uri)).cloned().collect(),
        _ => Vec::new(),
    };
    list.push(Value::obj().s("uri", uri).s("view", view).s("sort", sort).s("order", order).u("at", crate::ops::unix_now()).done());
    if list.len() > VIEW_PREFS_CAP {
        list.sort_by_key(|f| f.u64_field("at").unwrap_or(0));
        let drop = list.len() - VIEW_PREFS_CAP;
        list.drain(..drop);
    }
    let mut m = BTreeMap::new();
    m.insert("folder".to_string(), Value::Arr(list));
    write_named("views.toml", &Value::Obj(m))
}

pub fn clear_view_prefs() -> std::io::Result<()> {
    let mut m = BTreeMap::new();
    m.insert("folder".to_string(), Value::Arr(vec![]));
    write_named("views.toml", &Value::Obj(m))
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

#[cfg(test)]
mod volume_tests {
    #[test]
    fn unmounted_partitions_from_lsblk() {
        let json = r#"{"blockdevices":[{"path":"/dev/sda","label":null,"fstype":null,"mountpoint":null,"rm":false,"size":"1T","type":"disk","hotplug":false,"children":[{"path":"/dev/sda1","label":"root","fstype":"ext4","mountpoint":"/","rm":false,"size":"1T","type":"part","hotplug":false}]},{"path":"/dev/sdb","label":null,"fstype":null,"mountpoint":null,"rm":true,"size":"32G","type":"disk","hotplug":true,"children":[{"path":"/dev/sdb1","label":"STICK","fstype":"vfat","mountpoint":null,"rm":true,"size":"32G","type":"part","hotplug":true}]},{"path":"/dev/sdc1","label":"","fstype":"swap","mountpoint":null,"rm":false,"size":"8G","type":"part","hotplug":false}]}"#;
        let v = super::unmounted_from_lsblk(json);
        assert_eq!(v.len(), 1);
        assert_eq!(v[0].str_field("name"), Some("STICK"));
        assert_eq!(v[0].str_field("device"), Some("/dev/sdb1"));
        assert_eq!(v[0].get("removable"), Some(&crate::json::Value::Bool(true)));
        assert_eq!(v[0].get("mounted"), Some(&crate::json::Value::Bool(false)));
    }
}

#[cfg(test)]
mod view_pref_tests {
    #[test]
    fn remembers_per_folder_and_caps() {
        let _g = crate::ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let d = std::env::temp_dir().join(format!("kiki-views-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::env::set_var("KIKI_CONFIG_DIR", &d);
        super::set_view_pref("file:///a", "icon", "mtime", "desc").unwrap();
        super::set_view_pref("file:///b", "columns", "name", "asc").unwrap();
        super::set_view_pref("file:///a", "list", "size", "asc").unwrap(); // overwrite, not duplicate
        let p = super::view_prefs();
        assert_eq!(p.get("file:///a").unwrap().str_field("view"), Some("list"));
        assert_eq!(p.get("file:///a").unwrap().str_field("sort"), Some("size"));
        assert_eq!(p.get("file:///b").unwrap().str_field("view"), Some("columns"));
        assert!(matches!(&p, crate::json::Value::Obj(m) if m.len() == 2));
        super::clear_view_prefs().unwrap();
        assert!(matches!(super::view_prefs(), crate::json::Value::Obj(m) if m.is_empty()));
        std::env::remove_var("KIKI_CONFIG_DIR");
        let _ = std::fs::remove_dir_all(&d);
    }
}
