//! Directory watching. Linux: one inotify descriptor, one thread, events coalesced
//! for 50 ms, then the listing is rescanned (unchanged rows keep their metadata).
//! Other platforms: no-op, so native tests still run.

use crate::listing::Listing;
use std::sync::Arc;

pub const MAX_WATCHES: usize = 64;

#[cfg(target_os = "linux")]
mod imp {
    use super::*;
    use std::collections::HashMap;
    use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};
    use std::os::unix::ffi::OsStrExt;
    use std::path::{Path, PathBuf};
    use std::sync::{Mutex, OnceLock};
    use std::time::{Duration, Instant};

    struct State {
        fd: OwnedFd,
        by_wd: HashMap<i32, (PathBuf, Instant)>,
        by_path: HashMap<PathBuf, i32>,
        /// Watches on a repository's `.git` and `.git/refs/heads`: wd -> the repository's root.
        /// A listing's own watch sees files change; it cannot see `git add`, a commit or a
        /// checkout made in a terminal, which change every badge without touching a listed file.
        repo_by_wd: HashMap<i32, PathBuf>,
        repos: HashMap<PathBuf, Vec<i32>>,
    }

    /// Repositories watched at once. Two watches each, and a root nobody is listing any more is
    /// dropped the first time it reports.
    const MAX_REPOS: usize = 16;
    /// Plan 15: a burst of git's own writes (`index.lock`, `index`, `HEAD`, `ORIG_HEAD`, the ref)
    /// is one change.
    const REPO_DEBOUNCE: Duration = Duration::from_millis(300);

    fn state() -> &'static Mutex<State> {
        static S: OnceLock<Mutex<State>> = OnceLock::new();
        S.get_or_init(|| {
            // Blocking, on purpose: the reader polls before every read. The raw descriptor is
            // ours and nobody else's, so wrapping it hands its close to the `OwnedFd`.
            let fd = unsafe {
                let raw = libc::inotify_init1(libc::IN_CLOEXEC);
                if raw < 0 {
                    panic!("inotify_init: {}", std::io::Error::last_os_error());
                }
                OwnedFd::from_raw_fd(raw)
            };
            let raw = fd.as_raw_fd();
            std::thread::Builder::new().name("watch".into()).spawn(move || reader(raw)).expect("spawn watcher");
            Mutex::new(State { fd, by_wd: HashMap::new(), by_path: HashMap::new(), repo_by_wd: HashMap::new(), repos: HashMap::new() })
        })
    }

    /// The watch descriptor for `path`, or why not: a path with a NUL in it is refused here as
    /// the kernel would refuse it, and the rest (gone, not a directory, out of watches) comes
    /// back as errno.
    fn add_watch(fd: &OwnedFd, path: &Path, mask: u32) -> std::io::Result<i32> {
        let c_path = std::ffi::CString::new(path.as_os_str().as_bytes()).map_err(|_| std::io::Error::from_raw_os_error(libc::EINVAL))?;
        // The path is NUL-terminated and the descriptor is open for as long as `fd` is borrowed.
        let wd = unsafe { libc::inotify_add_watch(fd.as_raw_fd(), c_path.as_ptr(), mask) };
        if wd < 0 {
            return Err(std::io::Error::last_os_error());
        }
        Ok(wd)
    }

    fn rm_watch(fd: &OwnedFd, wd: i32) {
        // A `wd` the kernel has already dropped (IN_IGNORED came first) just answers EINVAL,
        // which nothing here needs to know.
        let _ = unsafe { libc::inotify_rm_watch(fd.as_raw_fd(), wd) };
    }

    fn watch_repo(s: &mut State, root: PathBuf) {
        if s.repos.contains_key(&root) {
            return;
        }
        if s.repos.len() >= MAX_REPOS {
            if let Some(old) = s.repos.keys().next().cloned() {
                drop_repo(s, &old);
            }
        }
        let flags = libc::IN_CREATE | libc::IN_DELETE | libc::IN_MOVED_TO | libc::IN_MOVED_FROM | libc::IN_MODIFY;
        let git = root.join(".git");
        let wds: Vec<i32> = [git.clone(), git.join("refs/heads")].iter().filter_map(|d| add_watch(&s.fd, d, flags).ok()).collect();
        for wd in &wds {
            s.repo_by_wd.insert(*wd, root.clone());
        }
        if !wds.is_empty() {
            s.repos.insert(root, wds);
        }
    }

    fn drop_repo(s: &mut State, root: &PathBuf) {
        for wd in s.repos.remove(root).unwrap_or_default() {
            rm_watch(&s.fd, wd);
            s.repo_by_wd.remove(&wd);
        }
    }

    pub fn watch(l: &Arc<Listing>) {
        // Found before the lock is taken: it is a walk up the tree, and nothing of ours.
        let repo = if l.uri.is_local() && crate::git::enabled() { crate::git::repo_root(&l.path) } else { None };
        let mut s = state().lock().unwrap();
        if let Some(root) = repo {
            watch_repo(&mut s, root);
        }
        if s.by_path.contains_key(&l.path) {
            return;
        }
        if s.by_path.len() >= MAX_WATCHES {
            // Evict the least recently registered watch; its listing becomes stale.
            if let Some((&wd, (path, _))) = s.by_wd.iter().min_by_key(|(_, (_, t))| *t) {
                let path = path.clone();
                rm_watch(&s.fd, wd);
                s.by_wd.remove(&wd);
                s.by_path.remove(&path);
                crate::listing::mark_stale(&path);
            }
        }
        let flags = libc::IN_CREATE | libc::IN_DELETE | libc::IN_MOVED_FROM | libc::IN_MOVED_TO | libc::IN_MODIFY | libc::IN_ATTRIB | libc::IN_DELETE_SELF | libc::IN_MOVE_SELF | libc::IN_ONLYDIR;
        match add_watch(&s.fd, &l.path, flags) {
            Ok(wd) => {
                s.by_wd.insert(wd, (l.path.clone(), Instant::now()));
                s.by_path.insert(l.path.clone(), wd);
            }
            Err(_) => crate::listing::mark_stale(&l.path),
        }
    }

    pub fn unwatch(l: &Arc<Listing>) {
        let mut s = state().lock().unwrap();
        if let Some(wd) = s.by_path.remove(&l.path) {
            rm_watch(&s.fd, wd);
            s.by_wd.remove(&wd);
        }
    }

    fn reader(raw: i32) {
        let mut buf = vec![0u8; 64 * 1024];
        struct Batch {
            at: Instant,
            added: Vec<Vec<u8>>,
            removed: Vec<Vec<u8>>,
            modified: Vec<Vec<u8>>,
            rescan: bool,
        }
        let mut pending: HashMap<PathBuf, Batch> = HashMap::new();
        // Repository root -> when its first unreported change was seen.
        let mut repos_pending: HashMap<PathBuf, Instant> = HashMap::new();
        let mut last_sweep = Instant::now();
        loop {
            // Poll with a short timeout so coalesced events flush even when quiet.
            let mut pfd = libc::pollfd { fd: raw, events: libc::POLLIN, revents: 0 };
            let ready = unsafe { libc::poll(&mut pfd, 1, 25) };
            if ready > 0 {
                let n = unsafe { libc::read(raw, buf.as_mut_ptr() as *mut libc::c_void, buf.len()) };
                if n > 0 {
                    let n = n as usize;
                    let mut off = 0;
                    while off + 16 <= n {
                        let wd = i32::from_ne_bytes([buf[off], buf[off + 1], buf[off + 2], buf[off + 3]]);
                        let mask = u32::from_ne_bytes([buf[off + 4], buf[off + 5], buf[off + 6], buf[off + 7]]);
                        let len = u32::from_ne_bytes([buf[off + 12], buf[off + 13], buf[off + 14], buf[off + 15]]) as usize;
                        let name_bytes = &buf[off + 16..off + 16 + len];
                        let name: Vec<u8> = name_bytes.iter().take_while(|&&b| b != 0).copied().collect();
                        off += 16 + len;
                        let s = state().lock().unwrap();
                        if let Some(root) = s.repo_by_wd.get(&wd) {
                            // git writes `x.lock` and renames it over `x`: the rename is the news.
                            if !name.ends_with(b".lock") {
                                repos_pending.entry(root.clone()).or_insert_with(Instant::now);
                            }
                            continue;
                        }
                        if let Some((path, _)) = s.by_wd.get(&wd) {
                            if mask & (libc::IN_DELETE_SELF | libc::IN_MOVE_SELF | libc::IN_IGNORED) != 0 {
                                let p = path.clone();
                                drop(s);
                                gone(&p);
                                continue;
                            }
                            let b = pending.entry(path.clone()).or_insert_with(|| Batch { at: Instant::now(), added: Vec::new(), removed: Vec::new(), modified: Vec::new(), rescan: false });
                            if mask & libc::IN_Q_OVERFLOW != 0 || name.is_empty() {
                                b.rescan = true;
                            } else if mask & (libc::IN_CREATE | libc::IN_MOVED_TO) != 0 {
                                b.added.push(name);
                            } else if mask & (libc::IN_DELETE | libc::IN_MOVED_FROM) != 0 {
                                b.removed.push(name);
                            } else {
                                b.modified.push(name);
                            }
                        }
                    }
                }
            }
            let now = Instant::now();
            // Deleted from under us? The kernel does not say: a cached listing holds its folder
            // open, IN_DELETE_SELF waits for the last holder to let go, and removing a folder
            // touches nothing inside it — an empty one vanishes without a single event. So once a
            // second each watched path is asked whether it still leads to the folder that was
            // opened there (one `stat`; there are at most MAX_WATCHES of them).
            if now.duration_since(last_sweep) >= Duration::from_secs(1) {
                last_sweep = now;
                let watched: Vec<PathBuf> = state().lock().unwrap().by_path.keys().cloned().collect();
                for p in watched {
                    if !crate::listing::still_there(&p) {
                        pending.remove(&p);
                        gone(&p);
                    }
                }
                // And the same sweep asks the projects inside the folders on screen whether git
                // has been at them. They have no watch — a folder of forty would want forty,
                // against sixty-four for the whole daemon — so two `stat`s a second each stand
                // in for one (plan 15).
                crate::listing::poll_repo_rows();
            }
            let settled: Vec<PathBuf> = repos_pending.iter().filter(|(_, at)| now.duration_since(**at) >= REPO_DEBOUNCE).map(|(r, _)| r.clone()).collect();
            for root in settled {
                repos_pending.remove(&root);
                let showing = crate::listing::under(&root);
                if showing.is_empty() {
                    drop_repo(&mut state().lock().unwrap(), &root);
                    continue;
                }
                // Status again for every listing under it, each on its own worker; each tells its
                // windows `RepoChanged`, which is what the branch chip listens for.
                for l in showing {
                    if !crate::git::is_slow(&l.path) {
                        l.git_status();
                    }
                    // A repository inside this one — a submodule — carries its own capsule, and
                    // what just changed may have been a step into or out of it.
                    l.repo_rows(None);
                }
            }
            let due: Vec<PathBuf> = pending.iter().filter(|(_, b)| now.duration_since(b.at) >= Duration::from_millis(50)).map(|(p, _)| p.clone()).collect();
            for p in due {
                let b = pending.remove(&p).unwrap();
                if let Some(l) = crate::listing::find(&p) {
                    if b.rescan || b.added.len() + b.removed.len() > 5_000 {
                        l.rescan();
                    } else {
                        l.patch(&b.added, &b.removed, &b.modified);
                    }
                }
            }
        }
    }

    fn gone(path: &PathBuf) {
        let mut s = state().lock().unwrap();
        if let Some(wd) = s.by_path.remove(path) {
            // Still registered when we worked it out ourselves rather than being told.
            rm_watch(&s.fd, wd);
            s.by_wd.remove(&wd);
        }
        drop(s);
        crate::listing::gone(path);
    }
}

#[cfg(not(target_os = "linux"))]
mod imp {
    use super::*;
    pub fn watch(_l: &Arc<Listing>) {}
    pub fn unwatch(_l: &Arc<Listing>) {}
}

pub use imp::{unwatch, watch};
