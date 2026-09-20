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
pub const PARALLEL_SORT_ABOVE: usize = 50_000;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SortRole {
    Name,
    Kind,
    Size,
    Mtime,
    Atime,
}

impl SortRole {
    pub fn parse(s: &str) -> Option<SortRole> {
        Some(match s {
            "name" => SortRole::Name,
            "kind" => SortRole::Kind,
            "size" => SortRole::Size,
            "mtime" => SortRole::Mtime,
            "atime" => SortRole::Atime,
            _ => return None,
        })
    }
    fn needs_meta(self) -> bool {
        matches!(self, SortRole::Size | SortRole::Mtime | SortRole::Atime)
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
    /// Only rows that have one: pool index -> thumbnail path.
    thumb: HashMap<u32, String>,
    thumb_queued: Vec<bool>,
    /// Only rows with a state other than clean: pool index -> git entry.
    git: HashMap<u32, crate::git::Entry>,
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
    /// Dot-files are in the view only when set (the setting's default, then per listing).
    show_hidden: bool,
    /// The listing URI, for the access-log lookup per row.
    uri_string: String,
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

mod names {
    use std::collections::HashMap;
    use std::sync::{Mutex, OnceLock};

    type NameCache = Mutex<(HashMap<u32, Option<String>>, HashMap<u32, Option<String>>)>;
    fn cache() -> &'static NameCache {
        static C: OnceLock<NameCache> = OnceLock::new();
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

mod cache;
mod rows;
mod scan;
mod stats;
mod view;
#[cfg(test)]
mod tests;

pub use cache::{changed, find, forget, gone, invalidate_authority, mark_stale, open, under};
pub use rows::{iso, meta_json};
