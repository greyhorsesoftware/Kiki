//! Open with… (plan 02/03): the applications that handle a file's MIME type, from the
//! freedesktop `mimeinfo.cache` and `mimeapps.list` files, and launching one with its `Exec`
//! line expanded per the Desktop Entry spec.

use crate::json::Value;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct App {
    pub id: String,
    pub name: String,
    pub exec: String,
    pub icon: String,
    pub terminal: bool,
    pub path: PathBuf,
}

impl App {
    pub fn to_json(&self, default: bool) -> Value {
        Value::obj().s("id", self.id.clone()).s("name", self.name.clone()).s("icon", self.icon.clone()).b("default", default).done()
    }
}

/// Directories holding `.desktop` files, most specific first (`KIKI_APP_DIRS` overrides for tests).
pub fn app_dirs() -> Vec<PathBuf> {
    if let Ok(v) = std::env::var("KIKI_APP_DIRS") {
        return std::env::split_paths(&v).collect();
    }
    let mut dirs = Vec::new();
    let data_home = std::env::var("XDG_DATA_HOME").map(PathBuf::from).unwrap_or_else(|_| crate::config::home().join(".local/share"));
    dirs.push(data_home.join("applications"));
    let data_dirs = std::env::var("XDG_DATA_DIRS").unwrap_or_else(|_| "/usr/local/share:/usr/share".into());
    for d in std::env::split_paths(&data_dirs) {
        dirs.push(d.join("applications"));
    }
    dirs
}

/// The MIME type of a local file: `xdg-mime` when present, else a small extension table.
pub fn mime_of(path: &Path) -> String {
    if path.is_dir() {
        return "inode/directory".into();
    }
    if let Ok(out) = Command::new("xdg-mime").arg("query").arg("filetype").arg(path).stderr(Stdio::null()).output() {
        if out.status.success() {
            let s = String::from_utf8_lossy(&out.stdout).trim().to_string();
            if !s.is_empty() {
                return s;
            }
        }
    }
    mime_from_name(&path.file_name().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default())
}

pub fn mime_from_name(name: &str) -> String {
    let ext = name.rsplit('.').next().unwrap_or("").to_ascii_lowercase();
    match ext.as_str() {
        "txt" | "log" | "cfg" | "conf" | "ini" => "text/plain",
        "md" | "markdown" => "text/markdown",
        "rs" => "text/rust",
        "py" => "text/x-python",
        "js" | "mjs" => "text/javascript",
        "ts" => "text/typescript",
        "json" => "application/json",
        "toml" => "application/toml",
        "yaml" | "yml" => "application/yaml",
        "html" | "htm" => "text/html",
        "css" => "text/css",
        "sh" => "application/x-shellscript",
        "c" | "h" => "text/x-c",
        "cpp" | "cc" | "hpp" => "text/x-c++",
        "go" => "text/x-go",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "svg" => "image/svg+xml",
        "heic" => "image/heic",
        "mp4" | "m4v" => "video/mp4",
        "mkv" => "video/x-matroska",
        "webm" => "video/webm",
        "mov" => "video/quicktime",
        "mp3" => "audio/mpeg",
        "flac" => "audio/flac",
        "ogg" | "oga" => "audio/ogg",
        "wav" => "audio/x-wav",
        "pdf" => "application/pdf",
        "zip" => "application/zip",
        "gz" | "tgz" => "application/gzip",
        "xz" => "application/x-xz",
        "zst" => "application/zstd",
        "7z" => "application/x-7z-compressed",
        "tar" => "application/x-tar",
        _ => "application/octet-stream",
    }
    .into()
}

fn read_desktop(path: &Path) -> Option<App> {
    let text = std::fs::read_to_string(path).ok()?;
    let mut in_entry = false;
    let (mut name, mut exec, mut icon, mut terminal, mut nodisplay, mut hidden) = (None, None, String::new(), false, false, false);
    for line in text.lines() {
        let line = line.trim();
        if line.starts_with('[') {
            in_entry = line == "[Desktop Entry]";
            continue;
        }
        if !in_entry {
            continue;
        }
        let Some((k, v)) = line.split_once('=') else { continue };
        match k.trim() {
            "Name" => name = name.or(Some(v.trim().to_string())),
            "Exec" => exec = exec.or(Some(v.trim().to_string())),
            "Icon" => icon = v.trim().to_string(),
            "Terminal" => terminal = v.trim() == "true",
            "NoDisplay" => nodisplay = v.trim() == "true",
            "Hidden" => hidden = v.trim() == "true",
            _ => {}
        }
    }
    if hidden {
        return None;
    }
    let _ = nodisplay; // NoDisplay apps still open files (they are just not listed in launchers)
    Some(App { id: path.file_name()?.to_string_lossy().into_owned(), name: name?, exec: exec?, icon, terminal, path: path.to_path_buf() })
}

fn find_desktop(id: &str) -> Option<App> {
    app_dirs().iter().map(|d| d.join(id)).find(|p| p.is_file()).and_then(|p| read_desktop(&p))
}

/// Parse a `mimeinfo.cache` or `mimeapps.list`-style `mime=a.desktop;b.desktop;` section.
fn parse_mime_map(text: &str, section: &str) -> BTreeMap<String, Vec<String>> {
    let mut out: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut in_section = false;
    for line in text.lines() {
        let line = line.trim();
        if line.starts_with('[') {
            in_section = line == section;
            continue;
        }
        if !in_section {
            continue;
        }
        if let Some((m, apps)) = line.split_once('=') {
            let list = out.entry(m.trim().to_string()).or_default();
            for a in apps.split(';').map(str::trim).filter(|a| !a.is_empty()) {
                if !list.iter().any(|x| x == a) {
                    list.push(a.to_string());
                }
            }
        }
    }
    out
}

/// Candidate MIME types, most specific first: the type, its `text/plain` fallback for `text/*`,
/// and `application/octet-stream` last for the "any file" handlers.
fn mime_chain(mime: &str) -> Vec<String> {
    let mut v = vec![mime.to_string()];
    if mime.starts_with("text/") && mime != "text/plain" {
        v.push("text/plain".into());
    }
    if mime == "application/json" || mime == "application/toml" || mime == "application/yaml" || mime == "application/x-shellscript" {
        v.push("text/plain".into());
    }
    v
}

/// Applications for a MIME type; the first is the default from `mimeapps.list` when set.
pub fn apps_for(mime: &str) -> Vec<(App, bool)> {
    let mut ids: Vec<String> = Vec::new();
    let mut default: Option<String> = None;
    let mut removed: Vec<String> = Vec::new();
    let chain = mime_chain(mime);
    for dir in app_dirs() {
        // mimeapps.list: [Default Applications], [Added Associations], [Removed Associations]
        if let Ok(text) = std::fs::read_to_string(dir.join("mimeapps.list")) {
            let defaults = parse_mime_map(&text, "[Default Applications]");
            let added = parse_mime_map(&text, "[Added Associations]");
            let rem = parse_mime_map(&text, "[Removed Associations]");
            for m in &chain {
                if default.is_none() {
                    if let Some(d) = defaults.get(m).and_then(|v| v.first()) {
                        default = Some(d.clone());
                    }
                }
                for a in added.get(m).into_iter().flatten() {
                    if !ids.contains(a) {
                        ids.push(a.clone());
                    }
                }
                for a in rem.get(m).into_iter().flatten() {
                    removed.push(a.clone());
                }
            }
        }
        if let Ok(text) = std::fs::read_to_string(dir.join("mimeinfo.cache")) {
            let cache = parse_mime_map(&text, "[MIME Cache]");
            for m in &chain {
                for a in cache.get(m).into_iter().flatten() {
                    if !ids.contains(a) {
                        ids.push(a.clone());
                    }
                }
            }
        }
    }
    if let Some(d) = &default {
        ids.retain(|i| i != d);
        ids.insert(0, d.clone());
    }
    let mut out = Vec::new();
    for id in ids {
        if removed.contains(&id) {
            continue;
        }
        if let Some(app) = find_desktop(&id) {
            let is_default = default.as_deref() == Some(app.id.as_str());
            out.push((app, is_default));
        }
    }
    out
}

pub fn apps_json(path: &Path) -> Value {
    let mime = mime_of(path);
    let apps = apps_for(&mime);
    Value::obj().s("mime", mime).v("apps", Value::Arr(apps.iter().map(|(a, d)| a.to_json(*d)).collect())).done()
}

/// Split an `Exec` value into arguments (double quotes and backslash escapes per the spec) and
/// expand the field codes with the given files or URIs.
pub fn expand_exec(exec: &str, app: Option<&App>, files: &[String]) -> Vec<String> {
    let mut args: Vec<String> = Vec::new();
    let mut cur = String::new();
    let mut in_quote = false;
    let mut chars = exec.chars().peekable();
    let mut any = false;
    while let Some(c) = chars.next() {
        match c {
            '"' => {
                in_quote = !in_quote;
                any = true;
            }
            '\\' if in_quote => {
                if let Some(n) = chars.next() {
                    cur.push(n);
                }
            }
            ' ' if !in_quote => {
                if any || !cur.is_empty() {
                    args.push(std::mem::take(&mut cur));
                    any = false;
                }
            }
            _ => cur.push(c),
        }
    }
    if any || !cur.is_empty() {
        args.push(cur);
    }
    let mut out: Vec<String> = Vec::new();
    for a in args {
        match a.as_str() {
            "%f" | "%u" => {
                if let Some(f) = files.first() {
                    out.push(f.clone());
                }
            }
            "%F" | "%U" => out.extend(files.iter().cloned()),
            "%i" => {
                if let Some(app) = app {
                    if !app.icon.is_empty() {
                        out.push("--icon".into());
                        out.push(app.icon.clone());
                    }
                }
            }
            "%c" => out.push(app.map(|a| a.name.clone()).unwrap_or_default()),
            "%k" => out.push(app.map(|a| a.path.to_string_lossy().into_owned()).unwrap_or_default()),
            "%d" | "%D" | "%n" | "%N" | "%v" | "%m" => {} // deprecated, removed
            _ => {
                // Field codes embedded in a longer argument (rare) and %% escapes.
                let mut s = a.replace("%%", "\u{0}");
                for code in ["%f", "%u"] {
                    s = s.replace(code, files.first().map(String::as_str).unwrap_or(""));
                }
                s = s.replace("%c", app.map(|a| a.name.as_str()).unwrap_or(""));
                out.push(s.replace('\u{0}', "%"));
            }
        }
    }
    out
}

/// Launch a desktop entry with the given URIs (local paths are passed as paths).
pub fn launch(app_id: &str, uris: &[String]) -> Result<(), String> {
    let app = find_desktop(app_id).ok_or_else(|| format!("no desktop entry {app_id}"))?;
    let wants_uris = app.exec.contains("%u") || app.exec.contains("%U");
    let files: Vec<String> = uris
        .iter()
        .map(|u| if wants_uris { u.clone() } else { crate::vfs::uri::Uri::parse(u).ok().filter(|x| x.is_local()).map(|x| x.to_path().to_string_lossy().into_owned()).unwrap_or_else(|| u.clone()) })
        .collect();
    let mut argv = expand_exec(&app.exec, Some(&app), &files);
    if argv.is_empty() {
        return Err("empty Exec".into());
    }
    if app.terminal {
        let term = std::env::var("TERMINAL").unwrap_or_else(|_| "alacritty".into());
        let mut t = vec![term, "-e".to_string()];
        t.append(&mut argv);
        argv = t;
    }
    spawn_detached(&argv)
}

pub fn spawn_detached(argv: &[String]) -> Result<(), String> {
    spawn_detached_in(argv, None)
}

/// …started in `dir`: a terminal opens where it is started.
pub fn spawn_detached_in(argv: &[String], dir: Option<&std::path::Path>) -> Result<(), String> {
    let mut cmd = Command::new(&argv[0]);
    if let Some(d) = dir {
        cmd.current_dir(d);
    }
    cmd.args(&argv[1..]).stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null());
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        // SAFETY: setsid in the child before exec detaches it from the daemon's session.
        unsafe {
            cmd.pre_exec(|| {
                libc::setsid();
                Ok(())
            });
        }
    }
    cmd.spawn().map(|_| ()).map_err(|e| format!("{}: {e}", argv[0]))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture() -> PathBuf {
        let d = std::env::temp_dir().join(format!("kiki-desktop-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(d.join("user")).unwrap();
        std::fs::create_dir_all(d.join("sys")).unwrap();
        std::fs::write(d.join("sys/mimeinfo.cache"), "[MIME Cache]\ntext/plain=nvim.desktop;code.desktop;\nimage/png=imv.desktop;gimp.desktop;\ntext/markdown=obsidian.desktop;\n").unwrap();
        std::fs::write(d.join("user/mimeapps.list"), "[Default Applications]\ntext/plain=code.desktop\n[Removed Associations]\nimage/png=gimp.desktop;\n").unwrap();
        for (id, name, exec, term) in [
            ("nvim.desktop", "Neovim", "nvim %F", true),
            ("code.desktop", "Code", "/usr/bin/code --new-window %F", false),
            ("imv.desktop", "imv", "imv %U", false),
            ("gimp.desktop", "GIMP", "gimp-2.10 %U", false),
            ("obsidian.desktop", "Obsidian", "obsidian %u", false),
        ] {
            std::fs::write(d.join("sys").join(id), format!("[Desktop Entry]\nName={name}\nExec={exec}\nIcon={}\nTerminal={term}\n", id.trim_end_matches(".desktop"))).unwrap();
        }
        d
    }

    #[test]
    fn apps_default_first_and_removed_respected() {
        let _g = crate::ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let d = fixture();
        std::env::set_var("KIKI_APP_DIRS", std::env::join_paths([d.join("user"), d.join("sys")]).unwrap());
        let apps = apps_for("text/plain");
        let names: Vec<&str> = apps.iter().map(|(a, _)| a.name.as_str()).collect();
        assert_eq!(names, vec!["Code", "Neovim"]);
        assert!(apps[0].1, "default flagged");
        // text/markdown: its own handler first, then the text/plain fallbacks
        let md: Vec<String> = apps_for("text/markdown").into_iter().map(|(a, _)| a.name).collect();
        assert_eq!(md, vec!["Code", "Obsidian", "Neovim"]);
        // removed association filtered out
        let png: Vec<String> = apps_for("image/png").into_iter().map(|(a, _)| a.name).collect();
        assert_eq!(png, vec!["imv"]);
        assert!(apps_for("application/x-unknown").is_empty());
        std::env::remove_var("KIKI_APP_DIRS");
        std::fs::remove_dir_all(&d).unwrap();
    }

    #[test]
    fn exec_expansion_follows_the_spec() {
        let app = App { id: "x.desktop".into(), name: "X App".into(), exec: String::new(), icon: "x-icon".into(), terminal: false, path: PathBuf::from("/a/x.desktop") };
        let files = vec!["/tmp/a b.txt".to_string(), "/tmp/c.txt".to_string()];
        assert_eq!(expand_exec("code --new-window %F", Some(&app), &files), vec!["code", "--new-window", "/tmp/a b.txt", "/tmp/c.txt"]);
        assert_eq!(expand_exec("obsidian %u", Some(&app), &files), vec!["obsidian", "/tmp/a b.txt"]);
        assert_eq!(expand_exec("\"/opt/My App/run\" --name=%c %i %f", Some(&app), &files), vec!["/opt/My App/run", "--name=X App", "--icon", "x-icon", "/tmp/a b.txt"]);
        assert_eq!(expand_exec("tool 100%% %d %f", Some(&app), &files), vec!["tool", "100%", "/tmp/a b.txt"]);
        assert_eq!(expand_exec("plain", None, &[]), vec!["plain"]);
    }

    #[test]
    fn mime_fallback_table() {
        assert_eq!(mime_from_name("notes.MD"), "text/markdown");
        assert_eq!(mime_from_name("a.tar.gz"), "application/gzip");
        assert_eq!(mime_from_name("noext"), "application/octet-stream");
    }
}
