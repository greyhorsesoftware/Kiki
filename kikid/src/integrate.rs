//! Omarchy integration (plan 09): making kiki the default file manager for the current user,
//! and undoing it. The package installs system files under /usr; everything here is per-user
//! and reversible:
//!
//! 1. MIME: `inode/directory` (and the sftp/ftps scheme handlers) in `~/.config/mimeapps.list`.
//! 2. D-Bus: user-level activation files for `org.freedesktop.FileManager1` ("Show in folder")
//!    and the portal backend name, which win over the distro's files in `/usr/share`.
//! 3. Hyprland: a marked block in `~/.config/hypr/bindings.conf` with the launch keys and the
//!    floating rule for the chooser, reloaded with a config-error check and automatic rollback.
//! 4. Portal: `org.freedesktop.impl.portal.FileChooser=kiki;gtk` in
//!    `~/.config/xdg-desktop-portal/portals.conf`, then a portal restart.
//!
//! Every step is idempotent and reports what it did; `KIKI_INTEGRATE_NO_EXEC=1` skips the
//! external commands (tests), `KIKI_INTEGRATE_HOME` points at a scratch home.

use crate::json::Value;
use std::path::{Path, PathBuf};
use std::process::Command;

pub const DESKTOP_ID: &str = "org.kiki.App.desktop";
pub const PORTAL_NAME: &str = "org.freedesktop.impl.portal.desktop.kiki";
pub const BEGIN: &str = "# kiki: begin";
pub const END: &str = "# kiki: end";
pub const PARTS: [&str; 4] = ["mime", "dbus", "hypr", "portal"];

fn home() -> PathBuf {
    std::env::var("KIKI_INTEGRATE_HOME").map(PathBuf::from).unwrap_or_else(|_| crate::config::home())
}

fn config_home() -> PathBuf {
    if std::env::var("KIKI_INTEGRATE_HOME").is_ok() {
        return home().join(".config");
    }
    std::env::var("XDG_CONFIG_HOME").map(PathBuf::from).unwrap_or_else(|_| home().join(".config"))
}

fn data_home() -> PathBuf {
    if std::env::var("KIKI_INTEGRATE_HOME").is_ok() {
        return home().join(".local/share");
    }
    std::env::var("XDG_DATA_HOME").map(PathBuf::from).unwrap_or_else(|_| home().join(".local/share"))
}

fn no_exec() -> bool {
    std::env::var("KIKI_INTEGRATE_NO_EXEC").is_ok()
}

fn mimeapps() -> PathBuf {
    config_home().join("mimeapps.list")
}
fn services_dir() -> PathBuf {
    data_home().join("dbus-1/services")
}
fn bindings_conf() -> PathBuf {
    config_home().join("hypr/bindings.conf")
}
fn portals_conf() -> PathBuf {
    config_home().join("xdg-desktop-portal/portals.conf")
}

fn kikid_path() -> String {
    std::env::current_exe().ok().map(|p| p.to_string_lossy().into_owned()).filter(|p| p.ends_with("kikid")).unwrap_or_else(|| "/usr/bin/kikid".into())
}

/// Write through a temp file and rename, so a crash never leaves a half-written config.
fn write_atomic(path: &Path, text: &str) -> std::io::Result<()> {
    if let Some(p) = path.parent() {
        std::fs::create_dir_all(p)?;
    }
    let tmp = path.with_extension(format!("kiki-tmp-{}", std::process::id()));
    std::fs::write(&tmp, text)?;
    std::fs::rename(&tmp, path)
}

fn run(cmd: &str, args: &[&str]) -> Result<String, String> {
    if no_exec() {
        return Ok(String::new());
    }
    let o = Command::new(cmd).args(args).output().map_err(|e| format!("{cmd}: {e}"))?;
    let out = String::from_utf8_lossy(&o.stdout).trim().to_string();
    if o.status.success() {
        Ok(out)
    } else {
        let err = String::from_utf8_lossy(&o.stderr).trim().to_string();
        Err(if err.is_empty() { format!("{cmd} exited with {}", o.status) } else { err })
    }
}

// ---------------------------------------------------------------- backup of what we replaced

/// `~/.config/kiki/integration.toml`: the values each part replaced, written on the first
/// apply only (a second apply never overwrites the true original) and restored on remove.
fn backup_all() -> Value {
    crate::config::read_named("integration.toml")
}

fn backup_get(part: &str) -> Option<Value> {
    backup_all().get(part).cloned()
}

fn backup_set(part: &str, v: Value) {
    let mut all = match backup_all() {
        Value::Obj(m) => m,
        _ => Default::default(),
    };
    if all.contains_key(part) {
        return;
    }
    all.insert(part.to_string(), v);
    let _ = crate::config::write_named("integration.toml", &Value::Obj(all));
}

fn backup_clear(part: &str) {
    if let Value::Obj(mut m) = backup_all() {
        m.remove(part);
        let _ = crate::config::write_named("integration.toml", &Value::Obj(m));
    }
}

// ---------------------------------------------------------------- 1. mime

const MIME_TYPES: [&str; 3] = ["inode/directory", "x-scheme-handler/sftp", "x-scheme-handler/ftps"];

/// Edits an ini-style file's section: sets or removes `key=value` lines under `[section]`.
fn ini_set(text: &str, section: &str, key: &str, value: Option<&str>) -> String {
    let mut out: Vec<String> = Vec::new();
    let mut in_section = false;
    let mut seen_section = false;
    let mut done = false;
    for line in text.lines() {
        let t = line.trim();
        if t.starts_with('[') {
            if in_section && !done {
                if let Some(v) = value {
                    out.push(format!("{key}={v}"));
                }
                done = true;
            }
            in_section = t == format!("[{section}]");
            seen_section |= in_section;
            out.push(line.to_string());
            continue;
        }
        if in_section {
            if let Some((k, _)) = t.split_once('=') {
                if k.trim() == key {
                    if !done {
                        if let Some(v) = value {
                            out.push(format!("{key}={v}"));
                        }
                        done = true;
                    }
                    continue;
                }
            }
        }
        out.push(line.to_string());
    }
    if !done {
        if let Some(v) = value {
            if !seen_section {
                if !out.is_empty() && !out.last().map(|l| l.is_empty()).unwrap_or(true) {
                    out.push(String::new());
                }
                out.push(format!("[{section}]"));
            }
            out.push(format!("{key}={v}"));
        }
    }
    let mut s = out.join("\n");
    s.push('\n');
    s
}

fn ini_get(text: &str, section: &str, key: &str) -> Option<String> {
    let mut in_section = false;
    for line in text.lines() {
        let t = line.trim();
        if t.starts_with('[') {
            in_section = t == format!("[{section}]");
            continue;
        }
        if in_section {
            if let Some((k, v)) = t.split_once('=') {
                if k.trim() == key {
                    return Some(v.trim().to_string());
                }
            }
        }
    }
    None
}

fn mime_status() -> bool {
    let text = std::fs::read_to_string(mimeapps()).unwrap_or_default();
    ini_get(&text, "Default Applications", "inode/directory").map(|v| v.split(';').any(|d| d == DESKTOP_ID)).unwrap_or(false)
}

fn mime_apply() -> Result<String, String> {
    // Remember what each type pointed at ("" = no entry) so Remove can put it back.
    let before = std::fs::read_to_string(mimeapps()).unwrap_or_default();
    let mut prev = Value::obj();
    for m in MIME_TYPES {
        prev = prev.s(m, ini_get(&before, "Default Applications", m).unwrap_or_default());
    }
    backup_set("mime", prev.done());
    // Prefer xdg-mime (it also knows about other mimeapps locations), then verify by reading back.
    for m in MIME_TYPES {
        let _ = run("xdg-mime", &["default", DESKTOP_ID, m]);
    }
    if !mime_status() {
        let mut text = std::fs::read_to_string(mimeapps()).unwrap_or_default();
        for m in MIME_TYPES {
            text = ini_set(&text, "Default Applications", m, Some(DESKTOP_ID));
        }
        write_atomic(&mimeapps(), &text).map_err(|e| e.to_string())?;
    }
    if !mime_status() {
        return Err(format!("{} still not the inode/directory handler; is {DESKTOP_ID} installed under /usr/share/applications?", DESKTOP_ID));
    }
    Ok("kiki opens folders for other applications".into())
}

fn mime_remove() -> Result<String, String> {
    let path = mimeapps();
    let Ok(text) = std::fs::read_to_string(&path) else { return Ok("nothing to remove".into()) };
    let prev = backup_get("mime");
    let mut out = text.clone();
    for m in MIME_TYPES {
        // Put back what was there before kiki; without a record, just drop kiki from the line.
        let restored: Option<String> = match prev.as_ref().and_then(|p| p.str_field(m)) {
            Some("") => None,
            Some(v) => Some(v.to_string()),
            None => ini_get(&out, "Default Applications", m).map(|v| v.split(';').filter(|d| !d.is_empty() && *d != DESKTOP_ID).collect::<Vec<&str>>().join(";")).filter(|s| !s.is_empty()),
        };
        out = ini_set(&out, "Default Applications", m, restored.as_deref());
        if let Some(v) = ini_get(&out, "Added Associations", m) {
            let rest = v.split(';').filter(|d| !d.is_empty() && *d != DESKTOP_ID).collect::<Vec<&str>>().join(";");
            out = ini_set(&out, "Added Associations", m, if rest.is_empty() { None } else { Some(rest.as_str()) });
        }
    }
    if out != text {
        write_atomic(&path, &out).map_err(|e| e.to_string())?;
    }
    backup_clear("mime");
    Ok(match prev.as_ref().and_then(|p| p.str_field("inode/directory")).filter(|v| !v.is_empty()) {
        Some(v) => format!("folders open with {v} again"),
        None => "folder handler released".into(),
    })
}

// ---------------------------------------------------------------- 2. dbus

fn service_files() -> [(PathBuf, String); 2] {
    let dir = services_dir();
    let exec = kikid_path();
    [
        (dir.join("org.freedesktop.FileManager1.service"), format!("[D-BUS Service]\nName=org.freedesktop.FileManager1\nExec={exec}\nSystemdService=kikid.service\n")),
        (dir.join(format!("{PORTAL_NAME}.service")), format!("[D-BUS Service]\nName={PORTAL_NAME}\nExec={exec}\nSystemdService=kikid.service\n")),
    ]
}

fn dbus_status() -> bool {
    service_files().iter().all(|(p, _)| std::fs::read_to_string(p).map(|t| t.contains("kikid")).unwrap_or(false))
}

fn dbus_apply() -> Result<String, String> {
    let mut prev = Value::obj();
    for (p, _) in service_files() {
        if let Ok(existing) = std::fs::read_to_string(&p) {
            prev = prev.s(&p.file_name().unwrap().to_string_lossy(), existing);
        }
    }
    backup_set("dbus", prev.done());
    for (p, text) in service_files() {
        write_atomic(&p, &text).map_err(|e| format!("{}: {e}", p.display()))?;
    }
    // The session bus rescans its service directories on ReloadConfig.
    let _ = run("dbus-send", &["--session", "--dest=org.freedesktop.DBus", "--type=method_call", "/org/freedesktop/DBus", "org.freedesktop.DBus.ReloadConfig"]);
    Ok("\"Show in folder\" from browsers and chat apps opens kiki".into())
}

fn dbus_remove() -> Result<String, String> {
    let prev = backup_get("dbus");
    for (p, _) in service_files() {
        let name = p.file_name().unwrap().to_string_lossy().into_owned();
        match prev.as_ref().and_then(|v| v.str_field(&name)) {
            Some(original) => write_atomic(&p, original).map_err(|e| format!("{}: {e}", p.display()))?,
            None => {
                if p.exists() {
                    std::fs::remove_file(&p).map_err(|e| format!("{}: {e}", p.display()))?;
                }
            }
        }
    }
    backup_clear("dbus");
    let _ = run("dbus-send", &["--session", "--dest=org.freedesktop.DBus", "--type=method_call", "/org/freedesktop/DBus", "org.freedesktop.DBus.ReloadConfig"]);
    Ok("D-Bus activation files removed".into())
}

// ---------------------------------------------------------------- 3. hyprland

/// The launch keys, the chooser's floating rule, and Quick Look's: its window floats and is
/// pinned — above everything, on every workspace — matched by the title's constant ending
/// (owner, 2026-09-25: "quick look window should be a floating window ie above all").
pub fn hypr_block() -> String {
    format!(
        "{BEGIN}\n\
         bindd = SUPER SHIFT, F, File manager (kiki), exec, kiki\n\
         bindd = SUPER ALT SHIFT, F, File manager here (kiki), exec, kiki \"$(omarchy-cmd-terminal-cwd)\"\n\
         windowrulev2 = float, class:^(kiki-chooser)$\n\
         windowrulev2 = center, class:^(kiki-chooser)$\n\
         windowrulev2 = size 860 560, class:^(kiki-chooser)$\n\
         windowrulev2 = float, title:^(.* — Quick Look)$\n\
         windowrulev2 = pin, title:^(.* — Quick Look)$\n\
         {END}\n"
    )
}

/// Removes the marked block (and one blank line before it) from a config text.
pub fn strip_block(text: &str) -> String {
    let mut out: Vec<&str> = Vec::new();
    let mut inside = false;
    for line in text.lines() {
        if line.trim() == BEGIN {
            inside = true;
            if out.last().map(|l| l.trim().is_empty()).unwrap_or(false) {
                out.pop();
            }
            continue;
        }
        if inside {
            if line.trim() == END {
                inside = false;
            }
            continue;
        }
        out.push(line);
    }
    let mut s = out.join("\n");
    if !s.is_empty() {
        s.push('\n');
    }
    s
}

pub fn with_block(text: &str) -> String {
    let base = strip_block(text);
    if base.is_empty() {
        hypr_block()
    } else {
        format!("{}\n{}", base.trim_end_matches('\n'), hypr_block())
    }
}

fn hypr_status() -> bool {
    std::fs::read_to_string(bindings_conf()).map(|t| t.contains(BEGIN)).unwrap_or(false)
}

fn config_errors() -> Vec<String> {
    run("hyprctl", &["configerrors"]).map(|o| o.lines().map(str::trim).filter(|l| !l.is_empty()).map(str::to_string).collect()).unwrap_or_default()
}

/// Writes `new` to bindings.conf, reloads, and puts `old` back if Hyprland reports errors.
fn hypr_write_reload(old: &str, new: &str) -> Result<(), String> {
    let path = bindings_conf();
    write_atomic(&path, new).map_err(|e| format!("{}: {e}", path.display()))?;
    let _ = run("hyprctl", &["reload"]);
    let errors = config_errors();
    if !errors.is_empty() {
        let _ = write_atomic(&path, old);
        let _ = run("hyprctl", &["reload"]);
        return Err(format!("Hyprland rejected the change and it was rolled back: {}", errors.join("; ")));
    }
    Ok(())
}

fn hypr_apply() -> Result<String, String> {
    let pre = config_errors();
    if !pre.is_empty() {
        return Err(format!("your Hyprland config already has errors; fix those first: {}", pre.join("; ")));
    }
    let old = std::fs::read_to_string(bindings_conf()).unwrap_or_default();
    let new = with_block(&old);
    if new != old {
        hypr_write_reload(&old, &new)?;
    }
    Ok("Super+Shift+F opens kiki, Super+Alt+Shift+F opens the terminal's folder; the chooser floats; Quick Look floats above all".into())
}

fn hypr_remove() -> Result<String, String> {
    let path = bindings_conf();
    let Ok(old) = std::fs::read_to_string(&path) else { return Ok("nothing to remove".into()) };
    if !old.contains(BEGIN) {
        return Ok("nothing to remove".into());
    }
    hypr_write_reload(&old, &strip_block(&old))?;
    Ok("keybindings and window rules removed".into())
}

// ---------------------------------------------------------------- 4. portal

fn portal_status() -> bool {
    let text = std::fs::read_to_string(portals_conf()).unwrap_or_default();
    ini_get(&text, "preferred", "org.freedesktop.impl.portal.FileChooser").map(|v| v.split(';').next() == Some("kiki")).unwrap_or(false)
}

fn portal_apply() -> Result<String, String> {
    let text = std::fs::read_to_string(portals_conf()).unwrap_or_default();
    let current = ini_get(&text, "preferred", "org.freedesktop.impl.portal.FileChooser").unwrap_or_default();
    backup_set("portal", Value::obj().s("fileChooser", current.clone()).done());
    // Keep whatever was there as the fallback chain, gtk last so dialogs never vanish.
    let mut chain: Vec<String> = vec!["kiki".into()];
    for b in current.split(';').filter(|b| !b.is_empty() && *b != "kiki") {
        chain.push(b.to_string());
    }
    if !chain.iter().any(|b| b == "gtk") {
        chain.push("gtk".into());
    }
    let new = ini_set(&text, "preferred", "org.freedesktop.impl.portal.FileChooser", Some(&chain.join(";")));
    if new != text {
        write_atomic(&portals_conf(), &new).map_err(|e| e.to_string())?;
    }
    let _ = run("systemctl", &["--user", "restart", "xdg-desktop-portal"]);
    Ok("Open and Save dialogs from other applications use kiki".into())
}

fn portal_remove() -> Result<String, String> {
    let path = portals_conf();
    let Ok(text) = std::fs::read_to_string(&path) else { return Ok("nothing to remove".into()) };
    let current = ini_get(&text, "preferred", "org.freedesktop.impl.portal.FileChooser").unwrap_or_default();
    // The recorded original wins; without one, drop kiki from the chain and keep the rest.
    let rest = match backup_get("portal").and_then(|p| p.str_field("fileChooser").map(str::to_string)) {
        Some(original) => original,
        None => current.split(';').filter(|b| !b.is_empty() && *b != "kiki").collect::<Vec<&str>>().join(";"),
    };
    let new = ini_set(&text, "preferred", "org.freedesktop.impl.portal.FileChooser", if rest.is_empty() { None } else { Some(rest.as_str()) });
    backup_clear("portal");
    if new != text {
        write_atomic(&path, &new).map_err(|e| e.to_string())?;
    }
    let _ = run("systemctl", &["--user", "restart", "xdg-desktop-portal"]);
    Ok("chooser preference removed".into())
}

// ---------------------------------------------------------------- api

pub fn status_json() -> Value {
    Value::obj()
        .b("mime", mime_status())
        .b("dbus", dbus_status())
        .b("hypr", hypr_status())
        .b("portal", portal_status())
        .b("hyprlandAvailable", no_exec() || Command::new("hyprctl").arg("version").output().map(|o| o.status.success()).unwrap_or(false))
        .v("hyprConfigErrors", Value::Arr(config_errors().into_iter().map(Value::Str).collect()))
        .s("mimeapps", mimeapps().to_string_lossy().into_owned())
        .s("bindings", bindings_conf().to_string_lossy().into_owned())
        .s("portals", portals_conf().to_string_lossy().into_owned())
        .s("services", services_dir().to_string_lossy().into_owned())
        .done()
}

fn parts_of(v: Option<&Value>) -> Vec<String> {
    match v.and_then(Value::as_arr) {
        Some(a) if !a.is_empty() => a.iter().filter_map(Value::as_str).map(str::to_string).collect(),
        _ => PARTS.iter().map(|s| s.to_string()).collect(),
    }
}

/// Applies the requested parts (all when empty); each entry is `{ part, ok, message }`.
pub fn apply(parts: Option<&Value>) -> Value {
    let mut out = Vec::new();
    for p in parts_of(parts) {
        let r = match p.as_str() {
            "mime" => mime_apply(),
            "dbus" => dbus_apply(),
            "hypr" => hypr_apply(),
            "portal" => portal_apply(),
            _ => Err("unknown part".into()),
        };
        out.push(result_json(&p, r));
    }
    Value::obj().v("results", Value::Arr(out)).v("status", status_json()).done()
}

pub fn remove(parts: Option<&Value>) -> Value {
    let mut out = Vec::new();
    for p in parts_of(parts) {
        let r = match p.as_str() {
            "mime" => mime_remove(),
            "dbus" => dbus_remove(),
            "hypr" => hypr_remove(),
            "portal" => portal_remove(),
            _ => Err("unknown part".into()),
        };
        out.push(result_json(&p, r));
    }
    Value::obj().v("results", Value::Arr(out)).v("status", status_json()).done()
}

fn result_json(part: &str, r: Result<String, String>) -> Value {
    match r {
        Ok(m) => Value::obj().s("part", part).b("ok", true).s("message", m).done(),
        Err(m) => Value::obj().s("part", part).b("ok", false).s("message", m).done(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch() -> PathBuf {
        let d = std::env::temp_dir().join(format!("kiki-integrate-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        std::env::set_var("KIKI_INTEGRATE_HOME", &d);
        std::env::set_var("KIKI_INTEGRATE_NO_EXEC", "1");
        d
    }

    fn done(d: &Path) {
        std::env::remove_var("KIKI_INTEGRATE_HOME");
        std::env::remove_var("KIKI_INTEGRATE_NO_EXEC");
        let _ = std::fs::remove_dir_all(d);
    }

    #[test]
    fn ini_editing() {
        let t = "[Default Applications]\ntext/plain=nvim.desktop\n\n[Added Associations]\nimage/png=imv.desktop;\n";
        let s = ini_set(t, "Default Applications", "inode/directory", Some("org.kiki.App.desktop"));
        assert_eq!(ini_get(&s, "Default Applications", "inode/directory").as_deref(), Some("org.kiki.App.desktop"));
        assert_eq!(ini_get(&s, "Default Applications", "text/plain").as_deref(), Some("nvim.desktop"));
        let s2 = ini_set(&s, "Default Applications", "inode/directory", None);
        assert!(ini_get(&s2, "Default Applications", "inode/directory").is_none());
        assert!(s2.contains("[Added Associations]\nimage/png=imv.desktop;"));
        // a missing section is appended
        let s3 = ini_set("", "preferred", "org.freedesktop.impl.portal.FileChooser", Some("kiki;gtk"));
        assert_eq!(s3, "[preferred]\norg.freedesktop.impl.portal.FileChooser=kiki;gtk\n");
    }

    #[test]
    fn hypr_block_round_trip() {
        let base = "# my bindings\nbindd = SUPER, RETURN, Terminal, exec, alacritty\n";
        let with = with_block(base);
        assert!(with.starts_with(base));
        assert!(with.contains("SUPER SHIFT, F"));
        assert_eq!(with_block(&with), with, "idempotent");
        assert_eq!(strip_block(&with), base);
        assert_eq!(strip_block(base), base);
    }

    #[test]
    fn apply_and_remove_everything_per_user() {
        let _g = crate::ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let d = scratch();
        // pre-existing user config that must survive
        std::fs::create_dir_all(d.join(".config/hypr")).unwrap();
        std::fs::write(d.join(".config/hypr/bindings.conf"), "bindd = SUPER, RETURN, Terminal, exec, alacritty\n").unwrap();
        std::fs::create_dir_all(d.join(".config/xdg-desktop-portal")).unwrap();
        std::fs::write(d.join(".config/xdg-desktop-portal/portals.conf"), "[preferred]\ndefault=hyprland;gtk\norg.freedesktop.impl.portal.FileChooser=gtk\n").unwrap();
        std::fs::write(d.join(".config/mimeapps.list"), "[Default Applications]\ntext/plain=nvim.desktop\ninode/directory=org.gnome.Nautilus.desktop\n").unwrap();
        std::env::set_var("KIKI_CONFIG_DIR", d.join(".config/kiki"));

        let r = apply(None);
        let results = r.get("results").unwrap().as_arr().unwrap();
        assert!(results.iter().all(|x| x.get("ok") == Some(&Value::Bool(true))), "{}", crate::json::to_string(&r));
        let st = r.get("status").unwrap();
        for p in PARTS {
            assert_eq!(st.get(p), Some(&Value::Bool(true)), "{p}");
        }
        let mime = std::fs::read_to_string(d.join(".config/mimeapps.list")).unwrap();
        assert!(mime.contains("inode/directory=org.kiki.App.desktop"));
        assert!(mime.contains("text/plain=nvim.desktop"));
        let portals = std::fs::read_to_string(d.join(".config/xdg-desktop-portal/portals.conf")).unwrap();
        assert!(portals.contains("org.freedesktop.impl.portal.FileChooser=kiki;gtk"));
        assert!(portals.contains("default=hyprland;gtk"));
        assert!(d.join(".local/share/dbus-1/services/org.freedesktop.FileManager1.service").exists());
        let bindings = std::fs::read_to_string(d.join(".config/hypr/bindings.conf")).unwrap();
        assert!(bindings.starts_with("bindd = SUPER, RETURN"));
        assert!(bindings.contains(BEGIN) && bindings.contains(END));

        let r = remove(None);
        let st = r.get("status").unwrap();
        for p in PARTS {
            assert_eq!(st.get(p), Some(&Value::Bool(false)), "{p} still on");
        }
        assert_eq!(std::fs::read_to_string(d.join(".config/hypr/bindings.conf")).unwrap(), "bindd = SUPER, RETURN, Terminal, exec, alacritty\n");
        assert_eq!(std::fs::read_to_string(d.join(".config/xdg-desktop-portal/portals.conf")).unwrap(), "[preferred]\ndefault=hyprland;gtk\norg.freedesktop.impl.portal.FileChooser=gtk\n");
        assert_eq!(
            std::fs::read_to_string(d.join(".config/mimeapps.list")).unwrap(),
            "[Default Applications]\ntext/plain=nvim.desktop\ninode/directory=org.gnome.Nautilus.desktop\n",
            "the previous folder handler is back"
        );
        assert!(!d.join(".config/kiki/integration.toml").exists() || !std::fs::read_to_string(d.join(".config/kiki/integration.toml")).unwrap().contains("mime"), "backup cleared");
        std::env::remove_var("KIKI_CONFIG_DIR");
        assert!(!d.join(".local/share/dbus-1/services/org.freedesktop.FileManager1.service").exists());
        done(&d);
    }
}
