//! "If kiki is not running, stuff should not continue in the background." The window asks before
//! it closes on running jobs; this is the daemon's half, for a kiki that went without asking —
//! killed, crashed. Its jobs are stopped once the last window has been gone for a short grace,
//! a window that comes back inside the grace keeps them, and nobody else's jobs are touched.
//!
//! The jobs are compares of a server that takes four seconds to answer (`KIKI_STUB_SLOW_CONNECT`):
//! running for as long as the test needs them to be, and cancellable at once.

mod common;

use kikid::json::Value;
use kikid::vfs::uri::Uri;
use kikid::{jobs, locations};
use std::time::{Duration, Instant};

fn state(id: u64) -> String {
    jobs::list().as_arr().unwrap().iter().find(|j| j.u64_field("id") == Some(id)).and_then(|j| j.str_field("state").map(str::to_string)).expect("the job is in the list")
}

fn until(what: &str, patience: Duration, f: impl Fn() -> bool) {
    let start = Instant::now();
    while !f() {
        assert!(start.elapsed() < patience, "waited for {what} and it never happened");
        std::thread::sleep(Duration::from_millis(5));
    }
}

fn slow_compare(master: &std::path::Path, name: &str) -> u64 {
    let loc = Value::obj().s("name", name).s("plugin", "stub").s("remoteUri", format!("stub://{name}/")).v("config", Value::obj().s("name", name).done()).done();
    locations::upsert(loc).unwrap();
    let spec = Value::obj()
        .s("master", Uri::from_path(master).to_string())
        .s("replica", format!("stub://{name}/"))
        .s("direction", "upload")
        .b("deleteExtras", false)
        .b("clockOffsetAuto", false)
        .s("detector", "sizeOnly")
        .b("applyFilters", false)
        .done();
    let id = jobs::submit(Value::obj().s("op", "mirrorScan").v("spec", spec).done(), None).unwrap();
    until(&format!("the compare of {name} to start"), Duration::from_secs(5), || {
        let s = state(id);
        assert!(s == "running" || s == "queued", "{name}: {s}: {}", kikid::json::to_string(&jobs::list()));
        s == "running"
    });
    id
}

#[test]
fn a_window_that_has_gone_takes_its_jobs_with_it() {
    let dir = common::setup("shell-gone");
    std::env::set_var("KIKI_STATE_DIR", dir.join("state"));
    std::env::set_var("KIKI_STUB_SLOW_CONNECT", "4000");
    std::env::set_var("KIKI_SHELL_GRACE_MS", "300");
    let master = dir.join("m");
    std::fs::create_dir_all(&master).unwrap();
    std::fs::write(master.join("a.txt"), b"a").unwrap();

    // Gone and not back: the window's job is stopped, a script's is not.
    jobs::shell_came();
    let mine = slow_compare(&master, "slow-mine");
    jobs::own(mine);
    let theirs = slow_compare(&master, "slow-theirs");
    jobs::shell_went();
    assert_eq!(state(mine), "running", "stopped before the grace was up");
    until("the window's job to be stopped", Duration::from_secs(3), || state(mine) == "cancelled");
    assert_eq!(state(theirs), "running", "a job no window asked for was stopped with the window's");
    assert!(jobs::cancel(theirs));
    until("the other job to end", Duration::from_secs(3), || state(theirs) == "cancelled");

    // Back inside the grace — a dropped socket picked up again — and the job goes on. So it does
    // while a second window is still there.
    jobs::shell_came();
    let kept = slow_compare(&master, "slow-kept");
    jobs::own(kept);
    jobs::shell_went();
    jobs::shell_came();
    jobs::shell_came();
    jobs::shell_went();
    std::thread::sleep(Duration::from_millis(700));
    assert_eq!(state(kept), "running", "a window came back and its job was stopped all the same");
    jobs::shell_went();
    until("the job to be stopped once the last window went", Duration::from_secs(3), || state(kept) == "cancelled");

    for name in ["slow-mine", "slow-theirs", "slow-kept"] {
        locations::remove(name).unwrap();
    }
    std::fs::remove_dir_all(&dir).unwrap();
}
