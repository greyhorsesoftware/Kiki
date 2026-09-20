//! Jarvis (plan 19): runs the AI the user has selected (Omarchy's choice unless overridden) as its own
//! command-line tool in print mode with the file, streams the answer; exact questions are answered locally.

use crate::json::Value;
use crate::proto;
use crate::vfs::uri::Uri;
use crate::vfs::VfsError;
use std::sync::mpsc::Sender;

pub const ATTACH_CAP: usize = 200_000;
pub const HEAD_TAIL: usize = 50_000;

pub const PROVIDERS: &[&str] = &["anthropic", "openai", "gemini", "xai", "custom"];

fn jarvis_settings() -> Value {
    crate::config::settings().get("jarvis").cloned().unwrap_or(Value::Null)
}

/// Omarchy's chosen AI: the web app its AI keybinding launches. Heuristic; the override wins.
pub fn omarchy_provider() -> Option<&'static str> {
    let home = crate::config::home();
    for p in [home.join(".config/hypr/bindings.conf"), home.join(".local/share/omarchy/default/hypr/bindings.conf")] {
        let Ok(text) = std::fs::read_to_string(&p) else { continue };
        for line in text.lines() {
            let l = line.to_ascii_lowercase();
            let is_ai = l.contains("super, a,") || l.contains("super, a ,") || l.contains(", ai,") || l.contains("chatgpt") || l.contains("claude") || l.contains("grok") || l.contains("gemini");
            if !is_ai || !l.contains("webapp") && !l.contains("http") {
                continue;
            }
            if l.contains("claude.ai") || l.contains("anthropic") {
                return Some("anthropic");
            }
            if l.contains("chatgpt.com") || l.contains("openai") {
                return Some("openai");
            }
            if l.contains("gemini.google") {
                return Some("gemini");
            }
            if l.contains("grok.com") || l.contains("x.ai") {
                return Some("xai");
            }
        }
    }
    None
}

/// The effective provider and where the choice came from.
pub fn provider() -> (String, &'static str) {
    let s = jarvis_settings();
    match s.str_field("provider") {
        Some(p) if p != "omarchy" && PROVIDERS.contains(&p) => (p.to_string(), "settings"),
        _ => match omarchy_provider() {
            Some(p) => (p.to_string(), "omarchy"),
            None => ("anthropic".into(), "default"),
        },
    }
}

/// CLI print-mode command for a provider's own tool: (binary, args with {prompt}); the tool reads files itself.
pub fn cli_for(provider: &str) -> Option<(String, Vec<String>)> {
    let js = jarvis_settings();
    if let Some(c) = js.str_field("cliCommand").filter(|c| !c.is_empty()) {
        let mut parts = c.split_whitespace().map(str::to_string);
        let bin = parts.next()?;
        return Some((bin, parts.collect()));
    }
    let (bin, args): (&str, Vec<&str>) = match provider {
        "anthropic" => ("claude", vec!["-p", "{prompt}", "--output-format", "text"]),
        "openai" => ("codex", vec!["exec", "{prompt}"]),
        "gemini" => ("gemini", vec!["-p", "{prompt}"]),
        "xai" => ("grok", vec!["{prompt}"]),
        _ => return None, // "custom" needs cliCommand
    };
    Some((bin.to_string(), args.iter().map(|a| a.to_string()).collect()))
}

/// The provider's own tool started for a conversation — not print mode — with `prompt` as its
/// first message when there is one. `custom` has only a print-mode command line to go by: its
/// program, bare.
pub fn interactive_for(provider: &str, prompt: Option<&str>) -> Option<Vec<String>> {
    let js = jarvis_settings();
    if let Some(c) = js.str_field("cliCommand").filter(|c| !c.is_empty()) {
        return Some(vec![c.split_whitespace().next()?.to_string()]);
    }
    let (bin, lead): (&str, &[&str]) = match provider {
        "anthropic" => ("claude", &[]),
        "openai" => ("codex", &[]),
        "gemini" => ("gemini", &["-i"]),
        "xai" => return Some(vec!["grok".to_string()]),
        _ => return None,
    };
    let mut argv = vec![bin.to_string()];
    if let Some(p) = prompt {
        argv.extend(lead.iter().map(|s| s.to_string()));
        argv.push(p.to_string());
    }
    Some(argv)
}

/// What the AI is told first: which files this is about, and to wait for the question.
pub fn opening_prompt(paths: &[String]) -> String {
    let files = paths.iter().map(|p| format!("`{p}`")).collect::<Vec<_>>().join(", ");
    format!("I have questions about {files}. Read {} and wait for my first question.", if paths.len() == 1 { "it" } else { "them" })
}

/// The terminal a program is run in: the desktop's chosen one (`xdg-terminal-exec`, which is
/// how Omarchy opens its own), else `$TERMINAL -e`, else alacritty.
/// It is started in `dir` either way; `xdg-terminal-exec` is told as well, as Omarchy's own
/// launcher tells it — a terminal that is one server with many windows takes its folder from
/// the option, not from whoever started the client.
pub fn terminal_argv(program: Vec<String>, dir: &std::path::Path, have_xdg: bool, terminal_env: Option<String>) -> Vec<String> {
    let mut argv = if have_xdg { vec!["xdg-terminal-exec".to_string(), format!("--dir={}", dir.display())] } else { vec![terminal_env.filter(|t| !t.is_empty()).unwrap_or_else(|| "alacritty".into()), "-e".to_string()] };
    argv.extend(program);
    argv
}

fn on_path(bin: &str) -> bool {
    std::env::var_os("PATH").map(|p| std::env::split_paths(&p).any(|d| d.join(bin).is_file())).unwrap_or(false)
}

/// A terminal window in `dir`.
pub fn open_terminal(dir: &Uri) -> Result<(), VfsError> {
    if !dir.is_local() {
        return Err(VfsError::Io("a terminal opens in a folder on this machine".into()));
    }
    let argv = terminal_argv(Vec::new(), &dir.to_path(), on_path("xdg-terminal-exec"), std::env::var("TERMINAL").ok());
    // `$TERMINAL -e` with nothing after it is not a command line: the bare terminal, then.
    let argv: Vec<String> = if argv.last().map(|a| a == "-e").unwrap_or(false) { argv[..argv.len() - 1].to_vec() } else { argv };
    crate::desktop::spawn_detached_in(&argv, Some(&dir.to_path())).map_err(VfsError::Io)
}

/// "Open AI here…": the chosen AI in a terminal window of its own, started in `dir` and, when
/// files were selected, told which ones this is about. The conversation is the user's, in the
/// tool they chose, with everything that tool can do — not a panel of ours in front of it.
pub fn open_external(dir: &Uri, uris: &[Uri]) -> Result<String, VfsError> {
    let (p, _) = provider();
    if !dir.is_local() || uris.iter().any(|u| !u.is_local()) {
        return Err(VfsError::Io("the AI works on files on this machine; copy these here first".into()));
    }
    let paths: Vec<String> = uris.iter().map(|u| u.to_path().to_string_lossy().into_owned()).collect();
    let prompt = (!paths.is_empty()).then(|| opening_prompt(&paths));
    let program = interactive_for(&p, prompt.as_deref()).ok_or_else(|| VfsError::Io("no command-line tool is set for this AI (Settings → Jarvis)".into()))?;
    if !on_path(&program[0]) {
        return Err(VfsError::Io(format!("{} is not installed", program[0])));
    }
    let bin = program[0].clone();
    let argv = terminal_argv(program, &dir.to_path(), on_path("xdg-terminal-exec"), std::env::var("TERMINAL").ok());
    crate::desktop::spawn_detached_in(&argv, Some(&dir.to_path())).map_err(VfsError::Io)?;
    Ok(bin)
}

pub fn status() -> Value {
    let (p, chosen_by) = provider();
    let cli = cli_for(&p);
    let available = cli.as_ref().map(|(bin, _)| crate::openin::on_path(bin)).unwrap_or(false);
    Value::obj()
        .b("configured", available)
        .s("provider", p.clone())
        .s("chosenBy", chosen_by)
        .opt_s("omarchyProvider", omarchy_provider())
        .opt_s("cli", cli.map(|(b, _)| b).as_deref())
        .b("cliAvailable", available)
        .v("providers", Value::Arr(PROVIDERS.iter().map(|x| Value::Str(x.to_string())).collect()))
        .done()
}

/// Settings → Jarvis: provider ("omarchy" to follow Omarchy) and an optional custom command.
pub fn configure(provider_choice: Option<&str>, cli_command: Option<&str>) -> Result<(), VfsError> {
    let mut patch = Value::obj();
    if let Some(p) = provider_choice {
        patch = patch.s("provider", p);
    }
    if let Some(c) = cli_command {
        patch = patch.s("cliCommand", c);
    }
    crate::config::set_settings(&Value::obj().v("jarvis", patch.done()).done()).map_err(|e| VfsError::Io(e.to_string()))
}

fn attachment(u: &Uri) -> Result<(String, String, bool), VfsError> {
    let path = u.to_path();
    let md = std::fs::metadata(&path)?;
    if md.is_dir() {
        let mut names: Vec<String> = std::fs::read_dir(&path)?
            .flatten()
            .map(|e| {
                let m = e.metadata().ok();
                format!("{}{}  {}", e.file_name().to_string_lossy(), if m.as_ref().map(|m| m.is_dir()).unwrap_or(false) { "/" } else { "" }, m.map(|m| m.len()).unwrap_or(0))
            })
            .collect();
        names.sort();
        return Ok((u.name().to_string() + "/", names.join("\n"), false));
    }
    let name = u.name().to_string();
    let text = if name.to_ascii_lowercase().ends_with(".pdf") {
        let out = std::process::Command::new("pdftotext").arg(&path).arg("-").output().map_err(|_| VfsError::Io("pdftotext is not installed".into()))?;
        String::from_utf8_lossy(&out.stdout).into_owned()
    } else {
        let bytes = std::fs::read(&path)?;
        if bytes.iter().take(8192).any(|&b| b == 0) {
            return Err(VfsError::Unsupported);
        }
        String::from_utf8_lossy(&bytes).into_owned()
    };
    if text.len() > ATTACH_CAP {
        let head: String = text.chars().take(HEAD_TAIL).collect();
        let tail: String = text.chars().rev().take(HEAD_TAIL).collect::<Vec<_>>().into_iter().rev().collect();
        return Ok((name, format!("{head}\n\n[… {} characters omitted …]\n\n{tail}", text.len() - 2 * HEAD_TAIL), true));
    }
    Ok((name, text, false))
}

/// Exact questions answered without a model: counts and occurrences.
pub fn local_answer(question: &str, attachments: &[(String, String, bool)]) -> Option<String> {
    let q = question.trim().to_ascii_lowercase();
    let (name, text, truncated) = attachments.first()?;
    if *truncated {
        return None;
    }
    let mut lines = Vec::new();
    if q.contains("count lines") || q == "lines" || q.starts_with("how many lines") {
        lines.push(format!("{}: {} lines", name, text.lines().count()));
    } else if q.contains("count words") || q.starts_with("how many words") {
        lines.push(format!("{}: {} words", name, text.split_whitespace().count()));
    } else if q.contains("count bytes") || q.contains("count characters") || q.starts_with("how big") {
        lines.push(format!("{}: {} bytes, {} characters", name, text.len(), text.chars().count()));
    } else if q.starts_with("how many times") || q.contains("occurrences of") || q.starts_with("count ") {
        // a quoted or backticked term
        let term = question.split(['"', '`', '\'']).nth(1)?;
        let n = text.matches(term).count();
        let ci = text.to_ascii_lowercase().matches(&term.to_ascii_lowercase()).count();
        lines.push(if n == ci { format!("{name}: \"{term}\" appears {n} time(s)") } else { format!("{name}: \"{term}\" appears {n} time(s) exactly, {ci} ignoring case") });
    } else {
        return None;
    }
    Some(lines.join("\n"))
}

/// Streams a query: `AiDelta` events, then `AiDone` or `AiError`.
pub fn query(tx: Sender<Value>, id: u64, _session: String, uris: Vec<Uri>, question: String, history: Value) {
    std::thread::Builder::new()
        .name("ai".into())
        .spawn(move || {
            let mut attachments = Vec::new();
            for u in &uris {
                match attachment(u) {
                    Ok(a) => attachments.push(a),
                    Err(e) => {
                        let _ = tx.send(proto::event("AiError").u("id", id).s("code", e.code()).s("message", format!("{}: {}", u.name(), e.message())).done());
                        return;
                    }
                }
            }
            if let Some(a) = local_answer(&question, &attachments) {
                let _ = tx.send(proto::event("AiDone").u("id", id).s("text", a).b("local", true).v("usage", Value::obj().u("input", 0).u("output", 0).done()).done());
                return;
            }
            let (p, _) = provider();
            let available = cli_for(&p).map(|(bin, _)| crate::openin::on_path(&bin)).unwrap_or(false);
            if !available {
                let _ = tx
                    .send(proto::event("AiError").u("id", id).s("code", "Unsupported").s("message", format!("{}'s command-line tool is not installed; install it or set a custom command in Settings → Jarvis", p)).done());
                return;
            }
            run_cli(tx, id, &uris, &question, &history);
        })
        .expect("spawn ai");
}

/// CLI mode: run the provider's tool in print mode from the first file's directory with a prompt
/// naming the files, and stream its stdout into the panel. Uses the tool's own login.
fn run_cli(tx: Sender<Value>, id: u64, uris: &[Uri], question: &str, history: &Value) {
    use std::io::{BufRead, BufReader};
    use std::process::{Command, Stdio};
    let (p, _) = provider();
    let Some((bin, args)) = cli_for(&p) else {
        let _ = tx.send(proto::event("AiError").u("id", id).s("code", "Unsupported").s("message", "no command-line tool for this provider").done());
        return;
    };
    let paths: Vec<String> = uris.iter().filter(|u| u.is_local()).map(|u| u.to_path().to_string_lossy().into_owned()).collect();
    if paths.len() != uris.len() {
        let _ = tx.send(proto::event("AiError").u("id", id).s("code", "Unsupported").s("message", "CLI mode works on local files; API mode can read remote ones").done());
        return;
    }
    // A question with nothing selected is a fair question ("how big is this folder?"): answer it
    // from home rather than indexing an empty list, which used to take the daemon down with it.
    let cwd = paths.first().and_then(|p| std::path::Path::new(p).parent().map(|d| d.to_path_buf())).unwrap_or_else(crate::config::home);
    let mut prompt = String::new();
    if let Some(h) = history.as_arr() {
        for turn in h {
            prompt.push_str(&format!("{}: {}\n", if turn.str_field("role") == Some("assistant") { "Assistant" } else { "User" }, turn.str_field("text").unwrap_or("")));
        }
        if !h.is_empty() {
            prompt.push('\n');
        }
    }
    if paths.is_empty() {
        prompt.push_str(&format!("Answer concisely in plain text. Question: {question}"));
    } else {
        prompt.push_str(&format!("Read {} and answer concisely in plain text. Question: {}", paths.iter().map(|x| format!("`{x}`")).collect::<Vec<_>>().join(", "), question));
    }
    let mut cmd = Command::new(&bin);
    cmd.args(args.iter().map(|a| a.replace("{prompt}", &prompt).replace("{files}", &paths.join(" "))));
    cmd.current_dir(&cwd).env("KIKI_SELECTION", paths.join("\n")).stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped());
    let mut child = match cmd.spawn() {
        Ok(c) => c,
        Err(e) => {
            let _ = tx.send(proto::event("AiError").u("id", id).s("code", "Io").s("message", format!("{bin}: {e}")).done());
            return;
        }
    };
    cli_children().lock().unwrap().insert(id, child.id());
    let stdout = child.stdout.take().unwrap();
    let stderr = child.stderr.take().unwrap();
    let err_thread = std::thread::spawn(move || {
        let mut s = String::new();
        for l in BufReader::new(stderr).lines().map_while(|l| l.ok()) {
            s.push_str(&l);
            s.push('\n');
        }
        s
    });
    let mut text = String::new();
    let mut buf = [0u8; 4096];
    let mut reader = BufReader::new(stdout);
    loop {
        use std::io::Read;
        match reader.read(&mut buf) {
            Ok(0) | Err(_) => break,
            Ok(n) => {
                let chunk = String::from_utf8_lossy(&buf[..n]).into_owned();
                text.push_str(&chunk);
                let _ = tx.send(proto::event("AiDelta").u("id", id).s("text", chunk).done());
            }
        }
    }
    let status = child.wait();
    cli_children().lock().unwrap().remove(&id);
    let errs = err_thread.join().unwrap_or_default();
    match status {
        Ok(st) if st.success() || !text.trim().is_empty() => {
            let _ = tx.send(proto::event("AiDone").u("id", id).s("text", text.trim_end().to_string()).b("local", false).v("usage", Value::obj().u("input", 0).u("output", 0).s("via", bin.clone()).done()).done());
        }
        Ok(st) => {
            let _ = tx.send(proto::event("AiError").u("id", id).s("code", "Io").s("message", format!("{bin} exited with {st}: {}", errs.trim())).done());
        }
        Err(e) => {
            let _ = tx.send(proto::event("AiError").u("id", id).s("code", "Io").s("message", e.to_string()).done());
        }
    }
}

fn cli_children() -> &'static std::sync::Mutex<std::collections::HashMap<u64, u32>> {
    static C: std::sync::OnceLock<std::sync::Mutex<std::collections::HashMap<u64, u32>>> = std::sync::OnceLock::new();
    C.get_or_init(|| std::sync::Mutex::new(std::collections::HashMap::new()))
}

/// Cancel a CLI-mode query by killing its process.
pub fn cancel(id: u64) -> bool {
    if let Some(pid) = cli_children().lock().unwrap().remove(&id) {
        unsafe { libc::kill(pid as libc::pid_t, libc::SIGTERM) };
        return true;
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn local_answers() {
        let a = vec![("x.rs".to_string(), "fn main() {}\nfn ping() {}\nfn Ping() {}\n".to_string(), false)];
        assert_eq!(local_answer("count lines", &a).unwrap(), "x.rs: 3 lines");
        assert!(local_answer("how many times does `fn` appear", &a).unwrap().contains("3 time"));
        assert!(local_answer("how many times does \"ping\" appear", &a).unwrap().contains("1 time(s) exactly, 2 ignoring case"));
        assert!(local_answer("what does this do", &a).is_none());
    }

    #[test]
    fn the_conversation_is_started_with_the_files_and_not_in_print_mode() {
        let prompt = opening_prompt(&["/w/footer.js".into()]);
        assert_eq!(prompt, "I have questions about `/w/footer.js`. Read it and wait for my first question.");
        assert!(opening_prompt(&["/a".into(), "/b".into()]).contains("`/a`, `/b`. Read them"));
        assert_eq!(interactive_for("anthropic", Some(&prompt)), Some(vec!["claude".to_string(), prompt.clone()]));
        assert_eq!(interactive_for("gemini", Some("x")), Some(vec!["gemini".to_string(), "-i".to_string(), "x".to_string()]));
        assert!(!interactive_for("openai", Some("x")).unwrap().contains(&"exec".to_string()), "exec is codex's print mode");
        // Nothing selected: the tool alone, in the folder.
        assert_eq!(interactive_for("anthropic", None), Some(vec!["claude".to_string()]));
        assert_eq!(interactive_for("gemini", None), Some(vec!["gemini".to_string()]));
        assert_eq!(interactive_for("nobody", Some("x")), None);
    }

    #[test]
    fn it_runs_in_the_desktops_terminal() {
        let prog = vec!["claude".to_string(), "hi".to_string()];
        let d = std::path::Path::new("/w/site");
        assert_eq!(terminal_argv(prog.clone(), d, true, Some("kitty".into())), ["xdg-terminal-exec", "--dir=/w/site", "claude", "hi"]);
        assert_eq!(terminal_argv(prog.clone(), d, false, Some("kitty".into())), ["kitty", "-e", "claude", "hi"]);
        assert_eq!(terminal_argv(prog, d, false, None), ["alacritty", "-e", "claude", "hi"]);
    }

    #[test]
    fn cli_commands() {
        assert_eq!(cli_for("anthropic").unwrap().0, "claude");
        assert_eq!(cli_for("openai").unwrap().0, "codex");
        assert_eq!(cli_for("gemini").unwrap().0, "gemini");
        assert!(cli_for("custom").is_none()); // needs cliCommand
    }
}
