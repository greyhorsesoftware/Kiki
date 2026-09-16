//! `Query` → streamed `{ delta }` frames, then `{ text, usage }`. Calls the Claude Messages API
//! with streaming, adaptive thinking (the default), the server-side refusal fallback, and a
//! cache breakpoint on the attachments so follow-ups in a session reuse the prefix.

use kiki_plugin_sdk::json::{self, Value};
use kiki_plugin_sdk::{read_frame, write_json};
use std::io::{BufRead, BufReader, Read};

const MODEL: &str = "claude-opus-5";
const URL: &str = "https://api.anthropic.com/v1/messages";
const SYSTEM: &str = "You answer questions about the files attached in the conversation. Be concise and concrete. Quote the file where it helps. If only part of a file was attached (marked as omitted), say when the answer needs the whole file. Answer in plain text.";

fn credentials(req: &Value) -> Option<(String, bool)> {
    let key = req.str_field("apiKey").unwrap_or("");
    if !key.is_empty() {
        return Some((key.to_string(), false));
    }
    if req.str_field("source") == Some("ant") {
        let out = std::process::Command::new("ant").args(["auth", "print-credentials", "--access-token"]).output().ok()?;
        if out.status.success() {
            let t = String::from_utf8_lossy(&out.stdout).trim().to_string();
            if !t.is_empty() {
                return Some((t, true));
            }
        }
    }
    None
}

fn build_body(req: &Value) -> Value {
    let question = req.str_field("question").unwrap_or("");
    let mut messages: Vec<Value> = Vec::new();
    // Prior turns of this session, as plain text.
    if let Some(h) = req.get("history").and_then(Value::as_arr) {
        for turn in h {
            let role = turn.str_field("role").unwrap_or("user");
            let text = turn.str_field("text").unwrap_or("");
            messages.push(Value::obj().s("role", role).v("content", Value::Arr(vec![Value::obj().s("type", "text").s("text", text).done()])).done());
        }
    }
    // Attachments first in the first user turn, with a cache breakpoint, then the question.
    let mut content: Vec<Value> = Vec::new();
    if messages.is_empty() {
        if let Some(att) = req.get("attachments").and_then(Value::as_arr) {
            let mut block = String::new();
            for a in att {
                block.push_str(&format!("<file name=\"{}\"{}>\n{}\n</file>\n\n", a.str_field("name").unwrap_or(""), if a.get("truncated").and_then(Value::as_bool).unwrap_or(false) { " truncated=\"true\"" } else { "" }, a.str_field("text").unwrap_or("")));
            }
            if !block.is_empty() {
                content.push(Value::obj().s("type", "text").s("text", block).v("cache_control", Value::obj().s("type", "ephemeral").done()).done());
            }
        }
    }
    content.push(Value::obj().s("type", "text").s("text", question).done());
    messages.push(Value::obj().s("role", "user").v("content", Value::Arr(content)).done());
    let analytic = question.len() > 80 || ["explain", "why", "review", "summar", "refactor", "bug"].iter().any(|k| question.to_ascii_lowercase().contains(k));
    Value::obj()
        .s("model", MODEL)
        .u("max_tokens", 16000)
        .b("stream", true)
        .s("system", SYSTEM)
        .v("messages", Value::Arr(messages))
        .v("output_config", Value::obj().s("effort", if analytic { "high" } else { "medium" }).done())
        .s("fallbacks", "default")
        .done()
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
            "Describe" => Value::obj().u("id", id).v("ok", kiki_plugin_sdk::service_describe("ai", "Claude queries", env!("CARGO_PKG_VERSION"), &["Query"])).done(),
            "Shutdown" => {
                let _ = write_json(&mut stdout, &Value::obj().u("id", id).v("ok", Value::obj().done()).done());
                return;
            }
            "Query" => match credentials(&v) {
                None => Value::obj().u("id", id).v("err", Value::obj().s("code", "Auth").s("message", "no Anthropic credential: set a key in Settings, ANTHROPIC_API_KEY, or log in with `ant auth login`").done()).done(),
                Some((cred, oauth)) => {
                    let body = json::to_string(&build_body(&v));
                    let mut r = ureq::post(URL).set("content-type", "application/json").set("anthropic-version", "2023-06-01").set("anthropic-beta", if oauth { "oauth-2025-04-20,server-side-fallback-2026-07-01" } else { "server-side-fallback-2026-07-01" });
                    r = if oauth { r.set("authorization", &format!("Bearer {cred}")) } else { r.set("x-api-key", &cred) };
                    match r.send_string(&body) {
                        Ok(resp) => {
                            let mut text = String::new();
                            let mut input = 0u64;
                            let mut output = 0u64;
                            let mut cache_read = 0u64;
                            let mut stop = String::new();
                            let reader = BufReader::new(resp.into_reader());
                            for line in reader.lines().map_while(|l| l.ok()) {
                                let Some(data) = line.strip_prefix("data: ") else { continue };
                                let Ok(ev) = json::parse(data.as_bytes()) else { continue };
                                match ev.str_field("type") {
                                    Some("content_block_delta") => {
                                        if let Some(d) = ev.get("delta") {
                                            if d.str_field("type") == Some("text_delta") {
                                                let t = d.str_field("text").unwrap_or("");
                                                text.push_str(t);
                                                let _ = write_json(&mut stdout, &Value::obj().u("id", id).s("delta", t).done());
                                            }
                                        }
                                    }
                                    Some("message_start") => {
                                        if let Some(u) = ev.get("message").and_then(|m| m.get("usage")) {
                                            input = u.u64_field("input_tokens").unwrap_or(0);
                                            cache_read = u.u64_field("cache_read_input_tokens").unwrap_or(0);
                                        }
                                    }
                                    Some("message_delta") => {
                                        if let Some(u) = ev.get("usage") {
                                            output = u.u64_field("output_tokens").unwrap_or(output);
                                        }
                                        if let Some(s) = ev.get("delta").and_then(|d| d.str_field("stop_reason")) {
                                            stop = s.to_string();
                                        }
                                    }
                                    Some("error") => {
                                        let msg = ev.get("error").and_then(|e| e.str_field("message")).unwrap_or("stream error").to_string();
                                        let _ = write_json(&mut stdout, &Value::obj().u("id", id).v("err", Value::obj().s("code", "Io").s("message", msg).done()).done());
                                        continue;
                                    }
                                    _ => {}
                                }
                            }
                            if stop == "refusal" && text.is_empty() {
                                text = "The model declined this request.".into();
                            }
                            Value::obj().u("id", id).v("ok", Value::obj().s("text", text).s("stopReason", stop).v("usage", Value::obj().u("input", input).u("output", output).u("cacheRead", cache_read).done()).done()).done()
                        }
                        Err(ureq::Error::Status(code, resp)) => {
                            let mut body = String::new();
                            let _ = resp.into_reader().take(4096).read_to_string(&mut body);
                            let code_str = match code { 401 | 403 => "Auth", 429 => "Busy", _ => "Io" };
                            Value::obj().u("id", id).v("err", Value::obj().s("code", code_str).s("message", format!("HTTP {code}: {}", body.trim())).done()).done()
                        }
                        Err(e) => Value::obj().u("id", id).v("err", Value::obj().s("code", "Network").s("message", e.to_string()).done()).done(),
                    }
                }
            },
            _ => Value::obj().u("id", id).v("err", Value::obj().s("code", "Unsupported").s("message", "unknown request").done()).done(),
        };
        let _ = write_json(&mut stdout, &reply);
    }
}
