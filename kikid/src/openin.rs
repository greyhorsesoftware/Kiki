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
        t(
            "neovim",
            "Neovim",
            "nvim",
            "nvim --listen {socket} --cmd 'autocmd BufWritePost * silent! !qs -c kiki ipc call shell saved %:p' --cmd 'nnoremap <leader>k :silent! !qs -c kiki ipc call shell reveal %:p<CR>' +{line} {file}",
            true,
            "file",
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
    let runtime = std::env::var("XDG_RUNTIME_DIR").unwrap_or_else(|_| "/tmp".into());
    let socket = format!("{runtime}/kiki-{id}.sock");
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

pub fn terminal_command() -> Vec<String> {
    let pref = crate::config::settings().get("editor").and_then(|e| e.str_field("terminal").map(str::to_string)).unwrap_or_else(|| "auto".into());
    let candidates: Vec<&str> = if pref == "auto" { vec!["ghostty", "alacritty", "kitty", "foot", "wezterm"] } else { vec![pref.as_str()] };
    for c in candidates {
        if on_path(c) {
            return match c {
                "ghostty" => vec!["ghostty".into(), "--class=kiki-tool".into(), "-e".into()],
                "alacritty" => vec!["alacritty".into(), "--class".into(), "kiki-tool".into(), "-e".into()],
                "kitty" => vec!["kitty".into(), "--class".into(), "kiki-tool".into()],
                "foot" => vec!["foot".into(), "--app-id=kiki-tool".into()],
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
    let p = prepare(&tool, uris, line, tool.str_field("command").unwrap_or(""))?;
    let mut cmd = if p.terminal {
        let t = terminal_command();
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
    sessions.insert(id.clone(), Session { child, files: uris.iter().map(|u| u.to_string()).collect() });
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
}
