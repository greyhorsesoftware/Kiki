//! Phase 1: the scan, the rescan a watcher triggers, and the in-place patch that avoids one.

use super::cache::{cache, evict_if_needed};
use super::stats::{stat_pool, StatJob};
use super::*;

/// The `Count` that says the scan is over, and — this is the part that was missing — how it went.
/// The error used to be readable only in the reply to `Open`, so a folder that failed to list
/// **again** had nowhere to say so: a share whose server had gone away came back from Refresh as
/// nought items, done, no error, and the pane showed an empty folder where the files had been.
fn finished(lid: u64, n: u64, error: Option<&str>) -> Value {
    proto::event("Count").u("lid", lid).u("n", n).b("done", true).opt_s("error", error).done()
}

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
        let failed = inner.scan_error.clone();
        let subs = inner.subscribers.clone();
        drop(inner);
        for s in &subs {
            let _ = s.tx.send(finished(s.lid, n, failed.as_deref()));
            let _ = s.tx.send(proto::event("Reset").u("lid", s.lid).u("n", n).u("gen", gen).done());
        }
        if std::env::var_os("KIKI_TRACE").is_some() {
            eprintln!("scan {} entries in {:?}", total, elapsed);
        }
        if total <= SMALL_DIR {
            self.enrich_all(true);
        }
        self.git_status();
        self.repo_rows(None);
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
                        let key = name.as_bytes();
                        if interesting || inner.deco.git(key).is_some() {
                            changed.push(i);
                        }
                        inner.deco.set_git(key, e.filter(|_| interesting));
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

    /// The rows that are repositories of their own (plan 15). A folder like `~/Projects` is not
    /// a repository, so the status above has nothing to say about the projects in it: each is
    /// its own repository, and each row says which branch it is on, coloured by how that
    /// repository stands.
    ///
    /// Two passes on one worker, neither of them on the listing's path. First the branch, which
    /// is a `stat` and a small read each and lands at once; then the state, which is a `git`
    /// each — a few at a time (`git::aggregate` holds the slots) and pushed as they come, so a
    /// folder of forty projects is neither forty gits nor one long wait.
    ///
    /// `only` names the rows to work out again: what the watcher's poll passes when one
    /// project's `HEAD` has moved, and what `patch` passes for a folder that has just appeared.
    /// `None` is the whole folder.
    pub(crate) fn repo_rows(self: &Arc<Self>, only: Option<Vec<Vec<u8>>>) {
        if !self.uri.is_local() || !crate::git::enabled() {
            return;
        }
        let me = Arc::clone(self);
        thread::Builder::new().name("git-roots".into()).spawn(move || me.repo_rows_now(only)).expect("spawn git-roots");
    }

    fn repo_rows_now(self: &Arc<Self>, only: Option<Vec<Vec<u8>>>) {
        let (epoch, folders): (u64, Vec<(u32, Vec<u8>)>) = {
            let inner = self.inner.lock().unwrap();
            let names = (0..inner.pool.len() as u32)
                .filter(|&i| inner.pool.entry_type(i) == EntryType::Dir && !inner.pool.is_removed(i))
                .map(|i| (i, inner.pool.name(i).to_vec()))
                .filter(|(_, n)| only.as_ref().is_none_or(|o| o.contains(n)))
                .collect();
            (inner.epoch, names)
        };
        // Pass one: which of them are repositories, and on what branch. `None` for a folder that
        // is not one, which is also how a row stops being one (a `.git` deleted, a clone undone).
        /// A folder row: where it is, what it is called, where it lives, and its branch if it
        /// turned out to be a repository at all.
        type Found = (u32, Vec<u8>, PathBuf, Option<(String, bool)>);
        let found: Vec<Found> = folders
            .into_iter()
            .map(|(i, name)| {
                let path = self.path.join(OsStr::from_bytes(&name));
                let head = if crate::git::is_repo_root(&path) { crate::git::head(&path) } else { None };
                (i, name, path, head)
            })
            .collect();
        let mut roots = Vec::new();
        let mut changed = Vec::new();
        {
            let mut inner = self.inner.lock().unwrap();
            // A rescan since the names were read has dealt the indexes again, and has started a
            // pass of its own.
            if inner.epoch != epoch {
                return;
            }
            for (i, name, path, head) in found {
                let was = inner.deco.repo(&name).cloned();
                let mark = head.map(|(branch, detached)| crate::git::RepoMark {
                    // The colour it already has is kept while the state is worked out again, so
                    // a repository that was dirty a moment ago does not blink clean first.
                    state: was.as_ref().filter(|w| w.branch == branch && w.detached == detached).and_then(|w| w.state),
                    branch,
                    detached,
                });
                if mark.is_some() {
                    roots.push((i, name.clone(), path));
                }
                let same = match (&was, &mark) {
                    (Some(a), Some(b)) => a.branch == b.branch && a.detached == b.detached && a.state == b.state,
                    (None, None) => true,
                    _ => false,
                };
                if !same {
                    changed.push(i);
                }
                inner.deco.set_repo(&name, mark);
            }
        }
        self.push_rows(&changed);
        if roots.is_empty() {
            return;
        }
        // Pass two: the expensive half. One worker per repository would be forty threads and
        // forty gits; a handful of workers over the list is the same answer at a bounded cost.
        let next = std::sync::atomic::AtomicUsize::new(0);
        let roots = &roots;
        thread::scope(|s| {
            for _ in 0..roots.len().min(AGGREGATE_WORKERS) {
                s.spawn(|| loop {
                    let k = next.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                    let Some((i, name, path)) = roots.get(k) else { break };
                    let Some(state) = crate::git::aggregate(path) else { continue };
                    let mut inner = self.inner.lock().unwrap();
                    if inner.epoch != epoch {
                        break;
                    }
                    let Some(mark) = inner.deco.repo(name).cloned() else { continue };
                    if mark.state == Some(state) {
                        continue;
                    }
                    inner.deco.set_repo(name, Some(crate::git::RepoMark { state: Some(state), ..mark }));
                    drop(inner);
                    self.push_rows(&[*i]);
                });
            }
        });
    }

    /// Re-enumerate after a change. Metadata is kept for names that still exist; **decorations
    /// keep themselves** — they are held by name, so the rescan has nothing to copy and cannot
    /// forget one. A thumbnail made for a version of a file the scan now says is gone stops being
    /// shown by that rule alone (`Decorations::thumb_path`), and is asked for again.
    pub fn rescan(self: &Arc<Self>) {
        let (old, had_git) = {
            let inner = self.inner.lock().unwrap();
            let old: HashMap<Vec<u8>, Option<Meta>> = (0..inner.pool.len() as u32).filter(|&i| !inner.pool.is_removed(i)).map(|i| (inner.pool.name(i).to_vec(), inner.meta[i as usize].clone())).collect();
            (old, inner.git_done)
        };
        let mut old = old;
        let mut pool = StringPool::with_capacity(old.len());
        let mut meta = Vec::with_capacity(old.len());
        let mut present: std::collections::HashSet<Vec<u8>> = std::collections::HashSet::with_capacity(old.len());
        let result = self.dir.scan(&mut |chunk| {
            for e in chunk {
                pool.push(e.name.as_bytes(), e.kind);
                present.insert(e.name.as_bytes().to_vec());
                meta.push(e.meta.or_else(|| old.remove(e.name.as_bytes()).flatten()));
            }
        });
        let mut inner = self.inner.lock().unwrap();
        let old_total = inner.pool.len();
        let n = pool.len();
        inner.queued = vec![false; n];
        // Names that have gone take what was known about them; the rest is untouched, so badges
        // and thumbnails are shown as they were while status runs again below.
        inner.deco.retain_names(&present);
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
        let failed = inner.scan_error.clone();
        let subs = inner.subscribers.clone();
        drop(inner);
        {
            let mut c = cache().lock().unwrap();
            c.entries = c.entries.saturating_sub(old_total) + n;
        }
        for s in &subs {
            let _ = s.tx.send(finished(s.lid, count, failed.as_deref()));
            let _ = s.tx.send(proto::event("Reset").u("lid", s.lid).u("n", count).u("gen", gen).done());
        }
        if !crate::git::is_slow(&self.path) {
            self.git_status();
        }
        self.repo_rows(None);
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
            inner.pos.push(u32::MAX);
            to_stat.push(idx);
            changed = true;
        }
        for name in modified {
            if let Some(i) = inner.pool.find(name) {
                // The metadata goes, and with it the answer a thumbnail was made for: the stat
                // that follows brings a new time, and the row asks again by that alone.
                inner.meta[i as usize] = None;
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
        // Only the names that have just arrived: a `git init` inside a folder we are showing is
        // not something this folder's watch can see anyway (inotify does not descend), so the
        // one thing a patch can bring is a project that has been cloned or moved in.
        if !added.is_empty() {
            self.repo_rows(Some(added.to_vec()));
        }
        crate::index::patch_dir(&self.path);
    }
}
