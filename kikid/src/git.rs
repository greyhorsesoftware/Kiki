//! Git status for listings (plan 15): `git status --porcelain=v2 -z` scoped to the listed
//! directory, cached per (root, dir), folders aggregated.

use crate::json::Value;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::{Condvar, Mutex, OnceLock};
use std::time::{Duration, Instant, SystemTime};

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

/// A row that is itself a repository (plan 15): which branch it is on, and how the repository
/// stands — its own state, not what the folder above makes of it.
#[derive(Clone, Debug)]
pub struct RepoMark {
    pub branch: String,
    pub detached: bool,
    /// `None` until the aggregate has run: the branch is a file read and arrives with the
    /// listing, the state is a `git` and follows.
    pub state: Option<State>,
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

/// Is this folder itself a repository? `.git` is there — a directory, or a file naming one
/// elsewhere, which is what a worktree and a submodule have. One `stat`.
pub fn is_repo_root(dir: &Path) -> bool {
    dir.join(".git").exists()
}

/// Where a repository keeps itself: `<root>/.git`, or wherever a `.git` *file* points. A
/// worktree's and a submodule's `HEAD` are not under the folder at all, and reading
/// `<root>/.git/HEAD` from one gets nothing.
pub fn git_dir(root: &Path) -> Option<PathBuf> {
    let dot = root.join(".git");
    let md = std::fs::metadata(&dot).ok()?;
    if md.is_dir() {
        return Some(dot);
    }
    let text = std::fs::read_to_string(&dot).ok()?;
    let at = Path::new(text.lines().find_map(|l| l.trim().strip_prefix("gitdir:"))?.trim());
    Some(if at.is_absolute() { at.to_path_buf() } else { root.join(at) })
}

/// Which branch a repository is on, straight out of `HEAD` — one small read, no process, which
/// is what lets a folder of forty projects name all forty at once. Detached: the short hash,
/// and `true`, as git itself shows it.
pub fn head(root: &Path) -> Option<(String, bool)> {
    let text = std::fs::read_to_string(git_dir(root)?.join("HEAD")).ok()?;
    let head = text.trim();
    if head.is_empty() {
        return None;
    }
    match head.strip_prefix("ref: ") {
        Some(r) => Some((r.strip_prefix("refs/heads/").unwrap_or(r).to_string(), false)),
        None => Some((head.chars().take(8).collect(), true)),
    }
}

pub fn enabled() -> bool {
    crate::config::settings().get("git").and_then(|g| g.get("enabled")).and_then(Value::as_bool).unwrap_or(true)
}

/// `[git] showIgnored = "hide"`: ignored files and folders are left out of a listing, the way
/// dot-files are. ("dim" and "normal" are the shell's business: they are only how a row is drawn.)
pub fn hide_ignored() -> bool {
    crate::config::settings().get("git").and_then(|g| g.str_field("showIgnored").map(|s| s == "hide")).unwrap_or(false)
}

/// Did the last status of `dir` take over two seconds? Such a directory — a vast repository
/// without `core.untrackedCache` — is not run again every time git is used in a terminal; it
/// waits for the directory itself to change, or for an explicit refresh (plan 15).
pub fn is_slow(dir: &Path) -> bool {
    status_cache().lock().unwrap().get(dir).is_some_and(|s| s.slow)
}

fn status_cache() -> &'static Mutex<HashMap<PathBuf, Status>> {
    static C: OnceLock<Mutex<HashMap<PathBuf, Status>>> = OnceLock::new();
    C.get_or_init(|| Mutex::new(HashMap::new()))
}

/// The directory changed, or nobody is showing it any more: forget what was known about it,
/// whether it is in a repository included — and how the projects in it stood, which is what an
/// explicit refresh of a folder full of repositories is asking to be told again.
pub fn invalidate(dir: &Path) {
    status_cache().lock().unwrap().remove(dir);
    roots().lock().unwrap().remove(dir);
    aggs().lock().unwrap().retain(|root, _| root != dir && root.parent() != Some(dir));
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

// ---------------------------------------------------------------- repository-root rows (plan 15)
//
// A folder like `~/Projects` is not a repository, so the status above says nothing at all about
// the projects in it — each of them is its own repository and the folder above can see none of
// them. What such a row shows is its branch, coloured by how that repository stands, and the two
// halves cost very different things: the branch is one file read, the state is a `git` per
// project. So the branch lands with the listing and the state follows, bounded and remembered.

/// How many aggregate status runs may be in flight at once, over the whole daemon. A folder of
/// forty projects must not be forty gits: the machine has other work, and nobody asked for a
/// listing to be expensive.
const AGGREGATE_AT_ONCE: usize = 4;
/// How long an aggregate is believed while the repository's `.git` sits still. An edit in the
/// working tree moves neither `HEAD` nor the index, so this — not the stamps — is what makes a
/// folder listed again notice one.
const AGG_FOR: Duration = Duration::from_millis(1500);
/// Repositories whose aggregate is remembered.
const AGG_KEEP: usize = 256;

/// What `HEAD` and the index looked like when an aggregate ran. Two `stat`s, which is how a
/// commit or a checkout made in a terminal is noticed without a watch per project (`watch.rs`).
type Stamp = (Option<SystemTime>, Option<SystemTime>);

struct Agg {
    state: State,
    at: Instant,
    stamp: Stamp,
}

fn aggs() -> &'static Mutex<HashMap<PathBuf, Agg>> {
    static C: OnceLock<Mutex<HashMap<PathBuf, Agg>>> = OnceLock::new();
    C.get_or_init(|| Mutex::new(HashMap::new()))
}

fn stamp(root: &Path) -> Stamp {
    let Some(g) = git_dir(root) else { return (None, None) };
    let at = |p: PathBuf| std::fs::metadata(p).ok().and_then(|m| m.modified().ok());
    (at(g.join("HEAD")), at(g.join("index")))
}

/// Has a repository's `HEAD` or index moved since its aggregate was worked out? A repository
/// nobody has asked about yet answers `false`: it has nothing to be stale against, and saying
/// otherwise would have the poll run a `git` a second for a repository whose status fails.
pub fn moved(root: &Path) -> bool {
    let now = stamp(root);
    aggs().lock().unwrap().get(root).is_some_and(|a| a.stamp != now)
}

/// The slots `aggregate` runs in, and the guard that gives one back however the run ends.
fn slots() -> &'static (Mutex<usize>, Condvar) {
    static S: OnceLock<(Mutex<usize>, Condvar)> = OnceLock::new();
    S.get_or_init(|| (Mutex::new(AGGREGATE_AT_ONCE), Condvar::new()))
}

struct Slot;

impl Slot {
    fn take() -> Slot {
        let (n, cv) = slots();
        let mut n = n.lock().unwrap();
        while *n == 0 {
            n = cv.wait(n).unwrap();
        }
        *n -= 1;
        Slot
    }
}

impl Drop for Slot {
    fn drop(&mut self) {
        let (n, cv) = slots();
        *n.lock().unwrap() += 1;
        cv.notify_one();
    }
}

/// The worst state anywhere in a repository — the one thing a repository-root row's colour has
/// to say. Cheaper than the listing's own status on purpose: only the worst matters, so
/// untracked files need not be named one by one and ignored ones need not be found at all.
pub fn aggregate(root: &Path) -> Option<State> {
    let now = stamp(root);
    if let Some(a) = aggs().lock().unwrap().get(root) {
        if a.stamp == now && a.at.elapsed() < AGG_FOR {
            return Some(a.state);
        }
    }
    let out = {
        let _slot = Slot::take();
        git(root).args(["status", "--porcelain=v2", "-z", "--untracked-files=normal"]).output().ok()?
    };
    if !out.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&out.stdout);
    let mut worst = State::Clean;
    let mut fields = text.split('\0');
    while let Some(rec) = fields.next() {
        let state = match rec.as_bytes().first() {
            Some(b'?') => State::Untracked,
            Some(b'u') => State::Conflicted,
            Some(k @ (b'1' | b'2')) => {
                if *k == b'2' {
                    let _ = fields.next(); // a rename's old path is its own record
                }
                let xy = rec.split(' ').nth(1).unwrap_or("..").as_bytes();
                let (x, y) = (xy.first().copied().unwrap_or(b'.'), xy.get(1).copied().unwrap_or(b'.'));
                match if y != b'.' { y } else { x } {
                    b'A' => State::Added,
                    b'D' => State::Deleted,
                    b'R' | b'C' => State::Renamed,
                    _ => State::Modified,
                }
            }
            _ => continue,
        };
        if state.weight() > worst.weight() {
            worst = state;
        }
    }
    let mut c = aggs().lock().unwrap();
    c.insert(root.to_path_buf(), Agg { state: worst, at: Instant::now(), stamp: now });
    while c.len() > AGG_KEEP {
        let Some(oldest) = c.iter().min_by_key(|(_, a)| a.at).map(|(k, _)| k.clone()) else { break };
        c.remove(&oldest);
    }
    Some(worst)
}

pub fn entry_json(e: &Entry) -> Value {
    Value::obj().s("state", e.state.as_str()).b("staged", e.staged).done()
}

/// A repository-root row's `git`: the same shape as any other row's, plus what only a root has.
/// Until the aggregate lands the state is `clean`, which draws the capsule muted — "this is the
/// branch, and nothing is known against it yet" — and the colour arrives as a row update.
pub fn root_json(r: &RepoMark) -> Value {
    Value::obj().s("state", r.state.unwrap_or(State::Clean).as_str()).b("staged", false).b("root", true).s("branch", r.branch.clone()).b("detached", r.detached).done()
}

/// Branch chip: read `HEAD` directly, no process.
pub fn repo_json(dir: &Path) -> Value {
    let Some(root) = repo_root(dir) else { return Value::Null };
    let (branch, detached) = match head(&root) {
        Some((b, d)) => (Some(b), d),
        None => (None, false),
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
    let branch = head(&root).filter(|(_, detached)| !detached).map(|(b, _)| b);
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

    // ------------------------------------------------ repository-root rows (plan 15)

    /// A folder inside `~/Projects` is a repository the folder above knows nothing about: what
    /// it says for itself is its branch, and — for a worktree or a submodule — `.git` is a file
    /// naming a directory somewhere else entirely, which reading `<root>/.git/HEAD` never finds.
    #[test]
    fn a_repository_row_names_its_branch_wherever_its_git_lives() {
        if !crate::openin::on_path("git") {
            return;
        }
        let base = scratch("roots-branch");
        let d = base.join("project");
        std::fs::create_dir_all(&d).unwrap();
        git(&d, &["init", "-q", "-b", "main"]);
        git(&d, &["config", "user.email", "t@t"]);
        git(&d, &["config", "user.name", "t"]);
        std::fs::write(d.join("a.txt"), b"a").unwrap();
        git(&d, &["add", "-A"]);
        git(&d, &["commit", "-qm", "one"]);

        assert!(is_repo_root(&d));
        assert!(!is_repo_root(&base), "the folder projects live in is not one of them");
        assert_eq!(head(&d), Some(("main".into(), false)));
        assert_eq!(head(&base), None);

        // A branch with a slash in it keeps the slash, and is not mistaken for a hash.
        git(&d, &["checkout", "-qb", "feature/capsule"]);
        assert_eq!(head(&d), Some(("feature/capsule".into(), false)));

        // Detached: the short hash, and said to be detached.
        let hash = Command::new("git").arg("-C").arg(&d).args(["rev-parse", "HEAD"]).output().unwrap();
        let hash = String::from_utf8_lossy(&hash.stdout).trim().to_string();
        git(&d, &["checkout", "-q", &hash]);
        let (shown, detached) = head(&d).unwrap();
        assert!(detached, "a detached HEAD says so");
        assert_eq!(shown, hash[..8].to_string());

        // A worktree's `.git` is a file; its HEAD is not under the folder at all.
        git(&d, &["checkout", "-q", "main"]);
        let tree = base.join("wt");
        git(&d, &["worktree", "add", "-q", "-b", "side", &tree.to_string_lossy()]);
        assert!(tree.join(".git").is_file(), "a worktree's .git is a file");
        assert!(is_repo_root(&tree));
        assert_eq!(head(&tree), Some(("side".into(), false)));
        assert!(git_dir(&tree).unwrap().join("HEAD").exists());

        std::fs::remove_dir_all(&base).unwrap();
    }

    /// What a capsule is coloured by: the worst thing anywhere in the repository, whether or not
    /// the folder that is showing it is in a repository at all.
    #[test]
    fn the_aggregate_is_the_worst_state_in_the_repository() {
        if !crate::openin::on_path("git") {
            return;
        }
        let base = scratch("roots-aggregate");
        let d = base.join("project");
        std::fs::create_dir_all(d.join("deep/deeper")).unwrap();
        git(&d, &["init", "-q", "-b", "main"]);
        git(&d, &["config", "user.email", "t@t"]);
        git(&d, &["config", "user.name", "t"]);
        std::fs::write(d.join(".gitignore"), b"*.log\n").unwrap();
        std::fs::write(d.join("deep/deeper/a.txt"), b"a").unwrap();
        git(&d, &["add", "-A"]);
        git(&d, &["commit", "-qm", "one"]);
        assert_eq!(aggregate(&d), Some(State::Clean), "committed and quiet");

        // An ignored file is not a state: a repository full of build output is still clean.
        // (An edit moves neither HEAD nor the index, so the answer is believed for a moment —
        // `invalidate` is what a folder being listed again, or refreshed, goes through.)
        std::fs::write(d.join("build.log"), b"l").unwrap();
        invalidate(&d);
        assert_eq!(aggregate(&d), Some(State::Clean), "ignored files leave it clean");

        // Untracked, then modified, then conflicted — each one worse than the last, and the
        // worst is what comes back however deep it is.
        std::fs::write(d.join("new.txt"), b"n").unwrap();
        invalidate(&d);
        assert_eq!(aggregate(&d), Some(State::Untracked));
        std::fs::write(d.join("deep/deeper/a.txt"), b"changed").unwrap();
        invalidate(&d);
        assert_eq!(aggregate(&d), Some(State::Modified), "a change three folders down still counts");

        // A real conflict: two branches touching the same line.
        git(&d, &["checkout", "-q", "--", "deep/deeper/a.txt"]);
        std::fs::remove_file(d.join("new.txt")).unwrap();
        git(&d, &["checkout", "-qb", "other"]);
        std::fs::write(d.join("deep/deeper/a.txt"), b"theirs").unwrap();
        git(&d, &["commit", "-qam", "theirs"]);
        git(&d, &["checkout", "-q", "main"]);
        std::fs::write(d.join("deep/deeper/a.txt"), b"ours").unwrap();
        git(&d, &["commit", "-qam", "ours"]);
        let merge = Command::new("git").arg("-C").arg(&d).args(["merge", "other"]).stdout(Stdio::null()).stderr(Stdio::null()).status().unwrap();
        assert!(!merge.success(), "the merge is meant to conflict");
        assert_eq!(aggregate(&d), Some(State::Conflicted), "a conflict outranks everything");

        // A folder that is not a repository has no aggregate to give.
        assert_eq!(aggregate(&base), if repo_root(&base).is_some() { aggregate(&base) } else { None });
        std::fs::remove_dir_all(&base).unwrap();
    }

    /// A repository inside a repository — a submodule, or simply a clone made in a working tree
    /// — is a row of its own: the folder above can only say that something under it changed,
    /// and the capsule has to name the inner repository's branch and state.
    #[test]
    fn a_repository_inside_a_repository_answers_for_itself() {
        if !crate::openin::on_path("git") {
            return;
        }
        let outer = scratch("roots-nested");
        git(&outer, &["init", "-q", "-b", "main"]);
        git(&outer, &["config", "user.email", "t@t"]);
        git(&outer, &["config", "user.name", "t"]);
        std::fs::write(outer.join("a.txt"), b"a").unwrap();
        git(&outer, &["add", "-A"]);
        git(&outer, &["commit", "-qm", "one"]);

        let inner = outer.join("vendor");
        std::fs::create_dir_all(&inner).unwrap();
        git(&inner, &["init", "-q", "-b", "trunk"]);
        git(&inner, &["config", "user.email", "t@t"]);
        git(&inner, &["config", "user.name", "t"]);
        std::fs::write(inner.join("b.txt"), b"b").unwrap();
        git(&inner, &["add", "-A"]);
        git(&inner, &["commit", "-qm", "one"]);
        std::fs::write(inner.join("b.txt"), b"changed").unwrap();

        assert!(is_repo_root(&inner));
        assert_eq!(head(&inner), Some(("trunk".into(), false)), "the inner branch, not the outer one");
        assert_eq!(aggregate(&inner), Some(State::Modified), "and the inner repository's own state");
        assert_eq!(repo_root(&inner), Some(inner.clone()), "walking up stops at the inner one");
        std::fs::remove_dir_all(&outer).unwrap();
    }

    /// The poll that stands in for a watch: it asks whether `HEAD` or the index has moved since
    /// the aggregate ran, and a repository nobody has asked about yet is not "moved".
    #[test]
    fn a_commit_moves_what_the_poll_looks_at() {
        if !crate::openin::on_path("git") {
            return;
        }
        let d = scratch("roots-moved");
        assert!(!moved(&d), "nothing has been worked out for it, so there is nothing to be stale");
        git(&d, &["init", "-q", "-b", "main"]);
        git(&d, &["config", "user.email", "t@t"]);
        git(&d, &["config", "user.name", "t"]);
        std::fs::write(d.join("a.txt"), b"a").unwrap();
        git(&d, &["add", "-A"]);
        git(&d, &["commit", "-qm", "one"]);
        assert_eq!(aggregate(&d), Some(State::Clean));
        assert!(!moved(&d), "and nothing has happened since");
        // `git checkout -b` writes HEAD; the stat sees it without a watch being spent.
        std::thread::sleep(Duration::from_millis(10));
        git(&d, &["checkout", "-qb", "feature"]);
        assert!(moved(&d), "a checkout moves HEAD");
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
