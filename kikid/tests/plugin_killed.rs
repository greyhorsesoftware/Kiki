//! A plugin killed while a copy job is reading through it. Two things have to be true afterwards:
//! the job **says what happened** rather than reporting a short file as copied, and the **next
//! request gets a plugin again** — a crashed plugin must not leave the location dead until the
//! daemon is restarted.
//!
//! A real process is really killed (SIGKILL to its process group, which is what the daemon does
//! to a plugin that has to go), part-way through a `Read` whose bytes are already arriving.

mod common;

use kikid::json::Value;
use kikid::plugin::{self, Msg};
use kikid::vfs::uri::Uri;
use kikid::{jobs, listing, locations};
use std::sync::mpsc;
use std::sync::Arc;
use std::time::{Duration, Instant};

fn job_json(id: u64) -> Value {
    jobs::list().as_arr().unwrap().iter().find(|j| j.u64_field("id") == Some(id)).cloned().expect("the job is in the list")
}

/// Polls until the job has stopped, and returns it.
fn finished(id: u64, patience: Duration) -> Value {
    let start = Instant::now();
    loop {
        let j = job_json(id);
        if !matches!(j.str_field("state"), Some("running") | Some("queued")) {
            return j;
        }
        assert!(start.elapsed() < patience, "the job never stopped: {}", kikid::json::to_string(&j));
        std::thread::sleep(Duration::from_millis(5));
    }
}

/// Waits for `f` to hold, so that the kill lands while bytes are moving rather than at a guessed
/// moment. `slow.bin` takes about half a second, which is the room this has to work in.
fn until(what: &str, f: impl Fn() -> bool) {
    let start = Instant::now();
    while !f() {
        assert!(start.elapsed() < Duration::from_secs(10), "{what} never happened");
        std::thread::sleep(Duration::from_millis(2));
    }
}

#[test]
fn a_plugin_killed_mid_read_fails_the_job_and_the_next_request_gets_a_new_one() {
    let dir = common::setup("killed");
    std::env::set_var("KIKI_STATE_DIR", dir.join("state"));
    common::save_location("lab");
    let dst = dir.join("dst");
    std::fs::create_dir_all(&dst).unwrap();

    // The plugin is up before the job starts, so this is the process the job will read through.
    locations::connect(&locations::find("lab").unwrap(), "browse", None).unwrap();
    let doomed = plugin::get("stub").unwrap();
    assert!(doomed.alive());

    let (tx, _rx) = mpsc::channel();
    let id = jobs::submit(Value::obj().s("op", "copy").v("items", Value::Arr(vec![Value::Str("stub://lab/slow.bin".into())])).s("dest", Uri::from_path(&dst).to_string()).done(), Some(tx)).unwrap();

    // Bytes are landing in the part file: the `Read` is in flight, and some of it has arrived.
    let part = dst.join("slow.bin.kiki-part");
    until("the download started", || std::fs::metadata(&part).map(|m| m.len() > 0).unwrap_or(false));
    doomed.kill_group();

    let j = finished(id, Duration::from_secs(20));
    assert_eq!(j.str_field("state"), Some("failed"), "a plugin that died mid-read is not a copy that worked");
    let err = j.str_field("error").unwrap_or("");
    assert!(err.contains("slow.bin"), "the job names the file it lost: {err}");
    assert!(err.contains("plugin exited"), "and says what went wrong, in words: {err}");
    // What half arrived is not left lying about under the real name or the part name.
    assert!(!dst.join("slow.bin").exists(), "no short file under the name the user asked for");
    assert!(!part.exists(), "and no part file left behind");
    // The same sentence is in the job's log, which is what the activity view's Log button opens.
    let log = kikid::joblog::read_job(id, 0).map(|l| kikid::json::to_string(&l)).unwrap_or_default();
    assert!(log.contains("plugin exited"), "the log says it too: {log}");

    // ---------------------------------------------------------------- and it comes back
    assert!(!doomed.alive(), "the process really is gone");
    let (session, _) = locations::resolve(&Uri::parse("stub://lab/").unwrap()).expect("the location resolves again");
    assert!(!Arc::ptr_eq(&session.plugin, &doomed), "a new process, not the dead one handed back by the session cache");
    assert!(session.plugin.alive());
    let st = session.plugin.request(session.req("Stat").s("path", "/data.bin").done()).expect("the new plugin answers");
    assert_eq!(st.u64_field("size"), Some(4096));

    // And the whole way through, as a user would come back to it: list the folder, then copy the
    // file that failed — which now arrives whole.
    listing::invalidate_authority("stub", "lab");
    let (l, _) = listing::open(&Uri::parse("stub://lab/").unwrap()).unwrap();
    assert!(listing::wait_scan(&l, Duration::from_secs(5)));
    assert_eq!(l.count().0, 4);
    let mut got = Vec::new();
    session
        .plugin
        .read_stream(session.req("Read").s("path", "/docs/notes.txt").done(), |m| {
            if let Msg::Binary(b) = m {
                got.extend_from_slice(&b)
            }
        })
        .unwrap();
    assert_eq!(got, b"hello");

    let again = jobs::submit(Value::obj().s("op", "copy").v("items", Value::Arr(vec![Value::Str("stub://lab/slow.bin".into())])).s("dest", Uri::from_path(&dst).to_string()).done(), None).unwrap();
    let j = finished(again, Duration::from_secs(20));
    assert_eq!(j.str_field("state"), Some("done"), "{}", j.str_field("error").unwrap_or(""));
    assert_eq!(std::fs::metadata(dst.join("slow.bin")).unwrap().len(), 8192);

    locations::remove("lab").unwrap();
    std::env::remove_var("KIKI_STATE_DIR");
    std::fs::remove_dir_all(&dir).unwrap();
}
