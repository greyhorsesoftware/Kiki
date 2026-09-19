//! The plan: two side maps in, a list of actions out. Pure — no I/O, so it is all testable.

use super::*;

pub fn diff(master: &SideMap, replica: &SideMap, spec: &Spec, detector: Detector, now_ms: u64) -> Result<Plan, String> {
    if spec.delete_extras && master.is_empty() && !replica.is_empty() {
        return Err("master scan returned no entries; refusing to delete the entire replica".into());
    }
    let within = |m: &Entry| match spec.modified_within_ms {
        None => true,
        Some(w) => m.mtime_ms == 0 || (m.mtime_ms as i64 - spec.clock_offset_ms) >= now_ms as i64 - w as i64,
    };
    let mk = |rel: &str, kind: ActionKind, reason: Reason, bytes: u64, m: Option<&Entry>, r: Option<&Entry>| Action {
        rel: Arc::from(rel),
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
        // Ascending path order: parents before children.
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
            if master.contains(rel) {
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
