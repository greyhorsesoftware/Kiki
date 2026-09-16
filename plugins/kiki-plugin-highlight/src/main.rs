//! `Highlight { path, first, count }` → lines of `{ text, class }` spans. Parses the whole file once
//! per (path, mtime) and caches the line spans; files over 8 MB come back plain.

use kiki_plugin_sdk::json::{self, Value};
use kiki_plugin_sdk::{read_frame, write_json};
use std::collections::HashMap;
use std::io::{self, Read};
use tree_sitter_highlight::{HighlightConfiguration, HighlightEvent, Highlighter};

const CLASSES: &[&str] = &["keyword", "string", "comment", "number", "type", "function", "variable", "operator", "punctuation", "tag", "attribute", "constant", "property", "label", "embedded"];
const PLAIN_OVER: u64 = 8 * 1024 * 1024;

struct Grammar {
    conf: HighlightConfiguration,
}

fn grammar(lang: &str) -> Option<Grammar> {
    let (language, highlights, injections, locals): (tree_sitter::Language, &str, &str, &str) = match lang {
        "rust" => (tree_sitter_rust::LANGUAGE.into(), tree_sitter_rust::HIGHLIGHTS_QUERY, tree_sitter_rust::INJECTIONS_QUERY, ""),
        "c" => (tree_sitter_c::LANGUAGE.into(), tree_sitter_c::HIGHLIGHT_QUERY, "", ""),
        "cpp" => (tree_sitter_cpp::LANGUAGE.into(), tree_sitter_cpp::HIGHLIGHT_QUERY, "", ""),
        "go" => (tree_sitter_go::LANGUAGE.into(), tree_sitter_go::HIGHLIGHTS_QUERY, "", ""),
        "java" => (tree_sitter_java::LANGUAGE.into(), tree_sitter_java::HIGHLIGHTS_QUERY, "", ""),
        "javascript" => (tree_sitter_javascript::LANGUAGE.into(), tree_sitter_javascript::HIGHLIGHT_QUERY, tree_sitter_javascript::INJECTIONS_QUERY, tree_sitter_javascript::LOCALS_QUERY),
        "typescript" => (tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into(), tree_sitter_typescript::HIGHLIGHTS_QUERY, "", tree_sitter_typescript::LOCALS_QUERY),
        "tsx" => (tree_sitter_typescript::LANGUAGE_TSX.into(), tree_sitter_typescript::HIGHLIGHTS_QUERY, "", tree_sitter_typescript::LOCALS_QUERY),
        "python" => (tree_sitter_python::LANGUAGE.into(), tree_sitter_python::HIGHLIGHTS_QUERY, "", ""),
        "bash" => (tree_sitter_bash::LANGUAGE.into(), tree_sitter_bash::HIGHLIGHT_QUERY, "", ""),
        "json" => (tree_sitter_json::LANGUAGE.into(), tree_sitter_json::HIGHLIGHTS_QUERY, "", ""),
        "toml" => (tree_sitter_toml_ng::LANGUAGE.into(), tree_sitter_toml_ng::HIGHLIGHTS_QUERY, "", ""),
        "yaml" => (tree_sitter_yaml::LANGUAGE.into(), tree_sitter_yaml::HIGHLIGHTS_QUERY, "", ""),
        "html" => (tree_sitter_html::LANGUAGE.into(), tree_sitter_html::HIGHLIGHTS_QUERY, tree_sitter_html::INJECTIONS_QUERY, ""),
        "css" => (tree_sitter_css::LANGUAGE.into(), tree_sitter_css::HIGHLIGHTS_QUERY, "", ""),
        "markdown" => (tree_sitter_md::LANGUAGE.into(), tree_sitter_md::HIGHLIGHT_QUERY_BLOCK, tree_sitter_md::INJECTION_QUERY_BLOCK, ""),
        "lua" => (tree_sitter_lua::LANGUAGE.into(), tree_sitter_lua::HIGHLIGHTS_QUERY, "", ""),
        _ => return None,
    };
    let mut conf = HighlightConfiguration::new(language, lang, highlights, injections, locals).ok()?;
    conf.configure(CLASSES);
    Some(Grammar { conf })
}

fn detect(path: &str, head: &str) -> String {
    let ext = path.rsplit('.').next().unwrap_or("").to_ascii_lowercase();
    let by_ext = match ext.as_str() {
        "rs" => "rust",
        "c" | "h" => "c",
        "cpp" | "cc" | "cxx" | "hpp" | "hh" => "cpp",
        "go" => "go",
        "java" | "kt" => "java",
        "js" | "mjs" | "cjs" | "jsx" => "javascript",
        "ts" | "mts" => "typescript",
        "tsx" => "tsx",
        "py" => "python",
        "sh" | "bash" | "zsh" => "bash",
        "json" => "json",
        "toml" => "toml",
        "yml" | "yaml" => "yaml",
        "html" | "htm" | "qml" => "html",
        "css" | "scss" => "css",
        "md" | "markdown" => "markdown",
        "lua" => "lua",
        _ => "",
    };
    if !by_ext.is_empty() {
        return by_ext.into();
    }
    let first = head.lines().next().unwrap_or("");
    if first.starts_with("#!") {
        if first.contains("python") {
            return "python".into();
        }
        if first.contains("bash") || first.contains("/sh") || first.contains("zsh") {
            return "bash".into();
        }
    }
    "plain".into()
}

struct Cached {
    mtime: u64,
    lang: String,
    lines: Vec<Vec<(String, &'static str)>>,
}

fn highlight(text: &str, lang: &str, grammars: &mut HashMap<String, Option<Grammar>>) -> Vec<Vec<(String, &'static str)>> {
    let g = grammars.entry(lang.to_string()).or_insert_with(|| grammar(lang));
    let mut lines: Vec<Vec<(String, &'static str)>> = Vec::new();
    let Some(g) = g else {
        return text.lines().map(|l| vec![(l.to_string(), "plain")]).collect();
    };
    let mut hl = Highlighter::new();
    let events = match hl.highlight(&g.conf, text.as_bytes(), None, |_| None) {
        Ok(e) => e,
        Err(_) => return text.lines().map(|l| vec![(l.to_string(), "plain")]).collect(),
    };
    let mut current: &'static str = "plain";
    let mut stack: Vec<&'static str> = Vec::new();
    let mut line: Vec<(String, &'static str)> = Vec::new();
    for ev in events.flatten() {
        match ev {
            HighlightEvent::HighlightStart(h) => {
                stack.push(current);
                current = CLASSES.get(h.0).copied().unwrap_or("plain");
            }
            HighlightEvent::HighlightEnd => current = stack.pop().unwrap_or("plain"),
            HighlightEvent::Source { start, end } => {
                let chunk = &text[start..end];
                let mut parts = chunk.split('\n').peekable();
                while let Some(p) = parts.next() {
                    if !p.is_empty() {
                        line.push((p.to_string(), current));
                    }
                    if parts.peek().is_some() {
                        lines.push(std::mem::take(&mut line));
                    }
                }
            }
        }
    }
    if !line.is_empty() || lines.is_empty() {
        lines.push(line);
    }
    lines
}

fn main() {
    let mut grammars: HashMap<String, Option<Grammar>> = HashMap::new();
    let mut cache: HashMap<String, Cached> = HashMap::new();
    let mut stdin = io::stdin().lock();
    let mut stdout = io::stdout().lock();
    while let Ok(Some((kind, payload))) = read_frame(&mut stdin) {
        if kind != 0 {
            continue;
        }
        let Ok(v) = json::parse(&payload) else { continue };
        let id = v.u64_field("id").unwrap_or(0);
        let reply = match v.str_field("type").unwrap_or("") {
            "Ping" => Value::obj().u("id", id).v("ok", Value::obj().done()).done(),
            "Describe" => Value::obj().u("id", id).v("ok", kiki_plugin_sdk::service_describe("highlight", "Syntax highlighting", env!("CARGO_PKG_VERSION"), &["Highlight"])).done(),
            "Shutdown" => {
                let _ = write_json(&mut stdout, &Value::obj().u("id", id).v("ok", Value::obj().done()).done());
                return;
            }
            "Highlight" => {
                let path = v.str_field("path").unwrap_or("").to_string();
                let first = v.u64_field("first").unwrap_or(0) as usize;
                let count = v.u64_field("count").unwrap_or(200).min(2000) as usize;
                let md = std::fs::metadata(&path);
                let mtime = md.as_ref().ok().and_then(|m| m.modified().ok()).and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok()).map(|d| d.as_millis() as u64).unwrap_or(0);
                let fresh = cache.get(&path).map(|c| c.mtime == mtime).unwrap_or(false);
                if !fresh {
                    match std::fs::File::open(&path) {
                        Ok(mut f) => {
                            let size = md.as_ref().map(|m| m.len()).unwrap_or(0);
                            let mut text = String::new();
                            let mut raw = Vec::new();
                            let _ = f.read_to_end(&mut raw);
                            if raw.iter().take(8192).any(|&b| b == 0) {
                                let _ = write_json(&mut stdout, &Value::obj().u("id", id).v("err", Value::obj().s("code", "Unsupported").s("message", "binary file").done()).done());
                                continue;
                            }
                            text.push_str(&String::from_utf8_lossy(&raw));
                            let lang = if size > PLAIN_OVER { "plain".to_string() } else { detect(&path, &text[..text.len().min(512)]) };
                            let lines = if lang == "plain" { text.lines().map(|l| vec![(l.to_string(), "plain")]).collect() } else { highlight(&text, &lang, &mut grammars) };
                            cache.insert(path.clone(), Cached { mtime, lang, lines });
                            if cache.len() > 32 {
                                let oldest = cache.keys().next().cloned();
                                if let Some(k) = oldest {
                                    if k != path {
                                        cache.remove(&k);
                                    }
                                }
                            }
                        }
                        Err(e) => {
                            let _ = write_json(&mut stdout, &Value::obj().u("id", id).v("err", Value::obj().s("code", "NotFound").s("message", e.to_string()).done()).done());
                            continue;
                        }
                    }
                }
                let c = &cache[&path];
                let rows: Vec<Value> = c
                    .lines
                    .iter()
                    .enumerate()
                    .skip(first)
                    .take(count)
                    .map(|(n, spans)| Value::obj().u("n", n as u64 + 1).v("spans", Value::Arr(spans.iter().map(|(t, cl)| Value::obj().s("text", t.clone()).s("class", *cl).done()).collect())).done())
                    .collect();
                Value::obj().u("id", id).v("ok", Value::obj().v("lines", Value::Arr(rows)).u("total", c.lines.len() as u64).s("lang", c.lang.clone()).done()).done()
            }
            _ => Value::obj().u("id", id).v("err", Value::obj().s("code", "Unsupported").s("message", "unknown request").done()).done(),
        };
        let _ = write_json(&mut stdout, &reply);
    }
}
