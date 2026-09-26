//! The listing cache: one `Listing` per URI, evicted by age once it holds too many rows.

use super::*;

pub(super) struct Cache {
    pub(super) map: HashMap<String, Arc<Listing>>,
    pub(super) entries: usize,
}

pub(super) fn cache() -> &'static Mutex<Cache> {
    static CACHE: OnceLock<Mutex<Cache>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(Cache { map: HashMap::new(), entries: 0 }))
}

/// Opens (or reuses) the listing for a local path. `cached` tells whether it was served from memory.
/// Drop a cached listing so the next open reads the directory again (benchmarks, tests).
pub fn forget(uri: &Uri) {
    let mut c = cache().lock().unwrap();
    if let Some(l) = c.map.remove(&uri.to_string()) {
        let n = l.inner.lock().unwrap().pool.len();
        c.entries = c.entries.saturating_sub(n);
    }
}

pub fn open(uri: &Uri) -> Result<(Arc<Listing>, bool)> {
    let key = uri.to_string();
    let trash = uri.scheme == "trash";
    let path = if uri.is_local() {
        uri.to_path()
    } else if trash {
        let p = crate::ops::trash_dir().join("files").join(uri.path.trim_start_matches('/'));
        let _ = std::fs::create_dir_all(&p);
        let _ = std::fs::create_dir_all(crate::ops::trash_dir().join("info"));
        p
    } else {
        PathBuf::from(&key)
    };
    // The guard has to be let go before anything below takes the cache lock again: an `if let`
    // scrutinee lives to the end of its block, and `forget` would deadlock on it.
    let hit = cache().lock().unwrap().map.get(&key).cloned();
    if let Some(l) = hit {
        // A directory deleted and recreated under the same name is a different directory, and
        // the cached listing holds a handle on the old one: rescanning it reads the dead inode
        // and reports an empty folder for ever. Drop it and open the new one.
        if !l.dir.still_at(&l.path) {
            forget(uri);
        } else {
            let mut inner = l.inner.lock().unwrap();
            inner.last_used = Instant::now();
            let stale = inner.stale;
            // A folder of projects, opened again: an edit inside one of them moved neither its
            // `HEAD` nor its index, so the poll below cannot have seen it. Showing the folder is
            // the moment to ask again — and for a folder with no projects in it, nothing at all.
            let projects = !inner.deco.repo_names().is_empty();
            drop(inner);
            if projects && !stale {
                l.repo_rows(None);
            }
            if stale {
                l.rescan();
                // Stale is what an evicted watch leaves behind (`watch::watch` marks it so), and
                // a folder that is opened again wants one again: without this it was re-listed
                // once and then left dead, showing whatever it had at that moment for as long as
                // the window stayed on it.
                if l.dir.watchable() {
                    crate::watch::watch(&l);
                }
            }
            return Ok((l, !stale));
        }
    }
    let dir: Box<dyn Source> = if uri.is_local() || trash {
        Box::new(DirHandle::open(&path)?)
    } else {
        // Not connected here: the scan thread connects, so `Open` answers at once and the other
        // pane is not kept waiting behind a slow server (see `RemoteDir`).
        Box::new(crate::vfs::remote::RemoteDir::lazy(uri.clone()))
    };
    let listing = Arc::new(Listing {
        uri: uri.clone(),
        path: path.clone(),
        dir,
        inner: Mutex::new(Inner {
            pool: StringPool::with_capacity(256),
            meta: Vec::new(),
            queued: Vec::new(),
            deco: Default::default(),
            git_done: false,
            scan_done: false,
            scan_error: None,
            scan_said: None,
            view: Vec::new(),
            pos: Vec::new(),
            sort: (SortRole::Name, true),
            sorted: false,
            filter: None,
            show_hidden: crate::config::settings().get("view").and_then(|v| v.get("showHidden")).and_then(Value::as_bool).unwrap_or(false),
            uri_string: uri.to_string(),
            generation: 0,
            epoch: 0,
            enrich: None,
            subscribers: Vec::new(),
            stale: false,
            last_used: Instant::now(),
        }),
    });
    {
        let mut c = cache().lock().unwrap();
        c.map.insert(key, Arc::clone(&listing));
    }
    let l2 = Arc::clone(&listing);
    thread::Builder::new().name("scan".into()).spawn(move || l2.scan()).expect("spawn scanner");
    if listing.dir.watchable() {
        crate::watch::watch(&listing);
    }
    Ok((listing, false))
}

/// Drops entries from the cache until it fits, never evicting a listing with subscribers.
pub(super) fn evict_if_needed() {
    let mut c = cache().lock().unwrap();
    if c.entries <= CACHE_ENTRIES {
        return;
    }
    let mut candidates: Vec<(Instant, String, usize)> = c
        .map
        .iter()
        .filter_map(|(k, l)| {
            let i = l.inner.lock().unwrap();
            if i.subscribers.is_empty() {
                Some((i.last_used, k.clone(), i.pool.len()))
            } else {
                None
            }
        })
        .collect();
    candidates.sort();
    for (_, k, n) in candidates {
        if c.entries <= CACHE_ENTRIES {
            break;
        }
        if let Some(l) = c.map.remove(&k) {
            if l.dir.watchable() {
                crate::watch::unwatch(&l);
            }
            // Its git status was kept for this listing's rows; nobody is showing them now.
            if l.uri.is_local() {
                crate::git::invalidate(&l.path);
            }
            c.entries = c.entries.saturating_sub(n);
        }
    }
}

pub(super) fn key_of(path: &std::path::Path) -> String {
    Uri::from_path(path).to_string()
}

/// The listing showing `path`. The cache is keyed by URI, so the `file://` key answers for every
/// ordinary folder; the trash is the one listing whose URI is not its path — `trash:///` over
/// `<trash>/files` — and without the second look no watch event ever reached it. That was worth
/// a user's whole session: empty the trash, or trash another file with the trash view open, and
/// it went on showing what it held when it was first opened, for as long as the daemon lived,
/// because nothing marked it stale either.
fn at_path(path: &std::path::Path) -> Option<Arc<Listing>> {
    let c = cache().lock().unwrap();
    c.map.get(&key_of(path)).cloned().or_else(|| c.map.values().find(|l| l.path == path).cloned())
}

pub fn mark_stale(path: &std::path::Path) {
    if let Some(l) = at_path(path) {
        l.inner.lock().unwrap().stale = true;
    }
}

/// The directory itself was deleted or moved away: mark the listing stale for the windows that
/// are still on it, and take it out of the cache so the next open builds a fresh handle.
pub fn gone(path: &std::path::Path) {
    mark_stale(path);
    // The windows still showing it are told, so they can say the folder is gone rather than go
    // on showing rows for files that are not there (`WindowCache` has always listened for this).
    let hit = find(path);
    if let Some(l) = &hit {
        let subs = l.inner.lock().unwrap().subscribers.clone();
        for s in subs {
            let _ = s.tx.send(crate::proto::event("Gone").u("lid", s.lid).done());
        }
    }
    // Forgotten under its own URI, which for the trash is not the path's.
    forget(&hit.map(|l| l.uri.clone()).unwrap_or_else(|| Uri::from_path(path)));
}

/// Does `path` still lead to the folder this listing was opened on? False once it has been
/// deleted, or moved away and another put in its place.
pub fn still_there(path: &std::path::Path) -> bool {
    match find(path) {
        Some(l) => l.dir.still_at(&l.path),
        None => true,
    }
}

/// Every local listing in memory at or under `root`: what a change to a repository's state
/// (a commit, a checkout, a `git add`) can have made wrong.
pub fn under(root: &std::path::Path) -> Vec<Arc<Listing>> {
    cache().lock().unwrap().map.values().filter(|l| l.uri.is_local() && l.path.starts_with(root)).cloned().collect()
}

pub fn find(path: &std::path::Path) -> Option<Arc<Listing>> {
    at_path(path)
}

/// Repository-root rows have no watch of their own (plan 15). A folder of forty projects would
/// be forty watches and there are sixty-four for everything (`watch::MAX_WATCHES`), so their
/// `.git` is stat'd instead: two files apiece, once a second, and only for folders somebody is
/// looking at. A project whose `HEAD` or index has moved — a commit, a checkout, a `git add`
/// made in a terminal — has its row worked out again. Called from the watcher's sweep.
pub fn poll_repo_rows() {
    let live: Vec<Arc<Listing>> = cache().lock().unwrap().map.values().filter(|l| l.uri.is_local()).cloned().collect();
    for l in live {
        let names = {
            let inner = l.inner.lock().unwrap();
            if inner.subscribers.is_empty() {
                continue;
            }
            inner.deco.repo_names()
        };
        // Stat'd with nothing held: a repository on a slow filesystem must not hold the listing.
        let moved: Vec<Vec<u8>> = names.into_iter().filter(|n| crate::git::moved(&l.path.join(OsStr::from_bytes(n)))).collect();
        if !moved.is_empty() {
            l.repo_rows(Some(moved));
        }
    }
}

/// A job changed what is in `dir`. A local folder is watched and finds out by itself; a server
/// tells nobody, so a listing of it that a window has open is read again — in place, so the
/// windows showing it keep their subscription and see the rows change.
pub fn changed(dir: &Uri) {
    if dir.is_local() {
        return;
    }
    let hit = cache().lock().unwrap().map.get(&dir.to_string()).cloned();
    if let Some(l) = hit {
        l.rescan();
    }
}

/// Drops every cached listing of a remote location (after a job wrote there, or a disconnect).
pub fn invalidate_authority(scheme: &str, authority: &str) {
    let prefix = format!("{scheme}://{authority}");
    let mut c = cache().lock().unwrap();
    let keys: Vec<String> = c.map.keys().filter(|k| k.starts_with(&prefix)).cloned().collect();
    for k in keys {
        if let Some(l) = c.map.remove(&k) {
            let n = l.inner.lock().unwrap().pool.len();
            c.entries = c.entries.saturating_sub(n);
        }
    }
}
