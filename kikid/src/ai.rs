//! Jarvis (plan 19): provider selection (follows Omarchy's AI unless overridden), attachments, local answers, the jarvis plugin.

use crate::json::Value;
use crate::plugin::Msg;
use crate::proto;
use crate::vfs::uri::Uri;
use crate::vfs::VfsError;
use std::sync::mpsc::Sender;

pub const ATTACH_CAP: usize = 200_000;
pub const HEAD_TAIL: usize = 50_000;

pub const PROVIDERS: &[&str] = &["anthropic", "openai", "gemini", "xai", "ollama", "custom"];

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
            if l.contains("claude.ai") || l.contains("anthropic") { return Some("anthropic"); }
            if l.contains("chatgpt.com") || l.contains("openai") { return Some("openai"); }
            if l.contains("gemini.google") { return Some("gemini"); }
            if l.contains("grok.com") || l.contains("x.ai") { return Some("xai"); }
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

fn env_key(provider: &str) -> Option<String> {
    let var = match provider {
        "anthropic" => "ANTHROPIC_API_KEY",
        "openai" => "OPENAI_API_KEY",
        "gemini" => "GEMINI_API_KEY",
        "xai" => "XAI_API_KEY",
        "custom" => "JARVIS_API_KEY",
        _ => return None,
    };
    std::env::var(var).ok().filter(|k| !k.is_empty())
}

/// (credential, source) for a provider: env, keyring, the `ant` CLI for anthropic, or none (ollama needs none).
fn credential(provider: &str) -> (Option<String>, Option<&'static str>) {
    if let Some(k) = env_key(provider) {
        return (Some(k), Some("env"));
    }
    if let Some(k) = crate::locations::keyring::lookup("jarvis", provider) {
        return (Some(k), Some("keyring"));
    }
    if provider == "anthropic" && crate::openin::on_path("ant") {
        let ok = std::process::Command::new("ant").args(["auth", "status"]).stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null()).status().map(|s| s.success()).unwrap_or(false);
        if ok {
            return (None, Some("ant"));
        }
    }
    if provider == "ollama" {
        return (None, Some("local"));
    }
    (None, None)
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
        _ => return None,
    };
    Some((bin.to_string(), args.iter().map(|a| a.to_string()).collect()))
}

/// "api" | "cli" | "auto" (cli when the provider's tool is installed, else api).
pub fn mode() -> (&'static str, bool) {
    let js = jarvis_settings();
    let (p, _) = provider();
    let cli_ok = cli_for(&p).map(|(bin, _)| crate::openin::on_path(&bin)).unwrap_or(false);
    match js.str_field("mode") {
        Some("cli") => ("cli", cli_ok),
        Some("api") => ("api", cli_ok),
        _ => (if cli_ok { "cli" } else { "api" }, cli_ok),
    }
}

pub fn status() -> Value {
    let (p, chosen_by) = provider();
    let (_, source) = credential(&p);
    let s = jarvis_settings();
    let (m, cli_ok) = mode();
    let configured = if m == "cli" { cli_ok } else { source.is_some() };
    Value::obj()
        .b("configured", configured)
        .s("mode", m)
        .b("cliAvailable", cli_ok)
        .opt_s("cli", cli_for(&p).map(|(b, _)| b).as_deref())
        .s("provider", p.clone())
        .s("chosenBy", chosen_by)
        .opt_s("omarchyProvider", omarchy_provider())
        .opt_s("model", s.str_field("model").filter(|m| !m.is_empty()))
        .opt_s("baseUrl", s.str_field("baseUrl").filter(|m| !m.is_empty()))
        .opt_s("source", source)
        .v("providers", Value::Arr(PROVIDERS.iter().map(|x| Value::Str(x.to_string())).collect()))
        .done()
}

/// Settings → Jarvis: provider ("omarchy" to follow Omarchy), a key for a provider, model, base URL.
pub fn configure(provider_choice: Option<&str>, key_for: Option<&str>, api_key: Option<&str>, model: Option<&str>, base_url: Option<&str>, mode: Option<&str>, cli_command: Option<&str>) -> Result<(), VfsError> {
    let mut patch = Value::obj();
    if let Some(m) = mode {
        patch = patch.s("mode", m);
    }
    if let Some(c) = cli_command {
        patch = patch.s("cliCommand", c);
    }
    if let Some(p) = provider_choice {
        patch = patch.s("provider", p);
    }
    if let Some(m) = model {
        patch = patch.s("model", m);
    }
    if let Some(b) = base_url {
        patch = patch.s("baseUrl", b);
    }
    crate::config::set_settings(&Value::obj().v("jarvis", patch.done()).done()).map_err(|e| VfsError::Io(e.to_string()))?;
    if let Some(p) = key_for {
        match api_key {
            Some(k) if !k.is_empty() => crate::locations::keyring::store("jarvis", p, k)?,
            _ => crate::locations::keyring::clear("jarvis", p),
        }
    }
    Ok(())
}

fn attachment(u: &Uri) -> Result<(String, String, bool), VfsError> {
    let path = u.to_path();
    let md = std::fs::metadata(&path)?;
    if md.is_dir() {
        let mut names: Vec<String> = std::fs::read_dir(&path)?.flatten().map(|e| {
            let m = e.metadata().ok();
            format!("{}{}  {}", e.file_name().to_string_lossy(), if m.as_ref().map(|m| m.is_dir()).unwrap_or(false) { "/" } else { "" }, m.map(|m| m.len()).unwrap_or(0))
        }).collect();
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
pub fn query(tx: Sender<Value>, id: u64, session: String, uris: Vec<Uri>, question: String, history: Value) {
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
            let (m, cli_ok) = mode();
            if m == "cli" {
                if !cli_ok {
                    let _ = tx.send(proto::event("AiError").u("id", id).s("code", "Unsupported").s("message", "the provider's command-line tool is not installed; switch Jarvis to API mode in Settings").done());
                    return;
                }
                run_cli(tx, id, &uris, &question, &history);
                return;
            }
            let helper = match crate::helpers::get("kiki-plugin-jarvis") {
                Ok(h) => h,
                Err(e) => {
                    let _ = tx.send(proto::event("AiError").u("id", id).s("code", "Unsupported").s("message", e.message()).done());
                    return;
                }
            };
            let (prov, _) = provider();
            let (key, source) = credential(&prov);
            let js = jarvis_settings();
            let req = Value::obj()
                .s("type", "Query")
                .s("session", session)
                .s("question", question)
                .s("provider", prov.clone())
                .s("apiKey", key.unwrap_or_default())
                .s("source", source.unwrap_or("none"))
                .opt_s("model", js.str_field("model").filter(|m| !m.is_empty()))
                .opt_s("baseUrl", js.str_field("baseUrl").filter(|m| !m.is_empty()))
                .v("attachments", Value::Arr(attachments.iter().map(|(n, t, tr)| Value::obj().s("name", n.clone()).s("text", t.clone()).b("truncated", *tr).done()).collect()))
                .v("history", history)
                .done();
            let tx2 = tx.clone();
            let r = helper.request_stream(req, |m| {
                if let Msg::Json(v) = m {
                    if let Some(t) = v.str_field("delta") {
                        let _ = tx2.send(proto::event("AiDelta").u("id", id).s("text", t).done());
                    }
                }
            });
            match r {
                Ok(v) => {
                    let _ = tx.send(proto::event("AiDone").u("id", id).s("text", v.str_field("text").unwrap_or("")).b("local", false).v("usage", v.get("usage").cloned().unwrap_or(Value::Null)).done());
                }
                Err(e) => {
                    let _ = tx.send(proto::event("AiError").u("id", id).s("code", e.code()).s("message", e.message()).done());
                }
            }
        })
        .expect("spawn ai");
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
    let cwd = std::path::Path::new(&paths[0]).parent().map(|d| d.to_path_buf()).unwrap_or_else(crate::config::home);
    let mut prompt = String::new();
    if let Some(h) = history.as_arr() {
        for turn in h {
            prompt.push_str(&format!("{}: {}\n", if turn.str_field("role") == Some("assistant") { "Assistant" } else { "User" }, turn.str_field("text").unwrap_or("")));
        }
        if !h.is_empty() {
            prompt.push_str("\n");
        }
    }
    prompt.push_str(&format!("Read {} and answer concisely in plain text. Question: {}", paths.iter().map(|x| format!("`{x}`")).collect::<Vec<_>>().join(", "), question));
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
