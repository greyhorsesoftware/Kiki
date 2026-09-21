//! Cancelling a mirror's compare (plan 08's Preflight screen), through the whole machine: a real
//! plugin process, the job queue, and the session bookkeeping a cancelled job leaves behind.
//!
//! Two things have to be true, and neither was:
//! - the **plugin** stops walking. The daemon reading a stream can stop listening at any time;
//!   that is not the same as the server being told, and a tree of 200,000 files goes on being
//!   walked — and the connection goes on being busy — long after the person gave up on it.
//! - a compare of a server that is **slow to answer** ends when it is cancelled, not when the
//!   connect times out two minutes later.
//!
//! The stub answers a recursive `Scan` with a big synthetic tree, slowly, from a thread of its own
//! (`KIKI_STUB_BIG_SCAN`), and writes down how far it got (`KIKI_STUB_SCAN_LOG`) — which is how a
//! test can say whether the plugin's own work stopped.

mod common;

use kikid::json::Value;
use kikid::vfs::uri::Uri;
use kikid::{jobs, listing, locations};
use std::time::{Duration, Instant};

const TREE: u64 = 50_000;

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

fn until(what: &str, patience: Duration, f: impl Fn() -> bool) {
    let start = Instant::now();
    while !f() {
        assert!(start.elapsed() < patience, "waited for {what} and it never happened");
        std::thread::sleep(Duration::from_millis(5));
    }
}

fn spec(master: &std::path::Path, replica: &str) -> Value {
    Value::obj()
        .s("master", Uri::from_path(master).to_string())
        .s("replica", replica.to_string())
        .s("direction", "upload")
        .b("deleteExtras", false)
        .b("clockOffsetAuto", false)
        .s("detector", "sizeOnly")
        .b("applyFilters", false)
        .done()
}

#[test]
fn a_cancelled_compare_stops_everywhere() {
    let dir = common::setup("mirror-cancel");
    // The mirror writes an audit log; not into the developer's own state directory.
    std::env::set_var("KIKI_STATE_DIR", dir.join("state"));
    // Read by the stub PROCESS, so both are set before it is ever spawned.
    std::env::set_var("KIKI_STUB_BIG_SCAN", TREE.to_string());
    std::env::set_var("KIKI_STUB_SCAN_LOG", dir.join("scan.log"));
    std::env::set_var("KIKI_STUB_SLOW_CONNECT", "4000");

    let master = dir.join("m");
    std::fs::create_dir_all(&master).unwrap();
    std::fs::write(master.join("a.txt"), b"a").unwrap();

    the_plugin_stops_walking_too(&dir, &master);
    a_server_that_never_answers_does_not_hold_the_cancel(&master);

    std::fs::remove_dir_all(&dir).unwrap();
}

/// Cancel while the server's side is streaming: the job ends cancelled at once, the PLUGIN stops
/// walking where it was, and the browser can still list the location afterwards — the compare's
/// own session was logged out cleanly although the cancellation token was already set.
fn the_plugin_stops_walking_too(dir: &std::path::Path, master: &std::path::Path) {
    common::save_location("lab");
    let log = dir.join("scan.log");
    let _ = std::fs::remove_file(&log);

    let id = jobs::submit(Value::obj().s("op", "mirrorScan").v("spec", spec(master, "stub://lab/")).done(), None).unwrap();
    // Cancel while it is walking, not before it starts: the count it reports as it goes says so.
    until("the compare to get going", Duration::from_secs(20), || job_json(id).u64_field("done").unwrap_or(0) > 0);

    let asked = Instant::now();
    assert!(jobs::cancel(id));
    let j = finished(id, Duration::from_secs(5));
    assert_eq!(j.str_field("state"), Some("cancelled"), "{}", kikid::json::to_string(&j));
    assert!(asked.elapsed() < Duration::from_secs(2), "the compare took {:?} to give up", asked.elapsed());

    // What the plugin itself did: it stopped where it was told to, rather than walking the rest
    // of the tree into a pipe nobody was reading.
    until("the plugin to write down where it stopped", Duration::from_secs(5), || log.exists());
    let line = std::fs::read_to_string(&log).unwrap();
    let sent: u64 = line.split_whitespace().next().unwrap().parse().unwrap();
    assert!(sent < TREE, "the plugin walked the whole tree anyway: {line}");
    assert!(sent < TREE / 2, "the plugin kept going a long way past the cancel: {line}");

    // And the location still browses: the compare's sessions were its own and were let go.
    let (l, _) = listing::open(&Uri::parse("stub://lab/docs").unwrap()).unwrap();
    assert!(listing::wait_scan(&l, Duration::from_secs(5)));
    assert_eq!(l.window(1, 1, 0, 10, None).u64_field("n"), Some(2), "the browser still lists the server");

    listing::invalidate_authority("stub", "lab");
    locations::remove("lab").unwrap();
}

/// A server that is slow to answer — or not there at all. The connect is part of the compare, so
/// Cancel must not sit behind it: before, nothing looked at the cancellation token until the
/// plugin replied, and the job stayed "running" for the whole two-minute request timeout.
fn a_server_that_never_answers_does_not_hold_the_cancel(master: &std::path::Path) {
    let loc = Value::obj().s("name", "slow-lab").s("plugin", "stub").s("remoteUri", "stub://slow-lab/").v("config", Value::obj().s("name", "slow-lab").done()).done();
    locations::upsert(loc).unwrap(); // saved without connecting: nothing to wait for yet

    let id = jobs::submit(Value::obj().s("op", "mirrorScan").v("spec", spec(master, "stub://slow-lab/")).done(), None).unwrap();
    until("the compare to start", Duration::from_secs(5), || job_json(id).str_field("state") == Some("running"));
    std::thread::sleep(Duration::from_millis(100)); // it is inside the connect by now

    let asked = Instant::now();
    assert!(jobs::cancel(id));
    let j = finished(id, Duration::from_secs(3));
    assert_eq!(j.str_field("state"), Some("cancelled"), "{}", kikid::json::to_string(&j));
    assert!(asked.elapsed() < Duration::from_secs(2), "the cancel waited {:?} for a server that was never going to answer", asked.elapsed());

    locations::remove("slow-lab").unwrap();
}
