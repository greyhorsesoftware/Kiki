//! One-way mirror: scan → diff (pure) → execute. See docs/0.1.0/08-mirror.md and its appendix.

use crate::json::Value;
use crate::locations::{self, Session};
use crate::plugin::Msg;
use crate::vfs::uri::Uri;
use crate::vfs::VfsError;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};

pub const TOLERANCE_MS: i64 = 2000;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Direction {
    Upload,
    Download,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Detector {
    Auto,
    SizeMtime,
    SizeOnly,
    Digest,
}

#[derive(Clone, Debug)]
pub struct Spec {
    pub master: Uri,
    pub replica: Uri,
    pub direction: Direction,
    pub delete_extras: bool,
    pub blast_radius: f64,
    pub confirmed_large_delete: bool,
    pub clock_offset_ms: i64,
    pub clock_offset_auto: bool,
    pub detector: Detector,
    pub modified_within_ms: Option<u64>,
    pub apply_filters: bool,
}

impl Spec {
    pub fn from_json(v: &Value) -> Result<Spec, String> {
        let uri = |k: &str| v.str_field(k).ok_or(format!("missing {k}")).and_then(|s| Uri::parse(s).map_err(|e| e.0.to_string()));
        Ok(Spec {
            master: uri("master")?,
            replica: uri("replica")?,
            direction: if v.str_field("direction") == Some("download") { Direction::Download } else { Direction::Upload },
            delete_extras: v.get("deleteExtras").and_then(Value::as_bool).unwrap_or(false),
            blast_radius: match v.get("blastRadius") {
                Some(Value::Float(f)) => *f,
                Some(Value::Uint(u)) => *u as f64,
                _ => 0.5,
            },
            confirmed_large_delete: v.get("confirmedLargeDelete").and_then(Value::as_bool).unwrap_or(false),
            clock_offset_ms: v.get("clockOffsetMs").and_then(Value::as_i64).unwrap_or(0),
            clock_offset_auto: v.get("clockOffsetAuto").and_then(Value::as_bool).unwrap_or(true),
            detector: match v.str_field("detector") {
                Some("sizeMtime") => Detector::SizeMtime,
                Some("sizeOnly") => Detector::SizeOnly,
                Some("digest") => Detector::Digest,
                _ => Detector::Auto,
            },
            modified_within_ms: v.u64_field("modifiedWithinMs"),
            apply_filters: v.get("applyFilters").and_then(Value::as_bool).unwrap_or(true),
        })
    }

    pub fn to_json(&self) -> Value {
        Value::obj()
            .s("master", self.master.to_string())
            .s("replica", self.replica.to_string())
            .s("direction", if self.direction == Direction::Upload { "upload" } else { "download" })
            .b("deleteExtras", self.delete_extras)
            .v("blastRadius", Value::Float(self.blast_radius))
            .b("confirmedLargeDelete", self.confirmed_large_delete)
            .i("clockOffsetMs", self.clock_offset_ms)
            .b("clockOffsetAuto", self.clock_offset_auto)
            .s(
                "detector",
                match self.detector {
                    Detector::Auto => "auto",
                    Detector::SizeMtime => "sizeMtime",
                    Detector::SizeOnly => "sizeOnly",
                    Detector::Digest => "digest",
                },
            )
            .v("modifiedWithinMs", self.modified_within_ms.map(Value::Uint).unwrap_or(Value::Null))
            .b("applyFilters", self.apply_filters)
            .done()
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Entry {
    /// Index of this entry's relative path in its `SideMap` arena (meaningless once copied into
    /// an `Action`, which carries the path itself).
    pub path: u32,
    pub is_dir: bool,
    pub size: u64,
    pub mtime_ms: u64,
    pub digest: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ActionKind {
    Copy,
    Mkdir,
    Delete,
    Rmdir,
    Skip,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Reason {
    New,
    Changed,
    Extra,
    Equal,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum State {
    Pending,
    Running,
    Done,
    Skipped,
}

#[derive(Clone, Debug)]
pub struct Action {
    pub rel: Arc<str>,
    pub kind: ActionKind,
    pub reason: Reason,
    pub bytes: u64,
    pub master: Option<Entry>,
    pub replica: Option<Entry>,
    pub checked: bool,
    pub state: State,
    pub progress: u8,
    pub error: Option<String>,
}

#[derive(Clone, Debug, Default)]
pub struct Plan {
    pub actions: Vec<Action>,
    pub replica_entry_count: usize,
    pub filtered_count: usize,
    pub clock_offset_ms: i64,
}

impl Plan {
    pub fn delete_count(&self) -> usize {
        self.actions.iter().filter(|a| a.checked && matches!(a.kind, ActionKind::Delete | ActionKind::Rmdir)).count()
    }
    pub fn blast_radius_fraction(&self) -> f64 {
        if self.replica_entry_count == 0 {
            0.0
        } else {
            self.delete_count() as f64 / self.replica_entry_count as f64
        }
    }
    pub fn copy_bytes(&self) -> u64 {
        self.actions.iter().filter(|a| a.checked && a.kind == ActionKind::Copy).map(|a| a.bytes).sum()
    }
    pub fn count(&self, r: Reason) -> usize {
        self.actions.iter().filter(|a| a.reason == r).count()
    }
    pub fn counts_json(&self) -> Value {
        Value::obj()
            .u("new", self.count(Reason::New) as u64)
            .u("changed", self.count(Reason::Changed) as u64)
            .u("equal", self.count(Reason::Equal) as u64)
            .u("extra", self.count(Reason::Extra) as u64)
            .u("deletes", self.delete_count() as u64)
            .u("copyBytes", self.copy_bytes())
            .u("replicaEntries", self.replica_entry_count as u64)
            .u("filtered", self.filtered_count as u64)
            .done()
    }
}

/// One side of a mirror: every relative path in one contiguous byte arena with `u32` offsets,
/// and one `Entry` per path, sorted by path bytes once `finish` has run. Parents sort before
/// their children (a prefix is shorter), which the diff relies on. Lookups are binary searches;
/// there is no per-path allocation and no tree-map node.
#[derive(Default, Clone)]
pub struct SideMap {
    buf: Vec<u8>,
    off: Vec<u32>,
    entries: Vec<Entry>,
    sorted: bool,
}

impl SideMap {
    pub fn new() -> SideMap {
        SideMap { buf: Vec::new(), off: vec![0], entries: Vec::new(), sorted: true }
    }

    fn path_at(&self, id: u32) -> &str {
        let (a, b) = (self.off[id as usize] as usize, self.off[id as usize + 1] as usize);
        std::str::from_utf8(&self.buf[a..b]).unwrap_or("")
    }

    pub fn path_of(&self, e: &Entry) -> &str {
        self.path_at(e.path)
    }

    pub fn insert(&mut self, rel: &str, mut e: Entry) {
        self.buf.extend_from_slice(rel.as_bytes());
        self.off.push(self.buf.len() as u32);
        e.path = (self.off.len() - 2) as u32;
        self.entries.push(e);
        self.sorted = self.entries.len() <= 1;
    }

    /// Sort by path bytes; a path inserted twice keeps its last entry.
    pub fn finish(&mut self) {
        if self.sorted {
            return;
        }
        let buf = &self.buf;
        let off = &self.off;
        let key = |e: &Entry| &buf[off[e.path as usize] as usize..off[e.path as usize + 1] as usize];
        self.entries.sort_by(|a, b| key(a).cmp(key(b)));
        self.entries.dedup_by(|later, earlier| {
            if key(later) == key(earlier) {
                std::mem::swap(later, earlier);
                true
            } else {
                false
            }
        });
        self.sorted = true;
    }

    fn position(&self, rel: &str) -> Option<usize> {
        if self.sorted {
            self.entries.binary_search_by(|e| self.path_of(e).as_bytes().cmp(rel.as_bytes())).ok()
        } else {
            self.entries.iter().rposition(|e| self.path_of(e) == rel)
        }
    }

    pub fn get(&self, rel: &str) -> Option<&Entry> {
        self.position(rel).map(|i| &self.entries[i])
    }

    pub fn get_mut(&mut self, rel: &str) -> Option<&mut Entry> {
        let i = self.position(rel)?;
        Some(&mut self.entries[i])
    }

    pub fn contains(&self, rel: &str) -> bool {
        self.position(rel).is_some()
    }

    pub fn iter(&self) -> impl DoubleEndedIterator<Item = (&str, &Entry)> + '_ {
        self.entries.iter().map(move |e| (self.path_of(e), e))
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn clear(&mut self) {
        self.buf.clear();
        self.off.clear();
        self.off.push(0);
        self.entries.clear();
        self.sorted = true;
    }
}

impl<K: AsRef<str>> FromIterator<(K, Entry)> for SideMap {
    fn from_iter<I: IntoIterator<Item = (K, Entry)>>(iter: I) -> Self {
        let mut m = SideMap::new();
        for (k, e) in iter {
            m.insert(k.as_ref(), e);
        }
        m.finish();
        m
    }
}

impl std::ops::Index<&str> for SideMap {
    type Output = Entry;
    fn index(&self, rel: &str) -> &Entry {
        self.get(rel).expect("path in side map")
    }
}

pub enum Side {
    Local(PathBuf),
    Remote(Arc<Session>, String),
}

pub fn side_for(uri: &Uri) -> Result<Side, VfsError> {
    side_for_cancellable(uri, &AtomicBool::new(false))
}

/// The same, where connecting to the server is part of what a cancel has to be able to stop.
pub fn side_for_cancellable(uri: &Uri, cancel: &AtomicBool) -> Result<Side, VfsError> {
    if uri.is_local() {
        Ok(Side::Local(uri.to_path()))
    } else {
        let (s, p) = locations::resolve_cancellable(uri, Some(cancel))?;
        Ok(Side::Remote(s, p))
    }
}

fn join_rel(root: &str, rel: &str) -> String {
    if rel.is_empty() {
        root.to_string()
    } else {
        format!("{}/{}", root.trim_end_matches('/'), rel)
    }
}

mod detect;
mod diff;
mod execute;
mod filters;
mod report;
mod scan;
mod store;
#[cfg(test)]
mod tests;

pub use detect::{auto_offset, is_changed, pick_detector};
pub use diff::diff;
pub use execute::{execute, ExecCtx, Outcome, part_name};
pub use filters::{check_rules, default_rules, filtered, filtered_rel, filters, filters_json, load_filters, restore_default_filters, set_filters, Rule};
pub use report::{action_json, report};
pub use scan::{scan, scan_counting, scan_side};
pub use execute::copy_file;
pub use store::{run_finished, store, stored, unview, view, Stored, KEEP};
