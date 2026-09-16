//! AI query (plan 19): attachments, local answers for exact questions, and the Claude helper.

use crate::json::Value;
use crate::plugin::Msg;
use crate::proto;
use crate::vfs::uri::Uri;
use crate::vfs::VfsError;
use std::sync::mpsc::Sender;

pub const ATTACH_CAP: usize = 200_000;
pub const HEAD_TAIL: usize = 50_000;

pub fn status() -> Value {
    let (source, configured) = credential_source();
    Value::obj().b("configured", configured).s("model", "claude-opus-5").opt_s("source", source).done()
}

fn credential_source() -> (Option<&'static str>, bool) {
    if std::env::var("ANTHROPIC_API_KEY").map(|k| !k.is_empty()).unwrap_or(false) {
        return (Some("env"), true);
    }
    if crate::locations::keyring::lookup("ai", "anthropic").is_some() {
        return (Some("keyring"), true);
    }
    if crate::openin::on_path("ant") {
        let ok = std::process::Command::new("ant").args(["auth", "status"]).stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null()).status().map(|s| s.success()).unwrap_or(false);
        if ok {
            return (Some("ant"), true);
        }
    }
    (None, false)
}

pub fn configure(api_key: Option<&str>) -> Result<(), VfsError> {
    match api_key {
        Some(k) if !k.is_empty() => crate::locations::keyring::store("ai", "anthropic", k),
        _ => {
            crate::locations::keyring::clear("ai", "anthropic");
            Ok(())
        }
    }
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
            let helper = match crate::helpers::get("kiki-plugin-ai") {
                Ok(h) => h,
                Err(e) => {
                    let _ = tx.send(proto::event("AiError").u("id", id).s("code", "Unsupported").s("message", e.message()).done());
                    return;
                }
            };
            let (source, _) = credential_source();
            let key = match source {
                Some("env") => std::env::var("ANTHROPIC_API_KEY").unwrap_or_default(),
                Some("keyring") => crate::locations::keyring::lookup("ai", "anthropic").unwrap_or_default(),
                _ => String::new(),
            };
            let req = Value::obj()
                .s("type", "Query")
                .s("session", session)
                .s("question", question)
                .s("apiKey", key)
                .s("source", source.unwrap_or("none"))
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
