//! Phase 2: the stat pool, thumbnails, and `Enrich` — everything that fills a row in after
//! the names are already on screen.

use super::*;

pub(super) struct StatJob {
    pub(super) listing: Arc<Listing>,
    pub(super) rows: Vec<u32>,
    pub(super) low_priority: bool,
}

pub(super) struct StatPool {
    pub(super) tx: Sender<StatJob>,
}

pub(super) fn stat_pool() -> &'static StatPool {
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

impl Listing {

    /// Run on a stat worker: fetch metadata for rows, then push changed rows into live windows.
    pub(super) fn run_stats(self: &Arc<Self>, rows: Vec<u32>, low_priority: bool) {
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
            if visible && crate::thumbs::thumbable(kind) && !inner.thumb.contains_key(&idx) && !inner.thumb_queued[idx as usize] {
                inner.thumb_queued[idx as usize] = true;
                let mtime = inner.meta[idx as usize].as_ref().map(|m| m.mtime_ms).unwrap_or(0);
                let name = String::from_utf8_lossy(&name).into_owned();
                drop(inner);
                self.submit_thumb(idx, kind, mtime, &name);
                continue;
            }
        }
        self.push_rows(&done);
        if low_priority {
            self.enrich_progress();
        }
    }

    /// Queue one row's thumbnail. The caller sets `thumb_queued` while it holds the lock.
    pub(super) fn submit_thumb(self: &Arc<Self>, idx: u32, kind: crate::kinds::Kind, mtime_ms: u64, name: &str) {
        let uri = self.uri.join(name);
        let me = Arc::clone(self);
        crate::thumbs::submit(crate::thumbs::ThumbJob {
            uri,
            kind,
            mtime_ms,
            size: crate::thumbs::Size::Normal,
            done: Box::new(move |path| {
                {
                    let mut inner = me.inner.lock().unwrap();
                    if (idx as usize) < inner.pool.len() {
                        inner.thumb.insert(idx, path.map(|p| p.to_string_lossy().into_owned()).unwrap_or_default());
                        inner.thumb_queued[idx as usize] = false;
                    }
                }
                me.push_rows(&[idx]);
            }),
        });
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

    pub(super) fn enrich_all(self: &Arc<Self>, low_priority: bool) {
        let rows: Vec<u32> = {
            let mut inner = self.inner.lock().unwrap();
            // Every row without metadata, not just the ones the view is showing. `total` below
            // counts them all and `enrich_progress` only finishes when they all have metadata, so
            // queueing the view alone left a filtered listing permanently mid-enrichment — and a
            // `Sort` waiting on it (sort by size, filter typed) never got its reply.
            let rows: Vec<u32> = (0..inner.meta.len() as u32).filter(|&i| inner.meta[i as usize].is_none() && !inner.queued[i as usize]).collect();
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

    pub(super) fn enrich_progress(self: &Arc<Self>) {
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
