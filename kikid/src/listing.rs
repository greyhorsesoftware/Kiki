//! Listings: two-phase virtualised readdir, windows, sort and filter, cache.

use crate::json::{Obj, Value};
use crate::proto;
use crate::string_pool::StringPool;
use crate::vfs::local::{self, DirHandle};
use crate::vfs::uri::Uri;
use crate::vfs::{EntryType, Meta, Result, Source, VfsError};
use std::collections::HashMap;
use std::ffi::OsStr;
use std::os::unix::ffi::OsStrExt;
use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex, OnceLock};
use std::thread;
use std::time::Instant;

pub const SMALL_DIR: usize = 2_000; // fully enriched right after phase 1
pub const WINDOW_MAX: u32 = 512;
pub const CACHE_ENTRIES: usize = 500_000;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SortRole {
    Name,
    Kind,
    Size,
    Mtime,
}

impl SortRole {
    pub fn parse(s: &str) -> Option<SortRole> {
        Some(match s {
            "name" => SortRole::Name,
            "kind" => SortRole::Kind,
            "size" => SortRole::Size,
            "mtime" => SortRole::Mtime,
            _ => return None,
        })
    }
    fn needs_meta(self) -> bool {
        matches!(self, SortRole::Size | SortRole::Mtime)
    }
}

/// Someone watching a listing: a client connection's live window for one `lid`.
#[derive(Clone)]
pub struct Subscriber {
    pub client: u64,
    pub lid: u64,
    pub tx: Sender<Value>,
    pub first: u32,
    pub count: u32,
}

impl Subscriber {
    fn covers(&self, pos: u32) -> bool {
        pos >= self.first && pos < self.first + self.count
    }
}

struct Inner {
    pool: StringPool,
    meta: Vec<Option<Meta>>,
    queued: Vec<bool>,
    thumb: Vec<Option<String>>,
    thumb_queued: Vec<bool>,
    git: Vec<Option<crate::git::Entry>>,
    git_done: bool,
    scan_done: bool,
    scan_error: Option<String>,
    /// Display order after sort and filter; index into the pool.
    view: Vec<u32>,
    /// Inverse of `view`: pool index -> position, or u32::MAX when hidden.
    pos: Vec<u32>,
    sort: (SortRole, bool),
    sorted: bool,
    filter: Option<Vec<u8>>,
    generation: u64,
    enrich: Option<Enrich>,
    subscribers: Vec<Subscriber>,
    pub stale: bool,
    last_used: Instant,
}

struct Enrich {
    total: u32,
    done: u32,
    waiters: Vec<(Sender<Value>, u64)>,
}

pub struct Listing {
    pub uri: Uri,
    /// Local path for local listings; for remote ones the URI string (used as the cache key).
    pub path: PathBuf,
    dir: Box<dyn Source>,
    inner: Mutex<Inner>,
}

// ---------------------------------------------------------------- stat pool

struct StatJob {
    listing: Arc<Listing>,
    rows: Vec<u32>,
    low_priority: bool,
}

struct StatPool {
    tx: Sender<StatJob>,
}

fn stat_pool() -> &'static StatPool {
    static POOL: OnceLock<StatPool> = OnceLock::new();
    POOL.get_or_init(|| {
        let (tx, rx) = mpsc::channel::<StatJob>();
        let rx = Arc::new(Mutex::new(rx));
        let n = thread::available_parallelism().map(|n| n.get()).unwrap_or(2).clamp(2, 8);
        for i in 0..n {
            let rx = Arc::clone(&rx);
            thread::Builder::new()
                .name(format!("stat-{i}"))
                .spawn(move || loop {
                    let job = { rx.lock().unwrap().recv() };
                    match job {
                        Ok(job) => job.listing.run_stats(job.rows, job.low_priority),
                        Err(_) => return,
                    }
                })
                .expect("spawn stat worker");
        }
        StatPool { tx }
    })
}

// ---------------------------------------------------------------- cache

struct Cache {
    map: HashMap<String, Arc<Listing>>,
    entries: usize,
}

fn cache() -> &'static Mutex<Cache> {
    static CACHE: OnceLock<Mutex<Cache>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(Cache { map: HashMap::new(), entries: 0 }))
}

/// Opens (or reuses) the listing for a local path. `cached` tells whether it was served from memory.
pub fn open(uri: &Uri) -> Result<(Arc<Listing>, bool)> {
    let key = uri.to_string();
    let path = if uri.is_local() { uri.to_path() } else { PathBuf::from(&key) };
    if let Some(l) = cache().lock().unwrap().map.get(&key).cloned() {
        let mut inner = l.inner.lock().unwrap();
        inner.last_used = Instant::now();
        let stale = inner.stale;
        drop(inner);
        if stale {
            l.rescan();
        }
        return Ok((l, !stale));
    }
    let dir: Box<dyn Source> = if uri.is_local() {
        Box::new(DirHandle::open(&path)?)
    } else {
        let (session, rpath) = crate::locations::resolve(uri)?;
        Box::new(crate::vfs::remote::RemoteDir { session, path: rpath })
    };
    let listing = Arc::new(Listing {
        uri: uri.clone(),
        path: path.clone(),
        dir,
        inner: Mutex::new(Inner {
            pool: StringPool::with_capacity(256),
            meta: Vec::new(),
            queued: Vec::new(),
            thumb: Vec::new(),
            thumb_queued: Vec::new(),
            git: Vec::new(),
            git_done: false,
            scan_done: false,
            scan_error: None,
            view: Vec::new(),
            pos: Vec::new(),
            sort: (SortRole::Name, true),
            sorted: false,
            filter: None,
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
fn evict_if_needed() {
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

fn key_of(path: &std::path::Path) -> String {
    Uri::from_path(path).to_string()
}

pub fn mark_stale(path: &std::path::Path) {
    if let Some(l) = cache().lock().unwrap().map.get(&key_of(path)).cloned() {
        l.inner.lock().unwrap().stale = true;
    }
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

// ---------------------------------------------------------------- listing

impl Listing {
    /// Phase 1: enumerate names and kinds into the pool, publishing counts as chunks land.
    fn scan(self: &Arc<Self>) {
        let start = Instant::now();
        let result = self.dir.scan(&mut |chunk| {
            let mut inner = self.inner.lock().unwrap();
            for e in chunk {
                let idx = inner.pool.push(e.name.as_bytes(), e.kind);
                inner.meta.push(e.meta);
                inner.queued.push(false);
                inner.thumb.push(None);
                inner.thumb_queued.push(false);
                inner.git.push(None);
                inner.pos.push(u32::MAX);
                if inner.filter.is_none() {
                    inner.view.push(idx);
                    let p = inner.view.len() as u32 - 1;
                    inner.pos[idx as usize] = p;
                }
            }
            let n = inner.view.len() as u64;
            let subs = inner.subscribers.clone();
            drop(inner);
            for s in subs {
                let _ = s.tx.send(proto::event("Count").u("lid", s.lid).u("n", n).b("done", false).done());
            }
        });
        let mut inner = self.inner.lock().unwrap();
        inner.scan_done = true;
        if let Err(e) = result {
            inner.scan_error = Some(e.message());
        }
        let total = inner.pool.len();
        {
            let mut c = cache().lock().unwrap();
            c.entries += total;
        }
        let elapsed = start.elapsed();
        inner.rebuild_view();
        inner.generation += 1;
        let n = inner.view.len() as u64;
        let subs = inner.subscribers.clone();
        drop(inner);
        for s in &subs {
            let _ = s.tx.send(proto::event("Count").u("lid", s.lid).u("n", n).b("done", true).done());
            let _ = s.tx.send(proto::event("Reset").u("lid", s.lid).u("n", n).done());
        }
        if std::env::var_os("KIKI_TRACE").is_some() {
            eprintln!("scan {} entries in {:?}", total, elapsed);
        }
        if total <= SMALL_DIR {
            self.enrich_all(true);
        }
        self.git_status();
        evict_if_needed();
    }

    /// Git badges for a local directory inside a repository (plan 15), on a worker, pushed as rows.
    fn git_status(self: &Arc<Self>) {
        if !self.uri.is_local() || !crate::git::enabled() || crate::git::repo_root(&self.path).is_none() {
            return;
        }
        let me = Arc::clone(self);
        thread::Builder::new()
            .name("git".into())
            .spawn(move || {
                crate::git::invalidate(&me.path);
                if crate::git::status(&me.path).is_none() {
                    return;
                }
                let names: Vec<(u32, String, bool)> = {
                    let inner = me.inner.lock().unwrap();
                    (0..inner.pool.len() as u32).map(|i| (i, String::from_utf8_lossy(inner.pool.name(i)).into_owned(), inner.pool.entry_type(i) == EntryType::Dir)).collect()
                };
                let mut changed = Vec::new();
                {
                    let mut inner = me.inner.lock().unwrap();
                    for (i, name, is_dir) in names {
                        if (i as usize) >= inner.git.len() {
                            break;
                        }
                        let e = crate::git::state_for(&me.path, &name, is_dir);
                        if e.as_ref().map(|e| e.state != crate::git::State::Clean).unwrap_or(false) {
                            changed.push(i);
                        }
                        inner.git[i as usize] = e;
                    }
                    inner.git_done = true;
                }
                me.push_rows(&changed);
                let subs = me.inner.lock().unwrap().subscribers.clone();
                for s in subs {
                    let _ = s.tx.send(proto::event("RepoChanged").s("root", Uri::from_path(&crate::git::repo_root(&me.path).unwrap_or_default()).to_string()).u("lid", s.lid).done());
                }
            })
            .expect("spawn git");
    }

    /// Re-enumerate after a change, keeping metadata for names that still exist.
    pub fn rescan(self: &Arc<Self>) {
        let (old_pool_names, old_meta) = {
            let inner = self.inner.lock().unwrap();
            let names: Vec<Vec<u8>> = (0..inner.pool.len() as u32).map(|i| inner.pool.name(i).to_vec()).collect();
            (names, inner.meta.clone())
        };
        let old: HashMap<Vec<u8>, Option<Meta>> = old_pool_names.into_iter().zip(old_meta).collect();
        let mut pool = StringPool::with_capacity(old.len());
        let mut meta = Vec::with_capacity(old.len());
        let result = self.dir.scan(&mut |chunk| {
            for e in chunk {
                pool.push(e.name.as_bytes(), e.kind);
                meta.push(e.meta.or_else(|| old.get(e.name.as_bytes()).cloned().flatten()));
            }
        });
        let mut inner = self.inner.lock().unwrap();
        let old_total = inner.pool.len();
        let n = pool.len();
        inner.queued = vec![false; n];
        inner.thumb = vec![None; n];
        inner.thumb_queued = vec![false; n];
        inner.git = vec![None; n];
        inner.git_done = false;
        inner.pos = vec![u32::MAX; n];
        inner.pool = pool;
        inner.meta = meta;
        inner.scan_done = true;
        inner.scan_error = result.err().map(|e| e.message());
        inner.stale = false;
        inner.sorted = false;
        inner.rebuild_view();
        inner.generation += 1;
        let count = inner.view.len() as u64;
        let subs = inner.subscribers.clone();
        drop(inner);
        {
            let mut c = cache().lock().unwrap();
            c.entries = c.entries.saturating_sub(old_total) + n;
        }
        for s in &subs {
            let _ = s.tx.send(proto::event("Count").u("lid", s.lid).u("n", count).b("done", true).done());
            let _ = s.tx.send(proto::event("Reset").u("lid", s.lid).u("n", count).done());
        }
    }

    pub fn subscribe(&self, sub: Subscriber) {
        let mut inner = self.inner.lock().unwrap();
        inner.subscribers.retain(|s| !(s.client == sub.client && s.lid == sub.lid));
        inner.subscribers.push(sub);
        inner.last_used = Instant::now();
    }

    pub fn unsubscribe(&self, client: u64, lid: u64) {
        let mut inner = self.inner.lock().unwrap();
        inner.subscribers.retain(|s| !(s.client == client && s.lid == lid));
        inner.last_used = Instant::now();
    }

    pub fn count(&self) -> (u64, bool) {
        let inner = self.inner.lock().unwrap();
        (inner.view.len() as u64, inner.scan_done)
    }

    pub fn error(&self) -> Option<String> {
        self.inner.lock().unwrap().scan_error.clone()
    }

    /// Answer a window immediately with what is known and queue stats for the rest.
    pub fn window(self: &Arc<Self>, client: u64, lid: u64, first: u32, count: u32) -> Value {
        let count = count.min(WINDOW_MAX);
        let mut inner = self.inner.lock().unwrap();
        inner.last_used = Instant::now();
        if let Some(s) = inner.subscribers.iter_mut().find(|s| s.client == client && s.lid == lid) {
            s.first = first;
            s.count = count;
        }
        let n = inner.view.len() as u32;
        let end = first.saturating_add(count).min(n);
        let mut rows = Vec::new();
        let mut missing: Vec<(u32, u32)> = Vec::new(); // (distance from centre, pool idx)
        let centre = first + count / 2;
        for p in first.min(n)..end {
            let idx = inner.view[p as usize];
            rows.push(inner.row_json(idx));
            if inner.meta[idx as usize].is_none() && !inner.queued[idx as usize] {
                inner.queued[idx as usize] = true;
                missing.push((p.abs_diff(centre), idx));
            }
        }
        let done = inner.scan_done;
        drop(inner);
        if !missing.is_empty() {
            missing.sort_unstable();
            let rows: Vec<u32> = missing.into_iter().map(|(_, i)| i).collect();
            let _ = stat_pool().tx.send(StatJob { listing: Arc::clone(self), rows, low_priority: false });
        }
        Value::obj().u("first", first as u64).u("n", n as u64).b("done", done).v("rows", Value::Arr(rows)).done()
    }

    /// Run on a stat worker: fetch metadata for rows, then push changed rows into live windows.
    fn run_stats(self: &Arc<Self>, rows: Vec<u32>, low_priority: bool) {
        let mut done: Vec<u32> = Vec::with_capacity(rows.len());
        for idx in rows {
            // Skip rows that scrolled out of every live window (unless enriching everything).
            let name = {
                let inner = self.inner.lock().unwrap();
                if !low_priority {
                    let p = inner.pos[idx as usize];
                    if p == u32::MAX || !inner.subscribers.iter().any(|s| s.covers(p)) {
                        continue;
                    }
                }
                inner.pool.name(idx).to_vec()
            };
            let res = self.dir.stat_child(OsStr::from_bytes(&name));
            let mut inner = self.inner.lock().unwrap();
            inner.queued[idx as usize] = false;
            match res {
                Ok((m, t)) => {
                    if inner.pool.entry_type(idx) == EntryType::Unknown {
                        inner.pool.set_entry_type(idx, t);
                    }
                    inner.meta[idx as usize] = Some(m);
                }
                Err(_) => {
                    inner.meta[idx as usize] = Some(Meta::default());
                }
            }
            done.push(idx);
            if let Some(e) = inner.enrich.as_mut() {
                if low_priority {
                    e.done += 1;
                }
            }
            // Thumbnails only for rows inside a live window, at low priority.
            let kind = inner.pool.kind(idx);
            let p = inner.pos[idx as usize];
            let visible = p != u32::MAX && inner.subscribers.iter().any(|s| s.covers(p));
            if visible && crate::thumbs::thumbable(kind) && inner.thumb[idx as usize].is_none() && !inner.thumb_queued[idx as usize] {
                inner.thumb_queued[idx as usize] = true;
                let mtime = inner.meta[idx as usize].as_ref().map(|m| m.mtime_ms).unwrap_or(0);
                let uri = self.uri.join(&String::from_utf8_lossy(&name));
                let me = Arc::clone(self);
                drop(inner);
                crate::thumbs::submit(crate::thumbs::ThumbJob { uri, kind, mtime_ms: mtime, size: crate::thumbs::Size::Normal, done: Box::new(move |path| {
                    {
                        let mut inner = me.inner.lock().unwrap();
                        if (idx as usize) < inner.thumb.len() {
                            inner.thumb[idx as usize] = Some(path.map(|p| p.to_string_lossy().into_owned()).unwrap_or_default());
                            inner.thumb_queued[idx as usize] = false;
                        }
                    }
                    me.push_rows(&[idx]);
                }) });
                continue;
            }
        }
        self.push_rows(&done);
        if low_priority {
            self.enrich_progress();
        }
    }

    /// Send `Rows` events for the given pool rows to every subscriber whose window covers them.
    fn push_rows(&self, changed: &[u32]) {
        if changed.is_empty() {
            return;
        }
        let inner = self.inner.lock().unwrap();
        for s in &inner.subscribers {
            let mut positions: Vec<u32> = changed.iter().map(|&i| inner.pos[i as usize]).filter(|&p| p != u32::MAX && s.covers(p)).collect();
            if positions.is_empty() {
                continue;
            }
            positions.sort_unstable();
            // Send contiguous runs as one event each.
            let mut run_start = positions[0];
            let mut prev = positions[0];
            let flush = |start: u32, end: u32| {
                let rows: Vec<Value> = (start..=end).map(|p| inner.row_json(inner.view[p as usize])).collect();
                let _ = s.tx.send(proto::event("Rows").u("lid", s.lid).u("first", start as u64).v("rows", Value::Arr(rows)).done());
            };
            for &p in &positions[1..] {
                if p != prev + 1 {
                    flush(run_start, prev);
                    run_start = p;
                }
                prev = p;
            }
            flush(run_start, prev);
        }
    }

    pub fn sort(self: &Arc<Self>, role: SortRole, asc: bool, waiter: Option<(Sender<Value>, u64)>) -> u64 {
        let mut inner = self.inner.lock().unwrap();
        inner.sort = (role, asc);
        inner.sorted = false;
        if role.needs_meta() && inner.meta.iter().any(Option::is_none) {
            // Sort is applied when enrichment completes.
            drop(inner);
            self.enrich_all(true);
            if let Some(w) = waiter {
                let mut inner = self.inner.lock().unwrap();
                if let Some(e) = inner.enrich.as_mut() {
                    e.waiters.push(w);
                } else {
                    let _ = w.0.send(proto::ok(w.1, Value::obj().done()));
                }
            }
            return self.count().0;
        }
        inner.rebuild_view();
        inner.generation += 1;
        let n = inner.view.len() as u64;
        let subs = inner.subscribers.clone();
        drop(inner);
        for s in subs {
            let _ = s.tx.send(proto::event("Reset").u("lid", s.lid).u("n", n).done());
        }
        if let Some(w) = waiter {
            let _ = w.0.send(proto::ok(w.1, Value::obj().u("n", n).done()));
        }
        n
    }

    pub fn filter(&self, text: &str) -> u64 {
        let mut inner = self.inner.lock().unwrap();
        inner.filter = if text.is_empty() { None } else { Some(text.to_ascii_lowercase().into_bytes()) };
        inner.rebuild_view();
        inner.generation += 1;
        let n = inner.view.len() as u64;
        let subs = inner.subscribers.clone();
        drop(inner);
        for s in subs {
            let _ = s.tx.send(proto::event("Reset").u("lid", s.lid).u("n", n).done());
        }
        n
    }

    /// Stat every row at low priority; replies to `waiter` when complete.
    pub fn enrich(self: &Arc<Self>, waiter: Option<(Sender<Value>, u64)>) {
        let complete = {
            let mut inner = self.inner.lock().unwrap();
            let complete = inner.meta.iter().all(Option::is_some);
            if !complete {
                if let Some(w) = waiter.clone() {
                    match inner.enrich.as_mut() {
                        Some(e) => e.waiters.push(w),
                        None => {
                            inner.enrich = Some(Enrich { total: 0, done: 0, waiters: vec![w] });
                        }
                    }
                }
            }
            complete
        };
        if complete {
            if let Some(w) = waiter {
                let _ = w.0.send(proto::ok(w.1, Value::obj().done()));
            }
            return;
        }
        self.enrich_all(true);
    }

    fn enrich_all(self: &Arc<Self>, low_priority: bool) {
        let rows: Vec<u32> = {
            let mut inner = self.inner.lock().unwrap();
            let rows: Vec<u32> = inner.view.iter().copied().filter(|&i| inner.meta[i as usize].is_none() && !inner.queued[i as usize]).collect();
            for &i in &rows {
                inner.queued[i as usize] = true;
            }
            let total = inner.meta.iter().filter(|m| m.is_none()).count() as u32;
            match inner.enrich.as_mut() {
                Some(e) => {
                    e.total = e.done + total;
                }
                None => inner.enrich = Some(Enrich { total, done: 0, waiters: Vec::new() }),
            }
            rows
        };
        if rows.is_empty() {
            self.enrich_progress();
            return;
        }
        for batch in rows.chunks(256) {
            let _ = stat_pool().tx.send(StatJob { listing: Arc::clone(self), rows: batch.to_vec(), low_priority });
        }
    }

    fn enrich_progress(self: &Arc<Self>) {
        let mut inner = self.inner.lock().unwrap();
        let Some(e) = inner.enrich.as_ref() else { return };
        let (done, total) = (e.done, e.total);
        let complete = inner.meta.iter().all(Option::is_some);
        let subs = inner.subscribers.clone();
        if !complete {
            drop(inner);
            for s in subs {
                let _ = s.tx.send(proto::event("Progress").u("lid", s.lid).u("done", done as u64).u("total", total as u64).done());
            }
            return;
        }
        let e = inner.enrich.take().unwrap();
        let needs_resort = inner.sort.0.needs_meta() && !inner.sorted;
        if needs_resort {
            inner.rebuild_view();
            inner.generation += 1;
        }
        let n = inner.view.len() as u64;
        drop(inner);
        for s in &subs {
            if needs_resort {
                let _ = s.tx.send(proto::event("Reset").u("lid", s.lid).u("n", n).done());
            }
        }
        for (tx, id) in e.waiters {
            let _ = tx.send(proto::ok(id, Value::obj().u("n", n).done()));
        }
    }

    pub fn stat_uri(uri: &Uri) -> Result<Value> {
        if !uri.is_local() {
            let (session, rpath) = crate::locations::resolve(uri)?;
            let v = session.plugin.request(Value::obj().s("type", "Stat").s("location", session.location.clone()).s("path", rpath).done())?;
            return Ok(meta_json(&crate::vfs::remote::meta_from(&v)));
        }
        let path = uri.to_path();
        let parent = path.parent().ok_or(VfsError::NotFound)?;
        let name = path.file_name().ok_or(VfsError::NotFound)?;
        let dir = DirHandle::open(parent)?;
        let (m, _) = dir.stat_child(name)?;
        Ok(meta_json(&m))
    }
}

impl Inner {
    fn rebuild_view(&mut self) {
        let n = self.pool.len() as u32;
        let mut view: Vec<u32> = match &self.filter {
            None => (0..n).collect(),
            Some(f) => (0..n).filter(|&i| self.pool.name_contains(i, f)).collect(),
        };
        if self.scan_done {
            let (role, asc) = self.sort;
            let pool = &self.pool;
            let meta = &self.meta;
            // Folders first, then the role, then name as a tiebreak.
            let dir_rank = |i: u32| u8::from(pool.entry_type(i) != EntryType::Dir);
            match role {
                SortRole::Name => view.sort_unstable_by(|&a, &b| dir_rank(a).cmp(&dir_rank(b)).then_with(|| pool.key(a).cmp(pool.key(b)))),
                SortRole::Kind => view.sort_unstable_by(|&a, &b| dir_rank(a).cmp(&dir_rank(b)).then_with(|| (pool.kind(a) as u8).cmp(&(pool.kind(b) as u8))).then_with(|| pool.key(a).cmp(pool.key(b)))),
                SortRole::Size => view.sort_unstable_by(|&a, &b| {
                    let sa = meta[a as usize].as_ref().map(|m| m.size).unwrap_or(0);
                    let sb = meta[b as usize].as_ref().map(|m| m.size).unwrap_or(0);
                    dir_rank(a).cmp(&dir_rank(b)).then_with(|| sa.cmp(&sb)).then_with(|| pool.key(a).cmp(pool.key(b)))
                }),
                SortRole::Mtime => view.sort_unstable_by(|&a, &b| {
                    let ta = meta[a as usize].as_ref().map(|m| m.mtime_ms).unwrap_or(0);
                    let tb = meta[b as usize].as_ref().map(|m| m.mtime_ms).unwrap_or(0);
                    dir_rank(a).cmp(&dir_rank(b)).then_with(|| ta.cmp(&tb)).then_with(|| pool.key(a).cmp(pool.key(b)))
                }),
            }
            if !asc {
                // Keep folders first even when descending.
                let split = view.iter().position(|&i| pool.entry_type(i) != EntryType::Dir).unwrap_or(view.len());
                view[..split].reverse();
                view[split..].reverse();
            }
            self.sorted = true;
        }
        self.pos.clear();
        self.pos.resize(n as usize, u32::MAX);
        for (p, &i) in view.iter().enumerate() {
            self.pos[i as usize] = p as u32;
        }
        self.view = view;
    }

    fn row_json(&self, idx: u32) -> Value {
        let t = self.pool.entry_type(idx);
        let name = String::from_utf8_lossy(self.pool.name(idx)).into_owned();
        let meta = match &self.meta[idx as usize] {
            Some(m) => meta_json(m),
            None => Value::Null,
        };
        Value::obj()
            .s("name", name)
            .s("kind", self.pool.kind(idx).as_str())
            .b("isDir", t == EntryType::Dir)
            .b("isLink", t == EntryType::Link)
            .v("meta", meta)
            .v("thumb", match &self.thumb[idx as usize] { Some(p) => Value::Str(p.clone()), None => Value::Null })
            .v("git", match &self.git[idx as usize] { Some(e) => crate::git::entry_json(e), None => Value::Null })
            .done()
    }
}

pub fn meta_json(m: &Meta) -> Value {
    let owner = m.uid.and_then(names::user);
    let group = m.gid.and_then(names::group);
    let o: Obj = Value::obj().u("size", m.size).u("mtime", m.mtime_ms);
    let o = match m.mode {
        Some(mode) => o.u("mode", mode as u64),
        None => o.v("mode", Value::Null),
    };
    o.opt_s("owner", owner.as_deref()).opt_s("group", group.as_deref()).v("digest", Value::Null).done()
}

mod names {
    use std::collections::HashMap;
    use std::sync::{Mutex, OnceLock};

    fn cache() -> &'static Mutex<(HashMap<u32, Option<String>>, HashMap<u32, Option<String>>)> {
        static C: OnceLock<Mutex<(HashMap<u32, Option<String>>, HashMap<u32, Option<String>>)>> = OnceLock::new();
        C.get_or_init(|| Mutex::new((HashMap::new(), HashMap::new())))
    }

    pub fn user(uid: u32) -> Option<String> {
        let mut c = cache().lock().unwrap();
        c.0.entry(uid).or_insert_with(|| super::local::user_name(uid)).clone()
    }

    pub fn group(gid: u32) -> Option<String> {
        let mut c = cache().lock().unwrap();
        c.1.entry(gid).or_insert_with(|| super::local::group_name(gid)).clone()
    }
}

pub fn iso(ms: u64) -> String {
    if ms == 0 {
        return "unknown".into();
    }
    let secs = (ms / 1000) as libc::time_t;
    let mut tm: libc::tm = unsafe { std::mem::zeroed() };
    unsafe { libc::localtime_r(&secs, &mut tm) };
    format!("{:04}-{:02}-{:02} {:02}:{:02}:{:02}", tm.tm_year + 1900, tm.tm_mon + 1, tm.tm_mday, tm.tm_hour, tm.tm_min, tm.tm_sec)
}

/// Receive side helper used by tests and the bench: waits for the scan to finish.
pub fn wait_scan(l: &Listing, timeout: std::time::Duration) -> bool {
    let start = Instant::now();
    loop {
        if l.count().1 {
            return true;
        }
        if start.elapsed() > timeout {
            return false;
        }
        thread::sleep(std::time::Duration::from_millis(2));
    }
}

#[allow(dead_code)]
pub fn drain(rx: &Receiver<Value>) -> Vec<Value> {
    let mut v = Vec::new();
    while let Ok(x) = rx.try_recv() {
        v.push(x);
    }
    v
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn temp_tree(n: usize) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("kiki-listing-{}-{}", std::process::id(), n));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("sub")).unwrap();
        for i in 0..n {
            std::fs::write(dir.join(format!("file{i}.txt")), vec![b'x'; i % 7]).unwrap();
        }
        dir
    }

    #[test]
    fn open_window_and_stats() {
        let dir = temp_tree(50);
        let uri = Uri::from_path(&dir);
        let (l, cached) = open(&uri).unwrap();
        assert!(!cached);
        assert!(wait_scan(&l, Duration::from_secs(5)));
        let (tx, rx) = mpsc::channel();
        l.subscribe(Subscriber { client: 1, lid: 7, tx, first: 0, count: 10 });
        let w = l.window(1, 7, 0, 10);
        assert_eq!(w.u64_field("n"), Some(51));
        let rows = w.get("rows").unwrap().as_arr().unwrap();
        assert_eq!(rows.len(), 10);
        assert_eq!(rows[0].str_field("name"), Some("sub")); // folders first
        assert_eq!(rows[1].str_field("name"), Some("file0.txt")); // natural order
        assert_eq!(rows[2].str_field("name"), Some("file1.txt"));
        // small dir: enrichment lands soon; wait for a Rows event or metadata
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            let w = l.window(1, 7, 0, 10);
            let rows = w.get("rows").unwrap().as_arr().unwrap();
            if rows[1].get("meta").map(|m| m != &Value::Null).unwrap_or(false) {
                assert_eq!(rows[1].get("meta").unwrap().u64_field("size"), Some(0));
                break;
            }
            assert!(Instant::now() < deadline, "metadata never arrived");
            thread::sleep(Duration::from_millis(5));
        }
        let _ = drain(&rx);
        // second open is served from the cache
        let (_l2, cached) = open(&uri).unwrap();
        assert!(cached);
        // filter
        assert_eq!(l.filter("file1"), 11); // file1, file10..file19
        assert_eq!(l.filter(""), 51);
        // sort by size descending waits for enrichment then resets
        let (tx2, rx2) = mpsc::channel();
        l.sort(SortRole::Size, false, Some((tx2, 99)));
        let reply = rx2.recv_timeout(Duration::from_secs(5)).unwrap();
        assert_eq!(reply.u64_field("id"), Some(99));
        let w = l.window(1, 7, 0, 3);
        let rows = w.get("rows").unwrap().as_arr().unwrap();
        assert_eq!(rows[0].str_field("name"), Some("sub"));
        assert_eq!(rows[1].get("meta").unwrap().u64_field("size"), Some(6));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn rescan_keeps_meta_for_unchanged() {
        let dir = temp_tree(5);
        let uri = Uri::from_path(&dir);
        let (l, _) = open(&uri).unwrap();
        assert!(wait_scan(&l, Duration::from_secs(5)));
        let (tx, rx) = mpsc::channel();
        l.subscribe(Subscriber { client: 2, lid: 1, tx, first: 0, count: 10 });
        l.enrich(None);
        let deadline = Instant::now() + Duration::from_secs(5);
        while l.inner.lock().unwrap().meta.iter().any(Option::is_none) {
            assert!(Instant::now() < deadline);
            thread::sleep(Duration::from_millis(5));
        }
        std::fs::write(dir.join("new.txt"), b"1").unwrap();
        l.rescan();
        let w = l.window(2, 1, 0, 20);
        let rows = w.get("rows").unwrap().as_arr().unwrap();
        assert_eq!(rows.len(), 7);
        let kept = rows.iter().find(|r| r.str_field("name") == Some("file1.txt")).unwrap();
        assert!(kept.get("meta").unwrap() != &Value::Null);
        let fresh = rows.iter().find(|r| r.str_field("name") == Some("new.txt")).unwrap();
        assert_eq!(fresh.get("meta").unwrap(), &Value::Null);
        let events = drain(&rx);
        assert!(events.iter().any(|e| e.str_field("event") == Some("Reset")));
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
