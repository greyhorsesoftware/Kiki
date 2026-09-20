//! Scan results kept by job id, so a run can be started from a plan the user reviewed.
//!
//! A plan lists every file on both sides, so it is kept only while something wants it: a client
//! showing it (`view` / `unview`), or a run that was started from it and has not finished
//! (`jobs::plan_wanted`). Whatever slips through that — a scan nobody ever opened, a client that
//! died between scan and view — is caught by the cap.

use super::*;
use std::sync::atomic::{AtomicUsize, Ordering};

/// Plans kept at most. Those being shown or run are never the ones dropped.
pub const KEEP: usize = 8;

pub struct Stored {
    pub spec: Spec,
    pub plan: Arc<Mutex<Plan>>,
    viewers: AtomicUsize,
}

fn plans() -> &'static Mutex<HashMap<u64, Arc<Stored>>> {
    static P: OnceLock<Mutex<HashMap<u64, Arc<Stored>>>> = OnceLock::new();
    P.get_or_init(|| Mutex::new(HashMap::new()))
}

pub fn store(job: u64, spec: Spec, plan: Plan) {
    // Asked before the lock is taken: `plan_wanted` takes the job queue's.
    let wanted: Vec<u64> = ids().into_iter().filter(|id| crate::jobs::plan_wanted(*id)).collect();
    let mut p = plans().lock().unwrap();
    p.insert(job, Arc::new(Stored { spec, plan: Arc::new(Mutex::new(plan)), viewers: AtomicUsize::new(0) }));
    let mut idle: Vec<u64> = p.iter().filter(|(id, s)| **id != job && s.viewers.load(Ordering::SeqCst) == 0 && !wanted.contains(id)).map(|(id, _)| *id).collect();
    idle.sort_unstable();
    let over = p.len().saturating_sub(KEEP);
    for id in idle.into_iter().take(over) {
        p.remove(&id);
    }
}

pub fn stored(job: u64) -> Option<Arc<Stored>> {
    plans().lock().unwrap().get(&job).cloned()
}

pub(super) fn ids() -> Vec<u64> {
    plans().lock().unwrap().keys().copied().collect()
}

/// A client starts showing the plan. Every `view` is paired with an `unview`.
pub fn view(job: u64) -> Option<Arc<Stored>> {
    let s = stored(job)?;
    s.viewers.fetch_add(1, Ordering::SeqCst);
    Some(s)
}

/// A client stopped showing it (closed the view, or went away). The last one out drops the
/// plan, unless a run started from it is still queued or running — that run drops it instead.
pub fn unview(job: u64) {
    let wanted = crate::jobs::plan_wanted(job);
    let mut p = plans().lock().unwrap();
    let Some(s) = p.get(&job) else { return };
    let left = s.viewers.fetch_update(Ordering::SeqCst, Ordering::SeqCst, |v| Some(v.saturating_sub(1))).unwrap_or(1).saturating_sub(1);
    if left == 0 && !wanted {
        p.remove(&job);
    }
}

/// The run started from this plan is over. Nobody looking at it any more: drop it.
pub fn run_finished(job: u64) {
    let mut p = plans().lock().unwrap();
    if p.get(&job).map(|s| s.viewers.load(Ordering::SeqCst) == 0).unwrap_or(false) {
        p.remove(&job);
    }
}
