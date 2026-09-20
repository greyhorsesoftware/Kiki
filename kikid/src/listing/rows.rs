//! Windows: who is subscribed to what, the rows they get, and the JSON a row becomes.

use super::*;
use super::cache::cache;
use super::stats::{stat_pool, StatJob};

impl Listing {

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
        // A remote listing still scanning with nobody watching: stop the plugin's work and drop
        // the half-built pool so the next open starts clean.
        if inner.subscribers.is_empty() && !inner.scan_done && !self.dir.watchable() {
            inner.stale = true;
            drop(inner);
            self.dir.cancel();
            cache().lock().unwrap().map.remove(&self.uri.to_string());
        }
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
        let mut want_thumbs: Vec<(u32, crate::kinds::Kind, u64, String)> = Vec::new();
        for p in first.min(n)..end {
            let idx = inner.view[p as usize];
            rows.push(inner.row_json(idx));
            if inner.meta[idx as usize].is_none() && !inner.queued[idx as usize] {
                inner.queued[idx as usize] = true;
                missing.push((p.abs_diff(centre), idx));
                continue;
            }
            let kind = inner.pool.kind(idx);
            if crate::thumbs::thumbable(kind) && !inner.thumb.contains_key(&idx) && !inner.thumb_queued[idx as usize] {
                inner.thumb_queued[idx as usize] = true;
                let mtime = inner.meta[idx as usize].as_ref().map(|m| m.mtime_ms).unwrap_or(0);
                let name = String::from_utf8_lossy(inner.pool.name(idx)).into_owned();
                want_thumbs.push((idx, kind, mtime, name));
            }
        }
        let done = inner.scan_done;
        let gen = inner.generation;
        let epoch = inner.epoch;
        drop(inner);
        for (idx, kind, mtime, name) in want_thumbs {
            self.submit_thumb(idx, kind, mtime, &name);
        }
        if !missing.is_empty() {
            missing.sort_unstable();
            let rows: Vec<u32> = missing.into_iter().map(|(_, i)| i).collect();
            let _ = stat_pool().tx.send(StatJob { listing: Arc::clone(self), rows, epoch, low_priority: false });
        }
        Value::obj().u("first", first as u64).u("n", n as u64).b("done", done).u("gen", gen).v("rows", Value::Arr(rows)).done()
    }

    /// Send `Rows` events for the given pool rows to every subscriber whose window covers them.
    pub(super) fn push_rows(&self, changed: &[u32]) {
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

}

impl Inner {

    pub(super) fn row_json(&self, idx: u32) -> Value {
        let t = self.pool.entry_type(idx);
        let name = String::from_utf8_lossy(self.pool.name(idx)).into_owned();
        let meta = match &self.meta[idx as usize] {
            Some(m) => meta_json(m),
            None => Value::Null,
        };
        let opened = crate::access::opened_child(&self.uri_string, &name);
        let o = Value::obj().s("name", name).s("kind", self.pool.kind(idx).as_str()).b("isDir", t == EntryType::Dir).b("isLink", t == EntryType::Link).v("meta", meta);
        let o = match opened {
            Some(ms) => o.u("opened", ms),
            None => o,
        };
        o.v(
            "thumb",
            match self.thumb.get(&idx) {
                Some(p) => Value::Str(p.clone()),
                None => Value::Null,
            },
        )
        .v(
            "git",
            match self.git.get(&idx) {
                Some(e) => crate::git::entry_json(e),
                None => Value::Null,
            },
        )
        .done()
    }

}

pub fn meta_json(m: &Meta) -> Value {
    let owner = Meta::opt(m.uid).and_then(names::user);
    let group = Meta::opt(m.gid).and_then(names::group);
    let o: Obj = Value::obj().u("size", m.size).u("mtime", m.mtime_ms).u("atime", m.atime_ms);
    let o = match m.mode() {
        Some(mode) => o.u("mode", mode as u64),
        None => o.v("mode", Value::Null),
    };
    o.opt_s("owner", owner.as_deref()).opt_s("group", group.as_deref()).v("digest", Value::Null).done()
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
