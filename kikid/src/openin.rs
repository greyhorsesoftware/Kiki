//! "Open in…" (plan 14): editors, agent harnesses and tools as TOML command templates.

use crate::json::Value;
use crate::vfs::uri::Uri;
use std::collections::{BTreeMap, HashMap};
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::{Mutex, OnceLock};

pub fn presets() -> Vec<Value> {
    let t = |id: &str, name: &str, detect: &str, command: &str, terminal: bool, accepts: &str, role: Option<&str>, reuse: Option<&str>, follow: bool| {
        let mut o = Value::obj().s("id", id).s("name", name).s("detect", detect).s("command", command).b("terminal", terminal).s("accepts", accepts).s("placement", "right").b("follow", follow);
        if let Some(r) = role {
            o = o.s("role", r);
        }
        if let Some(r) = reuse {
            o = o.s("reuse", r);
        }
        o.done()
    };
    vec![
        t("claude", "Claude Code", "claude", "claude", true, "both", Some("agent"), None, false),
        t("codex", "Codex", "codex", "codex", true, "both", None, None, false),
        t("gemini", "Gemini CLI", "gemini", "gemini", true, "both", None, None, false),
        t("aider", "Aider", "aider", "aider {files}", true, "both", None, None, false),
        t("opencode", "OpenCode", "opencode", "opencode", true, "both", None, None, false),
        // The editor calls back through the launcher (`kiki --ipc`), which knows where the running
        // shell is. It used to call `qs -c kiki`, a named config nobody installs: the calls went
        // nowhere, so saving never refreshed the listing and <leader>k revealed nothing.
        t(
            "neovim",
            "Neovim",
            "nvim",
            "nvim --listen {socket} --cmd 'autocmd BufWritePost * silent! !kiki --ipc saved %:p' --cmd 'nnoremap <leader>k :silent! !kiki --ipc reveal %:p<CR>' +{line} {file}",
            true,
            "both",
            Some("editor"),
            Some("nvim --server {socket} --remote-send '<Esc>:e {file}<CR>:{line}<CR>'"),
            true,
        ),
        t("helix", "Helix", "hx", "hx {file}:{line}", true, "file", None, None, false),
        t("vscode", "VS Code", "code", "code --new-window {dir}", false, "both", None, Some("code --reuse-window --goto {file}:{line}"), true),
        t("zed", "Zed", "zed", "zed {dir}", false, "both", None, Some("zed {file}:{line}"), true),
        t("terminal", "Terminal here", "", "$SHELL", true, "folder", None, None, false),
        t("lazygit", "lazygit", "lazygit", "lazygit", true, "folder", None, None, false),
        t("system", "System default", "xdg-open", "xdg-open {file}", false, "both", None, None, false),
    ]
}

pub fn on_path(bin: &str) -> bool {
    if bin.is_empty() {
        return true;
    }
    std::env::var_os("PATH").map(|p| std::env::split_paths(&p).any(|d| d.join(bin).is_file())).unwrap_or(false)
}

/// Presets merged with the user's `open-in.toml` (same id overrides), in file order.
pub fn tools() -> Vec<Value> {
    let mut list = presets();
    let user = crate::config::read_named("open-in.toml");
    if let Some(entries) = user.get("tool").and_then(Value::as_arr) {
        for e in entries {
            let id = e.str_field("id").unwrap_or("").to_string();
            match list.iter().position(|t| t.str_field("id") == Some(&id)) {
                Some(i) => {
                    if let (Value::Obj(base), Value::Obj(over)) = (&mut list[i], e) {
                        for (k, v) in over {
                            base.insert(k.clone(), v.clone());
                        }
                    }
                }
                None => list.push(e.clone()),
            }
        }
        // User order wins for the default: entries listed by the user come first.
        let user_ids: Vec<String> = entries.iter().filter_map(|e| e.str_field("id").map(str::to_string)).collect();
        list.sort_by_key(|t| user_ids.iter().position(|u| Some(u.as_str()) == t.str_field("id")).unwrap_or(usize::MAX));
    }
    list
}

pub fn list_json() -> Value {
    Value::Arr(
        tools()
            .into_iter()
            .map(|t| {
                let detect = t.str_field("detect").unwrap_or("");
                let enabled = on_path(detect);
                let mut o = Value::obj()
                    .s("id", t.str_field("id").unwrap_or(""))
                    .s("name", t.str_field("name").unwrap_or(""))
                    .s("icon", t.str_field("icon").unwrap_or("terminal"))
                    .s("accepts", t.str_field("accepts").unwrap_or("both"))
                    .opt_s("role", t.str_field("role"))
                    .b("enabled", enabled)
                    .b("terminal", t.get("terminal").and_then(Value::as_bool).unwrap_or(true))
                    .s("command", t.str_field("command").unwrap_or(""));
                if !enabled {
                    o = o.s("reason", format!("{detect} not found on PATH"));
                }
                o.done()
            })
            .collect(),
    )
}

pub fn find(id_or_role: &str) -> Option<Value> {
    let all = tools();
    all.iter().find(|t| t.str_field("id") == Some(id_or_role)).or_else(|| all.iter().find(|t| t.str_field("role") == Some(id_or_role))).cloned()
}

fn shell_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', "'\\''"))
}

pub struct Prepared {
    pub command: String,
    pub cwd: PathBuf,
    pub terminal: bool,
    pub env: Vec<(String, String)>,
}

/// Where a tool's control socket goes (`nvim --listen`). Whoever can reach that socket can drive
/// the editor — open files, run commands — as the user, so it must be somewhere only the user can
/// get to. `$XDG_RUNTIME_DIR` is (0700, the user's). Without one this used to fall back to plain
/// `/tmp`, where any local user could connect; now it is a folder of our own there, made 0700 and
/// refused unless it is a real directory, ours, and closed to everyone else.
pub fn socket_dir() -> std::io::Result<std::path::PathBuf> {
    if let Some(rt) = std::env::var_os("XDG_RUNTIME_DIR").filter(|v| !v.is_empty()) {
        return Ok(rt.into());
    }
    // `getuid` takes nothing, touches nothing of ours and cannot fail.
    let uid = unsafe { libc::getuid() };
    private_dir(&std::env::temp_dir().join(format!("kiki-{uid}")))
}

fn private_dir(dir: &std::path::Path) -> std::io::Result<std::path::PathBuf> {
    use std::os::unix::fs::{DirBuilderExt, MetadataExt, PermissionsExt};
    match std::fs::DirBuilder::new().mode(0o700).create(dir) {
        Ok(()) => {}
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {}
        Err(e) => return Err(e),
    }
    // Not followed: a symlink planted under this name is not a directory of ours.
    let md = std::fs::symlink_metadata(dir)?;
    // As above: `getuid` cannot fail.
    let mine = md.uid() == unsafe { libc::getuid() };
    if !md.is_dir() || !mine || md.permissions().mode() & 0o077 != 0 {
        return Err(std::io::Error::new(std::io::ErrorKind::PermissionDenied, format!("{} is not a private directory of this user", dir.display())));
    }
    Ok(dir.to_path_buf())
}

fn socket_path(id: &str) -> std::io::Result<std::path::PathBuf> {
    Ok(socket_dir()?.join(format!("kiki-{id}.sock")))
}

/// A session that has ended leaves its socket file behind, and the next `--listen` on that path
/// fails with "address already in use": cleared before a launch and when a session is closed.
fn clear_socket(id: &str) {
    if let Ok(p) = socket_path(id) {
        let _ = std::fs::remove_file(p);
    }
}

/// Substitutes placeholders. `line` is 1-based; `socket` is per-entry under the runtime dir.
pub fn prepare(tool: &Value, uris: &[Uri], line: Option<u64>, template: &str) -> Result<Prepared, String> {
    let id = tool.str_field("id").unwrap_or("tool");
    let paths: Vec<PathBuf> = uris.iter().filter(|u| u.is_local()).map(|u| u.to_path()).collect();
    if paths.len() != uris.len() {
        return Err("only local files can be opened in a tool; open the location's local folder instead".into());
    }
    let first = paths.first().cloned().unwrap_or_else(crate::config::home);
    let is_dir = first.is_dir();
    let dir = if is_dir { first.clone() } else { first.parent().map(|p| p.to_path_buf()).unwrap_or_else(crate::config::home) };
    let accepts = tool.str_field("accepts").unwrap_or("both");
    if accepts == "file" && is_dir && template.contains("{file}") {
        return Err("this tool opens files, not folders".into());
    }
    if accepts == "folder" && !is_dir && template.contains("{file}") {
        return Err("this tool opens folders".into());
    }
    let socket = socket_path(id).map_err(|e| format!("no private place for the editor's socket: {e}"))?.to_string_lossy().into_owned();
    let files = paths.iter().map(|p| shell_quote(&p.to_string_lossy())).collect::<Vec<_>>().join(" ");
    let file = if is_dir { String::new() } else { shell_quote(&first.to_string_lossy()) };
    let prompt = tool.str_field("prompt").unwrap_or("").replace("{files}", &files);
    let command = template
        .replace("{dir}", &shell_quote(&dir.to_string_lossy()))
        .replace("{files}", &files)
        .replace("{file}", &file)
        .replace("{uris}", &uris.iter().map(|u| shell_quote(&u.to_string())).collect::<Vec<_>>().join(" "))
        .replace("{uri}", &uris.first().map(|u| shell_quote(&u.to_string())).unwrap_or_default())
        .replace("{line}", &line.unwrap_or(1).to_string())
        .replace("{socket}", &shell_quote(&socket))
        .replace("{prompt}", &shell_quote(&prompt));
    let env = vec![
        ("KIKI_SELECTION".to_string(), paths.iter().map(|p| p.to_string_lossy().into_owned()).collect::<Vec<_>>().join("\n")),
        ("KIKI_DIR".to_string(), dir.to_string_lossy().into_owned()),
        ("KIKI_SOCKET_FOR_TOOL".to_string(), socket),
    ];
    Ok(Prepared { command, cwd: dir, terminal: tool.get("terminal").and_then(Value::as_bool).unwrap_or(true), env })
}

/// The window class a tool's terminal is given, so the compositor can tell kiki's editor from
/// kiki's agent. One class for every tool left `Arrange` to match the first window it found for
/// both roles.
pub fn window_class(id: &str) -> String {
    let safe: String = id.chars().map(|c| if c.is_ascii_alphanumeric() { c } else { '-' }).collect();
    format!("kiki-tool-{safe}")
}

pub fn terminal_command(class: &str) -> Vec<String> {
    let pref = crate::config::settings().get("editor").and_then(|e| e.str_field("terminal").map(str::to_string)).unwrap_or_else(|| "auto".into());
    let candidates: Vec<&str> = if pref == "auto" { vec!["ghostty", "alacritty", "kitty", "foot", "wezterm"] } else { vec![pref.as_str()] };
    for c in candidates {
        if on_path(c) {
            return match c {
                "ghostty" => vec!["ghostty".into(), format!("--class={class}"), "-e".into()],
                "alacritty" => vec!["alacritty".into(), "--class".into(), class.into(), "-e".into()],
                "kitty" => vec!["kitty".into(), "--class".into(), class.into()],
                "foot" => vec!["foot".into(), format!("--app-id={class}")],
                _ => vec![c.to_string(), "-e".into()],
            };
        }
    }
    if let Ok(t) = std::env::var("TERMINAL") {
        return vec![t, "-e".into()];
    }
    vec!["xterm".into(), "-e".into()]
}

struct Session {
    child: Child,
    files: Vec<String>,
    /// Where it was started: asking for the same tool in the same place again is asking for the
    /// one that is running, not for a second.
    cwd: PathBuf,
}

fn sessions() -> &'static Mutex<HashMap<String, Session>> {
    static S: OnceLock<Mutex<HashMap<String, Session>>> = OnceLock::new();
    S.get_or_init(|| Mutex::new(HashMap::new()))
}

fn alive(s: &mut Session) -> bool {
    matches!(s.child.try_wait(), Ok(None))
}

/// Launches (or reuses) the tool; returns (pid, reused).
pub fn open(id_or_role: &str, uris: &[Uri], line: Option<u64>) -> Result<(u32, bool), String> {
    let tool = find(id_or_role).ok_or_else(|| format!("no tool {id_or_role}"))?;
    let id = tool.str_field("id").unwrap_or("").to_string();
    if !on_path(tool.str_field("detect").unwrap_or("")) {
        return Err(format!("{} is not installed", tool.str_field("name").unwrap_or(&id)));
    }
    let mut sessions = sessions().lock().unwrap();
    let running = sessions.get_mut(&id).map(alive).unwrap_or(false);
    if running {
        if let Some(reuse) = tool.str_field("reuse") {
            let p = prepare(&tool, uris, line, reuse)?;
            let st = Command::new("sh").arg("-c").arg(&p.command).current_dir(&p.cwd).envs(p.env.clone()).stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null()).status().map_err(|e| e.to_string())?;
            if !st.success() {
                return Err(format!("reuse command failed: {}", p.command));
            }
            let s = sessions.get_mut(&id).unwrap();
            for u in uris {
                s.files.push(u.to_string());
            }
            return Ok((s.child.id(), true));
        }
    }
    // A tool with no way to be handed more files (an agent in a terminal) that is already
    // running in this very folder is simply the one that was asked for: leaving project mode and
    // coming back used to start a second agent beside the first.
    if running && tool.str_field("reuse").is_none() {
        let here = prepare(&tool, uris, line, tool.str_field("command").unwrap_or(""))?.cwd;
        if let Some(s) = sessions.get(&id) {
            if s.cwd == here {
                return Ok((s.child.id(), true));
            }
        }
    }
    // Nothing of ours is listening (or this would have been a reuse): a socket file still there
    // is the last session's, and would stop this one from listening.
    clear_socket(&id);
    let p = prepare(&tool, uris, line, tool.str_field("command").unwrap_or(""))?;
    let mut cmd = if p.terminal {
        let t = terminal_command(&window_class(&id));
        let mut c = Command::new(&t[0]);
        c.args(&t[1..]).arg("sh").arg("-c").arg(&p.command);
        c
    } else {
        let mut c = Command::new("sh");
        c.arg("-c").arg(&p.command);
        c
    };
    let child = cmd.current_dir(&p.cwd).envs(p.env.clone()).stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null()).spawn().map_err(|e| format!("launch failed: {e}"))?;
    let pid = child.id();
    sessions.insert(id.clone(), Session { child, files: uris.iter().map(|u| u.to_string()).collect(), cwd: p.cwd.clone() });
    drop(sessions);
    place(tool.str_field("placement").unwrap_or("right"));
    Ok((pid, false))
}

/// Best-effort Hyprland placement of the window that just opened.
fn place(placement: &str) {
    let dir = match placement {
        "right" => "r",
        "left" => "l",
        _ => return,
    };
    if !on_path("hyprctl") {
        return;
    }
    let dir = dir.to_string();
    std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_millis(400));
        let _ = Command::new("hyprctl").args(["dispatch", "movewindow", &dir]).stdout(Stdio::null()).stderr(Stdio::null()).status();
    });
}

pub fn sessions_json() -> Value {
    let mut s = sessions().lock().unwrap();
    s.retain(|_, v| alive(v));
    Value::Arr(s.iter().map(|(id, v)| Value::obj().s("id", id.clone()).u("pid", v.child.id() as u64).v("files", Value::Arr(v.files.iter().map(|f| Value::Str(f.clone())).collect())).done()).collect())
}

pub fn close(id: &str) -> bool {
    let mut s = sessions().lock().unwrap();
    match s.remove(id) {
        Some(mut v) => {
            let _ = v.child.kill();
            let _ = v.child.wait();
            clear_socket(id);
            true
        }
        None => false,
    }
}

pub fn write_tools(list: &[Value]) -> std::io::Result<()> {
    let mut m = BTreeMap::new();
    m.insert("tool".to_string(), Value::Arr(list.to_vec()));
    crate::config::write_named("open-in.toml", &Value::Obj(m))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn substitutes_and_quotes() {
        let tool = presets().into_iter().find(|t| t.str_field("id") == Some("aider")).unwrap();
        let d = std::env::temp_dir().join(format!("kiki-openin-{}", std::process::id()));
        std::fs::create_dir_all(&d).unwrap();
        std::fs::write(d.join("it's a file.txt"), b"x").unwrap();
        let uris = vec![Uri::from_path(&d.join("it's a file.txt"))];
        let p = prepare(&tool, &uris, Some(7), "aider {files} --line {line} {dir}").unwrap();
        assert!(p.command.starts_with("aider '"));
        assert!(p.command.contains("it'\\''s a file.txt"));
        assert!(p.command.contains("--line 7"));
        assert_eq!(p.cwd, d);
        assert!(p.env.iter().any(|(k, v)| k == "KIKI_SELECTION" && v.ends_with("file.txt")));
        // a folder-only tool refuses a file when it uses {file}
        let term = presets().into_iter().find(|t| t.str_field("id") == Some("terminal")).unwrap();
        assert!(prepare(&term, &uris, None, "$SHELL").is_ok());
        assert!(prepare(&Value::obj().s("id", "x").s("accepts", "folder").done(), &uris, None, "x {file}").is_err());
        assert!(list_json().as_arr().unwrap().iter().any(|t| t.str_field("id") == Some("system")));
        std::fs::remove_dir_all(&d).unwrap();
    }

    /// The fallback when there is no runtime directory: ours, 0700, and nothing else will do.
    #[test]
    fn the_socket_directory_is_private_or_refused() {
        use std::os::unix::fs::PermissionsExt;
        let base = std::env::temp_dir().join(format!("kiki-sockdir-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        std::fs::create_dir_all(&base).unwrap();

        let fresh = base.join("fresh");
        assert_eq!(private_dir(&fresh).unwrap(), fresh);
        assert_eq!(std::fs::metadata(&fresh).unwrap().permissions().mode() & 0o777, 0o700, "made closed to everyone else");
        assert!(private_dir(&fresh).is_ok(), "and accepted again as it is");

        // Somebody else could have made it first, open: a socket in there is anybody's.
        let open = base.join("open");
        std::fs::create_dir(&open).unwrap();
        std::fs::set_permissions(&open, std::fs::Permissions::from_mode(0o755)).unwrap();
        assert!(private_dir(&open).is_err(), "a directory others can enter is refused");

        // Or planted a link under the name, pointing somewhere of their choosing.
        let link = base.join("link");
        std::os::unix::fs::symlink(&fresh, &link).unwrap();
        assert!(private_dir(&link).is_err(), "a symlink is not a directory of ours");

        std::fs::remove_dir_all(&base).unwrap();
    }
}
