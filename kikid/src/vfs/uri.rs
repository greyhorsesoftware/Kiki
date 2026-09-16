//! `Uri`: the one way kiki names things. `file:///home/david` or `sftp://homelab/srv`.

use std::fmt;
use std::path::{Path, PathBuf};

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Uri {
    pub scheme: String,
    /// Empty for `file`.
    pub authority: String,
    /// Absolute, percent-decoded, `/`-separated, no trailing slash except root.
    pub path: String,
}

#[derive(Debug)]
pub struct UriError(pub &'static str);

impl Uri {
    /// Accepts a full URI, a bare absolute path, or `~`-prefixed path.
    pub fn parse(s: &str) -> Result<Uri, UriError> {
        if let Some(rest) = s.strip_prefix('~') {
            let home = std::env::var("HOME").map_err(|_| UriError("no HOME"))?;
            return Uri::local(&format!("{home}{rest}"));
        }
        if s.starts_with('/') {
            return Uri::local(s);
        }
        let (scheme, rest) = s.split_once("://").ok_or(UriError("not a uri"))?;
        if scheme.is_empty() || !scheme.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'+' || b == b'-' || b == b'.') {
            return Err(UriError("bad scheme"));
        }
        let scheme = scheme.to_ascii_lowercase();
        let (authority, path) = match rest.find('/') {
            Some(i) => (&rest[..i], &rest[i..]),
            None => (rest, "/"),
        };
        if scheme == "file" && !authority.is_empty() && authority != "localhost" {
            return Err(UriError("file uri with host"));
        }
        Ok(Uri { scheme: scheme.clone(), authority: if scheme == "file" { String::new() } else { authority.to_string() }, path: normalize(&percent_decode(path)) })
    }

    pub fn local(path: &str) -> Result<Uri, UriError> {
        if !path.starts_with('/') {
            return Err(UriError("path not absolute"));
        }
        Ok(Uri { scheme: "file".into(), authority: String::new(), path: normalize(path) })
    }

    pub fn from_path(p: &Path) -> Uri {
        Uri { scheme: "file".into(), authority: String::new(), path: normalize(&p.to_string_lossy()) }
    }

    pub fn is_local(&self) -> bool {
        self.scheme == "file"
    }

    pub fn to_path(&self) -> PathBuf {
        PathBuf::from(&self.path)
    }

    pub fn name(&self) -> &str {
        self.path.rsplit('/').next().unwrap_or("")
    }

    pub fn parent(&self) -> Option<Uri> {
        if self.path == "/" {
            return None;
        }
        let i = self.path.rfind('/')?;
        let p = if i == 0 { "/".to_string() } else { self.path[..i].to_string() };
        Some(Uri { scheme: self.scheme.clone(), authority: self.authority.clone(), path: p })
    }

    pub fn join(&self, name: &str) -> Uri {
        let mut p = self.path.clone();
        if !p.ends_with('/') {
            p.push('/');
        }
        p.push_str(name);
        Uri { scheme: self.scheme.clone(), authority: self.authority.clone(), path: p }
    }

    /// Human form: `~/Projects/kiki` locally, `homelab/srv/kiki` on a location.
    pub fn display(&self) -> String {
        if self.is_local() {
            if let Ok(home) = std::env::var("HOME") {
                if self.path == home {
                    return "~".into();
                }
                if let Some(rest) = self.path.strip_prefix(&format!("{home}/")) {
                    return format!("~/{rest}");
                }
            }
            self.path.clone()
        } else {
            format!("{}{}", self.authority, self.path)
        }
    }
}

impl fmt::Display for Uri {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}://{}{}", self.scheme, self.authority, percent_encode(&self.path))
    }
}

fn normalize(p: &str) -> String {
    let mut parts: Vec<&str> = Vec::new();
    for seg in p.split('/') {
        match seg {
            "" | "." => {}
            ".." => {
                parts.pop();
            }
            s => parts.push(s),
        }
    }
    if parts.is_empty() {
        "/".to_string()
    } else {
        format!("/{}", parts.join("/"))
    }
}

fn percent_decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' && i + 2 < b.len() {
            let h = &s[i + 1..i + 3];
            if let Ok(v) = u8::from_str_radix(h, 16) {
                out.push(v);
                i += 3;
                continue;
            }
        }
        out.push(b[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn percent_encode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for &b in s.as_bytes() {
        let keep = b.is_ascii_alphanumeric() || matches!(b, b'/' | b'-' | b'_' | b'.' | b'~' | b'@' | b'+' | b':' | b',' | b'=');
        if keep {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_forms() {
        let a = Uri::parse("file:///tmp/x").unwrap();
        let b = Uri::parse("/tmp/x").unwrap();
        assert_eq!(a, b);
        assert!(a.is_local());
        let r = Uri::parse("sftp://homelab/srv/kiki/").unwrap();
        assert_eq!(r.authority, "homelab");
        assert_eq!(r.path, "/srv/kiki");
        assert_eq!(r.display(), "homelab/srv/kiki");
        assert!(Uri::parse("nope").is_err());
        assert!(Uri::parse("ht tp://x/").is_err());
    }

    #[test]
    fn parent_join_display() {
        let u = Uri::parse("/a/b/c").unwrap();
        assert_eq!(u.parent().unwrap().path, "/a/b");
        assert_eq!(u.join("d").path, "/a/b/c/d");
        assert_eq!(Uri::parse("/").unwrap().parent(), None);
        assert_eq!(Uri::parse("file:///a/../b/./c%20d").unwrap().path, "/b/c d");
        assert_eq!(Uri::parse("/a/../b/./c%20d").unwrap().path, "/b/c%20d"); // bare paths are taken literally
        assert_eq!(Uri::parse("/a/b c").unwrap().to_string(), "file:///a/b%20c");
    }
}
