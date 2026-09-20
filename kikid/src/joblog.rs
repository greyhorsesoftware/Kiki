//! A log per job (plan 32): what kiki did, and beneath it what the SSH, FTP or TLS library said
//! while it did it. It is what the activity view's **Log** button opens, and what somebody pastes
//! into a bug report when an upload to their server fails and "Denied" is all the job can say.
//!
//! Lines come from two places. kiki's own (`say`): the job started, each file begun, a collision
//! answered, cancel asked for, how it ended. And the plugins' (`from_plugin`): every record the
//! libraries write through the `log` facade, forwarded by the SDK with secrets already taken out.
//! A library line carries the role of the session it was written for; a job's sessions are its
//! own (`job-<id>`), so that is whose line it is. A line from one of the library's background
//! threads carries none, and goes to every job with a session open on that plugin just then.
//!
//! Everything is a ring: a job keeps its last 2,000 lines or 256 KB, a plugin the same, and says
//! how many earlier ones it let go. Nothing is written to disk except the log of a job that
//! FAILED, appended to `failed-jobs.log` — a failure from last night should still be readable in
//! the morning, after the job itself has been cleared.

use crate::json::Value;
use std::collections::{HashMap, VecDeque};
use std::sync::{Mutex, OnceLock};

pub const MAX_LINES: usize = 2_000;
pub const MAX_BYTES: usize = 256 * 1024;
const FAILED_LOG_CAP: u64 = 1024 * 1024;

#[derive(Clone)]
struct Line {
    t: u64,
    level: String,
    /// `kiki`, or the plugin and the library's own target: `sftp russh::client`.
    source: String,
    text: String,
}

#[derive(Default)]
struct Ring {
    lines: VecDeque<Line>,
    bytes: usize,
    /// How many have been let go: also the sequence number of the first line still held.
    dropped: u64,
}

impl Ring {
    fn push(&mut self, l: Line) {
        self.bytes += l.text.len() + l.source.len();
        self.lines.push_back(l);
        while self.lines.len() > MAX_LINES || self.bytes > MAX_BYTES {
            match self.lines.pop_front() {
                Some(old) => {
                    self.bytes -= old.text.len() + old.source.len();
                    self.dropped += 1;
                }
                None => break,
            }
        }
    }

    /// Lines from sequence number `from` on; `next` is what to ask for the time after.
    fn read(&self, from: u64) -> Value {
        let skip = from.saturating_sub(self.dropped) as usize;
        let lines: Vec<Value> = self.lines.iter().skip(skip).map(|l| Value::obj().u("t", l.t).s("level", l.level.clone()).s("source", l.source.clone()).s("text", l.text.clone()).done()).collect();
        Value::obj().v("lines", Value::Arr(lines)).u("next", self.dropped + self.lines.len() as u64).u("dropped", self.dropped).done()
    }
}

#[derive(Default)]
struct Logs {
    jobs: HashMap<u64, Ring>,
    plugins: HashMap<String, Ring>,
}

fn logs() -> &'static Mutex<Logs> {
    static L: OnceLock<Mutex<Logs>> = OnceLock::new();
    L.get_or_init(|| Mutex::new(Logs::default()))
}

fn now_ms() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0)
}

/// One of kiki's own lines about a job.
pub fn say(job: u64, level: &str, text: impl Into<String>) {
    let line = Line { t: now_ms(), level: level.to_string(), source: "kiki".into(), text: text.into() };
    logs().lock().unwrap().jobs.entry(job).or_default().push(line);
}

/// `job-12` -> 12.
fn job_of(role: &str) -> Option<u64> {
    role.strip_prefix("job-").and_then(|n| n.parse().ok())
}

/// A `Log` event from a plugin process. `using` is the jobs that have a session open on that
/// plugin right now: who a line that names nobody is given to.
pub fn from_plugin(scheme: &str, event: &Value, using: &[u64]) {
    let target = event.str_field("target").unwrap_or("");
    let line = Line {
        t: now_ms(),
        level: event.str_field("level").unwrap_or("info").to_string(),
        source: if target.is_empty() { scheme.to_string() } else { format!("{scheme} {target}") },
        text: event.str_field("message").unwrap_or("").to_string(),
    };
    let mut all = logs().lock().unwrap();
    match job_of(event.str_field("role").unwrap_or("")) {
        Some(job) => all.jobs.entry(job).or_default().push(line.clone()),
        None if event.str_field("role").unwrap_or("").is_empty() => {
            for job in using {
                all.jobs.entry(*job).or_default().push(line.clone());
            }
        }
        None => {} // the browser's: the plugin's own log below is where it belongs
    }
    all.plugins.entry(scheme.to_string()).or_default().push(line);
}

pub fn read_job(job: u64, from: u64) -> Option<Value> {
    logs().lock().unwrap().jobs.get(&job).map(|r| r.read(from))
}

/// The connection log of a location kind: everything its plugin has said, whoever it was for.
/// What to read when a location will not connect at all — there is no job to look under.
pub fn read_plugin(scheme: &str, from: u64) -> Value {
    logs().lock().unwrap().plugins.get(scheme).map(|r| r.read(from)).unwrap_or_else(|| Ring::default().read(0))
}

/// The job has been cleared from the list: its log goes with it.
pub fn forget(job: u64) {
    logs().lock().unwrap().jobs.remove(&job);
}

/// A job failed: its log, as text, onto the end of `failed-jobs.log` (kept under 1 MB by starting
/// the file again when it would grow past that — the newest failure is the one that matters).
pub fn keep_failure(job: u64, title: &str, error: &str) {
    let text = {
        let all = logs().lock().unwrap();
        let Some(ring) = all.jobs.get(&job) else { return };
        let mut out = format!("==== job {job}: {title}\n==== failed: {error}\n");
        if ring.dropped > 0 {
            out.push_str(&format!("… {} earlier lines dropped\n", ring.dropped));
        }
        for l in &ring.lines {
            out.push_str(&format!("{} {:5} {:<24} {}\n", crate::listing::iso(l.t), l.level, l.source, l.text));
        }
        out.push('\n');
        out
    };
    let path = crate::jobs::state_dir().join("failed-jobs.log");
    let _ = std::fs::create_dir_all(crate::jobs::state_dir());
    let too_big = std::fs::metadata(&path).map(|m| m.len() + text.len() as u64 > FAILED_LOG_CAP).unwrap_or(false);
    use std::io::Write;
    let file = std::fs::OpenOptions::new().create(true).write(true).append(!too_big).truncate(too_big).open(&path);
    if let Ok(mut f) = file {
        let _ = f.write_all(text.as_bytes());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lib(role: &str, text: &str) -> Value {
        Value::obj().s("event", "Log").s("level", "debug").s("target", "russh::client").s("message", text).s("role", role).done()
    }

    fn texts(v: &Value) -> Vec<String> {
        v.get("lines").and_then(Value::as_arr).unwrap().iter().map(|l| l.str_field("text").unwrap().to_string()).collect()
    }

    #[test]
    fn a_line_goes_to_the_job_it_was_written_for() {
        let (a, b) = (900_001, 900_002);
        say(a, "info", "started");
        from_plugin("sftp", &lib("job-900001", "channel open"), &[a, b]);
        from_plugin("sftp", &lib("job-900002", "other job's channel"), &[a, b]);
        // The browser's connection is nobody's job…
        from_plugin("sftp", &lib("browse", "listing /srv"), &[a, b]);
        // …and a line from the library's own thread is every job's that is using the plugin.
        from_plugin("sftp", &lib("", "keepalive"), &[a, b]);
        assert_eq!(texts(&read_job(a, 0).unwrap()), ["started", "channel open", "keepalive"]);
        assert_eq!(texts(&read_job(b, 0).unwrap()), ["other job's channel", "keepalive"]);
        let src = read_job(a, 0).unwrap();
        let first_lib = &src.get("lines").and_then(Value::as_arr).unwrap()[1];
        assert_eq!(first_lib.str_field("source"), Some("sftp russh::client"));
        // The plugin's own log has all of it, the browser's lines too.
        assert!(texts(&read_plugin("sftp", 0)).contains(&"listing /srv".to_string()));
        forget(a);
        forget(b);
        assert!(read_job(a, 0).is_none(), "a cleared job's log goes with it");
    }

    #[test]
    fn a_log_is_a_ring_and_says_what_it_let_go() {
        let job = 900_010;
        for i in 0..(MAX_LINES + 25) {
            say(job, "debug", format!("line {i}"));
        }
        let all = read_job(job, 0).unwrap();
        assert_eq!(all.u64_field("dropped"), Some(25));
        assert_eq!(texts(&all).len(), MAX_LINES);
        assert_eq!(texts(&all)[0], "line 25");
        // Reading on from where the last read ended gets only what is new.
        let next = all.u64_field("next").unwrap();
        say(job, "info", "one more");
        assert_eq!(texts(&read_job(job, next).unwrap()), ["one more"]);
        // Size counts as well as number: a few enormous lines do not hold a quarter of a megabyte each.
        let big = 900_011;
        for _ in 0..10 {
            say(big, "debug", "x".repeat(100 * 1024));
        }
        assert!(texts(&read_job(big, 0).unwrap()).len() <= 2);
        forget(job);
        forget(big);
    }

    /// Last night's failure is still there in the morning, after the job has been cleared.
    #[test]
    fn a_failed_jobs_log_is_kept_on_disk_and_the_file_stays_small() {
        let _g = crate::ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let d = std::env::temp_dir().join(format!("kiki-joblog-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::env::set_var("KIKI_STATE_DIR", &d);
        let job = 900_020;
        say(job, "info", "Copy site to www — started");
        from_plugin("sftp", &lib("job-900020", "write /srv/www/index.html.kiki-part (17 bytes)"), &[]);
        say(job, "error", "failed: index.html: Denied");
        keep_failure(job, "Copy site to www", "index.html: Denied");
        forget(job);
        let kept = std::fs::read_to_string(d.join("failed-jobs.log")).unwrap();
        assert!(kept.contains("==== job 900020: Copy site to www"));
        assert!(kept.contains("==== failed: index.html: Denied"));
        assert!(kept.contains("sftp russh::client") && kept.contains("index.html.kiki-part"), "the library's lines are kept with kiki's own");

        // It does not grow for ever: past the cap it is begun again, with the newest failure in it.
        let big = 900_021;
        for _ in 0..4 {
            say(big, "debug", "y".repeat(60 * 1024));
        }
        for _ in 0..8 {
            keep_failure(big, "a big one", "boom");
        }
        assert!(std::fs::metadata(d.join("failed-jobs.log")).unwrap().len() <= FAILED_LOG_CAP);
        assert!(std::fs::read_to_string(d.join("failed-jobs.log")).unwrap().contains("a big one"));
        forget(big);
        std::env::remove_var("KIKI_STATE_DIR");
        let _ = std::fs::remove_dir_all(&d);
    }
}
