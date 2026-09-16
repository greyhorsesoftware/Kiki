//! One-way mirror: scan → diff (pure) → execute. See docs/0.1.0/08-mirror.md and its appendix.

use crate::json::Value;
use crate::locations::{self, Session};
use crate::plugin::Msg;
use crate::vfs::uri::Uri;
use crate::vfs::VfsError;
use std::collections::{BTreeMap, HashMap};
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
    pub rel: String,
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
    pub rel: String,
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

// ---------------------------------------------------------------- filters

#[derive(Clone, Debug)]
pub enum Rule {
    Contains(String),
    StartsWith(String),
    EndsWith(String),
    Matches(String),
}

pub fn load_filters() -> Vec<Rule> {
    let v = crate::config::read_named("filters.toml");
    let rules: Vec<Value> = v.get("rule").and_then(Value::as_arr).map(|a| a.to_vec()).unwrap_or_default();
    if rules.is_empty() {
        return vec![Rule::Matches(".git".into()), Rule::Matches(".DS_Store".into()), Rule::Matches("node_modules".into()), Rule::Matches("__pycache__".into())];
    }
    rules
        .iter()
        .filter_map(|r| {
            let val = r.str_field("value")?.to_string();
            Some(match r.str_field("kind")? {
                "contains" => Rule::Contains(val),
                "startsWith" => Rule::StartsWith(val),
                "endsWith" => Rule::EndsWith(val),
                _ => Rule::Matches(val),
            })
        })
        .collect()
}

pub fn filtered(name: &str, rules: &[Rule]) -> bool {
    rules.iter().any(|r| match r {
        Rule::Contains(s) => name.contains(s.as_str()),
        Rule::StartsWith(s) => name.starts_with(s.as_str()),
        Rule::EndsWith(s) => name.ends_with(s.as_str()),
        Rule::Matches(s) => name == s,
    })
}

// ---------------------------------------------------------------- detectors

fn size_mtime(m: &Entry, r: &Entry, offset_ms: i64) -> bool {
    if m.size != r.size {
        return true;
    }
    if m.mtime_ms == 0 || r.mtime_ms == 0 {
        return false;
    }
    let adjusted = m.mtime_ms as i64 - offset_ms;
    (adjusted - r.mtime_ms as i64).abs() > TOLERANCE_MS
}

fn usable_md5(d: &Option<String>) -> Option<String> {
    d.as_ref().filter(|s| s.len() == 32 && s.bytes().all(|b| b.is_ascii_hexdigit())).map(|s| s.to_ascii_lowercase())
}

pub fn is_changed(det: Detector, m: &Entry, r: &Entry, offset_ms: i64) -> bool {
    match det {
        Detector::SizeOnly => m.size != r.size,
        Detector::Digest => {
            if m.size != r.size {
                return true;
            }
            match (usable_md5(&m.digest), usable_md5(&r.digest)) {
                (Some(a), Some(b)) => a != b,
                _ => size_mtime(m, r, offset_ms), // single-sided digests need a local MD5; not available here, fall back
            }
        }
        Detector::SizeMtime | Detector::Auto => size_mtime(m, r, offset_ms),
    }
}

/// Median of (master − replica) mtime deltas over same-size file pairs; 0 below three samples.
pub fn auto_offset(master: &BTreeMap<String, Entry>, replica: &BTreeMap<String, Entry>) -> i64 {
    let mut deltas: Vec<i64> = master
        .iter()
        .filter_map(|(rel, m)| {
            let r = replica.get(rel)?;
            if m.is_dir || r.is_dir || m.size != r.size || m.mtime_ms == 0 || r.mtime_ms == 0 {
                return None;
            }
            Some(m.mtime_ms as i64 - r.mtime_ms as i64)
        })
        .collect();
    if deltas.len() < 3 {
        return 0;
    }
    deltas.sort_unstable();
    deltas[deltas.len() / 2]
}

// ---------------------------------------------------------------- diff (pure)

pub fn diff(master: &BTreeMap<String, Entry>, replica: &BTreeMap<String, Entry>, spec: &Spec, detector: Detector, now_ms: u64) -> Result<Plan, String> {
    if spec.delete_extras && master.is_empty() && !replica.is_empty() {
        return Err("master scan returned no entries; refusing to delete the entire replica".into());
    }
    let within = |m: &Entry| match spec.modified_within_ms {
        None => true,
        Some(w) => m.mtime_ms == 0 || (m.mtime_ms as i64 - spec.clock_offset_ms) >= now_ms as i64 - w as i64,
    };
    let mk = |rel: &str, kind: ActionKind, reason: Reason, bytes: u64, m: Option<&Entry>, r: Option<&Entry>| Action {
        rel: rel.to_string(),
        kind,
        reason,
        bytes,
        master: m.cloned(),
        replica: r.cloned(),
        checked: kind != ActionKind::Skip,
        state: State::Pending,
        progress: 0,
        error: None,
    };
    let mut creates = Vec::new();
    let mut deletes = Vec::new();
    let mut equals = Vec::new();
    for (rel, m) in master.iter() {
        // BTreeMap iterates ascending: parents before children.
        match replica.get(rel) {
            None => {
                if m.is_dir {
                    creates.push(mk(rel, ActionKind::Mkdir, Reason::New, 0, Some(m), None));
                } else if within(m) {
                    creates.push(mk(rel, ActionKind::Copy, Reason::New, m.size, Some(m), None));
                } else {
                    equals.push(mk(rel, ActionKind::Skip, Reason::Equal, 0, Some(m), None));
                }
            }
            Some(r) if m.is_dir && r.is_dir => equals.push(mk(rel, ActionKind::Skip, Reason::Equal, 0, Some(m), Some(r))),
            Some(r) if !m.is_dir && !r.is_dir => {
                if is_changed(detector, m, r, spec.clock_offset_ms) && within(m) {
                    creates.push(mk(rel, ActionKind::Copy, Reason::Changed, m.size, Some(m), Some(r)));
                } else {
                    equals.push(mk(rel, ActionKind::Skip, Reason::Equal, 0, Some(m), Some(r)));
                }
            }
            Some(r) => {
                if spec.delete_extras {
                    deletes.push(mk(rel, if r.is_dir { ActionKind::Rmdir } else { ActionKind::Delete }, Reason::Changed, 0, Some(m), Some(r)));
                }
                if m.is_dir {
                    creates.push(mk(rel, ActionKind::Mkdir, Reason::Changed, 0, Some(m), None));
                } else {
                    creates.push(mk(rel, ActionKind::Copy, Reason::Changed, m.size, Some(m), None));
                }
            }
        }
    }
    if spec.delete_extras {
        for (rel, r) in replica.iter().rev() {
            // descending: children before parents
            if master.contains_key(rel) {
                continue;
            }
            deletes.push(mk(rel, if r.is_dir { ActionKind::Rmdir } else { ActionKind::Delete }, Reason::Extra, 0, None, Some(r)));
        }
    }
    let mut actions = creates;
    actions.extend(deletes);
    actions.extend(equals);
    Ok(Plan { actions, replica_entry_count: replica.len(), filtered_count: 0, clock_offset_ms: spec.clock_offset_ms })
}

// ---------------------------------------------------------------- scan

pub enum Side {
    Local(PathBuf),
    Remote(Arc<Session>, String),
}

pub fn side_for(uri: &Uri) -> Result<Side, VfsError> {
    if uri.is_local() {
        Ok(Side::Local(uri.to_path()))
    } else {
        let (s, p) = locations::resolve(uri)?;
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

/// Enumerates a side into rel → Entry, skipping symlinks and filtered names (with their subtrees).
pub fn scan_side(side: &Side, rules: &[Rule], filtered_count: &mut usize, cancel: &AtomicBool) -> Result<BTreeMap<String, Entry>, VfsError> {
    let mut out = BTreeMap::new();
    match side {
        Side::Local(root) => scan_local(root, "", rules, filtered_count, &mut out, cancel)?,
        Side::Remote(session, root) => {
            // Try one recursive Scan; fall back to per-directory.
            let req = Value::obj().s("type", "Scan").s("location", session.location.clone()).s("path", root.clone()).b("recursive", true).done();
            let mut got_recursive = true;
            let r = session.plugin.request_stream_with(req, Some(cancel), |m| {
                if let Msg::Json(v) = m {
                    if let Some(entries) = v.get("entries").and_then(Value::as_arr) {
                        for e in entries {
                            let rel = e.str_field("rel").map(str::to_string).unwrap_or_else(|| e.str_field("name").unwrap_or("").to_string());
                            let name = rel.rsplit('/').next().unwrap_or("").to_string();
                            let kind = e.str_field("kind").unwrap_or("file");
                            if kind == "link" || rel.split('/').any(|seg| filtered(seg, rules)) {
                                if kind != "link" {
                                    *filtered_count += 1;
                                }
                                continue;
                            }
                            let m = e.get("meta").filter(|m| !matches!(m, Value::Null)).map(crate::vfs::remote::meta_from).unwrap_or_default();
                            out.insert(rel, Entry { rel: name, is_dir: kind == "dir", size: m.size, mtime_ms: m.mtime_ms, digest: None });
                        }
                    }
                }
            });
            match r {
                Ok(_) => {}
                Err(VfsError::Unsupported) => got_recursive = false,
                Err(e) => return Err(e),
            }
            if !got_recursive {
                out.clear();
                scan_remote(session, root, "", rules, filtered_count, &mut out, cancel)?;
            }
            // `rel` was used as the map key; store the full rel in the entry too.
            for (k, v) in out.iter_mut() {
                v.rel = k.clone();
            }
        }
    }
    Ok(out)
}

fn scan_local(root: &Path, prefix: &str, rules: &[Rule], filtered_count: &mut usize, out: &mut BTreeMap<String, Entry>, cancel: &AtomicBool) -> Result<(), VfsError> {
    let dir = if prefix.is_empty() { root.to_path_buf() } else { root.join(prefix) };
    for e in std::fs::read_dir(&dir)? {
        if cancel.load(Ordering::Relaxed) {
            return Err(VfsError::Io("cancelled".into()));
        }
        let e = e?;
        let name = e.file_name().to_string_lossy().into_owned();
        let md = e.metadata()?; // symlink_metadata via DirEntry
        if md.file_type().is_symlink() {
            continue;
        }
        if filtered(&name, rules) {
            *filtered_count += 1;
            continue;
        }
        let rel = if prefix.is_empty() { name.clone() } else { format!("{prefix}/{name}") };
        let mtime_ms = md.modified().ok().and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok()).map(|d| d.as_millis() as u64).unwrap_or(0);
        out.insert(rel.clone(), Entry { rel: rel.clone(), is_dir: md.is_dir(), size: if md.is_dir() { 0 } else { md.len() }, mtime_ms, digest: None });
        if md.is_dir() {
            scan_local(root, &rel, rules, filtered_count, out, cancel)?;
        }
    }
    Ok(())
}

fn scan_remote(session: &Arc<Session>, root: &str, prefix: &str, rules: &[Rule], filtered_count: &mut usize, out: &mut BTreeMap<String, Entry>, cancel: &AtomicBool) -> Result<(), VfsError> {
    if cancel.load(Ordering::Relaxed) {
        return Err(VfsError::Io("cancelled".into()));
    }
    let path = join_rel(root, prefix);
    let req = Value::obj().s("type", "Scan").s("location", session.location.clone()).s("path", path).done();
    let mut dirs = Vec::new();
    let mut missing_meta = Vec::new();
    session.plugin.request_stream_with(req, Some(cancel), |m| {
        if let Msg::Json(v) = m {
            if let Some(entries) = v.get("entries").and_then(Value::as_arr) {
                for e in entries {
                    let name = e.str_field("name").unwrap_or("").to_string();
                    let kind = e.str_field("kind").unwrap_or("file");
                    if kind == "link" {
                        continue;
                    }
                    if filtered(&name, rules) {
                        *filtered_count += 1;
                        continue;
                    }
                    let rel = if prefix.is_empty() { name.clone() } else { format!("{prefix}/{name}") };
                    let meta = e.get("meta").filter(|m| !matches!(m, Value::Null)).map(crate::vfs::remote::meta_from);
                    if meta.is_none() && kind != "dir" {
                        missing_meta.push(rel.clone());
                    }
                    let m = meta.unwrap_or_default();
                    out.insert(rel.clone(), Entry { rel: rel.clone(), is_dir: kind == "dir", size: m.size, mtime_ms: m.mtime_ms, digest: None });
                    if kind == "dir" {
                        dirs.push(rel);
                    }
                }
            }
        }
    })?;
    for rel in missing_meta {
        let v = session.plugin.request(Value::obj().s("type", "Stat").s("location", session.location.clone()).s("path", join_rel(root, &rel)).done())?;
        let m = crate::vfs::remote::meta_from(&v);
        if let Some(e) = out.get_mut(&rel) {
            e.size = m.size;
            e.mtime_ms = m.mtime_ms;
        }
    }
    for d in dirs {
        scan_remote(session, root, &d, rules, filtered_count, out, cancel)?;
    }
    Ok(())
}

/// For a local side, compute MD5 for files whose counterpart has a digest and the same size.
fn fill_local_digests(side: &Side, mine: &mut BTreeMap<String, Entry>, other: &BTreeMap<String, Entry>) {
    let Side::Local(root) = side else { return };
    for (rel, o) in other {
        if o.is_dir || usable_md5(&o.digest).is_none() {
            continue;
        }
        if let Some(e) = mine.get_mut(rel) {
            if !e.is_dir && e.size == o.size && e.digest.is_none() {
                if let Ok(bytes) = std::fs::read(root.join(rel)) {
                    e.digest = Some(crate::md5::hex(&bytes));
                }
            }
        }
    }
}

pub fn pick_detector(spec: &Spec) -> Detector {
    if spec.detector != Detector::Auto {
        return spec.detector;
    }
    // The remote side's plugin answers; local-only mirrors use size+mtime.
    let remote = if spec.direction == Direction::Upload { &spec.replica } else { &spec.master };
    if remote.is_local() {
        return Detector::SizeMtime;
    }
    let key = if spec.direction == Direction::Upload { "upload" } else { "download" };
    match crate::plugin::describe(&remote.scheme).and_then(|d| d.get("detector").and_then(|x| x.str_field(key).map(str::to_string))).as_deref() {
        Some("sizeOnly") => Detector::SizeOnly,
        Some("digest") => Detector::Digest,
        _ => Detector::SizeMtime,
    }
}

/// Full scan of both sides and the diff. Mutates the spec's offset when auto.
pub fn scan(spec: &mut Spec, cancel: &AtomicBool) -> Result<Plan, VfsError> {
    let rules = if spec.apply_filters { load_filters() } else { Vec::new() };
    let master_side = side_for(&spec.master)?;
    let replica_side = side_for(&spec.replica)?;
    let mut filtered_count = 0;
    let mut master = scan_side(&master_side, &rules, &mut filtered_count, cancel)?;
    let mut replica = scan_side(&replica_side, &rules, &mut filtered_count, cancel)?;
    let detector = pick_detector(spec);
    if detector == Detector::Digest {
        fill_local_digests(&master_side, &mut master, &replica);
        fill_local_digests(&replica_side, &mut replica, &master);
    }
    if spec.clock_offset_auto && detector != Detector::Digest {
        spec.clock_offset_ms = auto_offset(&master, &replica);
    }
    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0);
    let mut plan = diff(&master, &replica, spec, detector, now).map_err(VfsError::Io)?;
    plan.filtered_count = filtered_count;
    Ok(plan)
}

// ---------------------------------------------------------------- execute

pub struct Outcome {
    pub copies: u64,
    pub deletes: u64,
    pub bytes: u64,
    pub skipped: u64,
}

fn enforce_guards(plan: &Plan, spec: &Spec) -> Result<(), VfsError> {
    if plan.delete_count() == 0 {
        return Ok(());
    }
    if plan.blast_radius_fraction() > spec.blast_radius && !spec.confirmed_large_delete {
        return Err(VfsError::Io(format!("Safety: this would delete {} of {} replica items; confirm to proceed", plan.delete_count(), plan.replica_entry_count)));
    }
    for a in plan.actions.iter().filter(|a| a.checked && matches!(a.kind, ActionKind::Delete | ActionKind::Rmdir)) {
        if a.rel.is_empty() || a.rel.split('/').any(|s| s == "..") || a.rel.starts_with('/') {
            return Err(VfsError::Io(format!("Safety: refusing to delete outside the replica root: {}", a.rel)));
        }
    }
    Ok(())
}

fn audit(line: &str) {
    let d = std::env::var("XDG_STATE_HOME").map(PathBuf::from).unwrap_or_else(|_| crate::config::home().join(".local/state")).join("kiki");
    let _ = std::fs::create_dir_all(&d);
    if let Ok(mut f) = std::fs::OpenOptions::new().append(true).create(true).open(d.join("audit.log")) {
        use std::io::Write;
        let _ = writeln!(f, "{} {}", crate::ops::unix_now(), line);
    }
}

pub struct ExecCtx<'a> {
    pub cancel: &'a AtomicBool,
    pub workers: usize,
    /// Called after every action state change with the action index.
    pub on_change: &'a (dyn Fn(usize) + Sync),
    pub on_bytes: &'a (dyn Fn(u64) + Sync),
}

/// Runs the checked actions: creates level by level (parents first), then deletes deepest first.
pub fn execute(plan: &Arc<Mutex<Plan>>, spec: &Spec, ctx: &ExecCtx) -> Result<Outcome, VfsError> {
    {
        let p = plan.lock().unwrap();
        enforce_guards(&p, spec)?;
    }
    let master_side = side_for(&spec.master)?;
    let replica_side = side_for(&spec.replica)?;
    let snapshot: Vec<(usize, Action)> = plan.lock().unwrap().actions.iter().cloned().enumerate().filter(|(_, a)| a.checked && a.kind != ActionKind::Skip).collect();
    let depth = |rel: &str| rel.matches('/').count();
    let creates: Vec<&(usize, Action)> = snapshot.iter().filter(|(_, a)| matches!(a.kind, ActionKind::Copy | ActionKind::Mkdir)).collect();
    let deletes: Vec<&(usize, Action)> = snapshot.iter().filter(|(_, a)| matches!(a.kind, ActionKind::Delete | ActionKind::Rmdir)).collect();
    let counters = Mutex::new(Outcome { copies: 0, deletes: 0, bytes: 0, skipped: 0 });
    let run_level = |items: Vec<&(usize, Action)>| -> Result<(), VfsError> {
        let queue = Mutex::new(items.into_iter());
        std::thread::scope(|s| {
            let queue = &queue;
            let counters = &counters;
            let master_side = &master_side;
            let replica_side = &replica_side;
            let mut handles = Vec::new();
            for _ in 0..ctx.workers.max(1) {
                handles.push(s.spawn(move || loop {
                    if ctx.cancel.load(Ordering::Relaxed) {
                        return Err(VfsError::Io("cancelled".into()));
                    }
                    let next = { queue.lock().unwrap().next() };
                    let Some((idx, action)) = next else { return Ok(()) };
                    set_state(plan, *idx, State::Running, None);
                    (ctx.on_change)(*idx);
                    let r = run_action(action, master_side, replica_side, ctx);
                    match r {
                        Ok(()) => {
                            let mut c = counters.lock().unwrap();
                            match action.kind {
                                ActionKind::Copy => {
                                    c.copies += 1;
                                    c.bytes += action.bytes;
                                }
                                ActionKind::Delete | ActionKind::Rmdir => {
                                    c.deletes += 1;
                                    audit(&format!("mirror-delete {}", action.rel));
                                }
                                _ => {}
                            }
                            set_state(plan, *idx, State::Done, None);
                        }
                        Err(e) => {
                            if ctx.cancel.load(Ordering::Relaxed) {
                                return Err(e);
                            }
                            counters.lock().unwrap().skipped += 1;
                            audit(&format!("mirror-skip {} {}", action.rel, e.message()));
                            set_state(plan, *idx, State::Skipped, Some(e.message()));
                        }
                    }
                    (ctx.on_change)(*idx);
                }));
            }
            for h in handles {
                h.join().map_err(|_| VfsError::Io("worker panicked".into()))??;
            }
            Ok(())
        })
    };
    let max_depth = creates.iter().map(|(_, a)| depth(&a.rel)).max().unwrap_or(0);
    for d in 0..=max_depth {
        let level: Vec<&(usize, Action)> = creates.iter().copied().filter(|(_, a)| depth(&a.rel) == d).collect();
        if !level.is_empty() {
            run_level(level)?;
        }
    }
    let max_depth = deletes.iter().map(|(_, a)| depth(&a.rel)).max().unwrap_or(0);
    for d in (0..=max_depth).rev() {
        let level: Vec<&(usize, Action)> = deletes.iter().copied().filter(|(_, a)| depth(&a.rel) == d).collect();
        if !level.is_empty() {
            run_level(level)?;
        }
    }
    let c = counters.into_inner().unwrap();
    audit(&format!("MIRROR {} -> {} copies={} deletes={} bytes={} skipped={}", spec.master, spec.replica, c.copies, c.deletes, c.bytes, c.skipped));
    Ok(c)
}

fn set_state(plan: &Arc<Mutex<Plan>>, idx: usize, st: State, err: Option<String>) {
    let mut p = plan.lock().unwrap();
    if let Some(a) = p.actions.get_mut(idx) {
        a.state = st;
        a.error = err;
        if st == State::Done {
            a.progress = 100;
        }
    }
}

fn run_action(a: &Action, master: &Side, replica: &Side, ctx: &ExecCtx) -> Result<(), VfsError> {
    match a.kind {
        ActionKind::Mkdir => match replica {
            Side::Local(root) => std::fs::create_dir(root.join(&a.rel)).map_err(VfsError::from),
            Side::Remote(s, root) => s.plugin.request(Value::obj().s("type", "Mkdir").s("location", s.location.clone()).s("path", join_rel(root, &a.rel)).done()).map(|_| ()),
        },
        ActionKind::Delete | ActionKind::Rmdir => match replica {
            Side::Local(root) => crate::ops::remove_tree(&root.join(&a.rel)),
            Side::Remote(s, root) => s.plugin.request(Value::obj().s("type", "Delete").s("location", s.location.clone()).s("path", join_rel(root, &a.rel)).done()).map(|_| ()),
        },
        ActionKind::Copy => copy_action(a, master, replica, ctx),
        ActionKind::Skip => Ok(()),
    }
}

fn copy_action(a: &Action, master: &Side, replica: &Side, ctx: &ExecCtx) -> Result<(), VfsError> {
    let mtime = a.master.as_ref().map(|m| m.mtime_ms).unwrap_or(0);
    match (master, replica) {
        (Side::Local(mroot), Side::Local(rroot)) => {
            let dst = rroot.join(&a.rel);
            let _ = std::fs::remove_file(&dst);
            let mut p = crate::ops::Progress { cancel: ctx.cancel, bytes: &mut |n| (ctx.on_bytes)(n) };
            crate::ops::copy_file(&mroot.join(&a.rel), &dst, &mut p)?;
            Ok(())
        }
        (Side::Local(mroot), Side::Remote(s, rroot)) => {
            let mut f = std::fs::File::open(mroot.join(&a.rel))?;
            let req = Value::obj().s("type", "Write").s("location", s.location.clone()).s("path", join_rel(rroot, &a.rel)).u("size", a.bytes).u("mtime", mtime).done();
            let mut buf = vec![0u8; 512 * 1024];
            let cancel = ctx.cancel;
            s.plugin.write_stream(req, || {
                if cancel.load(Ordering::Relaxed) {
                    return None;
                }
                use std::io::Read;
                match f.read(&mut buf) {
                    Ok(0) | Err(_) => None,
                    Ok(n) => {
                        (ctx.on_bytes)(n as u64);
                        Some(buf[..n].to_vec())
                    }
                }
            })?;
            // Best-effort mtime so size+mtime stays idempotent on the next run.
            let _ = s.plugin.request(Value::obj().s("type", "SetMtime").s("location", s.location.clone()).s("path", join_rel(rroot, &a.rel)).u("mtime", mtime).done());
            Ok(())
        }
        (Side::Remote(s, mroot), Side::Local(rroot)) => {
            let dst = rroot.join(&a.rel);
            let tmp = dst.with_extension("kiki-part");
            let mut f = std::fs::File::create(&tmp)?;
            let req = Value::obj().s("type", "Read").s("location", s.location.clone()).s("path", join_rel(mroot, &a.rel)).done();
            let r = s.plugin.request_stream_with(req, Some(ctx.cancel), |m| {
                if let Msg::Binary(b) = m {
                    use std::io::Write;
                    let _ = f.write_all(&b);
                    (ctx.on_bytes)(b.len() as u64);
                }
            });
            drop(f);
            if let Err(e) = r {
                let _ = std::fs::remove_file(&tmp);
                return Err(e);
            }
            std::fs::rename(&tmp, &dst)?;
            if mtime > 0 {
                let _ = crate::ops::set_mtime(&dst, std::time::UNIX_EPOCH + std::time::Duration::from_millis(mtime));
            }
            Ok(())
        }
        (Side::Remote(ms, mroot), Side::Remote(rs, rroot)) if !Arc::ptr_eq(&ms.plugin, &rs.plugin) => {
            // Two plugin processes: stream the Read straight into the Write through a bounded
            // channel, so the transfer never touches the local disk and both sides run at once.
            let (tx, rx) = std::sync::mpsc::sync_channel::<Vec<u8>>(16);
            let read_req = Value::obj().s("type", "Read").s("location", ms.location.clone()).s("path", join_rel(mroot, &a.rel)).done();
            let write_req = Value::obj().s("type", "Write").s("location", rs.location.clone()).s("path", join_rel(rroot, &a.rel)).u("size", a.bytes).u("mtime", mtime).done();
            let reader = std::thread::scope(|scope| {
                let ms = Arc::clone(ms);
                let cancel = ctx.cancel;
                let producer = scope.spawn(move || {
                    let r = ms.plugin.request_stream_with(read_req, Some(cancel), |m| {
                        if let Msg::Binary(b) = m {
                            let _ = tx.send(b);
                        }
                    });
                    // Dropping tx ends the consumer's stream.
                    r.map(|_| ())
                });
                let w = rs.plugin.write_stream_with(write_req, Some(ctx.cancel), || match rx.recv() {
                    Ok(b) => {
                        (ctx.on_bytes)(b.len() as u64);
                        Some(b)
                    }
                    Err(_) => None,
                });
                let r = producer.join().unwrap_or_else(|_| Err(VfsError::Io("read thread panicked".into())));
                r.and(w.map(|_| ()))
            });
            reader
        }
        (Side::Remote(ms, mroot), Side::Remote(rs, rroot)) => {
            // Same plugin process on both sides: a plugin serves one binary stream at a time, so
            // spool through a temp file.
            let tmp = std::env::temp_dir().join(format!("kiki-mirror-{}-{}", std::process::id(), crate::md5::hex(a.rel.as_bytes())));
            let mut f = std::fs::File::create(&tmp)?;
            let req = Value::obj().s("type", "Read").s("location", ms.location.clone()).s("path", join_rel(mroot, &a.rel)).done();
            ms.plugin.request_stream(req, |m| {
                if let Msg::Binary(b) = m {
                    use std::io::Write;
                    let _ = f.write_all(&b);
                }
            })?;
            drop(f);
            let mut f = std::fs::File::open(&tmp)?;
            let req = Value::obj().s("type", "Write").s("location", rs.location.clone()).s("path", join_rel(rroot, &a.rel)).u("size", a.bytes).u("mtime", mtime).done();
            let mut buf = vec![0u8; 512 * 1024];
            let r = rs.plugin.write_stream(req, || {
                use std::io::Read;
                match f.read(&mut buf) {
                    Ok(0) | Err(_) => None,
                    Ok(n) => {
                        (ctx.on_bytes)(n as u64);
                        Some(buf[..n].to_vec())
                    }
                }
            });
            let _ = std::fs::remove_file(&tmp);
            r.map(|_| ())
        }
    }
}

// ---------------------------------------------------------------- report and json

pub fn report(spec: &Spec, plan: &Plan) -> String {
    let mut s = String::new();
    s.push_str("kiki mirror report\n==================\n\n");
    s.push_str(&format!("Direction:         {}\n", if spec.direction == Direction::Upload { "upload (local → remote)" } else { "download (remote → local)" }));
    s.push_str(&format!("Master (source):   {}\nReplica (dest):    {}\n", spec.master, spec.replica));
    s.push_str(&format!(
        "Detector:          {}\n",
        match pick_detector(spec) {
            Detector::SizeOnly => "size only",
            Detector::Digest => "digest",
            _ => "size+mtime",
        }
    ));
    s.push_str(&format!("Clock offset:      {} ms ({})\n", plan.clock_offset_ms, if spec.clock_offset_auto { "auto, subtracted from master mtime" } else { "manual" }));
    s.push_str(&format!(
        "Delete extras:     {}\nModified within:   {}\nFilters:           {} ({} filtered)\n\n",
        spec.delete_extras,
        spec.modified_within_ms.map(|w| format!("{} h", w / 3_600_000)).unwrap_or_else(|| "all files".into()),
        if spec.apply_filters { "on" } else { "off" },
        plan.filtered_count
    ));
    let copies = plan.actions.iter().filter(|a| a.kind == ActionKind::Copy).count();
    s.push_str(&format!("Summary: {} to copy, {} to delete, {} unchanged  (replica had {} items)\n\n", copies, plan.delete_count(), plan.count(Reason::Equal), plan.replica_entry_count));
    s.push_str("action/reason | path | master | replica | bytes\n");
    s.push_str(&"-".repeat(80));
    s.push('\n');
    let fmt = |e: &Option<Entry>| match e {
        Some(e) => format!("{}b @ {}", e.size, crate::listing::iso(e.mtime_ms)),
        None => "—".into(),
    };
    for a in plan.actions.iter().filter(|a| a.kind != ActionKind::Skip) {
        s.push_str(&format!("{:?}/{:?} | {} | master: {} | replica: {} | {}b\n", a.kind, a.reason, a.rel, fmt(&a.master), fmt(&a.replica), a.bytes));
    }
    s
}

pub fn action_json(a: &Action) -> Value {
    let e = |x: &Option<Entry>| match x {
        Some(e) => Value::obj().u("size", e.size).u("mtime", e.mtime_ms).v("mode", Value::Null).v("owner", Value::Null).v("group", Value::Null).v("digest", Value::Null).done(),
        None => Value::Null,
    };
    Value::obj()
        .s("rel", a.rel.clone())
        .s(
            "action",
            match a.kind {
                ActionKind::Copy => "copy",
                ActionKind::Mkdir => "mkdir",
                ActionKind::Delete => "delete",
                ActionKind::Rmdir => "rmdir",
                ActionKind::Skip => "skip",
            },
        )
        .s(
            "reason",
            match a.reason {
                Reason::New => "new",
                Reason::Changed => "changed",
                Reason::Extra => "extra",
                Reason::Equal => "equal",
            },
        )
        .u("bytes", a.bytes)
        .b("checked", a.checked)
        .v("master", e(&a.master))
        .v("replica", e(&a.replica))
        .s(
            "state",
            match a.state {
                State::Pending => "pending",
                State::Running => "running",
                State::Done => "done",
                State::Skipped => "skipped",
            },
        )
        .u("progress", a.progress as u64)
        .opt_s("error", a.error.as_deref())
        .done()
}

// ---------------------------------------------------------------- plan registry (scan results by job id)

pub struct Stored {
    pub spec: Spec,
    pub plan: Arc<Mutex<Plan>>,
}

fn plans() -> &'static Mutex<HashMap<u64, Arc<Stored>>> {
    static P: OnceLock<Mutex<HashMap<u64, Arc<Stored>>>> = OnceLock::new();
    P.get_or_init(|| Mutex::new(HashMap::new()))
}

pub fn store(job: u64, spec: Spec, plan: Plan) {
    plans().lock().unwrap().insert(job, Arc::new(Stored { spec, plan: Arc::new(Mutex::new(plan)) }));
}

pub fn stored(job: u64) -> Option<Arc<Stored>> {
    plans().lock().unwrap().get(&job).cloned()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn e(rel: &str, is_dir: bool, size: u64, mtime: u64) -> (String, Entry) {
        (rel.to_string(), Entry { rel: rel.to_string(), is_dir, size, mtime_ms: mtime, digest: None })
    }
    fn spec(delete: bool) -> Spec {
        Spec {
            master: Uri::parse("/m").unwrap(),
            replica: Uri::parse("/r").unwrap(),
            direction: Direction::Upload,
            delete_extras: delete,
            blast_radius: 0.5,
            confirmed_large_delete: false,
            clock_offset_ms: 0,
            clock_offset_auto: false,
            detector: Detector::SizeMtime,
            modified_within_ms: None,
            apply_filters: false,
        }
    }
    fn kinds(p: &Plan) -> Vec<(String, ActionKind, Reason)> {
        p.actions.iter().map(|a| (a.rel.clone(), a.kind, a.reason)).collect()
    }

    #[test]
    fn diff_matrix() {
        let m: BTreeMap<_, _> =
            [e("a", true, 0, 0), e("a/new.txt", false, 5, 1000), e("same.txt", false, 3, 5000), e("changed.txt", false, 3, 9000), e("tol.txt", false, 3, 7000), e("unknown.txt", false, 3, 0)].into_iter().collect();
        let r: BTreeMap<_, _> = [e("same.txt", false, 3, 5000), e("changed.txt", false, 3, 1000), e("tol.txt", false, 3, 5000), e("unknown.txt", false, 3, 123), e("extra", true, 0, 0), e("extra/old.txt", false, 1, 1)]
            .into_iter()
            .collect();
        let p = diff(&m, &r, &spec(false), Detector::SizeMtime, 100_000).unwrap();
        let k = kinds(&p);
        assert_eq!(k[0], ("a".into(), ActionKind::Mkdir, Reason::New)); // parent before child
        assert_eq!(k[1], ("a/new.txt".into(), ActionKind::Copy, Reason::New));
        assert_eq!(k[2], ("changed.txt".into(), ActionKind::Copy, Reason::Changed));
        assert!(k.iter().any(|x| x.0 == "tol.txt" && x.1 == ActionKind::Skip)); // exactly at tolerance is unchanged
        assert!(k.iter().any(|x| x.0 == "unknown.txt" && x.1 == ActionKind::Skip)); // unknown mtime: size only
        assert!(!k.iter().any(|x| x.1 == ActionKind::Delete)); // deletes off
        let p = diff(&m, &r, &spec(true), Detector::SizeMtime, 100_000).unwrap();
        let k = kinds(&p);
        let del: Vec<_> = k.iter().filter(|x| matches!(x.1, ActionKind::Delete | ActionKind::Rmdir)).collect();
        assert_eq!(del[0].0, "extra/old.txt"); // child before parent
        assert_eq!(del[1].0, "extra");
        assert_eq!(p.delete_count(), 2);
        assert!((p.blast_radius_fraction() - 2.0 / 6.0).abs() < 1e-9);
        assert_eq!(p.copy_bytes(), 8);
        // empty master with deletes on is refused
        assert!(diff(&BTreeMap::new(), &r, &spec(true), Detector::SizeMtime, 0).is_err());
        assert!(diff(&BTreeMap::new(), &r, &spec(false), Detector::SizeMtime, 0).is_ok());
    }

    #[test]
    fn window_and_offset() {
        let m: BTreeMap<_, _> = [e("old.txt", false, 1, 1_000), e("new.txt", false, 1, 90_000), e("ch.txt", false, 2, 95_000)].into_iter().collect();
        let r: BTreeMap<_, _> = [e("ch.txt", false, 3, 1), e("gone.txt", false, 1, 1)].into_iter().collect();
        let mut s = spec(true);
        s.modified_within_ms = Some(20_000);
        let p = diff(&m, &r, &s, Detector::SizeMtime, 100_000).unwrap();
        let k = kinds(&p);
        assert!(k.iter().any(|x| x.0 == "old.txt" && x.1 == ActionKind::Skip)); // outside the window: not copied
        assert!(k.iter().any(|x| x.0 == "new.txt" && x.1 == ActionKind::Copy));
        assert!(k.iter().any(|x| x.0 == "ch.txt" && x.1 == ActionKind::Copy));
        assert!(k.iter().any(|x| x.0 == "gone.txt" && x.1 == ActionKind::Delete)); // the window never prevents deletes
                                                                                   // offset: median of same-size pairs, needs three
        let m: BTreeMap<_, _> = [e("a", false, 1, 10_000), e("b", false, 1, 20_000), e("c", false, 1, 30_000), e("d", false, 9, 99_000)].into_iter().collect();
        let r: BTreeMap<_, _> = [e("a", false, 1, 6_400), e("b", false, 1, 16_400), e("c", false, 1, 26_500), e("d", false, 9, 1)].into_iter().collect();
        assert_eq!(auto_offset(&m, &r), 3_600);
        let two: BTreeMap<_, _> = m.iter().take(2).map(|(k, v)| (k.clone(), v.clone())).collect();
        assert_eq!(auto_offset(&two, &r), 0);
        assert!(!is_changed(Detector::SizeMtime, &m["a"], &r["a"], 3_600));
        assert!(is_changed(Detector::SizeMtime, &m["a"], &r["a"], 0));
        assert!(!is_changed(Detector::SizeOnly, &m["a"], &r["a"], 0));
    }

    #[test]
    fn local_end_to_end_is_idempotent() {
        let _guard = crate::ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let d = std::env::temp_dir().join(format!("kiki-mirror-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(d.join("m/sub")).unwrap();
        std::fs::create_dir_all(d.join("r")).unwrap();
        std::fs::write(d.join("m/a.txt"), b"aaa").unwrap();
        std::fs::write(d.join("m/sub/b.txt"), b"bb").unwrap();
        std::fs::write(d.join("m/sub/c.txt"), b"c").unwrap();
        std::fs::write(d.join("r/keep.txt"), b"k").unwrap();
        let mut s = spec(false);
        s.master = Uri::from_path(&d.join("m"));
        s.replica = Uri::from_path(&d.join("r"));
        let cancel = AtomicBool::new(false);
        let plan = scan(&mut s, &cancel).unwrap();
        assert_eq!(plan.actions.iter().filter(|a| a.kind == ActionKind::Copy).count(), 3);
        assert_eq!(plan.actions.iter().filter(|a| a.kind == ActionKind::Mkdir).count(), 1);
        let plan = Arc::new(Mutex::new(plan));
        let ctx = ExecCtx { cancel: &cancel, workers: 3, on_change: &|_| {}, on_bytes: &|_| {} };
        let out = execute(&plan, &s, &ctx).unwrap();
        assert_eq!((out.copies, out.deletes, out.skipped), (3, 0, 0));
        assert_eq!(std::fs::read(d.join("r/sub/b.txt")).unwrap(), b"bb");
        assert!(d.join("r/keep.txt").exists()); // additive run keeps replica-only files
                                                // second scan: nothing to do (mtime preserved)
        let plan2 = scan(&mut s, &cancel).unwrap();
        assert!(plan2.actions.iter().all(|a| a.kind == ActionKind::Skip));
        // edit one file: exactly one changed copy
        std::thread::sleep(std::time::Duration::from_millis(1100));
        std::fs::write(d.join("m/a.txt"), b"aaaa").unwrap();
        let plan3 = scan(&mut s, &cancel).unwrap();
        let ch: Vec<_> = plan3.actions.iter().filter(|a| a.kind == ActionKind::Copy).collect();
        assert_eq!(ch.len(), 1);
        assert_eq!(ch[0].reason, Reason::Changed);
        // deletes: blast radius refuses without confirmation, then removes the extra
        s.delete_extras = true;
        let plan4 = Arc::new(Mutex::new(scan(&mut s, &cancel).unwrap()));
        assert_eq!(plan4.lock().unwrap().delete_count(), 1);
        let big = {
            let mut p = plan4.lock().unwrap().clone();
            p.replica_entry_count = 1;
            p
        };
        assert!(execute(&Arc::new(Mutex::new(big)), &s, &ctx).is_err());
        s.confirmed_large_delete = true;
        let out = execute(&plan4, &s, &ctx).unwrap();
        assert_eq!(out.deletes, 1);
        assert!(!d.join("r/keep.txt").exists());
        let text = report(&s, &plan4.lock().unwrap());
        assert!(text.contains("Delete/Extra | keep.txt"));
        std::fs::remove_dir_all(&d).unwrap();
    }
}
