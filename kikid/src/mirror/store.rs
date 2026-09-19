//! Scan results kept by job id, so a run can be started from a plan the user reviewed.

use super::*;

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
