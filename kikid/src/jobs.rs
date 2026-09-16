//! The job queue: every write is a job with progress, cancellation and, where
//! possible, a journal entry whose inverse Ctrl+Z replays.

use crate::json::Value;
use crate::ops::{self, Progress};
use crate::proto;
use crate::vfs::uri::Uri;
use crate::vfs::VfsError;
use std::collections::{HashMap, VecDeque};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

pub const SLOTS: usize = 3;
pub const JOURNAL_CAP: usize = 100;
const PROGRESS_EVERY: Duration = Duration::from_millis(100);

#[derive(Clone, Debug, PartialEq)]
pub enum State {
    Queued,
    Running,
    Done,
    Failed(String),
    Cancelled,
}

impl State {
    fn as_str(&self) -> &'static str {
        match self {
            State::Queued => "queued",
            State::Running => "running",
            State::Done => "done",
            State::Failed(_) => "failed",
            State::Cancelled => "cancelled",
        }
    }
}

pub struct Job {
    pub id: u64,
    pub op: Value,
    pub kind: String,
    pub title: String,
    pub client: Option<Sender<Value>>,
    pub cancel: Arc<AtomicBool>,
    pub status: Mutex<Status>,
    prompt: Mutex<Option<Receiver<(String, bool)>>>,
    prompt_tx: Mutex<Option<Sender<(String, bool)>>>,
    policy: Mutex<Option<String>>,
}

#[derive(Clone, Debug)]
pub struct Status {
    pub state: State,
    pub done: u64,
    pub total: u64,
    pub bytes: u64,
    pub bytes_total: u64,
    pub undoable: bool,
    last_emit: Option<Instant>,
}

struct Queue {
    jobs: Vec<Arc<Job>>,
    pending: VecDeque<Arc<Job>>,
    running: usize,
    subscribers: Vec<Sender<Value>>,
    journal: Vec<Value>, // entries: { job, title, inverse: Op, redo: Op }
    redo: Vec<Value>,
    next_id: u64,
}

fn queue() -> &'static Mutex<Queue> {
    static Q: OnceLock<Mutex<Queue>> = OnceLock::new();
    Q.get_or_init(|| {
        let journal = load_journal();
        Mutex::new(Queue { jobs: Vec::new(), pending: VecDeque::new(), running: 0, subscribers: Vec::new(), journal, redo: Vec::new(), next_id: 1 })
    })
}

fn state_dir() -> PathBuf {
    if let Ok(d) = std::env::var("KIKI_STATE_DIR") {
        return PathBuf::from(d);
    }
    std::env::var("XDG_STATE_HOME").map(PathBuf::from).unwrap_or_else(|_| crate::config::home().join(".local/state")).join("kiki")
}

fn load_journal() -> Vec<Value> {
    let p = state_dir().join("journal.json");
    match std::fs::read(&p) {
        Ok(b) => crate::json::parse(&b).ok().and_then(|v| v.as_arr().map(|a| a.to_vec())).unwrap_or_default(),
        Err(_) => Vec::new(),
    }
}

fn save_journal(j: &[Value]) {
    let d = state_dir();
    let _ = std::fs::create_dir_all(&d);
    let _ = std::fs::write(d.join("journal.json"), crate::json::to_string(&Value::Arr(j.to_vec())));
}

pub fn subscribe(tx: Sender<Value>) {
    queue().lock().unwrap().subscribers.push(tx);
}

pub fn unsubscribe(tx: &Sender<Value>) {
    // Senders are compared by channel identity through a probe send failure; keep it simple: drop closed ones.
    let _ = tx;
    queue().lock().unwrap().subscribers.retain(|s| s.send(Value::Null).is_ok());
}

fn broadcast(v: Value) {
    let mut q = queue().lock().unwrap();
    q.subscribers.retain(|s| s.send(v.clone()).is_ok());
}

// ---------------------------------------------------------------- submit

pub fn submit(op: Value, client: Option<Sender<Value>>) -> Result<u64, (&'static str, String)> {
    let kind = op.str_field("op").ok_or(("Protocol", "missing op".to_string()))?.to_string();
    let title = title_for(&op);
    let undoable = !matches!(kind.as_str(), "delete" | "mirrorScan" | "mirrorRun");
    let (ptx, prx) = mpsc::channel();
    let job = {
        let mut q = queue().lock().unwrap();
        let id = q.next_id;
        q.next_id += 1;
        let job = Arc::new(Job {
            id,
            op,
            kind,
            title,
            client,
            cancel: Arc::new(AtomicBool::new(false)),
            status: Mutex::new(Status { state: State::Queued, done: 0, total: 0, bytes: 0, bytes_total: 0, undoable, last_emit: None }),
            prompt: Mutex::new(Some(prx)),
            prompt_tx: Mutex::new(Some(ptx)),
            policy: Mutex::new(None),
        });
        q.jobs.push(Arc::clone(&job));
        if q.jobs.len() > 200 {
            // Keep every live job and the most recent hundred finished ones.
            let finished: Vec<u64> = q.jobs.iter().filter(|j| !matches!(j.status.lock().unwrap().state, State::Running | State::Queued)).map(|j| j.id).collect();
            let drop_below = finished.get(finished.len().saturating_sub(100)).copied().unwrap_or(0);
            q.jobs.retain(|j| j.id >= drop_below || matches!(j.status.lock().unwrap().state, State::Running | State::Queued));
        }
        q.pending.push_back(Arc::clone(&job));
        job
    };
    broadcast(job.event());
    pump();
    Ok(job.id)
}

fn pump() {
    loop {
        let job = {
            let mut q = queue().lock().unwrap();
            if q.running >= SLOTS {
                return;
            }
            match q.pending.pop_front() {
                Some(j) => {
                    q.running += 1;
                    j
                }
                None => return,
            }
        };
        std::thread::Builder::new()
            .name(format!("job-{}", job.id))
            .spawn(move || {
                job.set_state(State::Running);
                let result = run(&job);
                let inverse = match result {
                    Ok(inv) => {
                        job.set_state(State::Done);
                        inv
                    }
                    Err(e) => {
                        if job.cancel.load(Ordering::Relaxed) {
                            job.set_state(State::Cancelled);
                        } else {
                            job.set_state(State::Failed(e.message()));
                        }
                        None
                    }
                };
                if let Some(inv) = inverse {
                    let mut q = queue().lock().unwrap();
                    q.journal.push(Value::obj().u("job", job.id).s("title", job.title.clone()).v("inverse", inv).v("redo", job.op.clone()).done());
                    if q.journal.len() > JOURNAL_CAP {
                        q.journal.remove(0);
                    }
                    q.redo.clear();
                    save_journal(&q.journal);
                    drop(q);
                    if matches!(job.kind.as_str(), "trash" | "move" | "rename" | "chmod" | "delete" | "extract" | "compress" | "copy") {
                        broadcast(proto::event("Toast").u("job", job.id).s("text", job.title.clone()).b("undoable", true).done());
                    }
                } else if job.kind == "delete" && job.status.lock().unwrap().state == State::Done {
                    broadcast(proto::event("Toast").u("job", job.id).s("text", job.title.clone()).b("undoable", false).done());
                }
                queue().lock().unwrap().running -= 1;
                pump();
            })
            .expect("spawn job");
    }
}

impl Job {
    fn set_state(&self, s: State) {
        self.status.lock().unwrap().state = s;
        broadcast(self.event());
    }

    fn progress(&self, done_delta: u64, bytes_delta: u64) {
        let mut st = self.status.lock().unwrap();
        st.done += done_delta;
        st.bytes += bytes_delta;
        let emit = st.last_emit.map(|t| t.elapsed() >= PROGRESS_EVERY).unwrap_or(true);
        if emit {
            st.last_emit = Some(Instant::now());
            drop(st);
            broadcast(self.event());
        }
    }

    fn set_totals(&self, total: u64, bytes_total: u64) {
        let mut st = self.status.lock().unwrap();
        st.total = total;
        st.bytes_total = bytes_total;
    }

    pub fn event(&self) -> Value {
        proto::event("JobEvent").v("job", self.json()).done()
    }

    pub fn json(&self) -> Value {
        let st = self.status.lock().unwrap();
        let err = match &st.state {
            State::Failed(m) => Value::Str(m.clone()),
            _ => Value::Null,
        };
        Value::obj()
            .u("id", self.id)
            .s("op", self.kind.clone())
            .s("state", st.state.as_str())
            .u("done", st.done)
            .u("total", st.total)
            .u("bytes", st.bytes)
            .u("bytesTotal", st.bytes_total)
            .s("title", self.title.clone())
            .v("error", err)
            .b("undoable", st.undoable)
            .done()
    }

    /// Ask the submitting client what to do about an existing destination.
    fn ask_collision(&self, dest: &std::path::Path, incoming: &std::path::Path) -> String {
        if let Some(p) = self.policy.lock().unwrap().clone() {
            return p;
        }
        let Some(tx) = &self.client else { return "skip".into() };
        let meta = |p: &std::path::Path| std::fs::symlink_metadata(p).map(|m| crate::vfs::Meta { size: m.len(), mtime_ms: m.modified().ok().and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok()).map(|d| d.as_millis() as u64).unwrap_or(0), mode: None, uid: None, gid: None }).unwrap_or_default();
        let _ = tx.send(
            proto::event("Prompt")
                .u("job", self.id)
                .s("kind", "collision")
                .s("uri", Uri::from_path(dest).to_string())
                .v("existing", crate::listing::meta_json(&meta(dest)))
                .v("incoming", crate::listing::meta_json(&meta(incoming)))
                .v("choices", Value::Arr(vec![Value::Str("replace".into()), Value::Str("keepBoth".into()), Value::Str("skip".into())]))
                .done(),
        );
        let rx = self.prompt.lock().unwrap().take();
        let answer = match rx {
            Some(rx) => {
                let a = rx.recv_timeout(Duration::from_secs(600)).unwrap_or(("skip".into(), false));
                *self.prompt.lock().unwrap() = Some(rx);
                a
            }
            None => ("skip".into(), false),
        };
        if answer.1 {
            *self.policy.lock().unwrap() = Some(answer.0.clone());
        }
        answer.0
    }
}

pub fn prompt_reply(job: u64, choice: &str, apply_to_all: bool) -> bool {
    let q = queue().lock().unwrap();
    if let Some(j) = q.jobs.iter().find(|j| j.id == job) {
        if let Some(tx) = j.prompt_tx.lock().unwrap().as_ref() {
            return tx.send((choice.to_string(), apply_to_all)).is_ok();
        }
    }
    false
}

pub fn cancel(id: u64) -> bool {
    let q = queue().lock().unwrap();
    match q.jobs.iter().find(|j| j.id == id) {
        Some(j) => {
            j.cancel.store(true, Ordering::Relaxed);
            let _ = prompt_reply_inner(j, "skip");
            true
        }
        None => false,
    }
}

fn prompt_reply_inner(j: &Job, choice: &str) -> bool {
    match j.prompt_tx.lock().unwrap().as_ref() {
        Some(tx) => tx.send((choice.to_string(), false)).is_ok(),
        None => false,
    }
}

pub fn list() -> Value {
    let q = queue().lock().unwrap();
    let n = q.jobs.len();
    Value::Arr(q.jobs.iter().skip(n.saturating_sub(50)).map(|j| j.json()).collect())
}

pub fn undo(client: Option<Sender<Value>>) -> Result<u64, (&'static str, String)> {
    let entry = {
        let mut q = queue().lock().unwrap();
        let e = q.journal.pop().ok_or(("NotFound", "nothing to undo".to_string()))?;
        q.redo.push(e.clone());
        save_journal(&q.journal);
        e
    };
    let inverse = entry.get("inverse").cloned().ok_or(("Io", "bad journal entry".to_string()))?;
    submit_unjournaled(inverse, client)
}

pub fn redo(client: Option<Sender<Value>>) -> Result<u64, (&'static str, String)> {
    let entry = {
        let mut q = queue().lock().unwrap();
        q.redo.pop().ok_or(("NotFound", "nothing to redo".to_string()))?
    };
    let op = entry.get("redo").cloned().ok_or(("Io", "bad journal entry".to_string()))?;
    submit(op, client)
}

/// Runs an op without adding it to the journal (used for undo replays).
fn submit_unjournaled(op: Value, client: Option<Sender<Value>>) -> Result<u64, (&'static str, String)> {
    let mut op = op;
    if let Value::Obj(m) = &mut op {
        m.insert("_unjournaled".into(), Value::Bool(true));
    }
    submit(op, client)
}

// ---------------------------------------------------------------- running ops

fn uris(op: &Value, key: &str) -> Result<Vec<PathBuf>, VfsError> {
    let arr = op.get(key).and_then(Value::as_arr).ok_or(VfsError::Io(format!("missing {key}")))?;
    arr.iter().map(|v| v.as_str().ok_or(VfsError::Io("bad uri".into())).and_then(|s| Uri::parse(s).map_err(|e| VfsError::Io(e.0.into()))).and_then(|u| ops::local_path(&u))).collect()
}

fn uri(op: &Value, key: &str) -> Result<PathBuf, VfsError> {
    let s = op.str_field(key).ok_or(VfsError::Io(format!("missing {key}")))?;
    ops::local_path(&Uri::parse(s).map_err(|e| VfsError::Io(e.0.into()))?)
}

fn uri_list(paths: &[PathBuf]) -> Value {
    Value::Arr(paths.iter().map(|p| Value::Str(Uri::from_path(p).to_string())).collect())
}

/// Executes the op; returns the inverse op for the journal when undoable.
fn run(job: &Job) -> Result<Option<Value>, VfsError> {
    let op = &job.op;
    let unjournaled = op.get("_unjournaled").and_then(Value::as_bool).unwrap_or(false);
    let cancel = Arc::clone(&job.cancel);
    let inverse = match job.kind.as_str() {
        "copy" | "move" => {
            let items = uris(op, "items")?;
            let dest = uri(op, "dest")?;
            let (files, bytes) = items.iter().map(|p| ops::tree_size(p)).fold((0, 0), |a, b| (a.0 + b.0, a.1 + b.1));
            job.set_totals(files.max(items.len() as u64), bytes);
            let mut created: Vec<PathBuf> = Vec::new();
            let mut moved: Vec<(PathBuf, PathBuf)> = Vec::new();
            for src in &items {
                if cancel.load(Ordering::Relaxed) {
                    return Err(VfsError::Io("cancelled".into()));
                }
                let name = src.file_name().ok_or(VfsError::NotFound)?.to_string_lossy().into_owned();
                let mut target = dest.join(&name);
                if target.exists() {
                    match job.ask_collision(&target, src).as_str() {
                        "replace" => ops::remove_tree(&target)?,
                        "keepBoth" => target = dest.join(ops::unique_name(&dest, &name)),
                        _ => {
                            job.progress(1, 0);
                            continue;
                        }
                    }
                }
                let mut p = Progress { cancel: &cancel, bytes: &mut |n| job.progress(0, n) };
                if job.kind == "copy" {
                    ops::copy_tree(src, &target, &mut p)?;
                    created.push(target);
                } else {
                    ops::move_path(src, &target, &mut p)?;
                    moved.push((src.clone(), target));
                }
                job.progress(1, 0);
            }
            if job.kind == "copy" {
                Some(Value::obj().s("op", "delete").v("items", uri_list(&created)).b("_silent", true).done())
            } else {
                Some(Value::obj().s("op", "movePairs").v("pairs", Value::Arr(moved.iter().map(|(a, b)| Value::Arr(vec![Value::Str(Uri::from_path(b).to_string()), Value::Str(Uri::from_path(a).to_string())])).collect())).done())
            }
        }
        "movePairs" => {
            let pairs = op.get("pairs").and_then(Value::as_arr).ok_or(VfsError::Io("missing pairs".into()))?;
            job.set_totals(pairs.len() as u64, 0);
            let mut back = Vec::new();
            for pair in pairs {
                let a = pair.as_arr().ok_or(VfsError::Io("bad pair".into()))?;
                let from = ops::local_path(&Uri::parse(a[0].as_str().unwrap_or("")).map_err(|e| VfsError::Io(e.0.into()))?)?;
                let to = ops::local_path(&Uri::parse(a[1].as_str().unwrap_or("")).map_err(|e| VfsError::Io(e.0.into()))?)?;
                let mut p = Progress { cancel: &cancel, bytes: &mut |n| job.progress(0, n) };
                ops::move_path(&from, &to, &mut p)?;
                back.push(Value::Arr(vec![Value::Str(Uri::from_path(&to).to_string()), Value::Str(Uri::from_path(&from).to_string())]));
                job.progress(1, 0);
            }
            Some(Value::obj().s("op", "movePairs").v("pairs", Value::Arr(back)).done())
        }
        "rename" => {
            let path = uri(op, "uri")?;
            let name = op.str_field("name").ok_or(VfsError::Io("missing name".into()))?;
            if name.is_empty() || name.contains('/') {
                return Err(VfsError::Io("bad name".into()));
            }
            let target = path.parent().ok_or(VfsError::NotFound)?.join(name);
            if target.exists() {
                return Err(VfsError::Exists);
            }
            std::fs::rename(&path, &target)?;
            job.set_totals(1, 0);
            job.progress(1, 0);
            Some(Value::obj().s("op", "rename").s("uri", Uri::from_path(&target).to_string()).s("name", path.file_name().unwrap().to_string_lossy()).done())
        }
        "trash" => {
            let items = uris(op, "items")?;
            job.set_totals(items.len() as u64, 0);
            let mut names = Vec::new();
            for p in &items {
                if cancel.load(Ordering::Relaxed) {
                    break;
                }
                names.push(Value::Str(ops::trash(p)?));
                job.progress(1, 0);
            }
            Some(Value::obj().s("op", "restore").v("names", Value::Arr(names)).done())
        }
        "restore" => {
            let names = op.get("names").and_then(Value::as_arr).ok_or(VfsError::Io("missing names".into()))?;
            job.set_totals(names.len() as u64, 0);
            let mut restored = Vec::new();
            for n in names {
                let p = ops::restore(n.as_str().unwrap_or(""))?;
                restored.push(p);
                job.progress(1, 0);
            }
            Some(Value::obj().s("op", "trash").v("items", uri_list(&restored)).done())
        }
        "delete" => {
            let items = uris(op, "items")?;
            job.set_totals(items.len() as u64, 0);
            for p in &items {
                if cancel.load(Ordering::Relaxed) {
                    break;
                }
                ops::remove_tree(p)?;
                job.progress(1, 0);
            }
            None
        }
        "mkdir" => {
            let path = uri(op, "uri")?;
            std::fs::create_dir(&path)?;
            job.set_totals(1, 0);
            job.progress(1, 0);
            Some(Value::obj().s("op", "rmdirIfEmpty").s("uri", Uri::from_path(&path).to_string()).done())
        }
        "rmdirIfEmpty" => {
            let path = uri(op, "uri")?;
            std::fs::remove_dir(&path)?;
            Some(Value::obj().s("op", "mkdir").s("uri", Uri::from_path(&path).to_string()).done())
        }
        "chmod" => {
            let items = uris(op, "items")?;
            let mode = op.u64_field("mode").ok_or(VfsError::Io("missing mode".into()))? as u32;
            let recursive = op.get("recursive").and_then(Value::as_bool).unwrap_or(false);
            let mut prev = Vec::new();
            for p in &items {
                ops::chmod(p, mode, recursive, &mut prev)?;
            }
            job.set_totals(prev.len() as u64, 0);
            job.progress(prev.len() as u64, 0);
            Some(Value::obj().s("op", "chmodList").v("list", Value::Arr(prev.iter().map(|(p, m)| Value::Arr(vec![Value::Str(Uri::from_path(p).to_string()), Value::Uint(*m as u64)])).collect())).done())
        }
        "chmodList" => {
            use std::os::unix::fs::PermissionsExt;
            let list = op.get("list").and_then(Value::as_arr).ok_or(VfsError::Io("missing list".into()))?;
            let mut prev = Vec::new();
            for item in list {
                let a = item.as_arr().ok_or(VfsError::Io("bad item".into()))?;
                let p = ops::local_path(&Uri::parse(a[0].as_str().unwrap_or("")).map_err(|e| VfsError::Io(e.0.into()))?)?;
                let m = a[1].as_u64().unwrap_or(0) as u32;
                let cur = std::fs::symlink_metadata(&p)?.permissions().mode() & 0o7777;
                std::fs::set_permissions(&p, std::fs::Permissions::from_mode(m))?;
                prev.push(Value::Arr(vec![Value::Str(Uri::from_path(&p).to_string()), Value::Uint(cur as u64)]));
            }
            Some(Value::obj().s("op", "chmodList").v("list", Value::Arr(prev)).done())
        }
        other => return Err(VfsError::Io(format!("unknown op {other}"))),
    };
    Ok(if unjournaled { None } else { inverse })
}

fn title_for(op: &Value) -> String {
    let kind = op.str_field("op").unwrap_or("");
    let items = op.get("items").and_then(Value::as_arr).map(|a| a.len()).unwrap_or(0);
    let first = op.get("items").and_then(Value::as_arr).and_then(|a| a.first()).and_then(Value::as_str).and_then(|s| Uri::parse(s).ok()).map(|u| u.name().to_string()).unwrap_or_default();
    let what = if items > 1 { format!("{items} items") } else { first };
    let dest = op.str_field("dest").and_then(|s| Uri::parse(s).ok()).map(|u| u.name().to_string()).unwrap_or_default();
    match kind {
        "copy" => format!("Copy {what} to {dest}"),
        "move" => format!("Move {what} to {dest}"),
        "movePairs" => "Move back".into(),
        "rename" => format!("Rename to {}", op.str_field("name").unwrap_or("")),
        "trash" => format!("Move {what} to Trash"),
        "restore" => "Restore from Trash".into(),
        "delete" => format!("Delete {what}"),
        "mkdir" => "New folder".into(),
        "rmdirIfEmpty" => "Remove folder".into(),
        "chmod" => format!("Change permissions of {what}"),
        "chmodList" => "Restore permissions".into(),
        other => other.to_string(),
    }
}

pub fn wait(id: u64, timeout: Duration) -> Option<State> {
    let start = Instant::now();
    loop {
        let st = {
            let q = queue().lock().unwrap();
            q.jobs.iter().find(|j| j.id == id).map(|j| j.status.lock().unwrap().state.clone())
        };
        match st {
            Some(State::Queued) | Some(State::Running) => {
                if start.elapsed() > timeout {
                    return st;
                }
                std::thread::sleep(Duration::from_millis(5));
            }
            other => return other,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trash_undo_redo_round_trip() {
        let d = std::env::temp_dir().join(format!("kiki-jobs-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        std::env::set_var("KIKI_TRASH_DIR", d.join("trash"));
        std::env::set_var("KIKI_STATE_DIR", d.join("state"));
        std::fs::write(d.join("x.txt"), b"x").unwrap();
        let (tx, rx) = mpsc::channel();
        subscribe(tx.clone());
        let id = submit(Value::obj().s("op", "trash").v("items", Value::Arr(vec![Value::Str(Uri::from_path(&d.join("x.txt")).to_string())])).done(), Some(tx.clone())).unwrap();
        assert_eq!(wait(id, Duration::from_secs(5)), Some(State::Done));
        assert!(!d.join("x.txt").exists());
        let events: Vec<Value> = rx.try_iter().collect();
        assert!(events.iter().any(|e| e.str_field("event") == Some("Toast")));
        let uid = undo(Some(tx.clone())).unwrap();
        assert_eq!(wait(uid, Duration::from_secs(5)), Some(State::Done));
        assert!(d.join("x.txt").exists());
        let rid = redo(Some(tx.clone())).unwrap();
        assert_eq!(wait(rid, Duration::from_secs(5)), Some(State::Done));
        assert!(!d.join("x.txt").exists());
        // copy with a collision answered keepBoth
        std::fs::write(d.join("a.txt"), b"a").unwrap();
        std::fs::create_dir_all(d.join("dst")).unwrap();
        std::fs::write(d.join("dst/a.txt"), b"old").unwrap();
        let cid = submit(Value::obj().s("op", "copy").v("items", Value::Arr(vec![Value::Str(Uri::from_path(&d.join("a.txt")).to_string())])).s("dest", Uri::from_path(&d.join("dst")).to_string()).done(), Some(tx.clone())).unwrap();
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            if let Ok(ev) = rx.recv_timeout(Duration::from_millis(50)) {
                if ev.str_field("event") == Some("Prompt") {
                    assert!(prompt_reply(cid, "keepBoth", false));
                    break;
                }
            }
            assert!(Instant::now() < deadline, "no prompt");
        }
        assert_eq!(wait(cid, Duration::from_secs(5)), Some(State::Done));
        assert_eq!(std::fs::read(d.join("dst/a (2).txt")).unwrap(), b"a");
        let uid = undo(Some(tx)).unwrap();
        assert_eq!(wait(uid, Duration::from_secs(5)), Some(State::Done));
        assert!(!d.join("dst/a (2).txt").exists());
        std::env::remove_var("KIKI_TRASH_DIR");
        std::env::remove_var("KIKI_STATE_DIR");
        std::fs::remove_dir_all(&d).unwrap();
    }
}
