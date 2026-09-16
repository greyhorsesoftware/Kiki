//! The TOML subset kiki's config files use: `key = value` lines, `[section]`,
//! `[[array.of.tables]]`, strings, integers, floats, booleans, and arrays of
//! scalars. Parsed into the same `Value` tree as JSON so the daemon can hand
//! config to the shell unchanged. Comments and blank lines are skipped;
//! unknown constructs are errors with a line number.

use crate::json::Value;
use std::collections::BTreeMap;

#[derive(Debug)]
pub struct TomlError {
    pub line: usize,
    pub message: &'static str,
}

impl std::fmt::Display for TomlError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "toml line {}: {}", self.line, self.message)
    }
}

pub fn parse(text: &str) -> Result<Value, TomlError> {
    let mut root: BTreeMap<String, Value> = BTreeMap::new();
    // Where `key = value` lines currently land: a path of keys, and whether the
    // last segment is an array-of-tables element (append to its last object).
    let mut path: Vec<String> = Vec::new();
    let mut in_array_table = false;
    for (n, raw) in text.lines().enumerate() {
        let line = n + 1;
        let l = strip_comment(raw).trim();
        if l.is_empty() {
            continue;
        }
        if let Some(inner) = l.strip_prefix("[[").and_then(|s| s.strip_suffix("]]")) {
            path = inner.split('.').map(|s| s.trim().to_string()).collect();
            in_array_table = true;
            let arr = ensure_path(&mut root, &path, true, line)?;
            if let Value::Arr(a) = arr {
                a.push(Value::Obj(BTreeMap::new()));
            }
            continue;
        }
        if let Some(inner) = l.strip_prefix('[').and_then(|s| s.strip_suffix(']')) {
            path = inner.split('.').map(|s| s.trim().to_string()).collect();
            in_array_table = false;
            ensure_path(&mut root, &path, false, line)?;
            continue;
        }
        let (k, v) = l.split_once('=').ok_or(TomlError { line, message: "expected key = value" })?;
        let key = k.trim().trim_matches('"').to_string();
        if key.is_empty() {
            return Err(TomlError { line, message: "empty key" });
        }
        let val = parse_value(v.trim(), line)?;
        let target = if path.is_empty() {
            &mut root
        } else {
            match ensure_path(&mut root, &path, in_array_table, line)? {
                Value::Obj(m) => m,
                Value::Arr(a) => match a.last_mut() {
                    Some(Value::Obj(m)) => m,
                    _ => return Err(TomlError { line, message: "bad array table" }),
                },
                _ => return Err(TomlError { line, message: "not a table" }),
            }
        };
        target.insert(key, val);
    }
    Ok(Value::Obj(root))
}

fn strip_comment(s: &str) -> &str {
    let mut in_str = false;
    for (i, c) in s.char_indices() {
        match c {
            '"' => in_str = !in_str,
            '#' if !in_str => return &s[..i],
            _ => {}
        }
    }
    s
}

/// Walks `path` creating tables; the final segment is an array of tables when `array`.
fn ensure_path<'a>(root: &'a mut BTreeMap<String, Value>, path: &[String], array: bool, line: usize) -> Result<&'a mut Value, TomlError> {
    let mut cur: &mut BTreeMap<String, Value> = root;
    for (i, seg) in path.iter().enumerate() {
        let last = i == path.len() - 1;
        if last {
            let entry = cur.entry(seg.clone()).or_insert_with(|| if array { Value::Arr(Vec::new()) } else { Value::Obj(BTreeMap::new()) });
            return match (entry, array) {
                (v @ Value::Arr(_), true) => Ok(v),
                (v @ Value::Obj(_), false) => Ok(v),
                _ => Err(TomlError { line, message: "table redefined with a different shape" }),
            };
        }
        let next = cur.entry(seg.clone()).or_insert_with(|| Value::Obj(BTreeMap::new()));
        cur = match next {
            Value::Obj(m) => m,
            Value::Arr(a) => match a.last_mut() {
                Some(Value::Obj(m)) => m,
                _ => return Err(TomlError { line, message: "bad nesting" }),
            },
            _ => return Err(TomlError { line, message: "not a table" }),
        };
    }
    Err(TomlError { line, message: "empty path" })
}

fn parse_value(s: &str, line: usize) -> Result<Value, TomlError> {
    if let Some(inner) = s.strip_prefix('"').and_then(|x| x.strip_suffix('"')) {
        return Ok(Value::Str(unescape(inner)));
    }
    if let Some(inner) = s.strip_prefix('\'').and_then(|x| x.strip_suffix('\'')) {
        return Ok(Value::Str(inner.to_string()));
    }
    if let Some(inner) = s.strip_prefix('[').and_then(|x| x.strip_suffix(']')) {
        let mut out = Vec::new();
        for item in split_top_level(inner) {
            let item = item.trim();
            if !item.is_empty() {
                out.push(parse_value(item, line)?);
            }
        }
        return Ok(Value::Arr(out));
    }
    match s {
        "true" => return Ok(Value::Bool(true)),
        "false" => return Ok(Value::Bool(false)),
        _ => {}
    }
    let num = s.replace('_', "");
    if let Ok(u) = num.parse::<u64>() {
        return Ok(Value::Uint(u));
    }
    if let Ok(i) = num.parse::<i64>() {
        return Ok(Value::Int(i));
    }
    if let Ok(f) = num.parse::<f64>() {
        return Ok(Value::Float(f));
    }
    Err(TomlError { line, message: "unsupported value" })
}

fn split_top_level(s: &str) -> Vec<&str> {
    let mut parts = Vec::new();
    let mut depth = 0;
    let mut in_str = false;
    let mut start = 0;
    for (i, c) in s.char_indices() {
        match c {
            '"' => in_str = !in_str,
            '[' if !in_str => depth += 1,
            ']' if !in_str => depth -= 1,
            ',' if !in_str && depth == 0 => {
                parts.push(&s[start..i]);
                start = i + 1;
            }
            _ => {}
        }
    }
    parts.push(&s[start..]);
    parts
}

fn unescape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut it = s.chars();
    while let Some(c) = it.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match it.next() {
            Some('n') => out.push('\n'),
            Some('t') => out.push('\t'),
            Some('"') => out.push('"'),
            Some('\\') => out.push('\\'),
            Some(o) => {
                out.push('\\');
                out.push(o);
            }
            None => out.push('\\'),
        }
    }
    out
}

/// Writes a tree back out. Top-level scalars first, then `[table]`s, then `[[array]]`s.
pub fn write(v: &Value) -> String {
    let mut out = String::new();
    if let Value::Obj(m) = v {
        for (k, x) in m {
            if !matches!(x, Value::Obj(_) | Value::Arr(_)) || is_scalar_array(x) {
                out.push_str(&format!("{} = {}\n", k, scalar(x)));
            }
        }
        for (k, x) in m {
            if let Value::Obj(t) = x {
                out.push_str(&format!("\n[{k}]\n"));
                for (tk, tv) in t {
                    out.push_str(&format!("{} = {}\n", tk, scalar(tv)));
                }
            }
        }
        for (k, x) in m {
            if let Value::Arr(a) = x {
                if is_scalar_array(x) {
                    continue;
                }
                for item in a {
                    if let Value::Obj(t) = item {
                        out.push_str(&format!("\n[[{k}]]\n"));
                        for (tk, tv) in t {
                            out.push_str(&format!("{} = {}\n", tk, scalar(tv)));
                        }
                    }
                }
            }
        }
    }
    out
}

fn is_scalar_array(v: &Value) -> bool {
    matches!(v, Value::Arr(a) if a.iter().all(|x| !matches!(x, Value::Obj(_) | Value::Arr(_))))
}

fn scalar(v: &Value) -> String {
    match v {
        Value::Str(s) => {
            let mut o = String::from("\"");
            for c in s.chars() {
                match c {
                    '"' => o.push_str("\\\""),
                    '\\' => o.push_str("\\\\"),
                    '\n' => o.push_str("\\n"),
                    '\t' => o.push_str("\\t"),
                    c => o.push(c),
                }
            }
            o.push('"');
            o
        }
        Value::Arr(a) => format!("[{}]", a.iter().map(scalar).collect::<Vec<_>>().join(", ")),
        other => crate::json::to_string(other),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_kiki_shapes() {
        let src = r#"
# favorites
version = 1
[index]
roots = ["file:///home/david", "file:///mnt/data"]
enabled = true

[[favorite]]
name = "Home"
uri = "file:///home/david"

[[favorite]]
name = "Projects" # trailing comment
uri = "file:///home/david/Projects"
"#;
        let v = parse(src).unwrap();
        assert_eq!(v.u64_field("version"), Some(1));
        assert_eq!(v.get("index").unwrap().get("roots").unwrap().as_arr().unwrap().len(), 2);
        let favs = v.get("favorite").unwrap().as_arr().unwrap();
        assert_eq!(favs.len(), 2);
        assert_eq!(favs[1].str_field("name"), Some("Projects"));
        let out = write(&v);
        let v2 = parse(&out).unwrap();
        assert_eq!(v, v2);
    }

    #[test]
    fn errors_carry_line() {
        let e = parse("a = 1\nb 2\n").unwrap_err();
        assert_eq!(e.line, 2);
    }
}
