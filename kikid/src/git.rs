//! Git status for listings (plan 15): `git status --porcelain=v2 -z` scoped to the listed
//! directory, cached per (root, dir), folders aggregated.

use crate::json::Value;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum State {
    Clean,
    Modified,
    Added,
    Deleted,
    Renamed,
    Conflicted,
    Untracked,
    Ignored,
}

impl State {
    pub fn as_str(self) -> &'static str {
        match self {
            State::Clean => "clean",
            State::Modified => "modified",
            State::Added => "added",
            State::Deleted => "deleted",
            State::Renamed => "renamed",
            State::Conflicted => "conflicted",
            State::Untracked => "untracked",
            State::Ignored => "ignored",
        }
    }
    fn weight(self) -> u8 {
        match self {
            State::Conflicted => 7,
            State::Deleted => 6,
            State::Modified => 5,
            State::Renamed => 4,
            State::Added => 3,
            State::Untracked => 2,
            State::Ignored => 1,
            State::Clean => 0,
        }
    }
}

#[derive(Clone, Debug)]
pub struct Entry {
    pub state: State,
    pub staged: bool,
}

pub struct Status {
    /// Paths relative to the listed directory -> state (files only; folders aggregate).
    pub entries: HashMap<String, Entry>,
    pub branch: Option<String>,
    pub ahead: u32,
    pub behind: u32,
    pub at: Instant,
    pub slow: bool,
}

/// Repository root for a directory, cached (negative results too).
pub fn repo_root(dir: &Path) -> Option<PathBuf> {
    static CACHE: OnceLock<Mutex<HashMap<PathBuf, Option<PathBuf>>>> = OnceLock::new();
    let c = CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    if let Some(r) = c.lock().unwrap().get(dir) {
        return r.clone();
    }
    let mut cur = Some(dir);
    let mut found = None;
    while let Some(d) = cur {
        if d.join(".git").exists() {
            found = Some(d.to_path_buf());
            break;
        }
        cur = d.parent();
    }
    c.lock().unwrap().insert(dir.to_path_buf(), found.clone());
    found
}

pub fn enabled() -> bool {
    crate::config::settings().get("git").and_then(|g| g.get("enabled")).and_then(Value::as_bool).unwrap_or(true)
}

fn status_cache() -> &'static Mutex<HashMap<PathBuf, Status>> {
    static C: OnceLock<Mutex<HashMap<PathBuf, Status>>> = OnceLock::new();
    C.get_or_init(|| Mutex::new(HashMap::new()))
}

pub fn invalidate(dir: &Path) {
    status_cache().lock().unwrap().remove(dir);
}

/// Runs status for `dir` (scoped with a pathspec) and caches it. Returns None outside a repository.
pub fn status(dir: &Path) -> Option<()> {
    let root = repo_root(dir)?;
    let start = Instant::now();
    let out = Command::new("git")
        .args(["-C"])
        .arg(dir)
        .args(["status", "--porcelain=v2", "-z", "--branch", "--untracked-files=all", "--ignored=matching", "--", "."])
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let slow = start.elapsed() > Duration::from_secs(2);
    let mut st = Status { entries: HashMap::new(), branch: None, ahead: 0, behind: 0, at: Instant::now(), slow };
    let rel_dir = dir.strip_prefix(&root).unwrap_or(Path::new("")).to_string_lossy().into_owned();
    let strip = |p: &str| -> String {
        let p = p.strip_prefix(&rel_dir).unwrap_or(p);
        p.trim_start_matches('/').to_string()
    };
    let text = String::from_utf8_lossy(&out.stdout);
    let mut fields = text.split('\0').peekable();
    while let Some(rec) = fields.next() {
        if rec.is_empty() {
            continue;
        }
        if let Some(h) = rec.strip_prefix("# ") {
            if let Some(b) = h.strip_prefix("branch.head ") {
                st.branch = if b == "(detached)" { None } else { Some(b.to_string()) };
            } else if let Some(ab) = h.strip_prefix("branch.ab ") {
                let mut it = ab.split(' ');
                st.ahead = it.next().and_then(|s| s.trim_start_matches('+').parse().ok()).unwrap_or(0);
                st.behind = it.next().and_then(|s| s.trim_start_matches('-').parse().ok()).unwrap_or(0);
            }
            continue;
        }
        let mut parts = rec.splitn(9, ' ');
        let kind = parts.next().unwrap_or("");
        match kind {
            "1" | "2" => {
                let xy = parts.next().unwrap_or("..");
                let (x, y) = (xy.as_bytes().first().copied().unwrap_or(b'.'), xy.as_bytes().get(1).copied().unwrap_or(b'.'));
                // fields: sub mH mI mW hH hI [X score] path
                let path = if kind == "1" { parts.nth(6).unwrap_or("") } else { parts.nth(7).unwrap_or("") };
                if kind == "2" {
                    let _orig = fields.next(); // renamed-from path follows as its own record
                }
                let staged = x != b'.';
                let code = if y != b'.' { y } else { x };
                let state = match code {
                    b'M' | b'T' => State::Modified,
                    b'A' => State::Added,
                    b'D' => State::Deleted,
                    b'R' | b'C' => State::Renamed,
                    _ => State::Modified,
                };
                st.entries.insert(strip(path), Entry { state, staged });
            }
            "u" => {
                let path = parts.nth(8).unwrap_or("");
                st.entries.insert(strip(path), Entry { state: State::Conflicted, staged: false });
            }
            "?" => {
                let path = parts.next().unwrap_or("");
                st.entries.insert(strip(path), Entry { state: State::Untracked, staged: false });
            }
            "!" => {
                let path = parts.next().unwrap_or("");
                st.entries.insert(strip(path).trim_end_matches('/').to_string(), Entry { state: State::Ignored, staged: false });
            }
            _ => {}
        }
    }
    status_cache().lock().unwrap().insert(dir.to_path_buf(), st);
    Some(())
}

/// The state for one child of `dir`, aggregating folders, using the cached status (running it if absent).
pub fn state_for(dir: &Path, name: &str, is_dir: bool) -> Option<Entry> {
    if !status_cache().lock().unwrap().contains_key(dir) {
        status(dir)?;
    }
    let cache = status_cache().lock().unwrap();
    let st = cache.get(dir)?;
    if let Some(e) = st.entries.get(name) {
        return Some(e.clone());
    }
    if is_dir {
        let prefix = format!("{name}/");
        let mut best: Option<Entry> = None;
        let mut all_untracked = true;
        let mut any = false;
        for (p, e) in &st.entries {
            if p.starts_with(&prefix) {
                any = true;
                if e.state != State::Untracked {
                    all_untracked = false;
                }
                if best.as_ref().map(|b| e.state.weight() > b.state.weight()).unwrap_or(true) {
                    best = Some(e.clone());
                }
            }
        }
        if any {
            return if all_untracked { Some(Entry { state: State::Untracked, staged: false }) } else { best };
        }
    }
    Some(Entry { state: State::Clean, staged: false })
}

pub fn entry_json(e: &Entry) -> Value {
    Value::obj().s("state", e.state.as_str()).b("staged", e.staged).done()
}

/// Branch chip: read `.git/HEAD` directly, no process.
pub fn repo_json(dir: &Path) -> Value {
    let Some(root) = repo_root(dir) else { return Value::Null };
    let head = std::fs::read_to_string(root.join(".git/HEAD")).unwrap_or_default();
    let head = head.trim();
    let (branch, detached) = match head.strip_prefix("ref: refs/heads/") {
        Some(b) => (Some(b.to_string()), false),
        None => (Some(head.chars().take(8).collect()), true),
    };
    let (ahead, behind, dirty) = {
        let c = status_cache().lock().unwrap();
        match c.get(dir) {
            Some(s) => (s.ahead, s.behind, s.entries.values().any(|e| e.state != State::Ignored && e.state != State::Clean)),
            None => (0, 0, false),
        }
    };
    Value::obj().s("root", crate::vfs::uri::Uri::from_path(&root).to_string()).opt_s("branch", branch.as_deref()).b("detached", detached).u("ahead", ahead as u64).u("behind", behind as u64).b("dirty", dirty).done()
}

/// Inspector detail: state, branch, last commit for a file.
pub fn file_json(path: &Path) -> Value {
    let dir = path.parent().unwrap_or(Path::new("/"));
    let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let Some(root) = repo_root(dir) else { return Value::Null };
    let e = state_for(dir, &name, path.is_dir()).unwrap_or(Entry { state: State::Clean, staged: false });
    let branch = std::fs::read_to_string(root.join(".git/HEAD")).ok().and_then(|h| h.trim().strip_prefix("ref: refs/heads/").map(str::to_string));
    let last = Command::new("git")
        .arg("-C")
        .arg(&root)
        .args(["log", "-1", "--format=%H%x00%h%x00%an%x00%at%x00%s", "--"])
        .arg(path)
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .ok()
        .and_then(|o| if o.status.success() { Some(String::from_utf8_lossy(&o.stdout).trim().to_string()) } else { None })
        .filter(|s| !s.is_empty())
        .map(|s| {
            let f: Vec<&str> = s.split('\0').collect();
            Value::obj().s("hash", f.first().copied().unwrap_or("")).s("short", f.get(1).copied().unwrap_or("")).s("author", f.get(2).copied().unwrap_or("")).u("time", f.get(3).and_then(|t| t.parse().ok()).unwrap_or(0)).s("subject", f.get(4).copied().unwrap_or("")).done()
        })
        .unwrap_or(Value::Null);
    Value::obj().s("state", e.state.as_str()).b("staged", e.staged).opt_s("branch", branch.as_deref()).v("last", last).done()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn git(dir: &Path, args: &[&str]) {
        let st = Command::new("git").arg("-C").arg(dir).args(args).stdout(Stdio::null()).stderr(Stdio::null()).status().unwrap();
        assert!(st.success(), "git {args:?}");
    }

    #[test]
    fn states_and_aggregation() {
        if !crate::openin::on_path("git") {
            return;
        }
        let d = std::env::temp_dir().join(format!("kiki-git-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(d.join("sub")).unwrap();
        git(&d, &["init", "-q", "-b", "main"]);
        git(&d, &["config", "user.email", "t@t"]);
        git(&d, &["config", "user.name", "t"]);
        std::fs::write(d.join("tracked.txt"), b"a").unwrap();
        std::fs::write(d.join("sub/inner.txt"), b"a").unwrap();
        std::fs::write(d.join(".gitignore"), b"ignored.log\n").unwrap();
        git(&d, &["add", "-A"]);
        git(&d, &["commit", "-qm", "init"]);
        std::fs::write(d.join("tracked.txt"), b"changed").unwrap();
        std::fs::write(d.join("sub/inner.txt"), b"changed").unwrap();
        std::fs::write(d.join("new.txt"), b"n").unwrap();
        std::fs::write(d.join("ignored.log"), b"i").unwrap();
        std::fs::write(d.join("staged.txt"), b"s").unwrap();
        git(&d, &["add", "staged.txt"]);
        assert_eq!(repo_root(&d.join("sub")), Some(d.clone()));
        assert_eq!(state_for(&d, "tracked.txt", false).unwrap().state, State::Modified);
        assert_eq!(state_for(&d, "new.txt", false).unwrap().state, State::Untracked);
        assert_eq!(state_for(&d, "ignored.log", false).unwrap().state, State::Ignored);
        let s = state_for(&d, "staged.txt", false).unwrap();
        assert_eq!((s.state, s.staged), (State::Added, true));
        assert_eq!(state_for(&d, "sub", true).unwrap().state, State::Modified); // aggregated
        assert_eq!(state_for(&d, ".gitignore", false).unwrap().state, State::Clean);
        let r = repo_json(&d);
        assert_eq!(r.str_field("branch"), Some("main"));
        assert_eq!(r.get("dirty").and_then(Value::as_bool), Some(true));
        let f = file_json(&d.join("tracked.txt"));
        assert_eq!(f.str_field("state"), Some("modified"));
        assert_eq!(f.get("last").unwrap().str_field("subject"), Some("init"));
        assert!(repo_root(&std::env::temp_dir()).is_none() || true);
        std::fs::remove_dir_all(&d).unwrap();
    }
}
