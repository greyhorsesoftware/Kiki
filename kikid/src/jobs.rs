//! The job queue: every write is a job with progress, cancellation and, where
//! possible, a journal entry whose inverse Ctrl+Z replays.

use crate::json::Value;
use crate::ops::{self, Progress};
use crate::proto;
use crate::vfs::uri::Uri;
use crate::vfs::VfsError;
use std::collections::VecDeque;
use std::path::{Path, PathBuf};
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
    /// Asked for from a kiki window, and so stopped when the last window has gone (`shell_went`).
    /// A script's jobs, the portal's and a test's are nobody's to stop but their own.
    shell: AtomicBool,
    pub cancel: Arc<AtomicBool>,
    pub status: Mutex<Status>,
    prompt: Mutex<Option<Receiver<(String, bool)>>>,
    prompt_tx: Mutex<Option<Sender<(String, bool)>>>,
    policy: Mutex<Option<String>>,
    /// How to take back what the job has done SO FAR. A job that is cancelled or fails half way
    /// has still changed things — forty files in the trash, twelve folders copied — and those
    /// must be as undoable as if it had finished.
    partial: Mutex<Option<Value>>,
    /// What the job is, in the terms the activity view shows it in; fixed when it is submitted.
    pub about: About,
    /// What it is doing at this moment, and what came of it.
    live: Mutex<Live>,
}

/// A job described for a person rather than for the runner (plan 32). `title` is a sentence
/// ("Copy 3 items to dst"); the activity view wants the pieces: what, how many, from where to
/// where, in which direction.
#[derive(Clone, Debug, Default)]
pub struct About {
    /// The file or folder the job is about — the first of them when there are several.
    pub name: String,
    pub count: u64,
    pub is_dir: bool,
    pub src: Option<String>,
    pub dest: Option<String>,
    /// `upload` | `download` | `remote` (server to server) | `local`.
    pub direction: &'static str,
    /// Machinery rather than something the user asked for: the inverse ops an undo runs, a
    /// mirror's preflight scan. They run like any job and stay out of the activity view.
    pub hidden: bool,
}

#[derive(Default)]
struct Live {
    /// The file being worked on: name (relative to the item), bytes of it done, its size.
    current: Option<(String, u64, u64)>,
    /// Bytes per second, smoothed: what every window shows, so they all show the same number.
    rate: f64,
    mark: Option<(Instant, u64)>,
    result: Option<Value>,
    reveal: Option<String>,
    /// Learnt as the job runs: a thing on a server is not known to be a folder until it is listed.
    is_dir: Option<bool>,
}

fn about(kind: &str, op: &Value) -> About {
    let parse = |s: &str| Uri::parse(s).ok();
    let items: Vec<Uri> = op.get("items").and_then(Value::as_arr).map(|a| a.iter().filter_map(Value::as_str).filter_map(parse).collect()).unwrap_or_default();
    let one = op.str_field("uri").or(op.str_field("archive")).and_then(parse);
    let first = items.first().cloned().or(one);
    let dest = op.str_field("dest").and_then(parse);
    let src = first.as_ref().and_then(|u| u.parent());
    let spec = op.get("spec");
    let (src, dest) = match spec {
        Some(sp) => (sp.str_field("master").and_then(parse), sp.str_field("replica").and_then(parse)),
        None => (src, dest),
    };
    let remote = |u: &Option<Uri>| u.as_ref().is_some_and(|u| !u.is_local() && u.scheme != "trash");
    let direction = match (remote(&src), remote(&dest)) {
        (false, true) => "upload",
        (true, false) if dest.is_some() => "download",
        (true, true) => "remote",
        _ => "local",
    };
    let name = match (kind, &first) {
        ("mirrorRun" | "mirrorScan", _) => format!("Mirroring {}", if direction == "download" { "remote → local" } else { "local → remote" }),
        ("emptyTrash", _) => "Trash".to_string(),
        (_, Some(u)) => u.name().to_string(),
        _ => String::new(),
    };
    // On a server this is not known yet: the transfer says once it has looked (`set_is_dir`).
    let is_dir = first.as_ref().is_some_and(|u| u.is_local() && u.to_path().is_dir());
    let silent = op.get("_silent").and_then(Value::as_bool).unwrap_or(false);
    About {
        name,
        count: (items.len() as u64).max(u64::from(first.is_some())),
        is_dir,
        src: src.map(|u| u.to_string()),
        dest: dest.map(|u| u.to_string()),
        direction,
        hidden: silent || matches!(kind, "movePairs" | "rmdirIfEmpty" | "chmodList" | "mirrorScan"),
    }
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

pub(crate) fn state_dir() -> PathBuf {
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

pub fn broadcast(v: Value) {
    let mut q = queue().lock().unwrap();
    q.subscribers.retain(|s| s.send(v.clone()).is_ok());
}

// ---------------------------------------------------------------- submit

pub fn submit(op: Value, client: Option<Sender<Value>>) -> Result<u64, (&'static str, String)> {
    let kind = op.str_field("op").ok_or(("Protocol", "missing op".to_string()))?.to_string();
    let title = title_for(&op);
    let undoable = !matches!(kind.as_str(), "delete" | "deleteCopies" | "emptyTrash" | "mirrorScan" | "mirrorRun" | "share");
    let (ptx, prx) = mpsc::channel();
    let job = {
        let mut q = queue().lock().unwrap();
        let id = q.next_id;
        q.next_id += 1;
        let about = about(&kind, &op);
        let job = Arc::new(Job {
            id,
            about,
            live: Mutex::new(Live::default()),
            op,
            kind,
            title,
            client,
            shell: AtomicBool::new(false),
            cancel: Arc::new(AtomicBool::new(false)),
            status: Mutex::new(Status { state: State::Queued, done: 0, total: 0, bytes: 0, bytes_total: 0, undoable, last_emit: None }),
            prompt: Mutex::new(Some(prx)),
            prompt_tx: Mutex::new(Some(ptx)),
            policy: Mutex::new(None),
            partial: Mutex::new(None),
        });
        q.jobs.push(Arc::clone(&job));
        if q.jobs.len() > 200 {
            // Keep every live job and the most recent hundred finished ones.
            let finished: Vec<u64> = q.jobs.iter().filter(|j| !matches!(j.status.lock().unwrap().state, State::Running | State::Queued)).map(|j| j.id).collect();
            let drop_below = finished.get(finished.len().saturating_sub(100)).copied().unwrap_or(0);
            for old in q.jobs.iter().filter(|j| j.id < drop_below && !matches!(j.status.lock().unwrap().state, State::Running | State::Queued)) {
                crate::joblog::forget(old.id);
            }
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
                crate::joblog::say(
                    job.id,
                    "info",
                    format!(
                        "{} — started{}",
                        job.title,
                        match (&job.about.src, &job.about.dest) {
                            (Some(s), Some(d)) => format!(": {s} → {d}"),
                            (Some(s), None) => format!(": {s}"),
                            _ => String::new(),
                        }
                    ),
                );
                let result = run(&job);
                let (ended, inverse) = match result {
                    Ok(inv) => {
                        // Stopped early is cancelled, whatever it returned: a trash or a delete
                        // leaves its loop quietly so that its inverse still reaches the journal.
                        let short = {
                            let st = job.status.lock().unwrap();
                            st.done < st.total
                        };
                        let ended = if short && job.cancel.load(Ordering::Relaxed) { State::Cancelled } else { State::Done };
                        if ended == State::Done {
                            // Finished is finished: an op that counts only some of what it does
                            // (a rename is one step however many files it moves) ends at its total.
                            let mut st = job.status.lock().unwrap();
                            st.done = st.done.max(st.total);
                            st.bytes = st.bytes.max(st.bytes_total);
                        }
                        {
                            let st = job.status.lock().unwrap();
                            crate::joblog::say(job.id, "info", format!("{}: {} of {} items, {} bytes", if ended == State::Done { "finished" } else { "cancelled" }, st.done, st.total, st.bytes));
                        }
                        (ended, inv)
                    }
                    Err(e) => {
                        let ended = if job.cancel.load(Ordering::Relaxed) {
                            crate::joblog::say(job.id, "warn", "cancelled");
                            State::Cancelled
                        } else {
                            crate::joblog::say(job.id, "error", format!("failed: {}", e.message()));
                            crate::joblog::keep_failure(job.id, &job.title, &e.message());
                            State::Failed(e.message())
                        };
                        // What it got done before it stopped — and only what is really there: a copy
                        // names its target before it starts on it, so one that failed at once has
                        // an inverse that would delete nothing, which is no inverse at all.
                        (ended, job.partial.lock().unwrap().take().and_then(something_to_undo))
                    }
                };
                let finished_well = ended == State::Done;
                // The end is said only once the journal has it — the event AND the status. The
                // event used to be said first, and a client quick enough to answer "done" with
                // `Undo` — a script, never a hand — took back whatever was done before this
                // instead. Then the status alone went to Done first, and a test polling it (or
                // a script on `JobStatus`) could start its next job in the gap: that one's
                // inverse reached the journal first, and `Undo` took back the wrong job. Seen on
                // a CI runner twice as slow as the machine it was written on, never here.
                let undoable_now = inverse.is_some();
                // A copy that landed on a server is taken back by deleting it there, and a
                // server has no trash (plan 07): the line that offers the undo says so, while
                // there is still a choice about clicking it.
                let permanent = inverse.as_ref().and_then(|inv| (inv.str_field("op") == Some("deleteCopies")).then(|| inv.str_field("dest").and_then(|d| Uri::parse(d).ok()).map(|u| u.authority).unwrap_or_default()));
                if let Some(inv) = inverse {
                    let mut q = queue().lock().unwrap();
                    q.journal.push(Value::obj().u("job", job.id).s("title", job.title.clone()).v("inverse", inv).v("redo", job.op.clone()).done());
                    if q.journal.len() > JOURNAL_CAP {
                        q.journal.remove(0);
                    }
                    q.redo.clear();
                    save_journal(&q.journal);
                }
                job.status.lock().unwrap().state = ended;
                job.announce();
                if undoable_now && matches!(job.kind.as_str(), "trash" | "move" | "rename" | "chmod" | "delete" | "extract" | "compress" | "copy") {
                    // A job that stopped half way did not do what its title says: the toast
                    // offers to take back the part that it did, and says that is what it is.
                    let mut text = if finished_well { job.title.clone() } else { format!("{} — stopped part-way", job.title) };
                    if let Some(host) = permanent {
                        text.push_str(&format!(" — Undo deletes {} from {}, permanently", if job.about.count > 1 { "them" } else { "it" }, server_name(&host)));
                    }
                    broadcast(proto::event("Toast").u("job", job.id).s("text", text).b("undoable", true).done());
                }
                if !undoable_now && matches!(job.kind.as_str(), "delete" | "emptyTrash") && finished_well {
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

    /// Tell everyone where the job stands now; for a state that was set quietly.
    fn announce(&self) {
        broadcast(self.event());
    }

    /// Record how to undo what has been done so far; journalled if the job then fails or is
    /// cancelled. A job that finishes returns its inverse instead, and this is dropped.
    pub(crate) fn undo_so_far(&self, inverse: Value) {
        *self.partial.lock().unwrap() = Some(inverse);
    }

    /// A file has been started: what the activity view's detail row names. `size` 0 when the
    /// file's own progress is not tracked (a mirror copies several at once).
    pub(crate) fn file_started(&self, name: &str, size: u64) {
        crate::joblog::say(self.id, "debug", if size > 0 { format!("{name} ({size} bytes)") } else { name.to_string() });
        self.live.lock().unwrap().current = Some((name.to_string(), 0, size));
    }

    pub(crate) fn set_is_dir(&self, is_dir: bool) {
        self.live.lock().unwrap().is_dir = Some(is_dir);
    }

    /// What came of it, for the completion line: a mirror's counts.
    pub(crate) fn set_result(&self, result: Value) {
        self.live.lock().unwrap().result = Some(result);
    }

    /// Something on this machine the job made, that "Reveal" can show.
    pub(crate) fn set_reveal(&self, uri: &str) {
        let mut l = self.live.lock().unwrap();
        if l.reveal.is_none() {
            l.reveal = Some(uri.to_string());
        }
    }

    fn measure(&self, bytes_delta: u64, bytes_now: u64) {
        let mut l = self.live.lock().unwrap();
        if let Some(c) = l.current.as_mut() {
            c.1 += bytes_delta;
        }
        let now = Instant::now();
        match l.mark {
            None => l.mark = Some((now, bytes_now)),
            Some((at, was)) if now.duration_since(at) >= Duration::from_millis(500) => {
                let inst = bytes_now.saturating_sub(was) as f64 / now.duration_since(at).as_secs_f64();
                l.rate = if l.rate == 0.0 { inst } else { 0.7 * l.rate + 0.3 * inst };
                l.mark = Some((now, bytes_now));
            }
            _ => {}
        }
    }

    pub(crate) fn progress(&self, done_delta: u64, bytes_delta: u64) {
        let mut st = self.status.lock().unwrap();
        st.done += done_delta;
        st.bytes += bytes_delta;
        if bytes_delta > 0 {
            self.measure(bytes_delta, st.bytes);
        }
        let emit = st.last_emit.map(|t| t.elapsed() >= PROGRESS_EVERY).unwrap_or(true);
        if emit {
            st.last_emit = Some(Instant::now());
            drop(st);
            broadcast(self.event());
        }
    }

    pub fn set_progress(&self, done: u64, bytes: u64) {
        let mut st = self.status.lock().unwrap();
        st.done = done;
        st.bytes = bytes;
        let emit = st.last_emit.map(|t| t.elapsed() >= PROGRESS_EVERY).unwrap_or(true);
        if emit {
            st.last_emit = Some(Instant::now());
            drop(st);
            broadcast(self.event());
        }
    }

    /// Said at once, not with the next progress event: until the totals are known a job is
    /// "preparing" (a transfer walks the whole tree first), and the view's bar is waiting for this.
    pub fn set_totals(&self, total: u64, bytes_total: u64) {
        {
            let mut st = self.status.lock().unwrap();
            st.total = total;
            st.bytes_total = bytes_total;
            st.last_emit = Some(Instant::now());
        }
        broadcast(self.event());
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
        let running = st.state == State::Running;
        let unfinished = running || st.state == State::Queued;
        let live = self.live.lock().unwrap();
        // Only while it is running: a finished job is not "on" any file, or moving at any speed.
        let current = match (&live.current, running) {
            (Some((name, bytes, size)), true) => Value::obj().s("name", name.clone()).u("bytes", *bytes).u("size", *size).done(),
            _ => Value::Null,
        };
        let rate = if running { live.rate.round() as u64 } else { 0 };
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
            // For the activity view (plan 32).
            .s("name", self.about.name.clone())
            .u("count", self.about.count)
            .b("isDir", live.is_dir.unwrap_or(self.about.is_dir))
            .opt_s("src", self.about.src.as_deref())
            .opt_s("dest", self.about.dest.as_deref())
            .s("direction", self.about.direction)
            .b("hidden", self.about.hidden)
            .s("phase", if running && st.total == 0 && st.bytes == 0 { "preparing" } else { "running" })
            .b("cancelling", unfinished && self.cancel.load(Ordering::Relaxed))
            .v("current", current)
            .u("rate", rate)
            .v("result", live.result.clone().unwrap_or(Value::Null))
            .opt_s("revealUri", live.reveal.as_deref())
            .done()
    }

    /// Ask the submitting client what to do about an existing destination.
    /// The same question when either file is on a server: what is known about them comes from
    /// a listing, not from this machine's filesystem.
    pub(crate) fn ask_collision_meta(&self, dest: &Uri, existing: &crate::vfs::Meta, incoming: &crate::vfs::Meta) -> String {
        if let Some(p) = self.policy.lock().unwrap().clone() {
            return p;
        }
        let Some(tx) = &self.client else { return "skip".into() };
        let _ = tx.send(
            proto::event("Prompt")
                .u("job", self.id)
                .s("kind", "collision")
                .s("uri", dest.to_string())
                .v("existing", crate::listing::meta_json(existing))
                .v("incoming", crate::listing::meta_json(incoming))
                .v("choices", Value::Arr(vec![Value::Str("replace".into()), Value::Str("keepBoth".into()), Value::Str("skip".into())]))
                .done(),
        );
        self.await_collision_answer()
    }

    fn ask_collision(&self, dest: &std::path::Path, incoming: &std::path::Path) -> String {
        if let Some(p) = self.policy.lock().unwrap().clone() {
            return p;
        }
        let Some(tx) = &self.client else { return "skip".into() };
        let meta = |p: &std::path::Path| {
            std::fs::symlink_metadata(p)
                .map(|m| crate::vfs::Meta {
                    hidden: false,
                    size: m.len(),
                    mtime_ms: m.modified().ok().and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok()).map(|d| d.as_millis() as u64).unwrap_or(0),
                    atime_ms: 0,
                    mode: crate::vfs::Meta::NONE,
                    uid: crate::vfs::Meta::NONE,
                    gid: crate::vfs::Meta::NONE,
                })
                .unwrap_or_default()
        };
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
        self.await_collision_answer()
    }

    fn await_collision_answer(&self) -> String {
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
            crate::joblog::say(j.id, "warn", "cancel requested");
            let _ = prompt_reply_inner(j, "skip");
            // "Cancelling…" at once, not when the worker next looks up from a large file.
            let event = j.event();
            drop(q);
            broadcast(event);
            true
        }
        None => false,
    }
}

// ---------------------------------------------------------------- the shell's jobs
//
// "If kiki is not running, stuff should not continue in the background." The window asks before
// it closes on running jobs and cancels them itself; this is for every other way of going — a
// kill, a crash, a compositor that went first. The daemon stays up (it is socket-activated and
// answers the desktop's file chooser too), but idle.

/// (windows connected, how many have ever connected). The second is how the grace knows that a
/// window came back while it slept, even if that one has gone again.
fn shells() -> &'static Mutex<(usize, u64)> {
    static S: OnceLock<Mutex<(usize, u64)>> = OnceLock::new();
    S.get_or_init(|| Mutex::new((0, 0)))
}

/// How long a window may be gone before its jobs are stopped: long enough for a dropped socket
/// to be picked up again, short enough that nothing much is copied behind the user's back.
fn shell_grace() -> std::time::Duration {
    std::time::Duration::from_millis(std::env::var("KIKI_SHELL_GRACE_MS").ok().and_then(|v| v.parse().ok()).unwrap_or(4000))
}

/// The job was asked for from a kiki window.
pub fn own(id: u64) {
    if let Some(j) = queue().lock().unwrap().jobs.iter().find(|j| j.id == id) {
        j.shell.store(true, Ordering::Relaxed);
    }
}

pub fn shell_came() {
    let mut s = shells().lock().unwrap();
    s.0 += 1;
    s.1 += 1;
}

pub fn shell_went() {
    let seen = {
        let mut s = shells().lock().unwrap();
        s.0 = s.0.saturating_sub(1);
        if s.0 > 0 {
            return;
        }
        s.1
    };
    let _ = std::thread::Builder::new().name("shell-grace".into()).spawn(move || {
        std::thread::sleep(shell_grace());
        if *shells().lock().unwrap() != (0, seen) {
            return;
        }
        let mine: Vec<u64> = queue().lock().unwrap().jobs.iter().filter(|j| j.shell.load(Ordering::Relaxed) && !finished(j)).map(|j| j.id).collect();
        for id in mine {
            crate::joblog::say(id, "warn", "kiki was closed: stopping");
            cancel(id);
        }
    });
}

fn prompt_reply_inner(j: &Job, choice: &str) -> bool {
    match j.prompt_tx.lock().unwrap().as_ref() {
        Some(tx) => tx.send((choice.to_string(), false)).is_ok(),
        None => false,
    }
}

fn finished(j: &Job) -> bool {
    !matches!(j.status.lock().unwrap().state, State::Running | State::Queued)
}

/// Every job still queued or running, however long ago it was submitted, and the fifty most recent
/// finished ones. (It used to be "the last fifty of everything", so a long transfer with fifty
/// small jobs after it dropped out of the list while it was still running.)
pub fn list() -> Value {
    let q = queue().lock().unwrap();
    let done: Vec<u64> = q.jobs.iter().filter(|j| finished(j)).map(|j| j.id).collect();
    let keep_from = done.get(done.len().saturating_sub(50)).copied().unwrap_or(0);
    Value::Arr(q.jobs.iter().filter(|j| !finished(j) || j.id >= keep_from).map(|j| j.json()).collect())
}

/// Forget finished jobs — one, or all of them. Here and not in the shell: the list is fetched
/// again on every reconnect, which would bring back whatever a window had merely hidden.
/// A job that is queued or running is never forgotten. Returns how many went.
pub fn dismiss(only: Option<u64>) -> u64 {
    let gone: Vec<u64> = {
        let mut q = queue().lock().unwrap();
        let gone: Vec<u64> = q.jobs.iter().filter(|j| finished(j) && only.is_none_or(|id| id == j.id)).map(|j| j.id).collect();
        q.jobs.retain(|j| !gone.contains(&j.id));
        gone
    };
    for id in &gone {
        crate::joblog::forget(*id);
    }
    if !gone.is_empty() {
        broadcast(proto::event("JobsCleared").v("jobs", Value::Arr(gone.iter().map(|id| Value::Uint(*id)).collect())).done());
    }
    gone.len() as u64
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

fn uri_value(op: &Value, key: &str) -> Result<Uri, VfsError> {
    let s = op.str_field(key).ok_or(VfsError::Io(format!("missing {key}")))?;
    Uri::parse(s).map_err(|e| VfsError::Io(e.0.into()))
}

fn uri_values(op: &Value, key: &str) -> Result<Vec<Uri>, VfsError> {
    let arr = op.get(key).and_then(Value::as_arr).ok_or(VfsError::Io(format!("missing {key}")))?;
    arr.iter().map(|v| v.as_str().ok_or(VfsError::Io("bad uri".into())).and_then(|s| Uri::parse(s).map_err(|e| VfsError::Io(e.0.into())))).collect()
}

/// Does any URI this op names live somewhere other than this machine?
fn remote_op(op: &Value) -> bool {
    let mut all: Vec<Uri> = Vec::new();
    for key in ["dest", "uri", "dir"] {
        all.extend(op.str_field(key).and_then(|s| Uri::parse(s).ok()));
    }
    all.extend(op.get("items").and_then(Value::as_arr).map(|a| a.iter().filter_map(Value::as_str).filter_map(|s| Uri::parse(s).ok()).collect::<Vec<_>>()).unwrap_or_default());
    crate::transfer::involves_remote(&all.iter().collect::<Vec<_>>())
}

fn uri_list(paths: &[PathBuf]) -> Value {
    Value::Arr(paths.iter().map(|p| Value::Str(Uri::from_path(p).to_string())).collect())
}

/// A partial inverse, less whatever it names that does not exist; `None` when that leaves nothing.
fn something_to_undo(inverse: Value) -> Option<Value> {
    if inverse.str_field("op") != Some("delete") {
        return Some(inverse);
    }
    let there: Vec<Value> = inverse
        .get("items")
        .and_then(Value::as_arr)
        .map(|a| a.iter().filter(|u| u.as_str().and_then(|s| Uri::parse(s).ok()).is_some_and(|u| !u.is_local() || u.to_path().symlink_metadata().is_ok())).cloned().collect())
        .unwrap_or_default();
    (!there.is_empty()).then(|| Value::obj().s("op", "delete").v("items", Value::Arr(there)).b("_silent", true).done())
}

/// An error that says which item it was about: "Denied" alone, in a copy of forty things, is no help.
fn about_file(e: VfsError, name: &str) -> VfsError {
    match e {
        VfsError::Io(m) if m == "cancelled" => VfsError::Io(m),
        other => VfsError::Io(format!("{name}: {}", other.message())),
    }
}

fn copy_inverse(created: &[PathBuf]) -> Value {
    Value::obj().s("op", "delete").v("items", uri_list(created)).b("_silent", true).done()
}

/// Each path a chmod changed, with the mode it had: `chmodList` puts them all back in one job.
fn chmod_inverse(prev: &[(PathBuf, u32)]) -> Value {
    Value::obj().s("op", "chmodList").v("list", Value::Arr(prev.iter().map(|(p, m)| Value::Arr(vec![Value::Str(Uri::from_path(p).to_string()), Value::Uint(u64::from(*m))])).collect())).done()
}

fn move_inverse(moved: &[(PathBuf, PathBuf)]) -> Value {
    let back = |(from, to): &(PathBuf, PathBuf)| Value::Arr(vec![Value::Str(Uri::from_path(to).to_string()), Value::Str(Uri::from_path(from).to_string())]);
    Value::obj().s("op", "movePairs").v("pairs", Value::Arr(moved.iter().map(back).collect())).done()
}

/// Executes the op; returns the inverse op for the journal when undoable.
fn run(job: &Job) -> Result<Option<Value>, VfsError> {
    let op = &job.op;
    let unjournaled = op.get("_unjournaled").and_then(Value::as_bool).unwrap_or(false);
    let cancel = Arc::clone(&job.cancel);
    // Long work on a server runs on connections of the job's own, closed when it ends however it
    // ends. Nothing is opened until the job resolves a remote URI, so a local job pays nothing;
    // `mkdir` and `rename` are one round trip and stay on the browser's, which is already up.
    let _sessions = matches!(job.kind.as_str(), "copy" | "move" | "delete" | "deleteCopies" | "share" | "mirrorScan" | "mirrorRun").then(|| crate::locations::JobSessions::enter(job.id));
    let inverse = match job.kind.as_str() {
        // Anything with an end that is not this machine goes through `transfer`: what follows
        // works on local paths.
        "copy" | "move" if remote_op(op) => {
            let items = uri_values(op, "items")?;
            let dest = uri_value(op, "dest")?;
            return crate::transfer::copy_or_move(job, job.kind == "move", &items, &dest, &cancel);
        }
        "delete" if remote_op(op) => {
            crate::transfer::delete_items(job, &uri_values(op, "items")?, &cancel)?;
            None
        }
        "trash" if remote_op(op) => return Err(VfsError::Io("a server has no trash: use Delete (Shift+Del), which cannot be undone".into())),
        "mkdir" if remote_op(op) => {
            crate::transfer::make_dir(&uri_value(op, "uri")?)?;
            job.set_totals(1, 0);
            job.progress(1, 0);
            None
        }
        "rename" if remote_op(op) => {
            crate::transfer::rename_item(&uri_value(op, "uri")?, op.str_field("name").unwrap_or(""))?;
            None
        }
        // Taking back a copy that landed on a server: delete what it made there, where that is
        // still what it left. Nothing on a server is trashed, so the toast says what was done.
        "deleteCopies" => {
            let host = uri_value(op, "dest").map(|u| u.authority).unwrap_or_default();
            let taken = crate::transfer::undo_copy(job, op, &cancel);
            let text = match &taken {
                Ok(out) => Some(undid_copy_text(&host, out)),
                // A hidden job that failed is a failure nobody would ever see; this one says so.
                Err(e) if !cancel.load(Ordering::Relaxed) => Some(format!("Could not undo the copy to {}: {}", server_name(&host), e.message())),
                Err(_) => None,
            };
            if let Some(text) = text {
                broadcast(proto::event("Toast").u("job", job.id).s("text", text).b("undoable", false).done());
            }
            taken?;
            None
        }
        "copy" | "move" => {
            let items = uris(op, "items")?;
            let dest = uri(op, "dest")?;
            let (files, bytes) = items.iter().map(|p| ops::tree_size(p)).fold((0, 0), |a, b| (a.0 + b.0, a.1 + b.1));
            job.set_totals(files.max(items.len() as u64), bytes);
            let mut created: Vec<PathBuf> = Vec::new();
            let mut moved: Vec<(PathBuf, PathBuf)> = Vec::new();
            // What could not be copied, noted so that the rest goes on (see `ops::Progress`).
            let mut failed: Vec<(PathBuf, String)> = Vec::new();
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
                // Files are counted as they finish, inside the tree; the file in hand is named as it
                // starts. (It used to count one per top-level item against a total of files, so
                // a folder copy ended at "1 of 458".)
                let root = src.parent().map(Path::to_path_buf).unwrap_or_default();
                let mut on_file = |e: ops::FileEvent| match e {
                    ops::FileEvent::Start(path, size) => job.file_started(&path.strip_prefix(&root).unwrap_or(path).to_string_lossy(), size),
                    ops::FileEvent::Done => job.progress(1, 0),
                };
                let mut p = Progress { cancel: &cancel, bytes: &mut |n| job.progress(0, n), file: Some(&mut on_file), failed: Some(&mut failed) };
                if job.kind == "copy" {
                    // Recorded before it is whole: a folder cancelled half way is half a folder
                    // at the destination, and undo has to be able to take that away too.
                    created.push(target.clone());
                    job.undo_so_far(copy_inverse(&created));
                    ops::copy_tree(src, &target, &mut p).map_err(|e| about_file(e, &name))?;
                } else {
                    ops::move_path(src, &target, &mut p).map_err(|e| about_file(e, &name))?;
                    // Still there means part of it could not be copied and the original was
                    // kept: that is not a move to be undone by moving it back on top of itself.
                    if src.symlink_metadata().is_err() {
                        moved.push((src.clone(), target.clone()));
                        job.undo_so_far(move_inverse(&moved));
                    }
                }
                job.set_reveal(&Uri::from_path(&target).to_string());
            }
            if !failed.is_empty() {
                // Fail at the end, saying which; the partial inverse (`undo_so_far`) is what the
                // journal gets, so what did arrive can still be taken back.
                for (path, why) in &failed {
                    crate::joblog::say(job.id, "error", format!("{}: {why}", path.display()));
                }
                let first: Vec<String> = failed.iter().take(3).map(|(p, why)| format!("{} ({why})", p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default())).collect();
                let more = if failed.len() > 3 { format!(", and {} more — see the log", failed.len() - 3) } else { String::new() };
                let what = if job.kind == "move" { "moved; their originals are untouched" } else { "copied" };
                return Err(VfsError::Io(format!("{} of {} could not be {what}: {}{more}", failed.len(), files.max(items.len() as u64), first.join(", "))));
            }
            if job.kind == "copy" {
                Some(copy_inverse(&created))
            } else {
                Some(move_inverse(&moved))
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
                let mut p = Progress { cancel: &cancel, bytes: &mut |n| job.progress(0, n), file: None, failed: None };
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
                job.undo_so_far(Value::obj().s("op", "restore").v("names", Value::Arr(names.clone())).done());
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
        "emptyTrash" => {
            job.set_totals(ops::trash_count(), 0);
            ops::empty_trash(&cancel, &mut || job.progress(1, 0))?;
            None
        }
        "delete" => {
            let items = uris(op, "items")?;
            job.set_totals(items.len() as u64, 0);
            for p in &items {
                if cancel.load(Ordering::Relaxed) {
                    break;
                }
                // An undo's delete names what a job set out to create; one that failed before it
                // made anything left nothing to remove, and that is not an error.
                let undoing = op.get("_silent").and_then(Value::as_bool).unwrap_or(false);
                if !(undoing && p.symlink_metadata().is_err()) {
                    ops::remove_tree(p)?;
                }
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
            // Two forms. `mask` and `bits`: the bits named by the mask are set to `bits`, and the
            // rest stay as each file has them — one job over a selection (0.1.1 plan 01), the
            // Permissions grid's touched boxes and nothing else. `mode` alone: the whole word,
            // which is a mask of 0o7777.
            let (mask, bits) = match (op.u64_field("mask"), op.u64_field("bits"), op.u64_field("mode")) {
                (Some(mask), Some(bits), _) => (mask as u32 & 0o7777, bits as u32 & 0o7777),
                (None, None, Some(mode)) => (0o7777, mode as u32 & 0o7777),
                (Some(_), None, _) | (None, Some(_), _) => return Err(VfsError::Io("mask and bits go together".into())),
                _ => return Err(VfsError::Io("missing mode".into())),
            };
            let recursive = op.get("recursive").and_then(Value::as_bool).unwrap_or(false);
            let mut prev = Vec::new();
            // What could not be changed, noted so that the rest goes on (as a copy carries on
            // past a file it cannot read).
            let mut failed: Vec<(PathBuf, String)> = Vec::new();
            // As it goes, item by item — not one jump to the end once it is all over.
            job.set_totals(items.len() as u64, 0);
            for p in &items {
                if cancel.load(Ordering::Relaxed) {
                    break;
                }
                job.file_started(&p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default(), 0);
                ops::chmod(p, mask, bits, recursive, &mut prev, Some(&mut failed))?;
                if !prev.is_empty() {
                    job.undo_so_far(chmod_inverse(&prev));
                }
                job.progress(1, 0);
            }
            if !failed.is_empty() {
                // Fail at the end, saying which; the partial inverse (`undo_so_far`) is what the
                // journal gets, so what did change can still be put back.
                for (path, why) in &failed {
                    crate::joblog::say(job.id, "error", format!("{}: {why}", path.display()));
                }
                let first: Vec<String> = failed.iter().take(3).map(|(p, why)| format!("{} ({why})", p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default())).collect();
                let more = if failed.len() > 3 { format!(", and {} more — see the log", failed.len() - 3) } else { String::new() };
                return Err(VfsError::Io(format!("{} of {} could not be changed: {}{more}", failed.len(), items.len().max(prev.len() + failed.len()), first.join(", "))));
            }
            Some(chmod_inverse(&prev))
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
        "compress" => {
            let items = uris(op, "items")?;
            let archive = uri(op, "archive")?;
            let format = op
                .str_field("format")
                .map(str::to_string)
                .or_else(|| archive.file_name().and_then(|n| crate::archive::format_from_name(&n.to_string_lossy())).map(str::to_string))
                .ok_or(VfsError::Io("unknown archive format".into()))?;
            let (files, bytes) = items.iter().map(|p| ops::tree_size(p)).fold((0, 0), |a, b| (a.0 + b.0, a.1 + b.1));
            job.set_totals(files.max(1), bytes);
            crate::archive::compress(&items, &archive, &format, &cancel, &mut |_| job.progress(1, 0))?;
            Some(Value::obj().s("op", "delete").v("items", uri_list(&[archive])).b("_silent", true).done())
        }
        "extract" => {
            let archive = uri(op, "archive")?;
            let dest = uri(op, "dest")?;
            let n = crate::archive::list(&archive)?.len() as u64;
            job.set_totals(n.max(1), 0);
            // One new thing under a free name, never a merge: so undo is exact (see `extract`).
            let made = crate::archive::extract(&archive, &dest, &cancel, &mut |_| job.progress(1, 0))?;
            made.map(|m| Value::obj().s("op", "delete").v("items", uri_list(&[m])).b("_silent", true).done())
        }
        "share" => {
            let plugin = op.str_field("plugin").ok_or(VfsError::Io("missing plugin".into()))?.to_string();
            let uris: Vec<Uri> = op.get("uris").and_then(Value::as_arr).map(|a| a.iter().filter_map(Value::as_str).filter_map(|s| Uri::parse(s).ok()).collect()).unwrap_or_default();
            let r = crate::share::run(job, &plugin, &uris, op.str_field("target"), op.get("compose").unwrap_or(&Value::Null), &cancel)?;
            // What was sent and to whom, in the toast and in the job's log: "sent" alone left
            // nothing to go on when a file that kiki had handed over did not turn up.
            let said = match r.str_field("detail") {
                Some(d) if !d.is_empty() => format!("{} — {}", r.str_field("result").unwrap_or("done"), d),
                _ => r.str_field("result").unwrap_or("done").to_string(),
            };
            crate::joblog::say(job.id, "info", format!("{plugin}: {said}"));
            broadcast(proto::event("Toast").u("job", job.id).s("text", format!("Shared via {plugin}: {said}")).b("undoable", false).done());
            None
        }
        "mirrorScan" => {
            let mut spec = crate::mirror::Spec::from_json(op.get("spec").ok_or(VfsError::Io("missing spec".into()))?).map_err(VfsError::Io)?;
            // A compare has no total — nobody knows how many entries a tree holds until it has
            // been walked — so what it reports as it goes is the count so far, which is what the
            // Preflight screen counts up instead of sitting on one unchanging line.
            let plan = crate::mirror::scan_counting(&mut spec, &cancel, &|seen| job.set_progress(seen, 0))?;
            let n = plan.actions.len() as u64;
            job.set_totals(n, 0);
            job.set_progress(n, 0);
            crate::mirror::store(job.id, spec, plan);
            None
        }
        "mirrorRun" => {
            let plan_id = op.u64_field("plan").ok_or(VfsError::Io("missing plan".into()))?;
            let stored = crate::mirror::stored(plan_id).ok_or(VfsError::Io("no such plan".into()))?;
            // However this ends, the plan is no longer being run from. (This job still counts
            // as running while the guard drops, which is why it does not ask `plan_wanted`.)
            struct Done(u64);
            impl Drop for Done {
                fn drop(&mut self) {
                    crate::mirror::run_finished(self.0);
                }
            }
            let _done = Done(plan_id);
            let mut spec = stored.spec.clone();
            if let Some(s) = op.get("spec") {
                if let Ok(over) = crate::mirror::Spec::from_json(s) {
                    spec.confirmed_large_delete = over.confirmed_large_delete;
                    spec.delete_extras = over.delete_extras;
                }
            }
            let (total, bytes) = {
                let p = stored.plan.lock().unwrap();
                (p.actions.iter().filter(|a| a.checked && a.kind != crate::mirror::ActionKind::Skip).count() as u64, p.copy_bytes())
            };
            job.set_totals(total, bytes);
            // Five at a time unless the client asks for another number, so a client that says
            // nothing runs what the workspace runs. (Against a server the bytes still move one
            // file at a time — a plugin process carries one binary stream — so this is what
            // overlaps the mkdirs, deletes and set-times around them.)
            let workers = op.u64_field("workers").unwrap_or(5).clamp(1, 8) as usize;
            // Items are counted as they finish and the one in hand is named as it starts. (Every
            // change used to be reported as "nothing done", so a run sat at 0 of 900 until the
            // end.) With several workers the file named is the one most recently begun, and its
            // own bar is left out: the bytes moving belong to all of them.
            let on_change = |i: usize| {
                use crate::mirror::{ActionKind, State as Act};
                let seen = stored.plan.lock().unwrap().actions.get(i).map(|a| (a.state, a.kind, a.rel.to_string(), a.bytes));
                match seen {
                    Some((Act::Running, kind, rel, bytes)) => {
                        let name = if kind == ActionKind::Copy { rel } else { format!("{} {rel}", format!("{kind:?}").to_lowercase()) };
                        job.file_started(&name, if workers == 1 && kind == ActionKind::Copy { bytes } else { 0 });
                        job.progress(0, 0);
                    }
                    Some((Act::Done | Act::Skipped, ..)) => job.progress(1, 0),
                    _ => job.progress(0, 0),
                }
            };
            let ctx = crate::mirror::ExecCtx { cancel: &cancel, workers, on_change: &on_change, on_bytes: &|n| job.progress(0, n), exact_times: false };
            let out = crate::mirror::execute(&stored.plan, &spec, &ctx)?;
            // For the completion line: "412 copied, 9 deleted, 3 skipped".
            job.set_result(Value::obj().u("copies", out.copies).u("deletes", out.deletes).u("skipped", out.skipped).done());
            audit_summary(job, &out);
            None
        }
        other => return Err(VfsError::Io(format!("unknown op {other}"))),
    };
    Ok(if unjournaled { None } else { inverse })
}

/// Is a run that was started from this plan still queued or running? While one is, the plan
/// outlives the view it was reviewed in.
pub fn plan_wanted(plan: u64) -> bool {
    queue()
        .lock()
        .unwrap()
        .jobs
        .iter()
        .any(|j| j.op.str_field("op") == Some("mirrorRun") && j.op.u64_field("plan") == Some(plan) && matches!(j.status.lock().unwrap().state, State::Running | State::Queued))
}

/// The location's own name, or something to call a server that has none.
fn server_name(host: &str) -> &str {
    if host.is_empty() {
        "the server"
    } else {
        host
    }
}

/// What the undo of a copy to a server says when it is over. A remote delete is final — there is
/// no trash on the other side (plan 07) — so the line says so, and says what it left alone.
fn undid_copy_text(host: &str, out: &crate::transfer::TakenBack) -> String {
    let items = |n: u64| format!("{n} item{}", if n == 1 { "" } else { "s" });
    let mut text = if out.deleted == 0 {
        format!("Undid copy — nothing was deleted from {}", server_name(host))
    } else {
        format!("Undid copy — {} deleted from {} (permanently)", items(out.deleted), server_name(host))
    };
    if out.kept > 0 {
        text.push_str(&format!(", {} left because {} had changed", items(out.kept), if out.kept == 1 { "it" } else { "they" }));
    }
    if out.failed > 0 {
        text.push_str(&format!(", {} could not be deleted — see the log", items(out.failed)));
    }
    text
}

fn audit_summary(job: &Job, out: &crate::mirror::Outcome) {
    let mut st = job.status.lock().unwrap();
    st.done = st.total;
    drop(st);
    broadcast(
        proto::event("Toast")
            .u("job", job.id)
            .s("text", format!("Mirror complete · {} copied · {} deleted{}", out.copies, out.deletes, if out.skipped > 0 { format!(" · {} skipped", out.skipped) } else { String::new() }))
            .b("undoable", false)
            .done(),
    );
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
        "deleteCopies" => "Undo copy".into(),
        "emptyTrash" => "Empty Trash".into(),
        "mkdir" => "New folder".into(),
        "rmdirIfEmpty" => "Remove folder".into(),
        "chmod" => format!("Change permissions of {what}"),
        "compress" => format!("Compress {what}"),
        "share" => format!("Share via {}", op.str_field("plugin").unwrap_or("")),
        "mirrorScan" => "Mirror preflight".into(),
        "mirrorRun" => "Mirror".into(),
        "extract" => "Extract archive".into(),
        "chmodList" => "Restore permissions".into(),
        other => other.to_string(),
    }
}

/// For this module's tests, which is all that calls it: a short poll is fine there.
#[cfg(test)]
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
        let _guard = crate::ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
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
        let cid =
            submit(Value::obj().s("op", "copy").v("items", Value::Arr(vec![Value::Str(Uri::from_path(&d.join("a.txt")).to_string())])).s("dest", Uri::from_path(&d.join("dst")).to_string()).done(), Some(tx.clone()))
                .unwrap();
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

    /// A job that stops half way has still changed things, and those are as undoable as if it
    /// had finished: before this, a trash or a copy that failed on its second item journalled
    /// nothing, and the first item could not be taken back.
    #[test]
    fn a_job_that_fails_half_way_can_still_be_undone() {
        let _guard = crate::ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let d = std::env::temp_dir().join(format!("kiki-jobs-partial-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(d.join("dst")).unwrap();
        std::env::set_var("KIKI_TRASH_DIR", d.join("trash"));
        std::env::set_var("KIKI_STATE_DIR", d.join("state"));
        let (tx, _rx) = mpsc::channel();
        let list = |names: &[&str]| Value::Arr(names.iter().map(|n| Value::Str(Uri::from_path(&d.join(n)).to_string())).collect());

        std::fs::write(d.join("real.txt"), b"real").unwrap();
        let id = submit(Value::obj().s("op", "trash").v("items", list(&["real.txt", "never-was.txt"])).done(), Some(tx.clone())).unwrap();
        assert!(matches!(wait(id, Duration::from_secs(5)), Some(State::Failed(_))), "the second item is not there");
        assert!(!d.join("real.txt").exists(), "but the first went to the trash");
        let uid = undo(Some(tx.clone())).unwrap();
        assert_eq!(wait(uid, Duration::from_secs(5)), Some(State::Done));
        assert_eq!(std::fs::read(d.join("real.txt")).unwrap(), b"real", "and undo brings it back");

        let id = submit(Value::obj().s("op", "copy").v("items", list(&["real.txt", "never-was.txt"])).s("dest", Uri::from_path(&d.join("dst")).to_string()).done(), Some(tx.clone())).unwrap();
        assert!(matches!(wait(id, Duration::from_secs(5)), Some(State::Failed(_))));
        assert!(d.join("dst/real.txt").exists(), "the first item was copied");
        // The inverse names the second item too — it was about to be made — and its not being
        // there is not an error.
        let uid = undo(Some(tx)).unwrap();
        assert_eq!(wait(uid, Duration::from_secs(5)), Some(State::Done));
        assert!(!d.join("dst/real.txt").exists(), "undo removes what the failed copy left");
        assert!(d.join("real.txt").exists(), "and not the original");

        std::env::remove_var("KIKI_TRASH_DIR");
        std::env::remove_var("KIKI_STATE_DIR");
        std::fs::remove_dir_all(&d).unwrap();
    }

    // ---------------------------------------------------------------- what the activity view is told

    fn op(json: &str) -> Value {
        crate::json::parse(json.as_bytes()).unwrap()
    }

    #[test]
    fn a_job_is_described_by_what_it_moves_and_which_way() {
        let up = about("copy", &op(r#"{"op":"copy","items":["file:///home/t/site/index.html","file:///home/t/site/app.js"],"dest":"sftp://lab/srv/www"}"#));
        assert_eq!((up.name.as_str(), up.count, up.direction), ("index.html", 2, "upload"));
        assert_eq!(up.src.as_deref(), Some("file:///home/t/site"));
        assert_eq!(up.dest.as_deref(), Some("sftp://lab/srv/www"));
        assert!(!up.hidden);

        let down = about("move", &op(r#"{"op":"move","items":["sftp://lab/srv/www/logs"],"dest":"file:///home/t/Downloads"}"#));
        assert_eq!((down.direction, down.is_dir), ("download", false), "whether it is a folder is learnt when the server is asked");
        assert_eq!(about("copy", &op(r#"{"op":"copy","items":["sftp://a/x"],"dest":"ftps://b/"}"#)).direction, "remote");
        assert_eq!(about("copy", &op(r#"{"op":"copy","items":["file:///a/x"],"dest":"file:///b"}"#)).direction, "local");
        // Into the trash is this machine, not a download from anywhere.
        assert_eq!(about("trash", &op(r#"{"op":"trash","items":["file:///a/x"]}"#)).direction, "local");

        let mirror = about("mirrorRun", &op(r#"{"op":"mirrorRun","spec":{"master":"file:///home/t/site","replica":"sftp://lab/srv/www"}}"#));
        assert_eq!((mirror.name.as_str(), mirror.direction), ("Mirroring local → remote", "upload"));
    }

    #[test]
    fn machinery_is_kept_out_of_the_activity_view() {
        for (kind, json) in [
            ("delete", r#"{"op":"delete","items":["file:///a/x"],"_silent":true}"#),
            ("movePairs", r#"{"op":"movePairs","pairs":[]}"#),
            ("rmdirIfEmpty", r#"{"op":"rmdirIfEmpty","uri":"file:///a"}"#),
            ("chmodList", r#"{"op":"chmodList","list":[]}"#),
            ("mirrorScan", r#"{"op":"mirrorScan","spec":{}}"#),
        ] {
            assert!(about(kind, &op(json)).hidden, "{kind}");
        }
        assert!(!about("delete", &op(r#"{"op":"delete","items":["file:///a/x"]}"#)).hidden, "a delete the user asked for is theirs to see");
    }

    /// One job, start to finish, as the activity view sees it: a folder copy counts FILES (it
    /// ended at "1 of 3" when it counted top-level items against a total of files), names the
    /// file in hand, ends at its totals, and says where to reveal what it made.
    #[test]
    fn a_folder_copy_reports_files_the_current_one_and_where_it_went() {
        let _guard = crate::ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let d = std::env::temp_dir().join(format!("kiki-jobs-view-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(d.join("site/img")).unwrap();
        std::fs::create_dir_all(d.join("dst")).unwrap();
        std::env::set_var("KIKI_STATE_DIR", d.join("state"));
        for (name, size) in [("site/a.txt", 10usize), ("site/b.txt", 20), ("site/img/c.bin", 3000)] {
            std::fs::write(d.join(name), vec![b'x'; size]).unwrap();
        }
        let (tx, rx) = mpsc::channel();
        subscribe(tx.clone());
        let id = submit(Value::obj().s("op", "copy").v("items", Value::Arr(vec![Value::Str(Uri::from_path(&d.join("site")).to_string())])).s("dest", Uri::from_path(&d.join("dst")).to_string()).done(), Some(tx)).unwrap();
        assert_eq!(wait(id, Duration::from_secs(5)), Some(State::Done));
        let events: Vec<Value> = rx.try_iter().filter(|e| e.str_field("event") == Some("JobEvent")).filter_map(|e| e.get("job").cloned()).filter(|j| j.u64_field("id") == Some(id)).collect();
        let last = events.last().unwrap();
        assert_eq!((last.u64_field("done"), last.u64_field("total")), (Some(3), Some(3)), "three files, all counted");
        assert_eq!((last.u64_field("bytes"), last.u64_field("bytesTotal")), (Some(3030), Some(3030)));
        assert_eq!(last.str_field("name"), Some("site"));
        assert_eq!(last.get("isDir"), Some(&Value::Bool(true)));
        assert_eq!(last.str_field("revealUri"), Some(Uri::from_path(&d.join("dst/site")).to_string().as_str()));
        assert_eq!(last.get("current"), Some(&Value::Null), "a finished job is not on any file");
        assert!(events.iter().any(|j| j.str_field("state") == Some("running") && j.u64_field("total") == Some(3)), "the totals are announced when they are known, not with the first progress");
        let named: Vec<&str> = events.iter().filter_map(|j| j.get("current").and_then(|c| c.str_field("name"))).collect();
        assert!(named.iter().all(|n| n.starts_with("site/")), "files are named from the item down: {named:?}");

        // Forgetting: a finished job goes, and everybody is told which.
        let (tx2, rx2) = mpsc::channel();
        subscribe(tx2);
        assert_eq!(dismiss(Some(id)), 1);
        assert!(list().as_arr().unwrap().iter().all(|j| j.u64_field("id") != Some(id)));
        let cleared = rx2.try_iter().find(|e| e.str_field("event") == Some("JobsCleared")).expect("JobsCleared");
        assert_eq!(cleared.get("jobs").and_then(Value::as_arr).map(|a| a.len()), Some(1));
        assert_eq!(dismiss(Some(id)), 0, "and only once");
        std::env::remove_var("KIKI_STATE_DIR");
        std::fs::remove_dir_all(&d).unwrap();
    }

    /// One file that cannot be read does not stop the rest: they arrive, the job fails at the end
    /// naming the one that did not, and what arrived can still be taken back.
    #[test]
    fn a_copy_carries_on_past_a_file_it_cannot_read() {
        use std::os::unix::fs::PermissionsExt;
        let _guard = crate::ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let d = std::env::temp_dir().join(format!("kiki-jobs-carry-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(d.join("site/img")).unwrap();
        std::fs::create_dir_all(d.join("dst")).unwrap();
        std::env::set_var("KIKI_STATE_DIR", d.join("state"));
        for name in ["site/a.txt", "site/locked.txt", "site/img/z.bin"] {
            std::fs::write(d.join(name), b"data").unwrap();
        }
        std::fs::set_permissions(d.join("site/locked.txt"), std::fs::Permissions::from_mode(0o000)).unwrap();
        if std::fs::read(d.join("site/locked.txt")).is_ok() {
            return; // running as root: nothing is unreadable
        }
        let (tx, _rx) = mpsc::channel();
        let id = submit(Value::obj().s("op", "copy").v("items", Value::Arr(vec![Value::Str(Uri::from_path(&d.join("site")).to_string())])).s("dest", Uri::from_path(&d.join("dst")).to_string()).done(), Some(tx.clone()))
            .unwrap();
        let Some(State::Failed(why)) = wait(id, Duration::from_secs(5)) else { panic!("it should fail") };
        assert!(why.starts_with("1 of 3 could not be copied: locked.txt"), "{why}");
        assert!(d.join("dst/site/a.txt").exists() && d.join("dst/site/img/z.bin").exists(), "the others arrived, the one after it included");
        assert!(!d.join("dst/site/locked.txt").exists());
        let uid = undo(Some(tx)).unwrap();
        assert_eq!(wait(uid, Duration::from_secs(5)), Some(State::Done));
        assert!(!d.join("dst/site").exists(), "and undo takes back what did arrive");
        std::fs::set_permissions(d.join("site/locked.txt"), std::fs::Permissions::from_mode(0o644)).unwrap();
        std::env::remove_var("KIKI_STATE_DIR");
        std::fs::remove_dir_all(&d).unwrap();
    }

    // ---------------------------------------------------------------- chmod over a selection

    fn mode_of(p: &Path) -> u32 {
        use std::os::unix::fs::PermissionsExt;
        std::fs::symlink_metadata(p).unwrap().permissions().mode() & 0o7777
    }

    fn set_mode(p: &Path, mode: u32) {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(p, std::fs::Permissions::from_mode(mode)).unwrap();
    }

    /// The journal's newest entry: what an undo would run.
    fn last_inverse() -> Value {
        queue().lock().unwrap().journal.last().and_then(|e| e.get("inverse").cloned()).expect("a journal entry")
    }

    /// One job for the selection (0.1.1 plan 01): the bits the mask names are set on every
    /// item and the rest are left as each file has them, one undo puts every mode back, and
    /// redo sets them again.
    #[test]
    fn a_masked_chmod_touches_only_the_named_bits_and_one_undo_puts_them_all_back() {
        let _guard = crate::ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let d = std::env::temp_dir().join(format!("kiki-jobs-chmod-mask-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        std::env::set_var("KIKI_STATE_DIR", d.join("state"));
        let files = ["a.txt", "b.sh", "c.key"];
        let was = [0o644u32, 0o755, 0o600];
        for (name, mode) in files.iter().zip(was) {
            std::fs::write(d.join(name), b"x").unwrap();
            set_mode(&d.join(name), mode);
        }
        let list = |names: &[&str]| Value::Arr(names.iter().map(|n| Value::Str(Uri::from_path(&d.join(n)).to_string())).collect());
        let modes = || files.map(|n| mode_of(&d.join(n)));
        let (tx, rx) = mpsc::channel();
        subscribe(tx.clone());

        // Group write on: the mask names one bit, `bits` sets it.
        let id = submit(Value::obj().s("op", "chmod").v("items", list(&files)).u("mask", 0o010).u("bits", 0o010).b("recursive", false).done(), Some(tx.clone())).unwrap();
        assert_eq!(wait(id, Duration::from_secs(5)), Some(State::Done));
        assert_eq!(modes(), [0o654, 0o755, 0o610], "the one bit moved and nothing else did");
        let events: Vec<Value> = rx.try_iter().filter(|e| e.str_field("event") == Some("JobEvent")).filter_map(|e| e.get("job").cloned()).filter(|j| j.u64_field("id") == Some(id)).collect();
        let last = events.last().unwrap();
        assert_eq!((last.u64_field("done"), last.u64_field("total")), (Some(3), Some(3)), "counted per item");
        assert_eq!(last.str_field("title"), Some("Change permissions of 3 items"));
        assert_eq!((last.str_field("name"), last.u64_field("count")), (Some("a.txt"), Some(3)));
        // The inverse is every mode that was changed, as it was: b.sh already had the bit and
        // is not in it.
        let inv = last_inverse();
        assert_eq!(inv.str_field("op"), Some("chmodList"));
        let listed: Vec<(String, u64)> = inv.get("list").unwrap().as_arr().unwrap().iter().map(|e| (e.as_arr().unwrap()[0].as_str().unwrap().to_string(), e.as_arr().unwrap()[1].as_u64().unwrap())).collect();
        assert_eq!(listed, vec![(Uri::from_path(&d.join("a.txt")).to_string(), 0o644), (Uri::from_path(&d.join("c.key")).to_string(), 0o600)]);

        let uid = undo(Some(tx.clone())).unwrap();
        assert_eq!(wait(uid, Duration::from_secs(5)), Some(State::Done));
        assert_eq!(modes(), was, "one undo puts all three back");
        let rid = redo(Some(tx.clone())).unwrap();
        assert_eq!(wait(rid, Duration::from_secs(5)), Some(State::Done));
        assert_eq!(modes(), [0o654, 0o755, 0o610], "and redo sets them again");
        let uid = undo(Some(tx.clone())).unwrap();
        assert_eq!(wait(uid, Duration::from_secs(5)), Some(State::Done));
        assert_eq!(modes(), was);

        // The older form, one file and a whole mode, is a mask of everything.
        let id = submit(Value::obj().s("op", "chmod").v("items", list(&["a.txt"])).u("mode", 0o600).b("recursive", false).done(), Some(tx.clone())).unwrap();
        assert_eq!(wait(id, Duration::from_secs(5)), Some(State::Done));
        assert_eq!(modes(), [0o600, 0o755, 0o600]);
        let uid = undo(Some(tx.clone())).unwrap();
        assert_eq!(wait(uid, Duration::from_secs(5)), Some(State::Done));
        assert_eq!(modes(), was);
        // Half of the pair is a request that cannot mean anything.
        let id = submit(Value::obj().s("op", "chmod").v("items", list(&["a.txt"])).u("mask", 0o010).b("recursive", false).done(), Some(tx)).unwrap();
        assert!(matches!(wait(id, Duration::from_secs(5)), Some(State::Failed(m)) if m == "mask and bits go together"));
        assert_eq!(modes(), was);

        std::env::remove_var("KIKI_STATE_DIR");
        std::fs::remove_dir_all(&d).unwrap();
    }

    /// One item that cannot be changed does not stop the rest: the others change, the job fails
    /// at the end naming the one that did not, and what changed can still be put back.
    #[test]
    fn a_chmod_carries_on_past_an_item_it_cannot_change() {
        let _guard = crate::ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let d = std::env::temp_dir().join(format!("kiki-jobs-chmod-carry-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        std::env::set_var("KIKI_STATE_DIR", d.join("state"));
        for name in ["a.txt", "c.txt"] {
            std::fs::write(d.join(name), b"x").unwrap();
            set_mode(&d.join(name), 0o644);
        }
        let list = |names: &[&str]| Value::Arr(names.iter().map(|n| Value::Str(Uri::from_path(&d.join(n)).to_string())).collect());
        let (tx, rx) = mpsc::channel();
        subscribe(tx.clone());
        let id = submit(Value::obj().s("op", "chmod").v("items", list(&["a.txt", "never-was.txt", "c.txt"])).u("mask", 0o022).u("bits", 0o022).b("recursive", false).done(), Some(tx.clone())).unwrap();
        let Some(State::Failed(why)) = wait(id, Duration::from_secs(5)) else { panic!("it should fail") };
        assert_eq!(why, "1 of 3 could not be changed: never-was.txt (NotFound)");
        assert_eq!((mode_of(&d.join("a.txt")), mode_of(&d.join("c.txt"))), (0o666, 0o666), "the others changed, the one after it included");
        let events: Vec<Value> = rx.try_iter().collect();
        let last = events.iter().filter(|e| e.str_field("event") == Some("JobEvent")).filter_map(|e| e.get("job")).rfind(|j| j.u64_field("id") == Some(id)).unwrap();
        assert_eq!((last.str_field("state"), last.str_field("error")), (Some("failed"), Some(why.as_str())));
        assert_eq!(last.get("undoable"), Some(&Value::Bool(true)));
        let toast = events.iter().find(|e| e.str_field("event") == Some("Toast") && e.u64_field("job") == Some(id)).expect("a toast offering the undo");
        assert_eq!(toast.str_field("text"), Some("Change permissions of 3 items — stopped part-way"));
        let log = crate::joblog::read_job(id, 0).expect("the job has a log");
        let logged = |t: &str| log.get("lines").unwrap().as_arr().unwrap().iter().any(|l| l.str_field("level") == Some("error") && l.str_field("text").is_some_and(|x| x.ends_with(t)));
        assert!(logged("never-was.txt: NotFound"), "the log names the path");
        let uid = undo(Some(tx)).unwrap();
        assert_eq!(wait(uid, Duration::from_secs(5)), Some(State::Done));
        assert_eq!((mode_of(&d.join("a.txt")), mode_of(&d.join("c.txt"))), (0o644, 0o644), "and undo puts back the two that changed");
        std::env::remove_var("KIKI_STATE_DIR");
        std::fs::remove_dir_all(&d).unwrap();
    }

    /// Recursive: the folder and what is under it change, and the inverse names every path so.
    #[test]
    fn a_recursive_masked_chmod_is_undone_all_the_way_down() {
        let _guard = crate::ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let d = std::env::temp_dir().join(format!("kiki-jobs-chmod-tree-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(d.join("site")).unwrap();
        std::env::set_var("KIKI_STATE_DIR", d.join("state"));
        std::fs::write(d.join("site/index.html"), b"x").unwrap();
        set_mode(&d.join("site"), 0o755);
        set_mode(&d.join("site/index.html"), 0o644);
        let (tx, _rx) = mpsc::channel();
        let site = Value::Arr(vec![Value::Str(Uri::from_path(&d.join("site")).to_string())]);
        // Take "other" away entirely.
        let id = submit(Value::obj().s("op", "chmod").v("items", site).u("mask", 0o007).u("bits", 0).b("recursive", true).done(), Some(tx.clone())).unwrap();
        assert_eq!(wait(id, Duration::from_secs(5)), Some(State::Done));
        assert_eq!((mode_of(&d.join("site")), mode_of(&d.join("site/index.html"))), (0o750, 0o640));
        let inv = last_inverse();
        assert_eq!(inv.get("list").unwrap().as_arr().unwrap().len(), 2, "the folder and the file in it");
        let uid = undo(Some(tx)).unwrap();
        assert_eq!(wait(uid, Duration::from_secs(5)), Some(State::Done));
        assert_eq!((mode_of(&d.join("site")), mode_of(&d.join("site/index.html"))), (0o755, 0o644));
        std::env::remove_var("KIKI_STATE_DIR");
        std::fs::remove_dir_all(&d).unwrap();
    }
}
