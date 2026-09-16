//! Directory watching. Linux: one inotify descriptor, one thread, events coalesced
//! for 50 ms, then the listing is rescanned (unchanged rows keep their metadata).
//! Other platforms: no-op, so native tests still run.

use crate::listing::Listing;
use std::sync::Arc;

pub const MAX_WATCHES: usize = 64;

#[cfg(target_os = "linux")]
mod imp {
    use super::*;
    use rustix::fs::inotify::{self, CreateFlags, WatchFlags};
    use std::collections::HashMap;
    use std::os::fd::{AsRawFd, OwnedFd};
    use std::path::PathBuf;
    use std::sync::{Mutex, OnceLock};
    use std::time::{Duration, Instant};

    struct State {
        fd: OwnedFd,
        by_wd: HashMap<i32, (PathBuf, Instant)>,
        by_path: HashMap<PathBuf, i32>,
    }

    fn state() -> &'static Mutex<State> {
        static S: OnceLock<Mutex<State>> = OnceLock::new();
        S.get_or_init(|| {
            let fd = inotify::inotify_init(CreateFlags::CLOEXEC).expect("inotify_init");
            let raw = fd.as_raw_fd();
            std::thread::Builder::new().name("watch".into()).spawn(move || reader(raw)).expect("spawn watcher");
            Mutex::new(State { fd, by_wd: HashMap::new(), by_path: HashMap::new() })
        })
    }

    pub fn watch(l: &Arc<Listing>) {
        let mut s = state().lock().unwrap();
        if s.by_path.contains_key(&l.path) {
            return;
        }
        if s.by_path.len() >= MAX_WATCHES {
            // Evict the least recently registered watch; its listing becomes stale.
            if let Some((&wd, (path, _))) = s.by_wd.iter().min_by_key(|(_, (_, t))| *t) {
                let path = path.clone();
                let _ = inotify::inotify_remove_watch(&s.fd, wd);
                s.by_wd.remove(&wd);
                s.by_path.remove(&path);
                crate::listing::mark_stale(&path);
            }
        }
        let flags =
            WatchFlags::CREATE | WatchFlags::DELETE | WatchFlags::MOVED_FROM | WatchFlags::MOVED_TO | WatchFlags::MODIFY | WatchFlags::ATTRIB | WatchFlags::DELETE_SELF | WatchFlags::MOVE_SELF | WatchFlags::ONLYDIR;
        match inotify::inotify_add_watch(&s.fd, &l.path, flags) {
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
            let _ = inotify::inotify_remove_watch(&s.fd, wd);
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
            s.by_wd.remove(&wd);
        }
        drop(s);
        crate::listing::mark_stale(path);
    }
}

#[cfg(not(target_os = "linux"))]
mod imp {
    use super::*;
    pub fn watch(_l: &Arc<Listing>) {}
    pub fn unwatch(_l: &Arc<Listing>) {}
}

pub use imp::{unwatch, watch};
