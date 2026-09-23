//! What the view shows and in what order: sort, filter, hidden files, and type-ahead seek.

use super::*;

/// Sort in chunks across threads, then k-way merge (plan 01: above 50k entries).
fn parallel_sort<F: Fn(&u32, &u32) -> std::cmp::Ordering + Sync>(v: &mut Vec<u32>, cmp: &F) {
    let threads = thread::available_parallelism().map(|n| n.get()).unwrap_or(2).clamp(2, 8);
    let chunk = v.len().div_ceil(threads);
    let mut parts: Vec<Vec<u32>> = v.chunks(chunk).map(|c| c.to_vec()).collect();
    thread::scope(|s| {
        for p in parts.iter_mut() {
            s.spawn(move || p.sort_unstable_by(cmp));
        }
    });
    let mut out: Vec<u32> = Vec::with_capacity(v.len());
    let mut idx = vec![0usize; parts.len()];
    loop {
        let mut best: Option<usize> = None;
        for (i, p) in parts.iter().enumerate() {
            if idx[i] < p.len() && best.map(|b| cmp(&p[idx[i]], &parts[b][idx[b]]) == std::cmp::Ordering::Less).unwrap_or(true) {
                best = Some(i);
            }
        }
        match best {
            Some(b) => {
                out.push(parts[b][idx[b]]);
                idx[b] += 1;
            }
            None => break,
        }
    }
    *v = out;
}

impl Listing {
    pub fn sort(self: &Arc<Self>, role: SortRole, asc: bool, waiter: Option<(Sender<Value>, u64)>) -> u64 {
        let mut inner = self.inner.lock().unwrap();
        // Clients state the order they want on every open, because a cached listing carries the
        // order an earlier client asked for. Restating the order already in force costs nothing.
        if inner.sort == (role, asc) && inner.sorted {
            let n = inner.view.len() as u64;
            drop(inner);
            if let Some(w) = waiter {
                let _ = w.0.send(proto::ok(w.1, Value::obj().u("n", n).done()));
            }
            return n;
        }
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
        let gen = inner.generation;
        let subs = inner.subscribers.clone();
        drop(inner);
        for s in subs {
            let _ = s.tx.send(proto::event("Reset").u("lid", s.lid).u("n", n).u("gen", gen).done());
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
        let gen = inner.generation;
        let subs = inner.subscribers.clone();
        drop(inner);
        for s in subs {
            let _ = s.tx.send(proto::event("Reset").u("lid", s.lid).u("n", n).u("gen", gen).done());
        }
        n
    }

    /// Type-ahead (plan 23): the first view position whose name starts with `prefix`
    /// (case-insensitive), searching from `after` and wrapping; the whole view, not just the
    /// rows a window holds.
    pub fn seek(&self, prefix: &str, after: Option<u32>) -> Option<u32> {
        let inner = self.inner.lock().unwrap();
        let p = prefix.to_lowercase();
        if p.is_empty() || inner.view.is_empty() {
            return None;
        }
        let n = inner.view.len();
        let start = after.map(|a| (a as usize + 1) % n).unwrap_or(0);
        let matches = |pos: usize| -> bool {
            let name = inner.pool.name(inner.view[pos]);
            let lower = String::from_utf8_lossy(name).to_lowercase();
            lower.starts_with(&p)
        };
        (0..n).map(|k| (start + k) % n).find(|&pos| matches(pos)).map(|pos| pos as u32)
    }

    /// Show or hide dot-files; a `Reset` follows like a filter change.
    pub fn set_hidden(&self, show: bool) -> u64 {
        let mut inner = self.inner.lock().unwrap();
        if inner.show_hidden == show {
            return inner.view.len() as u64;
        }
        inner.show_hidden = show;
        inner.rebuild_view();
        inner.generation += 1;
        let n = inner.view.len() as u64;
        let gen = inner.generation;
        let subs = inner.subscribers.clone();
        drop(inner);
        for s in subs {
            let _ = s.tx.send(proto::event("Reset").u("lid", s.lid).u("n", n).u("gen", gen).done());
        }
        n
    }
}

impl Inner {
    /// Order of two pool entries under the current name or kind sort (folders first).
    pub(super) fn order(&self, a: u32, b: u32) -> std::cmp::Ordering {
        let pool = &self.pool;
        let (role, asc) = self.sort;
        let dir_rank = |i: u32| u8::from(pool.entry_type(i) != EntryType::Dir);
        let base = dir_rank(a).cmp(&dir_rank(b));
        let by_role = match role {
            SortRole::Kind => (pool.kind(a) as u8).cmp(&(pool.kind(b) as u8)),
            _ => std::cmp::Ordering::Equal,
        };
        let o = by_role.then_with(|| pool.key(a).cmp(pool.key(b)));
        base.then(if asc { o } else { o.reverse() })
    }

    pub(super) fn view_insert(&mut self, i: u32) {
        let at = self.view.partition_point(|&j| self.order(j, i) == std::cmp::Ordering::Less);
        self.view.insert(at, i);
        if (i as usize) >= self.pos.len() {
            self.pos.resize(i as usize + 1, u32::MAX);
        }
        for q in at..self.view.len() {
            self.pos[self.view[q] as usize] = q as u32;
        }
    }

    pub(super) fn view_remove(&mut self, i: u32) {
        let Some(&p) = self.pos.get(i as usize) else { return };
        if p == u32::MAX || (p as usize) >= self.view.len() || self.view[p as usize] != i {
            return;
        }
        self.view.remove(p as usize);
        self.pos[i as usize] = u32::MAX;
        for q in (p as usize)..self.view.len() {
            self.pos[self.view[q] as usize] = q as u32;
        }
    }

    pub(super) fn rebuild_view(&mut self) {
        let n = self.pool.len() as u32;
        let hidden_ok = self.show_hidden;
        let pool = &self.pool;
        let metas = &self.meta;
        // Ignored by git and asked to be hidden: known only once status has run, so such a row is
        // there for a moment and then goes (`git_status` rebuilds the view when it lands).
        let deco = &self.deco;
        let hide_ignored = self.git_done && crate::git::hide_ignored();
        let ignored = |i: u32| hide_ignored && deco.git(pool.name(i)).is_some_and(|e| e.state == crate::git::State::Ignored);
        let visible = |i: u32| !pool.is_removed(i) && !ignored(i) && (hidden_ok || (pool.name(i).first() != Some(&b'.') && !metas.get(i as usize).and_then(|m| m.as_ref()).map(|m| m.hidden).unwrap_or(false)));
        let mut view: Vec<u32> = match &self.filter {
            None => (0..n).filter(|&i| visible(i)).collect(),
            Some(f) => (0..n).filter(|&i| visible(i) && pool.name_contains(i, f)).collect(),
        };
        if self.scan_done {
            let (role, asc) = self.sort;
            let pool = &self.pool;
            let meta = &self.meta;
            // Folders first, then the role, then name as a tiebreak.
            let dir_rank = |i: u32| u8::from(pool.entry_type(i) != EntryType::Dir);
            let cmp = |a: &u32, b: &u32| -> std::cmp::Ordering {
                let (a, b) = (*a, *b);
                let base = dir_rank(a).cmp(&dir_rank(b));
                let by_role = match role {
                    SortRole::Name => std::cmp::Ordering::Equal,
                    SortRole::Kind => (pool.kind(a) as u8).cmp(&(pool.kind(b) as u8)),
                    SortRole::Size => meta[a as usize].as_ref().map(|m| m.size).unwrap_or(0).cmp(&meta[b as usize].as_ref().map(|m| m.size).unwrap_or(0)),
                    SortRole::Mtime => meta[a as usize].as_ref().map(|m| m.mtime_ms).unwrap_or(0).cmp(&meta[b as usize].as_ref().map(|m| m.mtime_ms).unwrap_or(0)),
                    SortRole::Atime => meta[a as usize].as_ref().map(|m| m.atime_ms).unwrap_or(0).cmp(&meta[b as usize].as_ref().map(|m| m.atime_ms).unwrap_or(0)),
                };
                base.then(by_role).then_with(|| pool.key(a).cmp(pool.key(b)))
            };
            if view.len() > PARALLEL_SORT_ABOVE {
                parallel_sort(&mut view, &cmp);
            } else {
                view.sort_unstable_by(cmp);
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
}
