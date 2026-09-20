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
    sess.plugin.request(Value::obj().s("type", ty).s("location", sess.location.clone()).s("path", path).done()).map(|_| ())
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
            let req = Value::obj().s("type", "Scan").s("location", sess.location.clone()).s("path", root.clone()).done();
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
    (1..).map(|n| if n == 1 { format!("{stem} copy{ext}") } else { format!("{stem} copy {n}{ext}") }).find(|c| !taken.contains_key(c)).unwrap()
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

/// `copy` or `move` of `items` into the folder `dest`. Returns the op that undoes it, when there
/// is one: a copy that landed on this machine can be deleted again; anything that changed a
/// server is not journalled — there is no trash there to take it back from.
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
        plan.push(Item { name, from, is_dir, meta, tree });
    }
    job.set_totals(files.max(plan.len() as u64), bytes);

    // 2. One at a time, asking when the name is taken.
    let mut there = names(&to, cancel)?;
    let mut created: Vec<Uri> = Vec::new();
    let on_bytes = |n: u64| job.progress(0, n);
    let ctx = ExecCtx { cancel, workers: 1, on_change: &|_| {}, on_bytes: &on_bytes };
    for it in &plan {
        if cancel.load(Ordering::Relaxed) {
            return Err(VfsError::Io("cancelled".into()));
        }
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
            let (src_root, dst_root) = (child(&it.from, &it.name), child(&to, &target));
            for (rel, is_dir, size, mtime) in &it.tree {
                if cancel.load(Ordering::Relaxed) {
                    return Err(VfsError::Io("cancelled".into()));
                }
                if *is_dir {
                    mkdir(&dst_root, rel)?;
                } else {
                    copy_file(&src_root, rel, &dst_root, rel, *size, *mtime, &ctx)?;
                    job.progress(1, 0);
                }
            }
            if it.tree.iter().all(|t| t.1) {
                job.progress(1, 0);
            }
        } else {
            copy_file(&it.from, &it.name, &to, &target, it.meta.size, it.meta.mtime_ms, &ctx)?;
            job.progress(1, 0);
        }
        // The original goes only once everything of it has arrived.
        if moving && !same_place(&it.from, &to) {
            delete(&it.from, &it.name, it.is_dir, cancel)?;
        }
        there.insert(target.clone(), (it.is_dir, it.meta.clone()));
        created.push(dest.join(&target));
    }
    for u in items.iter().filter_map(|u| u.parent()).chain(std::iter::once(dest.clone())) {
        invalidate(&u);
    }
    Ok((!moving && dest.is_local()).then(|| Value::obj().s("op", "delete").v("items", Value::Arr(created.iter().map(|u| Value::Str(u.to_string())).collect())).b("_silent", true).done()))
}

fn rename(from: &Side, name: &str, to: &Side, target: &str) -> Result<(), VfsError> {
    match (from, to) {
        (Side::Local(a), Side::Local(b)) => std::fs::rename(a.join(name), b.join(target)).map_err(VfsError::from),
        (Side::Remote(s, a), Side::Remote(_, b)) => s
            .plugin
            .request(Value::obj().s("type", "Rename").s("location", s.location.clone()).s("from", format!("{}/{}", a.trim_end_matches('/'), name)).s("to", format!("{}/{}", b.trim_end_matches('/'), target)).done())
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
    fn a_taken_name_gets_copy_and_a_number_with_its_extension_kept() {
        let mut t: HashMap<String, (bool, Meta)> = HashMap::new();
        assert_eq!(unique(&t, "site"), "site copy");
        t.insert("index copy.html".into(), (false, Meta::default()));
        assert_eq!(unique(&t, "index.html"), "index copy 2.html");
        assert_eq!(unique(&t, ".env"), ".env copy", "a leading dot is not an extension");
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
