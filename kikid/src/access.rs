//! kiki's own access log (plan 22): when a file is opened through kiki (Launch, Open in…, the
//! code viewer, a share) the time is appended to `~/.local/share/kiki/access.log`, one
//! `unix_ms<TAB>uri` line per open. With the "kiki opens" heat source the Accessed column reads
//! from here instead of the filesystem's atime, so it works regardless of mount options.

use std::collections::HashMap;
use std::io::Write;
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};

/// Above this many lines the log is rewritten with the newest entry per URI.
pub const COMPACT_ABOVE: usize = 50_000;

struct Log {
    latest: HashMap<String, u64>,
    lines: usize,
    loaded: bool,
}

fn log() -> &'static Mutex<Log> {
    static L: OnceLock<Mutex<Log>> = OnceLock::new();
    L.get_or_init(|| Mutex::new(Log { latest: HashMap::new(), lines: 0, loaded: false }))
}

pub fn path() -> PathBuf {
    let base = std::env::var("KIKI_DATA_DIR").map(PathBuf::from).unwrap_or_else(|_| std::env::var("XDG_DATA_HOME").map(PathBuf::from).unwrap_or_else(|_| crate::config::home().join(".local/share")).join("kiki"));
    base.join("access.log")
}

/// Records and lookups use the percent-decoded URI without a trailing slash, so the client's
/// `encodeURIComponent` spelling and the daemon's own joins agree.
pub fn key(uri: &str) -> String {
    match crate::vfs::uri::Uri::parse(uri) {
        Ok(u) => format!("{}://{}{}", u.scheme, u.authority, u.path).trim_end_matches('/').to_string(),
        Err(_) => uri.trim_end_matches('/').to_string(),
    }
}

fn ensure_loaded(l: &mut Log) {
    if l.loaded {
        return;
    }
    l.loaded = true;
    if let Ok(text) = std::fs::read_to_string(path()) {
        for line in text.lines() {
            l.lines += 1;
            if let Some((ts, uri)) = line.split_once('\t') {
                if let Ok(ms) = ts.parse::<u64>() {
                    let e = l.latest.entry(uri.to_string()).or_insert(0);
                    if ms > *e {
                        *e = ms;
                    }
                }
            }
        }
    }
}

/// One open of `uri` now.
pub fn record(uri: &str) {
    let k = key(uri);
    let now = crate::ops::unix_now() * 1000;
    let mut l = log().lock().unwrap();
    ensure_loaded(&mut l);
    l.latest.insert(k.clone(), now);
    l.lines += 1;
    let p = path();
    if let Some(dir) = p.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(&p) {
        let _ = writeln!(f, "{now}\t{k}");
    }
    if l.lines > COMPACT_ABOVE {
        compact(&mut l);
    }
}

fn compact(l: &mut Log) {
    let p = path();
    let tmp = p.with_extension("log.tmp");
    let mut out = String::new();
    for (uri, ms) in &l.latest {
        out.push_str(&format!("{ms}\t{uri}\n"));
    }
    if std::fs::write(&tmp, out).is_ok() && std::fs::rename(&tmp, &p).is_ok() {
        l.lines = l.latest.len();
    }
}

/// When kiki last opened `uri`, in ms since the epoch.
pub fn opened(uri: &str) -> Option<u64> {
    let mut l = log().lock().unwrap();
    ensure_loaded(&mut l);
    l.latest.get(&key(uri)).copied()
}

/// For a listing row: the parent URI joined with the entry name.
pub fn opened_child(parent_uri: &str, name: &str) -> Option<u64> {
    let mut l = log().lock().unwrap();
    ensure_loaded(&mut l);
    if l.latest.is_empty() {
        return None;
    }
    let k = format!("{}/{}", key(parent_uri), name);
    l.latest.get(&k).copied()
}

pub fn clear() -> std::io::Result<()> {
    let mut l = log().lock().unwrap();
    l.latest.clear();
    l.lines = 0;
    l.loaded = true;
    match std::fs::remove_file(path()) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(e),
    }
}

pub fn count() -> usize {
    let mut l = log().lock().unwrap();
    ensure_loaded(&mut l);
    l.latest.len()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn records_latest_open_per_uri_and_clears() {
        let _g = crate::ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let d = std::env::temp_dir().join(format!("kiki-access-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::env::set_var("KIKI_DATA_DIR", &d);
        clear().unwrap();
        assert_eq!(opened("file:///tmp/a%20b.txt"), None);
        record("file:///tmp/a%20b.txt");
        let t = opened("file:///tmp/a b.txt").expect("recorded, decoded key");
        assert!(t > 0);
        assert_eq!(opened_child("file:///tmp/", "a b.txt"), Some(t));
        assert_eq!(count(), 1);
        // the file holds the line and survives a fresh load
        let text = std::fs::read_to_string(path()).unwrap();
        assert!(text.contains("\tfile:///tmp/a b.txt"));
        clear().unwrap();
        assert_eq!(opened("file:///tmp/a b.txt"), None);
        assert!(!path().exists());
        std::env::remove_var("KIKI_DATA_DIR");
        let _ = std::fs::remove_dir_all(&d);
    }
}
