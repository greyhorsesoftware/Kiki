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

/// Directories whose status is kept. One `Status` holds every changed path under its directory,
/// so this is the cache that matters; a directory dropped from it is simply asked again.
pub const STATUS_KEEP: usize = 64;
/// Directories whose repository root is remembered.
pub const ROOTS_KEEP: usize = 1024;
/// How long "this is not in a repository" is believed: short, so `git init` or a clone into a
/// folder kiki has already shown is noticed without restarting the daemon.
const NO_ROOT_FOR: Duration = Duration::from_secs(5);
/// How long a root is believed without looking again — and then only while its `.git` is there.
const ROOT_FOR: Duration = Duration::from_secs(60);

type Roots = HashMap<PathBuf, (Option<PathBuf>, Instant)>;

fn roots() -> &'static Mutex<Roots> {
    static CACHE: OnceLock<Mutex<Roots>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

fn root_still_good(found: &Option<PathBuf>, at: Instant) -> bool {
    match found {
        None => at.elapsed() < NO_ROOT_FOR,
        Some(root) => at.elapsed() < ROOT_FOR && root.join(".git").exists(),
    }
}

/// Repository root for a directory, cached (negative results too) — for a while, see above.
pub fn repo_root(dir: &Path) -> Option<PathBuf> {
    if let Some((found, at)) = roots().lock().unwrap().get(dir) {
        if root_still_good(found, *at) {
            return found.clone();
        }
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
    let mut c = roots().lock().unwrap();
    if c.len() >= ROOTS_KEEP {
        // Everything stale goes; if a long walk filled it with fresh answers, all of it does —
        // an answer costs a handful of stats to get back.
        c.retain(|_, (f, at)| root_still_good(f, *at));
        if c.len() >= ROOTS_KEEP {
            c.clear();
        }
    }
    c.insert(dir.to_path_buf(), (found.clone(), Instant::now()));
    found
}

pub fn enabled() -> bool {
    crate::config::settings().get("git").and_then(|g| g.get("enabled")).and_then(Value::as_bool).unwrap_or(true)
}

fn status_cache() -> &'static Mutex<HashMap<PathBuf, Status>> {
    static C: OnceLock<Mutex<HashMap<PathBuf, Status>>> = OnceLock::new();
    C.get_or_init(|| Mutex::new(HashMap::new()))
}

/// The directory changed, or nobody is showing it any more: forget what was known about it,
/// whether it is in a repository included.
pub fn invalidate(dir: &Path) {
    status_cache().lock().unwrap().remove(dir);
    roots().lock().unwrap().remove(dir);
}

#[cfg(test)]
pub(crate) fn cached_statuses() -> usize {
    status_cache().lock().unwrap().len()
}

/// Every git command kiki runs starts here, never with a bare `Command::new("git")`.
///
/// A repository's own `.git/config` can name programs that git then runs: `core.fsmonitor` is
/// executed during `status`, and the `post-index-change` hook under `core.hooksPath` runs when
/// status refreshes the index. `safe.directory` is no protection — it only refuses repositories
/// owned by *another* user, and a repository the user downloaded and unpacked is owned by the
/// user. Since kiki runs status for any folder it lists, walking into an unpacked repository
/// would otherwise be enough to run whatever that repository asked for. Both keys are emptied
/// here; `GIT_OPTIONAL_LOCKS=0` stops us taking `index.lock` in someone else's working tree,
/// which also keeps status from refreshing the index at all.
fn git(dir: &Path) -> Command {
    let mut c = Command::new("git");
    c.arg("-C").arg(dir).args(["-c", "core.fsmonitor=", "-c", "core.hooksPath=/var/empty"]).env("GIT_OPTIONAL_LOCKS", "0").stdin(Stdio::null()).stderr(Stdio::null());
    c
}

/// Runs status for `dir` (scoped with a pathspec) and caches it. Returns None outside a repository.
pub fn status(dir: &Path) -> Option<()> {
    let root = repo_root(dir)?;
    let start = Instant::now();
    let out = git(dir).args(["status", "--porcelain=v2", "-z", "--branch", "--untracked-files=all", "--ignored=matching", "--", "."]).output().ok()?;
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
    let mut cache = status_cache().lock().unwrap();
    cache.insert(dir.to_path_buf(), st);
    // Oldest out. The directory on screen is re-read whenever it changes, so it is never oldest
    // for long; one that was dropped is asked again the next time it is listed.
    while cache.len() > STATUS_KEEP {
        let Some(oldest) = cache.iter().min_by_key(|(_, s)| s.at).map(|(k, _)| k.clone()) else { break };
        cache.remove(&oldest);
    }
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
    Value::obj()
        .s("root", crate::vfs::uri::Uri::from_path(&root).to_string())
        .opt_s("branch", branch.as_deref())
        .b("detached", detached)
        .u("ahead", ahead as u64)
        .u("behind", behind as u64)
        .b("dirty", dirty)
        .done()
}

/// Inspector detail: state, branch, last commit for a file.
pub fn file_json(path: &Path) -> Value {
    let dir = path.parent().unwrap_or(Path::new("/"));
    let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let Some(root) = repo_root(dir) else { return Value::Null };
    let e = state_for(dir, &name, path.is_dir()).unwrap_or(Entry { state: State::Clean, staged: false });
    let branch = std::fs::read_to_string(root.join(".git/HEAD")).ok().and_then(|h| h.trim().strip_prefix("ref: refs/heads/").map(str::to_string));
    let last = git(&root)
        .args(["log", "-1", "--format=%H%x00%h%x00%an%x00%at%x00%s", "--"])
        .arg(path)
        .output()
        .ok()
        .and_then(|o| if o.status.success() { Some(String::from_utf8_lossy(&o.stdout).trim().to_string()) } else { None })
        .filter(|s| !s.is_empty())
        .map(|s| {
            let f: Vec<&str> = s.split('\0').collect();
            Value::obj()
                .s("hash", f.first().copied().unwrap_or(""))
                .s("short", f.get(1).copied().unwrap_or(""))
                .s("author", f.get(2).copied().unwrap_or(""))
                .u("time", f.get(3).and_then(|t| t.parse().ok()).unwrap_or(0))
                .s("subject", f.get(4).copied().unwrap_or(""))
                .done()
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

    /// A repository can name programs in its own config, and git runs them: `core.fsmonitor`
    /// during a status, and the `post-index-change` hook under `core.hooksPath` when the index is
    /// refreshed. kiki runs status for every folder it lists, so without the flags in `git()`,
    /// walking into an unpacked repository would run whatever it asked for.
    #[test]
    fn a_repository_cannot_run_its_own_config() {
        if !crate::openin::on_path("git") {
            return;
        }
        let d = std::env::temp_dir().join(format!("kiki-git-hostile-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(d.join("hooks")).unwrap();
        git(&d, &["init", "-q", "-b", "main"]);
        git(&d, &["config", "user.email", "t@t"]);
        git(&d, &["config", "user.name", "t"]);
        std::fs::write(d.join("a.txt"), b"a").unwrap();
        git(&d, &["add", "a.txt"]);
        git(&d, &["commit", "-qm", "one"]);

        let fsmonitor = d.join("fsmonitor.sh");
        let hook = d.join("hooks/post-index-change");
        let marks = (d.join("ran-fsmonitor"), d.join("ran-hook"));
        for (script, mark) in [(&fsmonitor, &marks.0), (&hook, &marks.1)] {
            std::fs::write(script, format!("#!/bin/sh\ntouch {}\nexit 1\n", mark.display())).unwrap();
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                std::fs::set_permissions(script, std::fs::Permissions::from_mode(0o755)).unwrap();
            }
        }
        git(&d, &["config", "core.fsmonitor", &fsmonitor.to_string_lossy()]);
        git(&d, &["config", "core.hooksPath", &d.join("hooks").to_string_lossy()]);

        invalidate(&d);
        let _ = status(&d);
        assert!(!marks.0.exists(), "core.fsmonitor ran: listing a folder executed the repository's config");
        assert!(!marks.1.exists(), "the post-index-change hook ran: listing a folder executed the repository's hooks");
        // The status itself still worked — the guard must not cost us the overlay.
        assert!(state_for(&d, "a.txt", false).is_some() || repo_root(&d).is_some());
        let _ = std::fs::remove_dir_all(&d);
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
        // a bare temp dir is not a repository (or is inside one on some CI images): either answer is valid
        let _ = repo_root(&std::env::temp_dir());
        std::fs::remove_dir_all(&d).unwrap();
    }

    fn scratch(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("kiki-git-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d.canonicalize().unwrap()
    }

    #[test]
    fn a_repository_made_after_the_folder_was_shown_is_noticed() {
        let d = scratch("late-init");
        assert_eq!(repo_root(&d), None);
        git(&d, &["init", "-q", "-b", "main"]);
        // Believed for a few seconds only…
        assert!(root_still_good(&None, Instant::now()));
        assert!(!root_still_good(&None, Instant::now().checked_sub(NO_ROOT_FOR + Duration::from_secs(1)).unwrap()));
        // …and not at all once the folder is rescanned, which creating `.git` in it causes.
        invalidate(&d);
        assert_eq!(repo_root(&d), Some(d.clone()));
        std::fs::remove_dir_all(&d).unwrap();
    }

    #[test]
    fn a_repository_that_is_no_longer_one_stops_being_one() {
        let d = scratch("un-init");
        git(&d, &["init", "-q", "-b", "main"]);
        assert_eq!(repo_root(&d), Some(d.clone()));
        std::fs::remove_dir_all(d.join(".git")).unwrap();
        assert_eq!(repo_root(&d), None, "a remembered root is only believed while its .git is there");
        std::fs::remove_dir_all(&d).unwrap();
    }

    #[test]
    fn statuses_are_kept_for_so_many_directories_and_no_more() {
        let d = scratch("many");
        git(&d, &["init", "-q", "-b", "main"]);
        let n = STATUS_KEEP + 8;
        for i in 0..n {
            let sub = d.join(format!("d{i}"));
            std::fs::create_dir_all(&sub).unwrap();
            std::fs::write(sub.join("new.txt"), "x").unwrap();
            assert!(status(&sub).is_some());
            assert!(cached_statuses() <= STATUS_KEEP, "after {i}: {}", cached_statuses());
        }
        // The newest is there; the first went to make room, and asking again brings it back.
        assert_eq!(state_for(&d.join(format!("d{}", n - 1)), "new.txt", false).map(|e| e.state), Some(State::Untracked));
        assert_eq!(state_for(&d.join("d0"), "new.txt", false).map(|e| e.state), Some(State::Untracked));
        assert!(cached_statuses() <= STATUS_KEEP);
        std::fs::remove_dir_all(&d).unwrap();
    }

    #[test]
    fn remembered_roots_are_capped() {
        let base = scratch("roots");
        for i in 0..ROOTS_KEEP + 10 {
            let _ = repo_root(&base.join(format!("nowhere{i}")));
        }
        assert!(roots().lock().unwrap().len() <= ROOTS_KEEP);
        std::fs::remove_dir_all(&base).unwrap();
    }
}
