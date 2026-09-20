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
                let names: Vec<(u32, String, bool)> = {
                    let inner = me.inner.lock().unwrap();
                    (0..inner.pool.len() as u32).map(|i| (i, String::from_utf8_lossy(inner.pool.name(i)).into_owned(), inner.pool.entry_type(i) == EntryType::Dir)).collect()
                };
                let mut changed = Vec::new();
                {
                    let mut inner = me.inner.lock().unwrap();
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
                            for s in inner.subscribers.clone() {
                                let _ = s.tx.send(proto::event("Reset").u("lid", s.lid).u("n", n).done());
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
        inner.thumb.clear();
        inner.thumb_queued = vec![false; n];
        inner.git.clear();
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
        if changed {
            // A handful of changes in a name- or kind-sorted, unfiltered view splice in place:
            // O(log n) to find the spot plus one memmove of the position table, instead of a
            // full sort of the pool.
            let small = removed_idx.len() + added_idx.len() <= 64;
            if small && inner.filter.is_none() && inner.sorted && inner.scan_done && matches!(inner.sort.0, SortRole::Name | SortRole::Kind) {
                for &i in &removed_idx {
                    inner.view_remove(i);
                }
                for &i in &added_idx {
                    inner.view_insert(i);
                }
            } else {
                inner.rebuild_view();
            }
            inner.generation += 1;
        }
        let n = inner.view.len() as u64;
        let subs = inner.subscribers.clone();
        {
            let c = &mut cache().lock().unwrap();
            c.entries += added.len();
        }
        drop(inner);
        if changed {
            for s in &subs {
                let _ = s.tx.send(proto::event("Reset").u("lid", s.lid).u("n", n).done());
            }
        }
        if !to_stat.is_empty() {
            let _ = stat_pool().tx.send(StatJob { listing: Arc::clone(self), rows: to_stat, low_priority: true });
        }
        self.git_status();
        crate::index::patch_dir(&self.path);
    }

}
