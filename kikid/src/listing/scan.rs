//! Phase 1: the scan, the rescan a watcher triggers, and the in-place patch that avoids one.

use super::*;
use super::cache::{cache, evict_if_needed};
use super::stats::{stat_pool, StatJob};

impl Listing {

    /// Phase 1: enumerate names and kinds into the pool, publishing counts as chunks land.
    pub(super) fn scan(self: &Arc<Self>) {
        let start = Instant::now();
        let result = self.dir.scan(&mut |chunk| {
            let mut inner = self.inner.lock().unwrap();
            for e in chunk {
                let idx = inner.pool.push(e.name.as_bytes(), e.kind);
                inner.meta.push(e.meta);
                inner.queued.push(false);
                inner.thumb_queued.push(false);
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
        let gen = inner.generation;
        let subs = inner.subscribers.clone();
        drop(inner);
        for s in &subs {
            let _ = s.tx.send(proto::event("Count").u("lid", s.lid).u("n", n).b("done", true).done());
            let _ = s.tx.send(proto::event("Reset").u("lid", s.lid).u("n", n).u("gen", gen).done());
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
    pub(crate) fn git_status(self: &Arc<Self>) {
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
                let (epoch, names): (u64, Vec<(u32, String, bool)>) = {
                    let inner = me.inner.lock().unwrap();
                    (inner.epoch, (0..inner.pool.len() as u32).map(|i| (i, String::from_utf8_lossy(inner.pool.name(i)).into_owned(), inner.pool.entry_type(i) == EntryType::Dir)).collect())
                };
                let mut changed = Vec::new();
                {
                    let mut inner = me.inner.lock().unwrap();
                    // A rescan since the names were read has dealt the indexes again, and has
                    // started a status of its own.
                    if inner.epoch != epoch {
                        return;
                    }
                    for (i, name, is_dir) in names {
                        if (i as usize) >= inner.pool.len() {
                            break;
                        }
                        let e = crate::git::state_for(&me.path, &name, is_dir);
                        let interesting = e.as_ref().map(|e| e.state != crate::git::State::Clean).unwrap_or(false);
                        if interesting || inner.git.contains_key(&i) {
                            changed.push(i);
                        }
                        match e {
                            Some(e) if interesting => {
                                inner.git.insert(i, e);
                            }
                            _ => {
                                inner.git.remove(&i);
                            }
                        }
                    }
                    inner.git_done = true;
                    // Hiding ignored files changes which rows there are, not how they look.
                    if crate::git::hide_ignored() {
                        let before = inner.view.len();
                        inner.rebuild_view();
                        if inner.view.len() != before {
                            inner.generation += 1;
                            let n = inner.view.len() as u64;
                            let gen = inner.generation;
                            for s in inner.subscribers.clone() {
                                let _ = s.tx.send(proto::event("Reset").u("lid", s.lid).u("n", n).u("gen", gen).done());
                            }
                            changed.clear();
                        }
                    }
                }
                me.push_rows(&changed);
                let subs = me.inner.lock().unwrap().subscribers.clone();
                for s in subs {
                    let _ = s.tx.send(proto::event("RepoChanged").s("root", Uri::from_path(&crate::git::repo_root(&me.path).unwrap_or_default()).to_string()).u("lid", s.lid).done());
                }
            })
            .expect("spawn git");
    }

    /// Re-enumerate after a change, keeping what is known — metadata, thumbnail, git state — for
    /// names that still exist. Pool indexes are dealt again here, so everything is carried by name.
    pub fn rescan(self: &Arc<Self>) {
        struct Kept {
            meta: Option<Meta>,
            thumb: Option<String>,
            git: Option<crate::git::Entry>,
        }
        let (mut old, had_git) = {
            let inner = self.inner.lock().unwrap();
            let old: HashMap<Vec<u8>, Kept> = (0..inner.pool.len() as u32)
                .filter(|&i| !inner.pool.is_removed(i))
                .map(|i| (inner.pool.name(i).to_vec(), Kept { meta: inner.meta[i as usize].clone(), thumb: inner.thumb.get(&i).cloned(), git: inner.git.get(&i).cloned() }))
                .collect();
            (old, inner.git_done)
        };
        let mut pool = StringPool::with_capacity(old.len());
        let mut meta = Vec::with_capacity(old.len());
        let mut thumb = HashMap::new();
        let mut git = HashMap::new();
        let result = self.dir.scan(&mut |chunk| {
            for e in chunk {
                let idx = pool.push(e.name.as_bytes(), e.kind);
                let kept = old.remove(e.name.as_bytes());
                let (was, t, g) = match kept {
                    Some(k) => (k.meta, k.thumb, k.git),
                    None => (None, None, None),
                };
                // A thumbnail is of one version of a file: it is carried only while the scan does
                // not say the file's time has changed. One that failed ("") is asked for again —
                // the marker on disk makes that a lookup, not a second decode.
                let same = match (&e.meta, &was) {
                    (Some(now), Some(then)) => now.mtime_ms == then.mtime_ms,
                    _ => true,
                };
                if let Some(t) = t.filter(|t| same && !t.is_empty()) {
                    thumb.insert(idx, t);
                }
                if let Some(g) = g {
                    git.insert(idx, g);
                }
                meta.push(e.meta.or(was));
            }
        });
        let mut inner = self.inner.lock().unwrap();
        let old_total = inner.pool.len();
        let n = pool.len();
        inner.queued = vec![false; n];
        inner.thumb = thumb;
        inner.thumb_queued = vec![false; n];
        // Shown as it was until status has run again (below), rather than blank and then back.
        inner.git = git;
        inner.git_done = had_git;
        inner.pos = vec![u32::MAX; n];
        inner.pool = pool;
        inner.meta = meta;
        inner.scan_done = true;
        inner.scan_error = result.err().map(|e| e.message());
        inner.stale = false;
        inner.sorted = false;
        inner.epoch += 1;
        inner.rebuild_view();
        inner.generation += 1;
        let gen = inner.generation;
        let count = inner.view.len() as u64;
        let subs = inner.subscribers.clone();
        drop(inner);
        {
            let mut c = cache().lock().unwrap();
            c.entries = c.entries.saturating_sub(old_total) + n;
        }
        for s in &subs {
            let _ = s.tx.send(proto::event("Count").u("lid", s.lid).u("n", count).b("done", true).done());
            let _ = s.tx.send(proto::event("Reset").u("lid", s.lid).u("n", count).u("gen", gen).done());
        }
        if !crate::git::is_slow(&self.path) {
            self.git_status();
        }
    }

    /// In-place patch from inotify (plan 01): added names are appended and stated, removed names
    /// become tombstones, modified names lose their metadata and thumbnail so they refetch.
    pub fn patch(self: &Arc<Self>, added: &[Vec<u8>], removed: &[Vec<u8>], modified: &[Vec<u8>]) {
        let mut inner = self.inner.lock().unwrap();
        let mut changed = false;
        let mut removed_idx = Vec::new();
        let mut added_idx = Vec::new();
        for name in removed {
            if let Some(i) = inner.pool.find(name) {
                inner.pool.remove(i);
                removed_idx.push(i);
                changed = true;
            }
        }
        let mut to_stat = Vec::new();
        for name in added {
            if inner.pool.find(name).is_some() {
                continue;
            }
            let kind = match self.dir.stat_child(std::ffi::OsStr::from_bytes(name)) {
                Ok((_, t)) => t,
                Err(_) => continue, // vanished again
            };
            let idx = inner.pool.push(name, kind);
            added_idx.push(idx);
            inner.meta.push(None);
            inner.queued.push(false);
            inner.thumb_queued.push(false);
            inner.pos.push(u32::MAX);
            to_stat.push(idx);
            changed = true;
        }
        for name in modified {
            if let Some(i) = inner.pool.find(name) {
                inner.meta[i as usize] = None;
                inner.thumb.remove(&i);
                inner.thumb_queued[i as usize] = false;
                if !inner.queued[i as usize] {
                    inner.queued[i as usize] = true;
                    to_stat.push(i);
                }
            }
        }
        // What a client has to do to its own rows, in order, when the change was small enough to
        // make in place. `None` is "too much changed: ask again" — a `Reset`.
        let mut splice: Option<Vec<Value>> = None;
        if changed {
            // A handful of changes in a name- or kind-sorted, unfiltered view splice in place:
            // O(log n) to find the spot plus one memmove of the position table, instead of a
            // full sort of the pool.
            let small = removed_idx.len() + added_idx.len() <= 64;
            if small && inner.filter.is_none() && inner.sorted && inner.scan_done && matches!(inner.sort.0, SortRole::Name | SortRole::Kind) {
                let mut ops = Vec::new();
                for &i in &removed_idx {
                    let p = inner.pos[i as usize];
                    inner.view_remove(i);
                    if p != u32::MAX {
                        ops.push(Value::obj().s("op", "remove").u("pos", p as u64).done());
                    }
                }
                for &i in &added_idx {
                    // A dot-file arriving in a view that hides them is in the pool and not shown.
                    if !inner.show_hidden && inner.pool.name(i).first() == Some(&b'.') {
                        continue;
                    }
                    inner.view_insert(i);
                    ops.push(Value::obj().s("op", "insert").u("pos", inner.pos[i as usize] as u64).v("row", inner.row_json(i)).done());
                }
                // A change nobody can see (a hidden name) is no change to the view.
                if !ops.is_empty() {
                    inner.generation += 1;
                }
                splice = Some(ops);
            } else {
                inner.rebuild_view();
                inner.generation += 1;
            }
        }
        let n = inner.view.len() as u64;
        let gen = inner.generation;
        let epoch = inner.epoch;
        {
            let c = &mut cache().lock().unwrap();
            c.entries += added.len();
        }
        if changed {
            // Sent with the lock held, so that no `Window` can be answered between the change and
            // the word of it; a reply already on its way carries the older `gen` and is dropped.
            for s in &inner.subscribers {
                let _ = match &splice {
                    Some(ops) if ops.is_empty() => continue,
                    Some(ops) => s.tx.send(proto::event("Splice").u("lid", s.lid).u("n", n).u("gen", gen).v("ops", Value::Arr(ops.clone())).done()),
                    None => s.tx.send(proto::event("Reset").u("lid", s.lid).u("n", n).u("gen", gen).done()),
                };
            }
        }
        drop(inner);
        if !to_stat.is_empty() {
            let _ = stat_pool().tx.send(StatJob { listing: Arc::clone(self), rows: to_stat, epoch, low_priority: true });
        }
        self.git_status();
        crate::index::patch_dir(&self.path);
    }

}
