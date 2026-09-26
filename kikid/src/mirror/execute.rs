//! Running a plan: the safety guards, the worker pool, and one action at a time.

use super::*;

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
        return Err(VfsError::said(
            1223,
            &[("n", &plan.delete_count()), ("total", &plan.replica_entry_count)],
            format!("Safety: this would delete {} of {} replica items; confirm to proceed", plan.delete_count(), plan.replica_entry_count),
        ));
    }
    for a in plan.actions.iter().filter(|a| a.checked && matches!(a.kind, ActionKind::Delete | ActionKind::Rmdir)) {
        if a.rel.is_empty() || a.rel.split('/').any(|s| s == "..") || a.rel.starts_with('/') {
            return Err(VfsError::said(1224, &[("path", &a.rel)], format!("Safety: refusing to delete outside the replica root: {}", a.rel)));
        }
    }
    Ok(())
}

fn audit(line: &str) {
    // The state directory every other part of the daemon writes to — the journal, the failed-job
    // log. This built its own from `XDG_STATE_HOME` and so ignored `KIKI_STATE_DIR`: every e2e run
    // appended its mirrors to the audit log in the developer's own home, which the harness
    // promises it leaves as it found it.
    let d = crate::jobs::state_dir();
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
    /// Stamp a download with the server's EXACT time for the file, asked of it (`Stat`), where
    /// the listing could only say the minute or the day (FTP's LIST). Right for a plain download,
    /// where the truest time is the best one. **Wrong for a mirror, and off there**: the next scan
    /// will read the listing again, so the replica must carry the time the LISTING gave — the
    /// one the comparison will see — or every such file looks changed on every run. (RelaySFTP's
    /// rule, which this engine is ported from: "stamp it back to the source so re-scans match".)
    pub exact_times: bool,
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
                        return Err(VfsError::said(1230, &[], "cancelled"));
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
            Side::Local(root) => std::fs::create_dir(root.join(&*a.rel)).map_err(VfsError::from),
            Side::Remote(s, root) => s.plugin.request(s.req("Mkdir").s("path", join_rel(root, &a.rel)).done()).map(|_| ()),
        },
        ActionKind::Delete | ActionKind::Rmdir => match replica {
            Side::Local(root) => crate::ops::remove_tree(&root.join(&*a.rel)),
            Side::Remote(s, root) => s.plugin.request(s.req("Delete").s("path", join_rel(root, &a.rel)).done()).map(|_| ()),
        },
        ActionKind::Copy => copy_action(a, master, replica, ctx),
        ActionKind::Skip => Ok(()),
    }
}

fn copy_action(a: &Action, master: &Side, replica: &Side, ctx: &ExecCtx) -> Result<(), VfsError> {
    let mtime = a.master.as_ref().map(|m| m.mtime_ms).unwrap_or(0);
    copy_file(master, &a.rel, replica, &a.rel, a.bytes, mtime, ctx)
}

/// The name a file is uploaded under until all of it has arrived.
pub fn part_name(path: &str) -> String {
    format!("{path}.kiki-part")
}

fn read_chunk(f: &mut std::fs::File) -> Result<Option<Vec<u8>>, VfsError> {
    use std::io::Read;
    let mut buf = vec![0u8; 512 * 1024];
    match f.read(&mut buf)? {
        0 => Ok(None),
        n => {
            buf.truncate(n);
            Ok(Some(buf))
        }
    }
}

/// Upload one file. A plugin is told a write is over by the bytes stopping, and cannot tell
/// "that was all of it" from "the job was cancelled" or "the disk we were reading gave an error"
/// — so, written straight to its name, a cancelled upload was a short file that the job then
/// reported as done. It goes to `<name>.kiki-part` instead and takes its name only once every
/// byte has arrived; anything else removes the part and is an error. `next` yields the bytes:
/// `Ok(None)` is the end of the file, `Err` is a source that failed.
fn put(s: &Arc<locations::Session>, path: &str, bytes: u64, mtime: u64, ctx: &ExecCtx, mut next: impl FnMut() -> Result<Option<Vec<u8>>, VfsError>) -> Result<(), VfsError> {
    let part = part_name(path);
    let mut stopped: Option<VfsError> = None;
    let req = s.req("Write").s("path", part.clone()).u("size", bytes).u("mtime", mtime).done();
    let wrote = s.plugin.write_stream(req, || {
        if ctx.cancel.load(Ordering::Relaxed) {
            stopped = Some(VfsError::said(1230, &[], "cancelled"));
            return None;
        }
        match next() {
            Ok(Some(b)) => {
                (ctx.on_bytes)(b.len() as u64);
                Some(b)
            }
            Ok(None) => None,
            Err(e) => {
                stopped = Some(e);
                None
            }
        }
    });
    let result = match (stopped, wrote) {
        (Some(e), _) | (None, Err(e)) => Err(e),
        (None, Ok(_)) => Ok(()),
    };
    if result.is_err() {
        let _ = s.plugin.request(s.req("Delete").s("path", part).done());
        return result;
    }
    // Into place. SFTP's rename refuses an existing target, so one that is in the way goes
    // first — it was about to be overwritten anyway, and until now the whole new file exists.
    let rename = || s.plugin.request(s.req("Rename").s("from", part.clone()).s("to", path).done());
    if rename().is_err() {
        let _ = s.plugin.request(s.req("Delete").s("path", path).done());
        if let Err(e) = rename() {
            let _ = s.plugin.request(s.req("Delete").s("path", part).done());
            return Err(e);
        }
    }
    // Best-effort mtime so size+mtime stays idempotent on the next run — asked only of a plugin
    // that can do it. FTP cannot, and asking anyway was a wasted round trip on every file.
    let can = s.plugin.describe().get("features").and_then(|f| f.get("setMtime")).and_then(Value::as_bool).unwrap_or(true);
    if can && mtime > 0 {
        // Not every server allows it (SETSTAT refused, a read-only attribute). The copy stands;
        // the audit log says the time could not be kept, because the next run's size-and-time
        // comparison will see that file as changed and this is the line that explains why.
        if let Err(e) = s.plugin.request(s.req("SetMtime").s("path", path).u("mtime", mtime).done()) {
            audit(&format!("mirror-mtime-skip {path} {}", e.message()));
        }
    }
    Ok(())
}

/// One file from `master`/`from` to `replica`/`to`, whichever of the two is this machine: the
/// four pairings of local and remote. Mirror copies under the same name on both sides; a plain
/// copy that was told "keep both" does not, which is why there are two.
pub fn copy_file(master: &Side, from: &str, replica: &Side, to: &str, bytes: u64, mtime: u64, ctx: &ExecCtx) -> Result<(), VfsError> {
    struct A<'a> {
        rel: &'a str,
        to: &'a str,
        bytes: u64,
    }
    let a = A { rel: from, to, bytes };
    match (master, replica) {
        (Side::Local(mroot), Side::Local(rroot)) => {
            let dst = rroot.join(a.to);
            let _ = std::fs::remove_file(&dst);
            let mut p = crate::ops::Progress { cancel: ctx.cancel, bytes: &mut |n| (ctx.on_bytes)(n), file: None, failed: None };
            crate::ops::copy_file(&mroot.join(a.rel), &dst, &mut p)?;
            Ok(())
        }
        (Side::Local(mroot), Side::Remote(s, rroot)) => {
            let mut f = std::fs::File::open(mroot.join(a.rel))?;
            put(s, &join_rel(rroot, a.to), a.bytes, mtime, ctx, || read_chunk(&mut f))
        }
        (Side::Remote(s, mroot), Side::Local(rroot)) => {
            let dst = rroot.join(a.to);
            // Appended, not swapped for the extension: `notes.txt` and `notes.md` arriving on two
            // workers at once must not share a part file.
            let tmp = {
                let mut n = dst.clone().into_os_string();
                n.push(".kiki-part");
                std::path::PathBuf::from(n)
            };
            let mut f = std::fs::File::create(&tmp)?;
            let req = s.req("Read").s("path", join_rel(mroot, a.rel)).done();
            let mut disk: Option<std::io::Error> = None;
            let r = s.plugin.read_stream_with(req, Some(ctx.cancel), |m| {
                if let Msg::Binary(b) = m {
                    use std::io::Write;
                    if disk.is_none() {
                        disk = f.write_all(&b).err();
                    }
                    (ctx.on_bytes)(b.len() as u64);
                }
            });
            drop(f);
            // A full disk is a failed download, not a short file under the right name.
            let r = match disk {
                Some(e) => Err(VfsError::from(e)),
                None => r,
            };
            if let Err(e) = r {
                let _ = std::fs::remove_file(&tmp);
                return Err(e);
            }
            std::fs::rename(&tmp, &dst)?;
            // (Plain downloads only — see `ExecCtx::exact_times`.)
            // A time that is a whole minute came from a listing that could say no better: FTP's
            // LIST gives minutes for a recent file and only the DAY for one older than six months
            // (a file from 16:53 came down dated midnight). The server knows the second — that is
            // what `Stat` asks it (MDTM) — so it is asked, once, for such a file. A listing with
            // real times (SFTP, MLSD) almost never lands on a whole minute and is not asked again.
            let mtime = if ctx.exact_times && mtime > 0 && mtime.is_multiple_of(60_000) {
                s.plugin.request(s.req("Stat").s("path", join_rel(mroot, a.rel)).done()).ok().and_then(|m| m.u64_field("mtime")).filter(|t| *t > 0).unwrap_or(mtime)
            } else {
                mtime
            };
            if mtime > 0 {
                let _ = crate::ops::set_mtime(&dst, std::time::UNIX_EPOCH + std::time::Duration::from_millis(mtime));
            }
            Ok(())
        }
        (Side::Remote(ms, mroot), Side::Remote(rs, rroot)) if !Arc::ptr_eq(&ms.plugin, &rs.plugin) => {
            // Two plugin processes: stream the Read straight into the Write through a bounded
            // channel, so the transfer never touches the local disk and both sides run at once.
            // The reader's verdict travels down the same channel as its bytes: the channel
            // closing says only that the reader stopped, not that the file was whole.
            let (tx, rx) = std::sync::mpsc::sync_channel::<Result<Vec<u8>, VfsError>>(16);
            let read_req = ms.req("Read").s("path", join_rel(mroot, a.rel)).done();
            std::thread::scope(|scope| {
                let ms = Arc::clone(ms);
                let cancel = ctx.cancel;
                scope.spawn(move || {
                    let r = ms.plugin.read_stream_with(read_req, Some(cancel), |m| {
                        if let Msg::Binary(b) = m {
                            let _ = tx.send(Ok(b));
                        }
                    });
                    if let Err(e) = r {
                        let _ = tx.send(Err(e));
                    }
                    // Dropping tx ends the consumer's stream.
                });
                put(rs, &join_rel(rroot, a.to), a.bytes, mtime, ctx, || match rx.recv() {
                    Ok(Ok(b)) => Ok(Some(b)),
                    Ok(Err(e)) => Err(e),
                    Err(_) => Ok(None),
                })
            })
        }
        (Side::Remote(ms, mroot), Side::Remote(rs, rroot)) => {
            // Same plugin process on both sides: a plugin serves one binary stream at a time, so
            // spool through a temp file.
            let tmp = std::env::temp_dir().join(format!("kiki-mirror-{}-{}", std::process::id(), crate::md5::hex(a.rel.as_bytes())));
            let mut f = std::fs::File::create(&tmp)?;
            let req = ms.req("Read").s("path", join_rel(mroot, a.rel)).done();
            let mut disk: Option<std::io::Error> = None;
            let spooled = ms.plugin.read_stream_with(req, Some(ctx.cancel), |m| {
                if let Msg::Binary(b) = m {
                    use std::io::Write;
                    if disk.is_none() {
                        disk = f.write_all(&b).err();
                    }
                }
            });
            drop(f);
            let spooled = match disk {
                Some(e) => Err(VfsError::from(e)),
                None => spooled.map(|_| ()),
            };
            if let Err(e) = spooled {
                let _ = std::fs::remove_file(&tmp);
                return Err(e);
            }
            let mut f = std::fs::File::open(&tmp)?;
            let r = put(rs, &join_rel(rroot, a.to), a.bytes, mtime, ctx, || read_chunk(&mut f));
            let _ = std::fs::remove_file(&tmp);
            r
        }
    }
}
