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

pub fn status() -> Value {
    let (p, chosen_by) = provider();
    let (_, source) = credential(&p);
    let s = jarvis_settings();
    Value::obj()
        .b("configured", source.is_some())
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
pub fn configure(provider_choice: Option<&str>, key_for: Option<&str>, api_key: Option<&str>, model: Option<&str>, base_url: Option<&str>) -> Result<(), VfsError> {
    let mut patch = Value::obj();
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
