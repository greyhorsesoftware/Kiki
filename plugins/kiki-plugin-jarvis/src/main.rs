//! Jarvis: kiki's assistant, provider-agnostic. `Query` → streamed `{ delta }` frames, then
//! `{ text, usage }`. Providers: anthropic (Messages API), openai-compatible (OpenAI, xAI,
//! Ollama, any base URL), gemini. The daemon picks the provider and passes credentials.

use kiki_plugin_sdk::json::{self, Value};
use kiki_plugin_sdk::{read_frame, write_json};
use std::io::{BufRead, BufReader, Read};

const SYSTEM: &str = "You are Jarvis, the assistant inside the kiki file manager. You answer questions about the files attached in the conversation. Be concise and concrete. Quote the file where it helps. If only part of a file was attached (marked as omitted), say when the answer needs the whole file. Answer in plain text.";

struct Provider {
    kind: String,     // anthropic | openai | gemini
    base_url: String,
    model: String,
    key: String,
    oauth: bool,
}

fn provider(req: &Value) -> Provider {
    let kind = req.str_field("provider").unwrap_or("anthropic").to_string();
    let default_base = match kind.as_str() {
        "anthropic" => "https://api.anthropic.com",
        "gemini" => "https://generativelanguage.googleapis.com",
        "xai" => "https://api.x.ai/v1",
        "ollama" => "http://localhost:11434/v1",
        _ => "https://api.openai.com/v1",
    };
    let base_url = req.str_field("baseUrl").filter(|s| !s.is_empty()).unwrap_or(default_base).trim_end_matches('/').to_string();
    let model = req.str_field("model").filter(|s| !s.is_empty()).unwrap_or(match kind.as_str() {
        "anthropic" => "claude-opus-5",
        "gemini" => "gemini-2.5-pro",
        "xai" => "grok-4",
        "ollama" => "llama3.1",
        _ => "gpt-5",
    }).to_string();
    let mut key = req.str_field("apiKey").unwrap_or("").to_string();
    let mut oauth = false;
    if key.is_empty() && kind == "anthropic" && req.str_field("source") == Some("ant") {
        if let Ok(out) = std::process::Command::new("ant").args(["auth", "print-credentials", "--access-token"]).output() {
            if out.status.success() {
                key = String::from_utf8_lossy(&out.stdout).trim().to_string();
                oauth = !key.is_empty();
            }
        }
    }
    let wire = if matches!(kind.as_str(), "openai" | "xai" | "ollama" | "custom") { "openai".to_string() } else { kind.clone() };
    Provider { kind: wire, base_url, model, key, oauth }
}

fn attachments_block(req: &Value) -> String {
    let mut block = String::new();
    if let Some(att) = req.get("attachments").and_then(Value::as_arr) {
        for a in att {
            block.push_str(&format!("<file name=\"{}\"{}>\n{}\n</file>\n\n", a.str_field("name").unwrap_or(""), if a.get("truncated").and_then(Value::as_bool).unwrap_or(false) { " truncated=\"true\"" } else { "" }, a.str_field("text").unwrap_or("")));
        }
    }
    block
}

/// (role, text) turns: prior history, then the attachments and the question in the last user turn.
fn turns(req: &Value) -> Vec<(String, String)> {
    let mut t: Vec<(String, String)> = Vec::new();
    if let Some(h) = req.get("history").and_then(Value::as_arr) {
        for turn in h {
            t.push((turn.str_field("role").unwrap_or("user").to_string(), turn.str_field("text").unwrap_or("").to_string()));
        }
    }
    let q = req.str_field("question").unwrap_or("").to_string();
    let block = if t.is_empty() { attachments_block(req) } else { String::new() };
    t.push(("user".into(), if block.is_empty() { q } else { format!("{block}{q}") }));
    t
}

fn analytic(q: &str) -> bool {
    q.len() > 80 || ["explain", "why", "review", "summar", "refactor", "bug"].iter().any(|k| q.to_ascii_lowercase().contains(k))
}

struct Outcome {
    text: String,
    input: u64,
    output: u64,
    stop: String,
}

fn stream(resp: ureq::Response, mut on_line: impl FnMut(&str)) {
    let reader = BufReader::new(resp.into_reader());
    for line in reader.lines().map_while(|l| l.ok()) {
        if let Some(data) = line.strip_prefix("data: ") {
            on_line(data);
        }
    }
}

fn err_of(e: ureq::Error) -> (&'static str, String) {
    match e {
        ureq::Error::Status(code, resp) => {
            let mut body = String::new();
            let _ = resp.into_reader().take(4096).read_to_string(&mut body);
            (match code { 401 | 403 => "Auth", 429 => "Busy", _ => "Io" }, format!("HTTP {code}: {}", body.trim()))
        }
        e => ("Network", e.to_string()),
    }
}

fn call_anthropic(p: &Provider, req: &Value, id: u64, out: &mut dyn std::io::Write) -> Result<Outcome, (&'static str, String)> {
    let q = req.str_field("question").unwrap_or("");
    let mut messages: Vec<Value> = Vec::new();
    let all = turns(req);
    let n = all.len();
    for (i, (role, text)) in all.into_iter().enumerate() {
        let mut content = Vec::new();
        if i == n - 1 {
            // Cache breakpoint on the attachments so follow-ups in a session reuse the prefix.
            let block = attachments_block(req);
            if i == 0 && !block.is_empty() {
                content.push(Value::obj().s("type", "text").s("text", block).v("cache_control", Value::obj().s("type", "ephemeral").done()).done());
                content.push(Value::obj().s("type", "text").s("text", q).done());
            } else {
                content.push(Value::obj().s("type", "text").s("text", text).done());
            }
        } else {
            content.push(Value::obj().s("type", "text").s("text", text).done());
        }
        messages.push(Value::obj().s("role", role).v("content", Value::Arr(content)).done());
    }
    let body = Value::obj().s("model", p.model.clone()).u("max_tokens", 16000).b("stream", true).s("system", SYSTEM).v("messages", Value::Arr(messages)).v("output_config", Value::obj().s("effort", if analytic(q) { "high" } else { "medium" }).done()).s("fallbacks", "default").done();
    let mut r = ureq::post(&format!("{}/v1/messages", p.base_url)).set("content-type", "application/json").set("anthropic-version", "2023-06-01").set("anthropic-beta", if p.oauth { "oauth-2025-04-20,server-side-fallback-2026-07-01" } else { "server-side-fallback-2026-07-01" });
    r = if p.oauth { r.set("authorization", &format!("Bearer {}", p.key)) } else { r.set("x-api-key", &p.key) };
    let resp = r.send_string(&json::to_string(&body)).map_err(err_of)?;
    let mut o = Outcome { text: String::new(), input: 0, output: 0, stop: String::new() };
    stream(resp, |data| {
        let Ok(ev) = json::parse(data.as_bytes()) else { return };
        match ev.str_field("type") {
            Some("content_block_delta") => {
                if let Some(d) = ev.get("delta") {
                    if d.str_field("type") == Some("text_delta") {
                        let t = d.str_field("text").unwrap_or("");
                        o.text.push_str(t);
                        let _ = write_json(out, &Value::obj().u("id", id).s("delta", t).done());
                    }
                }
            }
            Some("message_start") => {
                if let Some(u) = ev.get("message").and_then(|m| m.get("usage")) {
                    o.input = u.u64_field("input_tokens").unwrap_or(0);
                }
            }
            Some("message_delta") => {
                if let Some(u) = ev.get("usage") {
                    o.output = u.u64_field("output_tokens").unwrap_or(o.output);
                }
                if let Some(s) = ev.get("delta").and_then(|d| d.str_field("stop_reason")) {
                    o.stop = s.to_string();
                }
            }
            _ => {}
        }
    });
    if o.stop == "refusal" && o.text.is_empty() {
        o.text = "The model declined this request.".into();
    }
    Ok(o)
}

fn call_openai(p: &Provider, req: &Value, id: u64, out: &mut dyn std::io::Write) -> Result<Outcome, (&'static str, String)> {
    let mut messages = vec![Value::obj().s("role", "system").s("content", SYSTEM).done()];
    for (role, text) in turns(req) {
        messages.push(Value::obj().s("role", role).s("content", text).done());
    }
    let body = Value::obj().s("model", p.model.clone()).b("stream", true).v("messages", Value::Arr(messages)).v("stream_options", Value::obj().b("include_usage", true).done()).done();
    let mut r = ureq::post(&format!("{}/chat/completions", p.base_url)).set("content-type", "application/json");
    if !p.key.is_empty() {
        r = r.set("authorization", &format!("Bearer {}", p.key));
    }
    let resp = r.send_string(&json::to_string(&body)).map_err(err_of)?;
    let mut o = Outcome { text: String::new(), input: 0, output: 0, stop: String::new() };
    stream(resp, |data| {
        if data.trim() == "[DONE]" {
            return;
        }
        let Ok(ev) = json::parse(data.as_bytes()) else { return };
        if let Some(c) = ev.get("choices").and_then(Value::as_arr).and_then(|a| a.first()) {
            if let Some(t) = c.get("delta").and_then(|d| d.str_field("content")) {
                o.text.push_str(t);
                let _ = write_json(out, &Value::obj().u("id", id).s("delta", t).done());
            }
            if let Some(s) = c.str_field("finish_reason") {
                o.stop = s.to_string();
            }
        }
        if let Some(u) = ev.get("usage") {
            o.input = u.u64_field("prompt_tokens").unwrap_or(o.input);
            o.output = u.u64_field("completion_tokens").unwrap_or(o.output);
        }
    });
    Ok(o)
}

fn call_gemini(p: &Provider, req: &Value, id: u64, out: &mut dyn std::io::Write) -> Result<Outcome, (&'static str, String)> {
    let contents: Vec<Value> = turns(req).into_iter().map(|(role, text)| Value::obj().s("role", if role == "assistant" { "model" } else { "user" }).v("parts", Value::Arr(vec![Value::obj().s("text", text).done()])).done()).collect();
    let body = Value::obj().v("system_instruction", Value::obj().v("parts", Value::Arr(vec![Value::obj().s("text", SYSTEM).done()])).done()).v("contents", Value::Arr(contents)).done();
    let url = format!("{}/v1beta/models/{}:streamGenerateContent?alt=sse&key={}", p.base_url, p.model, p.key);
    let resp = ureq::post(&url).set("content-type", "application/json").send_string(&json::to_string(&body)).map_err(err_of)?;
    let mut o = Outcome { text: String::new(), input: 0, output: 0, stop: String::new() };
    stream(resp, |data| {
        let Ok(ev) = json::parse(data.as_bytes()) else { return };
        if let Some(c) = ev.get("candidates").and_then(Value::as_arr).and_then(|a| a.first()) {
            if let Some(parts) = c.get("content").and_then(|x| x.get("parts")).and_then(Value::as_arr) {
                for part in parts {
                    if let Some(t) = part.str_field("text") {
                        o.text.push_str(t);
                        let _ = write_json(out, &Value::obj().u("id", id).s("delta", t).done());
                    }
                }
            }
            if let Some(s) = c.str_field("finishReason") {
                o.stop = s.to_string();
            }
        }
        if let Some(u) = ev.get("usageMetadata") {
            o.input = u.u64_field("promptTokenCount").unwrap_or(o.input);
            o.output = u.u64_field("candidatesTokenCount").unwrap_or(o.output);
        }
    });
    Ok(o)
}

fn main() {
    let mut stdin = std::io::stdin().lock();
    let mut stdout = std::io::stdout().lock();
    while let Ok(Some((kind, payload))) = read_frame(&mut stdin) {
        if kind != 0 {
            continue;
        }
        let Ok(v) = json::parse(&payload) else { continue };
        let id = v.u64_field("id").unwrap_or(0);
        let reply = match v.str_field("type").unwrap_or("") {
            "Ping" => Value::obj().u("id", id).v("ok", Value::obj().done()).done(),
            "Describe" => Value::obj().u("id", id).v("ok", kiki_plugin_sdk::service_describe("jarvis", "Jarvis (anthropic, openai, gemini, xai, ollama, custom)", env!("CARGO_PKG_VERSION"), &["Query"])).done(),
            "Shutdown" => {
                let _ = write_json(&mut stdout, &Value::obj().u("id", id).v("ok", Value::obj().done()).done());
                return;
            }
            "Query" => {
                let p = provider(&v);
                if p.key.is_empty() && p.kind != "openai" {
                    Value::obj().u("id", id).v("err", Value::obj().s("code", "Auth").s("message", format!("no credential for {}: add a key in Settings → Jarvis", v.str_field("provider").unwrap_or("the provider"))).done()).done()
                } else if p.key.is_empty() && !p.base_url.contains("localhost") && !p.base_url.contains("127.0.0.1") {
                    Value::obj().u("id", id).v("err", Value::obj().s("code", "Auth").s("message", format!("no credential for {}: add a key in Settings → Jarvis", v.str_field("provider").unwrap_or("the provider"))).done()).done()
                } else {
                    let r = match p.kind.as_str() {
                        "anthropic" => call_anthropic(&p, &v, id, &mut stdout),
                        "gemini" => call_gemini(&p, &v, id, &mut stdout),
                        _ => call_openai(&p, &v, id, &mut stdout),
                    };
                    match r {
                        Ok(o) => Value::obj().u("id", id).v("ok", Value::obj().s("text", o.text).s("stopReason", o.stop).s("model", p.model.clone()).v("usage", Value::obj().u("input", o.input).u("output", o.output).done()).done()).done(),
                        Err((code, msg)) => Value::obj().u("id", id).v("err", Value::obj().s("code", code).s("message", msg).done()).done(),
                    }
                }
            }
            _ => Value::obj().u("id", id).v("err", Value::obj().s("code", "Unsupported").s("message", "unknown request").done()).done(),
        };
        let _ = write_json(&mut stdout, &reply);
    }
}
