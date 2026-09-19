//! Walking each side into a `SideMap`, locally or through a location plugin.

use super::detect::usable_md5;
use super::*;

/// Enumerates a side into rel → Entry, skipping symlinks and filtered names (with their subtrees).
pub fn scan_side(side: &Side, rules: &[Rule], filtered_count: &mut usize, cancel: &AtomicBool) -> Result<SideMap, VfsError> {
    let mut out = SideMap::new();
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
                            let _ = name;
                            out.insert(&rel, Entry { path: 0, is_dir: kind == "dir", size: m.size, mtime_ms: m.mtime_ms, digest: None });
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
        }
    }
    out.finish();
    Ok(out)
}

fn scan_local(root: &Path, prefix: &str, rules: &[Rule], filtered_count: &mut usize, out: &mut SideMap, cancel: &AtomicBool) -> Result<(), VfsError> {
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
        out.insert(&rel, Entry { path: 0, is_dir: md.is_dir(), size: if md.is_dir() { 0 } else { md.len() }, mtime_ms, digest: None });
        if md.is_dir() {
            scan_local(root, &rel, rules, filtered_count, out, cancel)?;
        }
    }
    Ok(())
}

fn scan_remote(session: &Arc<Session>, root: &str, prefix: &str, rules: &[Rule], filtered_count: &mut usize, out: &mut SideMap, cancel: &AtomicBool) -> Result<(), VfsError> {
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
                    out.insert(&rel, Entry { path: 0, is_dir: kind == "dir", size: m.size, mtime_ms: m.mtime_ms, digest: None });
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
        if let Some(e) = out.get_mut(rel.as_str()) {
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
fn fill_local_digests(side: &Side, mine: &mut SideMap, other: &SideMap) {
    let Side::Local(root) = side else { return };
    for (rel, o) in other.iter() {
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
