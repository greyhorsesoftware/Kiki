//! Which names a mirror never touches: the rules from `mirror-filters.toml`.

use super::*;

#[derive(Clone, Debug)]
pub enum Rule {
    Contains(String),
    StartsWith(String),
    EndsWith(String),
    Matches(String),
}

pub fn load_filters() -> Vec<Rule> {
    let v = crate::config::read_named("filters.toml");
    let rules: Vec<Value> = v.get("rule").and_then(Value::as_arr).map(|a| a.to_vec()).unwrap_or_default();
    if rules.is_empty() {
        return vec![Rule::Matches(".git".into()), Rule::Matches(".DS_Store".into()), Rule::Matches("node_modules".into()), Rule::Matches("__pycache__".into())];
    }
    rules
        .iter()
        .filter_map(|r| {
            let val = r.str_field("value")?.to_string();
            Some(match r.str_field("kind")? {
                "contains" => Rule::Contains(val),
                "startsWith" => Rule::StartsWith(val),
                "endsWith" => Rule::EndsWith(val),
                _ => Rule::Matches(val),
            })
        })
        .collect()
}

pub fn filtered(name: &str, rules: &[Rule]) -> bool {
    rules.iter().any(|r| match r {
        Rule::Contains(s) => name.contains(s.as_str()),
        Rule::StartsWith(s) => name.starts_with(s.as_str()),
        Rule::EndsWith(s) => name.ends_with(s.as_str()),
        Rule::Matches(s) => name == s,
    })
}
