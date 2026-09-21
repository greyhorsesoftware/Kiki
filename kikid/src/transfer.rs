//! Copy, move, delete, rename and new-folder when an end is not this machine.
//!
//! `jobs.rs` does these on local paths, and until 2026-09-19 that was all there was: a URI that
//! was not `file://` was `Unsupported`, so dragging a file to or from a server, pasting into one,
//! F5/F6 between the panes, Delete, Rename and New folder all failed on anything remote. Only
//! Mirror could move bytes to a server. This is the same machinery (`mirror::copy_file`: the four
//! pairings of local and remote) driven by a list of things to copy instead of a diff.

use crate::jobs::Job;
use crate::json::Value;
use crate::mirror::{copy_file, scan_side, ExecCtx, Side};
use crate::vfs::uri::Uri;
use crate::vfs::{Meta, VfsError};
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

/// Does this op touch anything that is not this machine? (The trash is this machine.)
pub fn involves_remote(uris: &[&Uri]) -> bool {
    uris.iter().any(|u| !u.is_local() && u.scheme != "trash")
}

fn side(uri: &Uri) -> Result<Side, VfsError> {
    crate::mirror::side_for(uri)
}

fn child(s: &Side, name: &str) -> Side {
    match s {
        Side::Local(p) => Side::Local(p.join(name)),
        Side::Remote(sess, p) => Side::Remote(Arc::clone(sess), format!("{}/{}", p.trim_end_matches('/'), name)),
    }
}

fn request(s: &Side, ty: &str, rel: &str) -> Result<(), VfsError> {
    let Side::Remote(sess, root) = s else { unreachable!("request() is for the remote side") };
    let path = if rel.is_empty() { root.clone() } else { format!("{}/{}", root.trim_end_matches('/'), rel) };
    sess.plugin.request(sess.req(ty).s("path", path).done()).map(|_| ())
}

fn mkdir(s: &Side, rel: &str) -> Result<(), VfsError> {
    match s {
        Side::Local(p) => std::fs::create_dir_all(if rel.is_empty() { p.clone() } else { p.join(rel) }).map_err(VfsError::from),
        Side::Remote(..) => request(s, "Mkdir", rel),
    }
}

/// Remove `rel` under `s`, and everything in it when it is a folder. A plugin's `Delete` is one
/// file or one EMPTY folder (that is the contract, and what SFTP's and FTP's own commands do), so
/// a folder on a server is emptied from here, deepest first — which also makes it work the same
/// for every plugin there will ever be.
fn delete(s: &Side, rel: &str, is_dir: bool, cancel: &AtomicBool) -> Result<(), VfsError> {
    match s {
        Side::Local(p) => crate::ops::remove_tree(&if rel.is_empty() { p.clone() } else { p.join(rel) }),
        Side::Remote(..) if !is_dir => request(s, "Delete", rel),
        Side::Remote(..) => {
            let root = child(s, rel);
            let map = scan_side(&root, &[], &mut 0, cancel)?;
            let mut inside: Vec<(String, bool)> = map.iter().map(|(r, e)| (r.to_string(), e.is_dir)).collect();
            // Files first, then folders from the deepest up.
            inside.sort_by(|a, b| a.1.cmp(&b.1).then(b.0.matches('/').count().cmp(&a.0.matches('/').count())));
            for (r, _) in &inside {
                if cancel.load(Ordering::Relaxed) {
                    return Err(VfsError::Io("cancelled".into()));
                }
                request(&root, "Delete", r)?;
            }
            request(&root, "Delete", "")
        }
    }
}

/// What is in the folder `s`: name -> (is a folder, its meta). One listing, not a walk.
fn names(s: &Side, cancel: &AtomicBool) -> Result<HashMap<String, (bool, Meta)>, VfsError> {
    let mut out = HashMap::new();
    match s {
        Side::Local(p) => {
            for e in std::fs::read_dir(p)?.flatten() {
                if let Ok(m) = e.metadata() {
                    let mtime = m.modified().ok().and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok()).map(|d| d.as_millis() as u64).unwrap_or(0);
                    out.insert(e.file_name().to_string_lossy().into_owned(), (m.is_dir(), Meta { size: m.len(), mtime_ms: mtime, ..Meta::default() }));
                }
            }
        }
        Side::Remote(sess, root) => {
            let req = sess.req("Scan").s("path", root.clone()).done();
            sess.plugin.request_stream_with(req, Some(cancel), |m| {
                if let crate::plugin::Msg::Json(v) = m {
                    for e in v.get("entries").and_then(Value::as_arr).into_iter().flatten() {
                        let meta = e.get("meta").filter(|m| !matches!(m, Value::Null)).map(crate::vfs::remote::meta_from).unwrap_or_default();
                        out.insert(e.str_field("name").unwrap_or("").to_string(), (e.str_field("kind") == Some("dir"), meta));
                    }
                }
            })?;
        }
    }
    Ok(out)
}

/// A name that is not taken in the destination: "site copy", "site copy 2", …, extension kept.
fn unique(taken: &HashMap<String, (bool, Meta)>, name: &str) -> String {
    let (stem, ext) = match name.rfind('.').filter(|i| *i > 0) {
        Some(i) => (&name[..i], &name[i..]),
        None => (name, ""),
    };
    // The same name a local copy is given (`ops::unique_name`): "a (2).txt", then "a (3).txt". It
    // was "a copy.txt" here — one choice, two names, depending on where the file happened to be.
    (2..).map(|n| format!("{stem} ({n}){ext}")).find(|c| !taken.contains_key(c)).unwrap()
}

fn same_place(a: &Side, b: &Side) -> bool {
    match (a, b) {
        (Side::Local(_), Side::Local(_)) => true,
        (Side::Remote(x, _), Side::Remote(y, _)) => Arc::ptr_eq(x, y) || (x.location == y.location && Arc::ptr_eq(&x.plugin, &y.plugin)),
        _ => false,
    }
}

struct Item {
    name: String,
    from: Side,
    is_dir: bool,
    meta: Meta,
    /// Everything under it when it is a folder: (relative path, is a folder, size, mtime).
    tree: Vec<(String, bool, u64, u64)>,
}

/// What a copy to a server made, so that its undo can take back exactly that and nothing else:
/// every file it created, with what the server says about it, and every folder it created.
///
/// A copy never merges into what is already there — a name that is taken is asked about (replace,
/// keep both, skip) and a folder the copy did not make is not in here — so the folder it landed
/// in, and anything anybody else put beside it, is no part of the inverse.
struct Made {
    dest: Uri,
    /// What a later look at the server can tell about these files (`guard_for`); empty for a
    /// transfer that is not journalled this way, and then nothing is written down at all.
    guard: &'static str,
    /// `[uri, size, mtime]` each, as the DESTINATION reported them once the file had arrived.
    files: Vec<Value>,
    dirs: Vec<Value>,
}

impl Made {
    fn of(dest: &Uri, guard: &'static str) -> Made {
        Made { dest: dest.clone(), guard, files: Vec::new(), dirs: Vec::new() }
    }

    /// A file that has just arrived, and what the server says about it — asked of the destination
    /// itself, as `arrived_whole` asks before a move takes an original away. It is the server's
    /// answer that the undo will compare against, so it is the server's answer that is written
    /// down: a file uploaded where times cannot be set carries the server's clock, not ours.
    fn file(&mut self, at: &Side, rel: &str, uri: &Uri, sent: u64) {
        if self.guard.is_empty() {
            return;
        }
        let (size, mtime) = landed(at, rel).unwrap_or((sent, 0));
        self.files.push(Value::Arr(vec![Value::Str(uri.to_string()), Value::Uint(size), Value::Uint(mtime)]));
    }

    fn dir(&mut self, uri: &Uri) {
        if !self.guard.is_empty() {
            self.dirs.push(Value::Str(uri.to_string()));
        }
    }

    /// The op that takes it back; `None` for a copy that is not journalled this way, or one that
    /// has made nothing yet.
    fn inverse(&self) -> Option<Value> {
        if self.guard.is_empty() || (self.files.is_empty() && self.dirs.is_empty()) {
            return None;
        }
        Some(
            Value::obj()
                .s("op", "deleteCopies")
                .s("dest", self.dest.to_string())
                .s("guard", self.guard)
                .v("files", Value::Arr(self.files.clone()))
                .v("dirs", Value::Arr(self.dirs.clone()))
                .b("_silent", true)
                .done(),
        )
    }

    /// Write down what has arrived so far: a copy that is cancelled or fails half way has still
    /// put things on the server, and those are as undoable as if it had finished (`undo_so_far`).
    fn journal(&self, job: &Job) {
        if let Some(inv) = self.inverse() {
            job.undo_so_far(inv);
        }
    }
}

/// Size and modification time as the destination itself reports them. `None` when it cannot be
/// asked; a time of 0 means none is kept there, and what compares them goes by size alone (the
/// rule `mirror::is_changed` follows for a side that gives no times).
fn landed(at: &Side, rel: &str) -> Option<(u64, u64)> {
    match at {
        Side::Local(root) => {
            let m = std::fs::metadata(root.join(rel)).ok()?;
            Some((m.len(), m.modified().ok().and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok()).map(|d| d.as_millis() as u64).unwrap_or(0)))
        }
        Side::Remote(sess, root) => {
            let path = format!("{}/{}", root.trim_end_matches('/'), rel);
            let m = sess.plugin.request(sess.req("Stat").s("path", path).done()).ok()?;
            Some((m.u64_field("size")?, m.u64_field("mtime").unwrap_or(0)))
        }
    }
}

/// What a later look at this server can honestly tell about a file a copy put there: `"sizeMtime"`
/// where a file keeps a time that can be compared, `"size"` where it cannot. The same question the
/// mirror asks about the same server before it calls a file changed — the plugin's own answer
/// (`Describe.detector`, read by `pick_detector` for an upload to it). FTP cannot set a file's
/// time, so its plugin says `sizeOnly` and the size is all there is to go on.
fn guard_for(dest: &Uri) -> &'static str {
    let spec = crate::mirror::Spec::from_json(&Value::obj().s("master", "file:///").s("replica", dest.to_string()).s("direction", "upload").done());
    match spec.map(|s| crate::mirror::pick_detector(&s)) {
        Ok(crate::mirror::Detector::SizeOnly) => "size",
        _ => "sizeMtime",
    }
}

/// `copy` or `move` of `items` into the folder `dest`. Returns the op that undoes it, when there
/// is one: a copy that landed on this machine can be deleted again, and so can one that landed on
/// a server — from a note of exactly what it made there (`Made`), checked against the server
/// before anything is deleted (`undo_copy`). A move that crossed to or from a server is still not
/// journalled: taking one back means putting an original back, and there is no trash on the other
/// side to take it from.
pub fn copy_or_move(job: &Job, moving: bool, items: &[Uri], dest: &Uri, cancel: &AtomicBool) -> Result<Option<Value>, VfsError> {
    let to = side(dest)?;
    // 1. What is being taken, from where, and how much of it there is.
    let mut listings: HashMap<String, HashMap<String, (bool, Meta)>> = HashMap::new();
    let mut plan = Vec::new();
    let (mut files, mut bytes) = (0u64, 0u64);
    for u in items {
        let parent = u.parent().ok_or(VfsError::NotFound)?;
        let key = parent.to_string();
        if !listings.contains_key(&key) {
            listings.insert(key.clone(), names(&side(&parent)?, cancel)?);
        }
        let name = u.name().to_string();
        let (is_dir, meta) = listings[&key].get(&name).cloned().ok_or(VfsError::NotFound)?;
        let from = side(&parent)?;
        let mut tree = Vec::new();
        if is_dir {
            let map = scan_side(&child(&from, &name), &[], &mut 0, cancel)?;
            for (rel, e) in map.iter() {
                tree.push((rel.to_string(), e.is_dir, e.size, e.mtime_ms));
            }
            // Parents before what is in them.
            tree.sort_by(|a, b| a.0.matches('/').count().cmp(&b.0.matches('/').count()).then(a.0.cmp(&b.0)));
            files += tree.iter().filter(|t| !t.1).count() as u64;
            bytes += tree.iter().filter(|t| !t.1).map(|t| t.2).sum::<u64>();
        } else {
            files += 1;
            bytes += meta.size;
        }
        if plan.is_empty() {
            job.set_is_dir(is_dir);
        }
        plan.push(Item { name, from, is_dir, meta, tree });
    }
    job.set_totals(files.max(plan.len() as u64), bytes);

    // 2. One at a time, asking when the name is taken.
    let mut there = names(&to, cancel)?;
    let mut created: Vec<Uri> = Vec::new();
    // A copy to a server is journalled from what it actually puts there, file by file; anything
    // else that touches a server is not (see the doc comment).
    let mut made = Made::of(dest, if !moving && !dest.is_local() { guard_for(dest) } else { "" });
    // Files that could not be copied (name, why), and originals a move could not remove.
    let mut lost: Vec<(String, String)> = Vec::new();
    let mut kept: Vec<String> = Vec::new();
    let on_bytes = |n: u64| job.progress(0, n);
    let ctx = ExecCtx { cancel, workers: 1, on_change: &|_| {}, on_bytes: &on_bytes, exact_times: true };
    for it in &plan {
        if cancel.load(Ordering::Relaxed) {
            made.journal(job);
            return Err(VfsError::Io("cancelled".into()));
        }
        // Did every part of this item arrive? Decides whether a move may take the original away.
        let mut whole = true;
        let mut target = it.name.clone();
        if let Some((_, existing)) = there.get(&target) {
            match job.ask_collision_meta(&dest.join(&target), existing, &it.meta).as_str() {
                "replace" => delete(&to, &target, there.get(&target).map(|t| t.0).unwrap_or(false), cancel)?,
                "keepBoth" => target = unique(&there, &it.name),
                _ => {
                    job.progress(if it.is_dir { it.tree.iter().filter(|t| !t.1).count().max(1) as u64 } else { 1 }, 0);
                    continue;
                }
            }
        }
        // A move inside one place is a rename: nothing crosses a wire.
        if moving && same_place(&it.from, &to) {
            rename(&it.from, &it.name, &to, &target)?;
            job.progress(if it.is_dir { it.tree.iter().filter(|t| !t.1).count().max(1) as u64 } else { 1 }, 0);
        } else if it.is_dir {
            mkdir(&to, &target)?;
            made.dir(&dest.join(&target));
            let (src_root, dst_root) = (child(&it.from, &it.name), child(&to, &target));
            let landing = dest.join(&target);
            for (rel, is_dir, size, mtime) in &it.tree {
                if cancel.load(Ordering::Relaxed) {
                    made.journal(job);
                    return Err(VfsError::Io("cancelled".into()));
                }
                let shown = format!("{}/{rel}", it.name);
                // A file inside a folder that failed is not attempted: there is nowhere for it.
                if lost.iter().any(|(gone, _)| shown.starts_with(&format!("{gone}/"))) {
                    continue;
                }
                let done = if *is_dir {
                    mkdir(&dst_root, rel)
                } else {
                    job.file_started(&shown, *size);
                    copy_file(&src_root, rel, &dst_root, rel, *size, *mtime, &ctx).and_then(|_| if moving { arrived_whole(&dst_root, rel, *size) } else { Ok(()) })
                };
                match done {
                    Ok(()) if !*is_dir => {
                        made.file(&dst_root, rel, &landing.join(rel), *size);
                        job.progress(1, 0);
                    }
                    Ok(()) => made.dir(&landing.join(rel)),
                    Err(e) if is_cancel(&e) => {
                        made.journal(job);
                        return Err(e);
                    }
                    // One bad file does not stop the other four hundred: it is noted, the rest
                    // goes on, and the job fails at the end saying which (as a mirror skips).
                    Err(e) => {
                        crate::joblog::say(job.id, "error", format!("{shown}: {}", e.message()));
                        lost.push((shown.clone(), why_lost(&shown, &e)));
                        whole = false;
                    }
                }
            }
            if it.tree.iter().all(|t| t.1) {
                job.progress(1, 0);
            }
        } else {
            job.file_started(&it.name, it.meta.size);
            match copy_file(&it.from, &it.name, &to, &target, it.meta.size, it.meta.mtime_ms, &ctx).and_then(|_| if moving { arrived_whole(&to, &target, it.meta.size) } else { Ok(()) }) {
                Ok(()) => {
                    made.file(&to, &target, &dest.join(&target), it.meta.size);
                    job.progress(1, 0);
                }
                Err(e) if is_cancel(&e) => {
                    made.journal(job);
                    return Err(e);
                }
                Err(e) => {
                    crate::joblog::say(job.id, "error", format!("{}: {}", it.name, e.message()));
                    lost.push((it.name.clone(), why_lost(&it.name, &e)));
                    whole = false;
                }
            }
        }
        // The original goes only once ALL of it has arrived and been seen to: a move between
        // machines is a copy and then a delete, and it loses nothing only in that order. An item
        // with anything missing keeps its original, whole.
        if moving && !same_place(&it.from, &to) && whole {
            crate::joblog::say(job.id, "debug", format!("{}: arrived and checked, removing the original", it.name));
            if let Err(e) = delete(&it.from, &it.name, it.is_dir, cancel) {
                if is_cancel(&e) {
                    return Err(e);
                }
                // Not "the move failed": everything is where it was sent. Say what is true.
                crate::joblog::say(job.id, "warn", format!("{}: copied, but the original could not be removed: {}", it.name, e.message()));
                kept.push(format!("{} ({})", it.name, e.message()));
            }
        }
        there.insert(target.clone(), (it.is_dir, it.meta.clone()));
        created.push(dest.join(&target));
        if dest.is_local() {
            // What "Reveal" shows, and — for a copy — what undo can take back even if the job
            // stops on a later item.
            job.set_reveal(&dest.join(&target).to_string());
            if !moving {
                job.undo_so_far(arrived(&created));
            }
        }
        // The same for a copy to a server, from what this item really put there.
        made.journal(job);
    }
    for u in items.iter().filter_map(|u| u.parent()).chain(std::iter::once(dest.clone())) {
        invalidate(&u);
    }
    if !lost.is_empty() {
        let first: Vec<String> = lost.iter().take(3).map(|(n, why)| format!("{n} ({why})")).collect();
        let more = if lost.len() > 3 { format!(", and {} more — see the log", lost.len() - 3) } else { String::new() };
        return Err(VfsError::Io(format!("{} of {} could not be {}: {}{more}", lost.len(), files.max(plan.len() as u64), if moving { "moved; their originals are untouched" } else { "copied" }, first.join(", "))));
    }
    if !kept.is_empty() {
        return Err(VfsError::Io(format!("copied, but the original could not be removed: {}", kept.join(", "))));
    }
    Ok(match made.inverse() {
        Some(inv) => Some(inv),
        None => (!moving && dest.is_local()).then(|| arrived(&created)),
    })
}

/// Why a file did not make it, for the person reading. One case gets words of its own: a name
/// that is not valid UTF-8. A transfer knows its files by text paths, so such a name has already
/// lost its odd bytes by the time it is opened, and "NotFound" for a file that is plainly there
/// explains nothing. (A limit of 0.1.0, recorded in plan 31; the listing itself shows such files.)
fn why_lost(name: &str, e: &VfsError) -> String {
    if name.contains('\u{FFFD}') && matches!(e, VfsError::NotFound) {
        "its name is not valid UTF-8, which kiki cannot transfer yet".to_string()
    } else {
        e.message()
    }
}

fn is_cancel(e: &VfsError) -> bool {
    matches!(e, VfsError::Io(m) if m == "cancelled")
}

/// Before a move deletes the original: is what arrived as long as what was sent? Asked of the
/// destination itself — the local file's length, or a `Stat` of the server's — and not taken
/// from the copy's own count of what it wrote. (Size only: FTP cannot set a file's time, so a
/// time that differs proves nothing there.)
fn arrived_whole(to: &Side, rel: &str, size: u64) -> Result<(), VfsError> {
    let got = match to {
        Side::Local(root) => std::fs::metadata(root.join(rel))?.len(),
        Side::Remote(sess, root) => {
            let path = format!("{}/{}", root.trim_end_matches('/'), rel);
            sess.plugin.request(sess.req("Stat").s("path", path).done())?.u64_field("size").unwrap_or(u64::MAX)
        }
    };
    if got == size {
        Ok(())
    } else {
        Err(VfsError::Io(format!("arrived as {got} bytes of {size}; the original is kept")))
    }
}

/// The inverse of a download: delete what arrived.
fn arrived(created: &[Uri]) -> Value {
    Value::obj().s("op", "delete").v("items", Value::Arr(created.iter().map(|u| Value::Str(u.to_string())).collect())).b("_silent", true).done()
}

/// How the undo of a copy to a server went, for the line that tells the person (`jobs`): what was
/// deleted, what was left where it is, and what the server refused.
#[derive(Default)]
pub struct TakenBack {
    /// Files deleted and folders removed — all of it for good.
    pub deleted: u64,
    /// Left alone: not what the copy left any more, gone already, or a folder somebody has since
    /// put something of their own in. Each one is named in the job's log.
    pub kept: u64,
    /// The server would not: named in the job's log, with what it said.
    pub failed: u64,
}

/// Is this still the file the copy put there? The check `guard` asks for (written down when the
/// copy finished): the size always, the time as well where the server keeps one that can be
/// compared. A time of 0 on either side means none is known — the rule `mirror::is_changed`
/// follows — and the size is then all there is.
fn still_ours(guard: &str, was: (u64, u64), now: (u64, u64)) -> bool {
    now.0 == was.0 && (guard == "size" || was.1 == 0 || now.1 == 0 || was.1 == now.1)
}

/// The inverse of a copy whose destination was a server: delete exactly what that copy created
/// there, and only where it is still what the copy left. A server has no trash, so this cannot be
/// taken back in its turn — which is why every file is asked about first, why a folder goes only
/// while it is empty (somebody may have put something of their own in it), and why what was left
/// is counted for the toast to say.
pub fn undo_copy(job: &Job, op: &Value, cancel: &AtomicBool) -> Result<TakenBack, VfsError> {
    let guard = op.str_field("guard").unwrap_or("sizeMtime");
    let files: Vec<Value> = op.get("files").and_then(Value::as_arr).map(<[Value]>::to_vec).unwrap_or_default();
    let mut dirs: Vec<Uri> = op
        .get("dirs")
        .and_then(Value::as_arr)
        .map(|a| a.iter().filter_map(Value::as_str).filter_map(|s| Uri::parse(s).ok()).collect())
        .unwrap_or_default();
    job.set_totals((files.len() + dirs.len()) as u64, 0);
    let mut out = TakenBack::default();
    let mut touched: Vec<Uri> = Vec::new();
    let note = |out: &mut TakenBack, level: &str, line: String| {
        crate::joblog::say(job.id, level, line);
        out.kept += 1;
    };
    for f in &files {
        if cancel.load(Ordering::Relaxed) {
            return Err(VfsError::Io("cancelled".into()));
        }
        let a = f.as_arr().unwrap_or_default();
        let Some(uri) = a.first().and_then(Value::as_str).and_then(|s| Uri::parse(s).ok()) else { continue };
        let was = (a.get(1).and_then(Value::as_u64).unwrap_or(0), a.get(2).and_then(Value::as_u64).unwrap_or(0));
        let parent = uri.parent().ok_or(VfsError::NotFound)?;
        let at = side(&parent)?;
        job.file_started(uri.name(), was.0);
        match landed(&at, uri.name()) {
            None => note(&mut out, "warn", format!("{}: no longer there — nothing to take back", uri.display())),
            Some(now) if !still_ours(guard, was, now) => note(
                &mut out,
                "warn",
                format!("{}: changed since the copy ({} bytes at {}, now {} at {}) — left alone", uri.display(), was.0, was.1, now.0, now.1),
            ),
            Some(_) => match delete(&at, uri.name(), false, cancel) {
                Ok(()) => {
                    out.deleted += 1;
                    touched.push(parent.clone());
                }
                Err(e) if is_cancel(&e) => return Err(e),
                Err(e) => {
                    crate::joblog::say(job.id, "error", format!("{}: {}", uri.display(), e.message()));
                    out.failed += 1;
                }
            },
        }
        job.progress(1, 0);
    }
    // The folders it made, the deepest first so that a tree empties from the bottom.
    dirs.sort_by_key(|u| std::cmp::Reverse(u.path.matches('/').count()));
    for uri in &dirs {
        if cancel.load(Ordering::Relaxed) {
            return Err(VfsError::Io("cancelled".into()));
        }
        let parent = uri.parent().ok_or(VfsError::NotFound)?;
        match names(&side(uri)?, cancel) {
            Err(_) => note(&mut out, "warn", format!("{}: no longer there — nothing to take back", uri.display())),
            Ok(inside) if !inside.is_empty() => note(&mut out, "warn", format!("{}: {} things in it still — left alone", uri.display(), inside.len())),
            // Empty, and a plugin's `Delete` takes an empty folder: no need to walk it again.
            Ok(_) => match delete(&side(&parent)?, uri.name(), false, cancel) {
                Ok(()) => {
                    out.deleted += 1;
                    touched.push(parent.clone());
                }
                Err(e) if is_cancel(&e) => return Err(e),
                Err(e) => {
                    crate::joblog::say(job.id, "error", format!("{}: {}", uri.display(), e.message()));
                    out.failed += 1;
                }
            },
        }
        job.progress(1, 0);
    }
    touched.sort_by_key(|u| u.to_string());
    touched.dedup();
    for u in &touched {
        invalidate(u);
    }
    Ok(out)
}

fn rename(from: &Side, name: &str, to: &Side, target: &str) -> Result<(), VfsError> {
    match (from, to) {
        (Side::Local(a), Side::Local(b)) => std::fs::rename(a.join(name), b.join(target)).map_err(VfsError::from),
        (Side::Remote(s, a), Side::Remote(_, b)) => s
            .plugin
            .request(s.req("Rename").s("from", format!("{}/{}", a.trim_end_matches('/'), name)).s("to", format!("{}/{}", b.trim_end_matches('/'), target)).done())
            .map(|_| ()),
        _ => Err(VfsError::Unsupported),
    }
}

/// A listing of `dir` that is cached, here or in the plugin, is no longer true.
fn invalidate(dir: &Uri) {
    crate::listing::changed(dir);
}

pub fn delete_items(job: &Job, items: &[Uri], cancel: &AtomicBool) -> Result<(), VfsError> {
    job.set_totals(items.len() as u64, 0);
    for u in items {
        if cancel.load(Ordering::Relaxed) {
            return Err(VfsError::Io("cancelled".into()));
        }
        let parent = u.parent().ok_or(VfsError::NotFound)?;
        let s = side(&parent)?;
        let is_dir = names(&s, cancel)?.get(u.name()).map(|e| e.0).ok_or(VfsError::NotFound)?;
        delete(&s, u.name(), is_dir, cancel)?;
        job.progress(1, 0);
        invalidate(&parent);
    }
    Ok(())
}

/// `uri` is the folder to be made, as the op names it: its parent and its name.
pub fn make_dir(uri: &Uri) -> Result<(), VfsError> {
    let parent = uri.parent().ok_or(VfsError::NotFound)?;
    mkdir(&side(&parent)?, uri.name())?;
    invalidate(&parent);
    Ok(())
}

pub fn rename_item(item: &Uri, name: &str) -> Result<Uri, VfsError> {
    if name.is_empty() || name.contains('/') {
        return Err(VfsError::Io("a name cannot be empty or contain /".into()));
    }
    let parent = item.parent().ok_or(VfsError::NotFound)?;
    let s = side(&parent)?;
    rename(&s, item.name(), &s, name)?;
    invalidate(&parent);
    Ok(parent.join(name))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_taken_name_gets_the_number_a_local_copy_would_with_its_extension_kept() {
        let mut t: HashMap<String, (bool, Meta)> = HashMap::new();
        assert_eq!(unique(&t, "site"), "site (2)");
        t.insert("index (2).html".into(), (false, Meta::default()));
        assert_eq!(unique(&t, "index.html"), "index (3).html");
        assert_eq!(unique(&t, ".env"), ".env (2)", "a leading dot is not an extension");
        // The same answer `ops::unique_name` gives on this machine: one choice, one name.
        let d = std::env::temp_dir().join(format!("kiki-unique-{}", std::process::id()));
        std::fs::create_dir_all(&d).unwrap();
        std::fs::write(d.join("a.txt"), b"").unwrap();
        assert_eq!(crate::ops::unique_name(&d, "a.txt"), unique(&HashMap::from([("a.txt".to_string(), (false, Meta::default()))]), "a.txt"));
        std::fs::remove_dir_all(&d).unwrap();
    }

    #[test]
    fn the_trash_and_this_machine_are_not_remote() {
        let l = Uri::parse("file:///home/t/a").unwrap();
        let t = Uri::parse("trash:///a").unwrap();
        let r = Uri::parse("sftp://nas/srv/a").unwrap();
        assert!(!involves_remote(&[&l, &t]));
        assert!(involves_remote(&[&l, &r]));
    }
}
