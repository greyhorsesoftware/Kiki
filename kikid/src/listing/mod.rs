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
/// Workers that work out the state of one folder's repository-root rows (plan 15). The cost is
/// a `git` apiece, so a folder of forty projects walks the list a few at a time rather than
/// spawning forty of them; `git::AGGREGATE_AT_ONCE` bounds it again across folders.
pub const AGGREGATE_WORKERS: usize = 4;

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
    /// The rows this connection holds: the viewport plus the look-ahead it scrolls into.
    pub first: u32,
    pub count: u32,
    /// The rows actually on screen. Thumbnails are made for these and no others: the look-ahead
    /// above can be thousands of rows during a scroll, and a thumbnail for a row that is not being
    /// looked at is a `pdftoppm` or an `ffmpeg` spawned for nobody. Rows must exist ahead of the
    /// scroll; their pictures need not.
    pub view_first: u32,
    pub view_count: u32,
}

impl Subscriber {
    fn covers(&self, pos: u32) -> bool {
        pos >= self.first && pos < self.first + self.count
    }

    /// On screen, as opposed to merely held. A client that does not say where it is looking is
    /// taken to be looking at everything it asked for, which is what the older behaviour was.
    fn covers_view(&self, pos: u32) -> bool {
        if self.view_count == 0 {
            return self.covers(pos);
        }
        pos >= self.view_first && pos < self.view_first + self.view_count
    }
}

struct Inner {
    pool: StringPool,
    meta: Vec<Option<Meta>>,
    queued: Vec<bool>,
    /// What is known about a row that the scan did not say — thumbnail, git state — by name, so
    /// that a worker finishing after a rescan still lands on the file it was asked about.
    deco: deco::Decorations,
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
    /// Counts every change to `view`. It goes out on `Window` replies and on `Reset` and `Splice`,
    /// which is how a client tells a reply computed before a change from one computed after.
    generation: u64,
    /// Counts rescans: pool indexes mean something only within one.
    epoch: u64,
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

/// Owner and group names. A local `Meta` holds the uid and gid, resolved here through `passwd`
/// and `group` and cached. A remote one holds a name the backend gave (SFTP's `djclark`, FTP's
/// listing column), interned into the same two u32s above `REMOTE` so 200k entries stay small;
/// `user` and `group` answer both. (Until 2026-09-24 `meta_from` dropped the remote names, so an
/// SFTP row had no owner or group even though the plugin sent them.)
pub(crate) mod names {
    use std::collections::HashMap;
    use std::sync::{Mutex, OnceLock};

    /// Ids at or above this are interned remote names, not local uids or gids. No real uid is
    /// up here (`nobody` is 65534), and `Meta::NONE` (`u32::MAX`) is filtered out before a lookup.
    const REMOTE: u32 = 0x8000_0000;

    #[derive(Default)]
    struct Names {
        users: HashMap<u32, Option<String>>,
        groups: HashMap<u32, Option<String>>,
        remote: Vec<String>,
        ids: HashMap<String, u32>,
    }
    fn cache() -> &'static Mutex<Names> {
        static C: OnceLock<Mutex<Names>> = OnceLock::new();
        C.get_or_init(|| Mutex::new(Names::default()))
    }

    /// The id for a name a remote backend gave; the same name gets the same id.
    pub fn intern(name: &str) -> u32 {
        let mut c = cache().lock().unwrap();
        if let Some(&id) = c.ids.get(name) {
            return id;
        }
        let id = REMOTE + c.remote.len() as u32;
        c.remote.push(name.to_string());
        c.ids.insert(name.to_string(), id);
        id
    }

    pub fn user(uid: u32) -> Option<String> {
        let mut c = cache().lock().unwrap();
        if uid >= REMOTE {
            return c.remote.get((uid - REMOTE) as usize).cloned();
        }
        c.users.entry(uid).or_insert_with(|| super::local::user_name(uid)).clone()
    }

    pub fn group(gid: u32) -> Option<String> {
        let mut c = cache().lock().unwrap();
        if gid >= REMOTE {
            return c.remote.get((gid - REMOTE) as usize).cloned();
        }
        c.groups.entry(gid).or_insert_with(|| super::local::group_name(gid)).clone()
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
pub mod deco;
mod rows;
mod scan;
mod stats;
#[cfg(test)]
mod tests;
mod view;

pub use cache::{changed, find, forget, gone, invalidate_authority, mark_stale, open, poll_repo_rows, still_there, under};
pub use rows::{iso, meta_json};
