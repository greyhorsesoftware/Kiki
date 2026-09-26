//! Walking each side into a `SideMap`, locally or through a location plugin.

use super::detect::usable_md5;
use super::*;

/// How many entries a side has been seen to hold so far, said every `REPORT_EVERY` entries, so
/// the Preflight screen can count up while a big tree is walked. There is no total to put it
/// against: nobody knows how many entries a tree holds until it has been walked. How often that
/// reaches a window is the job's own business — a `JobEvent` goes out at most ten times a second
/// however often it is told (`Job::set_progress`).
pub struct Seen<'a> {
    pub n: u64,
    said: u64,
    report: &'a dyn Fn(u64),
}

const REPORT_EVERY: u64 = 500;

impl<'a> Seen<'a> {
    pub fn new(report: &'a dyn Fn(u64)) -> Seen<'a> {
        Seen { n: 0, said: 0, report }
    }
    fn one(&mut self) {
        self.n += 1;
        if self.n - self.said >= REPORT_EVERY {
            self.flush();
        }
    }
    fn flush(&mut self) {
        self.said = self.n;
        (self.report)(self.n);
    }
}

/// What a walk of one side carries with it: the filter rules, where the skipped names are
/// counted, the token that stops it, and the running count of what it has seen.
struct Walk<'a, 'b> {
    rules: &'a [Rule],
    filtered: &'a mut usize,
    cancel: &'a AtomicBool,
    seen: &'a mut Seen<'b>,
}

/// Enumerates a side into rel → Entry, skipping symlinks and filtered names (with their subtrees).
pub fn scan_side(side: &Side, rules: &[Rule], filtered_count: &mut usize, cancel: &AtomicBool) -> Result<SideMap, VfsError> {
    scan_side_counting(side, rules, filtered_count, cancel, &mut Seen::new(&|_| {}))
}

pub fn scan_side_counting(side: &Side, rules: &[Rule], filtered_count: &mut usize, cancel: &AtomicBool, seen: &mut Seen) -> Result<SideMap, VfsError> {
    let w = &mut Walk { rules, filtered: filtered_count, cancel, seen };
    let mut out = SideMap::new();
    match side {
        Side::Local(root) => scan_local(root, "", w, &mut out)?,
        Side::Remote(session, root) => {
            // Try one recursive Scan; fall back to per-directory.
            let req = session.req("Scan").s("path", root.clone()).b("recursive", true).done();
            let mut got_recursive = true;
            let r = session.plugin.request_stream_with(req, Some(w.cancel), |m| {
                if let Msg::Json(v) = m {
                    if let Some(entries) = v.get("entries").and_then(Value::as_arr) {
                        for e in entries {
                            let rel = e.str_field("rel").map(str::to_string).unwrap_or_else(|| e.str_field("name").unwrap_or("").to_string());
                            let kind = e.str_field("kind").unwrap_or("file");
                            if kind == "link" {
                                continue;
                            }
                            // A filtered name takes its whole subtree with it, and is counted
                            // once where it starts. This walk sees every descendant of a skipped
                            // folder, where the per-directory one below never goes near them, and
                            // it used to count every one: the same tree read "6 filtered out"
                            // through SFTP and "1 filtered out" locally.
                            if let Some(topmost) = filtered_rel(&rel, w.rules) {
                                if topmost {
                                    *w.filtered += 1;
                                }
                                continue;
                            }
                            let meta = e.get("meta").filter(|m| !matches!(m, Value::Null));
                            let m = meta.map(crate::vfs::remote::meta_from).unwrap_or_default();
                            out.insert(&rel, Entry { path: 0, is_dir: kind == "dir", size: m.size, mtime_ms: m.mtime_ms, digest: digest_of(meta) });
                            w.seen.one();
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
                scan_remote(session, root, "", w, &mut out)?;
            }
        }
    }
    out.finish();
    Ok(out)
}

fn scan_local(root: &Path, prefix: &str, w: &mut Walk, out: &mut SideMap) -> Result<(), VfsError> {
    let dir = if prefix.is_empty() { root.to_path_buf() } else { root.join(prefix) };
    for e in std::fs::read_dir(&dir)? {
        if w.cancel.load(Ordering::Relaxed) {
            return Err(VfsError::said(1230, &[], "cancelled"));
        }
        let e = e?;
        let name = e.file_name().to_string_lossy().into_owned();
        let md = e.metadata()?; // symlink_metadata via DirEntry
        if md.file_type().is_symlink() {
            continue;
        }
        if filtered(&name, w.rules) {
            *w.filtered += 1;
            continue;
        }
        let rel = if prefix.is_empty() { name.clone() } else { format!("{prefix}/{name}") };
        let mtime_ms = md.modified().ok().and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok()).map(|d| d.as_millis() as u64).unwrap_or(0);
        out.insert(&rel, Entry { path: 0, is_dir: md.is_dir(), size: if md.is_dir() { 0 } else { md.len() }, mtime_ms, digest: None });
        w.seen.one();
        if md.is_dir() {
            scan_local(root, &rel, w, out)?;
        }
    }
    Ok(())
}

fn scan_remote(session: &Arc<Session>, root: &str, prefix: &str, w: &mut Walk, out: &mut SideMap) -> Result<(), VfsError> {
    if w.cancel.load(Ordering::Relaxed) {
        return Err(VfsError::said(1230, &[], "cancelled"));
    }
    let path = join_rel(root, prefix);
    let req = session.req("Scan").s("path", path).done();
    let mut dirs = Vec::new();
    let mut missing_meta = Vec::new();
    session.plugin.request_stream_with(req, Some(w.cancel), |m| {
        if let Msg::Json(v) = m {
            if let Some(entries) = v.get("entries").and_then(Value::as_arr) {
                for e in entries {
                    let name = e.str_field("name").unwrap_or("").to_string();
                    let kind = e.str_field("kind").unwrap_or("file");
                    if kind == "link" {
                        continue;
                    }
                    if filtered(&name, w.rules) {
                        *w.filtered += 1;
                        continue;
                    }
                    let rel = if prefix.is_empty() { name.clone() } else { format!("{prefix}/{name}") };
                    let raw = e.get("meta").filter(|m| !matches!(m, Value::Null));
                    let meta = raw.map(crate::vfs::remote::meta_from);
                    if meta.is_none() && kind != "dir" {
                        missing_meta.push(rel.clone());
                    }
                    let m = meta.unwrap_or_default();
                    out.insert(&rel, Entry { path: 0, is_dir: kind == "dir", size: m.size, mtime_ms: m.mtime_ms, digest: digest_of(raw) });
                    w.seen.one();
                    if kind == "dir" {
                        dirs.push(rel);
                    }
                }
            }
        }
    })?;
    for rel in missing_meta {
        let v = session.plugin.request(session.req("Stat").s("path", join_rel(root, &rel)).done())?;
        let m = crate::vfs::remote::meta_from(&v);
        if let Some(e) = out.get_mut(rel.as_str()) {
            e.size = m.size;
            e.mtime_ms = m.mtime_ms;
        }
    }
    for d in dirs {
        scan_remote(session, root, &d, w, out)?;
    }
    Ok(())
}

/// The content hash a backend hands out with a listing (an object store's ETag), where it does.
/// Nothing shipped answers this yet; the engine's Digest detector is what reads it.
fn digest_of(meta: Option<&Value>) -> Option<String> {
    meta.and_then(|m| m.str_field("digest")).filter(|d| !d.is_empty()).map(str::to_string)
}

/// For a local side, compute MD5 for files whose counterpart has a digest and the same size.
/// Streamed, and given up on the moment `cancel` is set: these are whole files, and a cancel that
/// was only looked at between them would wait out the one file somebody is cancelling because of.
pub(super) fn fill_local_digests(side: &Side, mine: &mut SideMap, other: &SideMap, cancel: &AtomicBool) {
    let Side::Local(root) = side else { return };
    for (rel, o) in other.iter() {
        if cancel.load(Ordering::Relaxed) {
            return;
        }
        if o.is_dir || usable_md5(&o.digest).is_none() {
            continue;
        }
        if let Some(e) = mine.get_mut(rel) {
            if !e.is_dir && e.size == o.size && e.digest.is_none() {
                if let Ok(mut f) = std::fs::File::open(root.join(rel)) {
                    e.digest = crate::md5::hex_reader(&mut f, cancel).ok().flatten();
                }
            }
        }
    }
}

/// A folder mirrored into something inside it copies its own tree into itself, and with deletes
/// on takes what it has just made for extras. The workspace refuses the pair before it asks; this
/// is the same rule for whoever asks without it.
pub(super) fn overlap(master: &Uri, replica: &Uri) -> Result<(), VfsError> {
    if master.scheme != replica.scheme || master.authority != replica.authority {
        return Ok(());
    }
    let inside = |inner: &str, outer: &str| inner.strip_prefix(outer.trim_end_matches('/')).is_some_and(|rest| rest.starts_with('/'));
    if master.path == replica.path {
        Err(VfsError::said(1220, &[], "the source and the destination are the same folder"))
    } else if inside(&replica.path, &master.path) {
        Err(VfsError::said(1221, &[], "the destination is inside the source folder"))
    } else if inside(&master.path, &replica.path) {
        Err(VfsError::said(1222, &[], "the source is inside the destination folder"))
    } else {
        Ok(())
    }
}

/// Full scan of both sides and the diff. Mutates the spec's offset when auto.
pub fn scan(spec: &mut Spec, cancel: &AtomicBool) -> Result<Plan, VfsError> {
    scan_counting(spec, cancel, &|_| {})
}

/// The same, saying how many entries it has seen as it goes: what the Preflight screen counts up
/// while a big tree is compared. The number is both sides added together, in the order they are
/// walked, because there is no total to put either of them against.
pub fn scan_counting(spec: &mut Spec, cancel: &AtomicBool, report: &dyn Fn(u64)) -> Result<Plan, VfsError> {
    overlap(&spec.master, &spec.replica)?;
    let rules = if spec.apply_filters { load_filters() } else { Vec::new() };
    // Connecting is part of the compare, and a server that never answers is the likeliest reason
    // to cancel one: the token goes with it, so Cancel does not wait out the connect timeout.
    let master_side = side_for_cancellable(&spec.master, cancel)?;
    let replica_side = side_for_cancellable(&spec.replica, cancel)?;
    let mut filtered_count = 0;
    let mut seen = Seen::new(report);
    let mut master = scan_side_counting(&master_side, &rules, &mut filtered_count, cancel, &mut seen)?;
    let mut replica = scan_side_counting(&replica_side, &rules, &mut filtered_count, cancel, &mut seen)?;
    seen.flush();
    let detector = pick_detector(spec);
    if detector == Detector::Digest {
        fill_local_digests(&master_side, &mut master, &replica, cancel);
        fill_local_digests(&replica_side, &mut replica, &master, cancel);
        if cancel.load(Ordering::Relaxed) {
            return Err(VfsError::said(1230, &[], "cancelled"));
        }
    }
    if spec.clock_offset_auto && detector != Detector::Digest {
        spec.clock_offset_ms = auto_offset(&master, &replica);
    }
    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0);
    let mut plan = diff(&master, &replica, spec, detector, now).map_err(VfsError::Io)?;
    plan.filtered_count = filtered_count;
    Ok(plan)
}
