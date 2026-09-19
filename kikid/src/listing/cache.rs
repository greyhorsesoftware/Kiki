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
            drop(inner);
            if stale {
                l.rescan();
            }
            return Ok((l, !stale));
        }
    }
    let dir: Box<dyn Source> = if uri.is_local() || trash {
        Box::new(DirHandle::open(&path)?)
    } else {
        let (session, rpath) = crate::locations::resolve(uri)?;
        Box::new(crate::vfs::remote::RemoteDir { session, path: rpath, cancel: Arc::new(std::sync::atomic::AtomicBool::new(false)) })
    };
    let listing = Arc::new(Listing {
        uri: uri.clone(),
        path: path.clone(),
        dir,
        inner: Mutex::new(Inner {
            pool: StringPool::with_capacity(256),
            meta: Vec::new(),
            queued: Vec::new(),
            thumb: HashMap::new(),
            thumb_queued: Vec::new(),
            git: HashMap::new(),
            git_done: false,
            scan_done: false,
            scan_error: None,
            view: Vec::new(),
            pos: Vec::new(),
            sort: (SortRole::Name, true),
            sorted: false,
            filter: None,
            show_hidden: crate::config::settings().get("view").and_then(|v| v.get("showHidden")).and_then(Value::as_bool).unwrap_or(false),
            uri_string: uri.to_string(),
            generation: 0,
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
            c.entries = c.entries.saturating_sub(n);
        }
    }
}

pub(super) fn key_of(path: &std::path::Path) -> String {
    Uri::from_path(path).to_string()
}

pub fn mark_stale(path: &std::path::Path) {
    if let Some(l) = cache().lock().unwrap().map.get(&key_of(path)).cloned() {
        l.inner.lock().unwrap().stale = true;
    }
}

/// The directory itself was deleted or moved away: mark the listing stale for the windows that
/// are still on it, and take it out of the cache so the next open builds a fresh handle.
pub fn gone(path: &std::path::Path) {
    mark_stale(path);
    forget(&Uri::from_path(path));
}

pub fn find(path: &std::path::Path) -> Option<Arc<Listing>> {
    cache().lock().unwrap().map.get(&key_of(path)).cloned()
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
