//! Which names a mirror never touches: the rules from `filters.toml`.

use super::*;
use std::collections::BTreeMap;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Rule {
    Contains(String),
    StartsWith(String),
    EndsWith(String),
    Matches(String),
}

/// The rules a mirror uses when nobody has said otherwise: nine names, one rule each.
///
/// Named rather than the one pattern `startsWith "."` it used to be, because a leading dot does
/// not mean "not worth uploading": `.htaccess`, `.htpasswd`, `.user.ini`, `.nojekyll` and
/// everything under `.well-known/` — Let's Encrypt's challenge among it — are exactly what a
/// website mirror exists to carry, and one pattern kept every one of them off the server with
/// nothing to show for it but "N filtered out". So the names instead: the version control
/// directory and its ignore file (owner, 2026-09-21: a site has no use for `.gitignore`), the two
/// desktop droppings, the secrets file nobody means to publish, the two editor directories, and
/// the two build directories that are rebuilt rather than carried across.
/// Whoever really does want every hidden name skipped adds `starts with .` in the dialog.
pub fn default_rules() -> Vec<Rule> {
    [".git", ".gitignore", ".DS_Store", ".env", ".idea", ".vscode", "Thumbs.db", "node_modules", "__pycache__"].iter().map(|n| Rule::Matches((*n).into())).collect()
}

/// The rules, and whether they are the built-in set rather than the user's own.
///
/// No `[[rule]]` at all means the defaults. `defaults = false` beside no rules means the user
/// deleted every one of them and meant it: that is a different thing from never having edited
/// them, and the defaults must not creep back in the next time the file is read.
pub fn filters() -> (Vec<Rule>, bool) {
    let v = crate::config::read_named("filters.toml");
    let raw: Vec<Value> = v.get("rule").and_then(Value::as_arr).map(|a| a.to_vec()).unwrap_or_default();
    if raw.is_empty() && v.get("defaults").and_then(Value::as_bool) != Some(false) {
        return (default_rules(), true);
    }
    // A rule missing its kind or its value is dropped rather than taken as something else.
    (raw.iter().filter_map(rule_from).collect(), false)
}

pub fn load_filters() -> Vec<Rule> {
    filters().0
}

fn rule_from(v: &Value) -> Option<Rule> {
    let val = v.str_field("value")?.to_string();
    Some(match v.str_field("kind")? {
        "contains" => Rule::Contains(val),
        "startsWith" => Rule::StartsWith(val),
        "endsWith" => Rule::EndsWith(val),
        _ => Rule::Matches(val),
    })
}

pub fn rule_json(r: &Rule) -> Value {
    let (kind, value) = match r {
        Rule::Contains(v) => ("contains", v),
        Rule::StartsWith(v) => ("startsWith", v),
        Rule::EndsWith(v) => ("endsWith", v),
        Rule::Matches(v) => ("matches", v),
    };
    Value::obj().s("kind", kind).s("value", value.clone()).done()
}

/// What the shell reads and shows: the rules, whether they are still the built-in set, and the
/// built-in set itself — which goes out whether or not it is in use, because the dialog's
/// "Restore defaults" has to show what restoring would give and there is only one list of it.
/// A copy in the shell is a copy that drifts the first time this one is edited.
pub fn filters_json() -> Value {
    let (rules, defaults) = filters();
    Value::obj().v("rules", Value::Arr(rules.iter().map(rule_json).collect())).b("defaults", defaults).v("defaultRules", Value::Arr(default_rules().iter().map(rule_json).collect())).done()
}

/// Every rule checked before any of it is written, so a file is never half a save: the kind must
/// be one kiki knows, the value must say something, and it must not carry a `/` — a rule matches
/// one NAME anywhere in the tree, not a path. A refusal names the rule and the field.
pub fn check_rules(rules: &[Value]) -> Result<Vec<Rule>, String> {
    let mut out = Vec::with_capacity(rules.len());
    for (i, v) in rules.iter().enumerate() {
        let n = i + 1;
        let kind = v.str_field("kind").unwrap_or("");
        if !matches!(kind, "contains" | "startsWith" | "endsWith" | "matches") {
            return Err(format!("rule {n}: kind must be contains, startsWith, endsWith or matches"));
        }
        let value = v.str_field("value").unwrap_or("");
        if value.is_empty() {
            return Err(format!("rule {n}: value must not be empty"));
        }
        if value.contains('/') {
            return Err(format!("rule {n}: value must not contain a slash — a rule matches a name, not a path"));
        }
        out.push(rule_from(v).expect("kind and value both checked"));
    }
    Ok(out)
}

/// The rules, written the way every other config file is written — through a temp file and a
/// rename, and never over a file that could not be read. An empty list is stored as
/// `defaults = false`, because an empty file means the defaults.
pub fn set_filters(rules: &[Rule]) -> std::io::Result<()> {
    let mut m = BTreeMap::new();
    if rules.is_empty() {
        m.insert("defaults".to_string(), Value::Bool(false));
    } else {
        m.insert("rule".to_string(), Value::Arr(rules.iter().map(rule_json).collect()));
    }
    crate::config::write_named("filters.toml", &Value::Obj(m))
}

/// "Restore defaults": the file goes, and the built-in rules are what is read next. A file that
/// was not there to begin with is not an error.
pub fn restore_default_filters() -> std::io::Result<()> {
    match std::fs::remove_file(crate::config::config_dir().join("filters.toml")) {
        Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(e),
        _ => Ok(()),
    }
}

/// Whether a relative path is skipped, and whether this entry is the one the "K filtered out"
/// count belongs to — the topmost filtered name, counted once with its whole subtree behind it.
/// A per-directory walk never descends into a skipped folder, so a recursive scan must not count
/// what the other would never have seen: the same tree has to give the same number either way.
pub fn filtered_rel(rel: &str, rules: &[Rule]) -> Option<bool> {
    let mut last = 0;
    let mut at = None;
    for (i, seg) in rel.split('/').enumerate() {
        last = i;
        if at.is_none() && filtered(seg, rules) {
            at = Some(i);
        }
    }
    at.map(|i| i == last)
}

pub fn filtered(name: &str, rules: &[Rule]) -> bool {
    rules.iter().any(|r| match r {
        Rule::Contains(s) => name.contains(s.as_str()),
        Rule::StartsWith(s) => name.starts_with(s.as_str()),
        Rule::EndsWith(s) => name.ends_with(s.as_str()),
        Rule::Matches(s) => name == s,
    })
}
