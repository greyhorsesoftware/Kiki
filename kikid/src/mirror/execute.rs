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
            Side::Local(root) => std::fs::create_dir(root.join(&*a.rel)).map_err(VfsError::from),
            Side::Remote(s, root) => s.plugin.request(Value::obj().s("type", "Mkdir").s("location", s.location.clone()).s("path", join_rel(root, &a.rel)).done()).map(|_| ()),
        },
        ActionKind::Delete | ActionKind::Rmdir => match replica {
            Side::Local(root) => crate::ops::remove_tree(&root.join(&*a.rel)),
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
            let dst = rroot.join(&*a.rel);
            let _ = std::fs::remove_file(&dst);
            let mut p = crate::ops::Progress { cancel: ctx.cancel, bytes: &mut |n| (ctx.on_bytes)(n) };
            crate::ops::copy_file(&mroot.join(&*a.rel), &dst, &mut p)?;
            Ok(())
        }
        (Side::Local(mroot), Side::Remote(s, rroot)) => {
            let mut f = std::fs::File::open(mroot.join(&*a.rel))?;
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
            let dst = rroot.join(&*a.rel);
            let tmp = dst.with_extension("kiki-part");
            let mut f = std::fs::File::create(&tmp)?;
            let req = Value::obj().s("type", "Read").s("location", s.location.clone()).s("path", join_rel(mroot, &a.rel)).done();
            let r = s.plugin.read_stream_with(req, Some(ctx.cancel), |m| {
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
                    let r = ms.plugin.read_stream_with(read_req, Some(cancel), |m| {
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
            ms.plugin.read_stream(req, |m| {
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
