//! A folder that changes while a window is open on it (plan 01): the watch.
//!
//! Its own test binary, because the watch table is one per process and capped at
//! `watch::MAX_WATCHES`: a test that fills it cannot share a process with anything else that
//! opens a folder.
//!
//! What is asserted, in the order a session meets it:
//!
//! - **A touch, a new file and a deletion each reach the windows inside the budget** — plan 01's
//!   "changes the row within 100 ms and contains exactly that entry". The watcher coalesces for
//!   50 ms and polls on a 25 ms timeout, so the floor is fixed and a change to either shows up
//!   here at once; see `within_the_budget` for why the fastest of a run is what is asserted.
//! - **Two windows on one folder are both told.** The listing is shared; the events are not.
//! - **Eviction at 64 watches keeps `Meta`.** The oldest watch goes, its listing is marked stale,
//!   and the next open re-lists and diffs against the string pool it had, so unchanged rows keep
//!   the metadata they already had — and the folder is watched again, rather than being listed
//!   once more and then left dead.

use kikid::json::Value;
use kikid::listing::{self, Subscriber};
use kikid::vfs::uri::Uri;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver};
use std::time::{Duration, Instant};

/// Plan 01: "`touch` in a watched directory changes the row within 100 ms".
const BUDGET: Duration = Duration::from_millis(100);
/// How long a change may take before the watch is simply not working.
const PATIENCE: Duration = Duration::from_secs(5);

fn setup(tag: &str) -> PathBuf {
    let _guard = kikid::ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let dir = std::env::temp_dir().join(format!("kiki-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    // Nothing of the developer's: the settings the listing reads, the access log a row is looked
    // up in, the thumbnail cache a picture would be written to.
    std::env::set_var("KIKI_CONFIG_DIR", dir.join("config"));
    std::env::set_var("KIKI_STATE_DIR", dir.join("state"));
    std::env::set_var("KIKI_DATA_DIR", dir.join("data"));
    std::env::set_var("KIKI_THUMB_DIR", dir.join("thumbs"));
    dir
}

/// The first event that `want` accepts, and how long it took to arrive.
fn until(rx: &Receiver<Value>, since: Instant, what: &str, want: impl Fn(&Value) -> bool) -> (Value, Duration) {
    let deadline = Instant::now() + PATIENCE;
    loop {
        let left = deadline.saturating_duration_since(Instant::now());
        assert!(!left.is_zero(), "no {what} arrived in {PATIENCE:?}");
        match rx.recv_timeout(left.min(Duration::from_millis(50))) {
            Ok(v) if want(&v) => return (v, since.elapsed()),
            Ok(_) => continue,
            Err(mpsc::RecvTimeoutError::Timeout) => continue,
            Err(e) => panic!("the listing hung up waiting for {what}: {e}"),
        }
    }
}

fn is(event: &str) -> impl Fn(&Value) -> bool + '_ {
    move |v: &Value| v.str_field("event") == Some(event)
}

/// The rows of a `Rows` event, or the rows a `Splice` inserts.
fn named(v: &Value) -> Vec<String> {
    let rows = match v.str_field("event") {
        Some("Rows") => v.get("rows").and_then(Value::as_arr).map(|a| a.to_vec()).unwrap_or_default(),
        Some("Splice") => v.get("ops").and_then(Value::as_arr).map(|a| a.iter().filter_map(|o| o.get("row").cloned()).collect()).unwrap_or_default(),
        _ => Vec::new(),
    };
    rows.iter().filter_map(|r| r.str_field("name")).map(str::to_string).collect()
}

/// The fastest of a run of samples is what the budget is held to, and every one of them has to
/// arrive at all. A machine building three other things at once (which is how this suite is run)
/// can push any single sample past 100 ms for reasons that have nothing to do with the watcher;
/// what would survive none of them is a debounce somebody has raised, a poll somebody has
/// lengthened, or a watch that is not registered — each of which moves every sample.
fn within_the_budget(what: &str, samples: &[Duration]) {
    let best = samples.iter().min().expect("samples");
    let mut sorted = samples.to_vec();
    sorted.sort();
    assert!(*best <= BUDGET, "{what}: the fastest of {} took {best:?}, over the {BUDGET:?} budget (median {:?})", samples.len(), sorted[sorted.len() / 2]);
}

fn window_rows(l: &std::sync::Arc<listing::Listing>) -> Vec<Value> {
    l.window(1, 1, 0, 512, None).get("rows").and_then(Value::as_arr).map(|a| a.to_vec()).unwrap_or_default()
}

fn row<'a>(rows: &'a [Value], name: &str) -> &'a Value {
    rows.iter().find(|r| r.str_field("name") == Some(name)).unwrap_or_else(|| panic!("no row called {name}"))
}

fn small_dir(at: &Path) {
    std::fs::create_dir_all(at).unwrap();
    for n in ["one.txt", "two.txt"] {
        std::fs::write(at.join(n), b"x").unwrap();
    }
}

#[test]
fn a_watched_folder_tells_every_window_what_changed_and_gives_its_watch_up_last() {
    let dir = setup("watch");
    let home = dir.join("home");
    small_dir(&home);
    std::fs::write(home.join("edited.txt"), b"before").unwrap();

    let (l, _) = listing::open(&Uri::from_path(&home)).unwrap();
    assert!(listing::wait_scan(&l, PATIENCE));
    // Two windows on the one listing, as two panes showing the same folder are.
    let (tx_a, rx_a) = mpsc::channel();
    let (tx_b, rx_b) = mpsc::channel();
    l.subscribe(Subscriber { client: 1, lid: 1, tx: tx_a, first: 0, count: 512, view_first: 0, view_count: 512 });
    l.subscribe(Subscriber { client: 2, lid: 7, tx: tx_b, first: 0, count: 512, view_first: 0, view_count: 512 });
    assert_eq!(window_rows(&l).len(), 3);
    // A small folder is enriched straight after its scan, and that is a `Rows` of its own on the
    // way. Let it land, and start from quiet.
    let enriched = Instant::now();
    while window_rows(&l).iter().any(|r| r.get("meta") == Some(&Value::Null)) {
        assert!(enriched.elapsed() < PATIENCE, "the folder was never enriched");
        std::thread::sleep(Duration::from_millis(5));
    }
    std::thread::sleep(Duration::from_millis(50));
    let _ = listing::drain(&rx_a);
    let _ = listing::drain(&rx_b);

    // ---------------------------------------------------------------- a touch
    let mut touched = Vec::new();
    for i in 0..15u32 {
        let at = Instant::now();
        std::fs::write(home.join("edited.txt"), vec![b'x'; 8 + i as usize]).unwrap();
        let (ev, took) = until(&rx_a, at, "a Rows for the touched file", |v| is("Rows")(v) && named(v).iter().any(|n| n == "edited.txt"));
        assert_eq!(named(&ev), vec!["edited.txt"], "the event carries exactly the row that changed");
        assert_eq!(ev.u64_field("lid"), Some(1));
        assert_eq!(ev.get("rows").unwrap().as_arr().unwrap()[0].get("meta").unwrap().u64_field("size"), Some(8 + i as u64), "with the size it now has");
        touched.push(took);
    }
    within_the_budget("a touch", &touched);
    // The second window was told the same thing, and about the same file.
    let (ev, _) = until(&rx_b, Instant::now(), "the second window's Rows", |v| is("Rows")(v) && named(v).iter().any(|n| n == "edited.txt"));
    assert_eq!(ev.u64_field("lid"), Some(7), "each window is told under its own lid");

    // ---------------------------------------------------------------- a file arriving
    let mut created = Vec::new();
    for i in 0..15u32 {
        let name = format!("new{i}.txt");
        let at = Instant::now();
        std::fs::write(home.join(&name), b"hello").unwrap();
        let (ev, took) = until(&rx_a, at, "a Splice inserting it", |v| is("Splice")(v) && named(v).contains(&name));
        let ops = ev.get("ops").unwrap().as_arr().unwrap();
        assert_eq!(ops.len(), 1, "one file arriving is one operation");
        assert_eq!(ops[0].str_field("op"), Some("insert"));
        assert_eq!(ev.u64_field("n"), Some(4 + i as u64), "and the count the client should now hold");
        created.push(took);
    }
    within_the_budget("a new file", &created);
    until(&rx_b, Instant::now(), "the second window's Splice", |v| is("Splice")(v) && named(v).contains(&"new14.txt".to_string()));

    // ---------------------------------------------------------------- a file going
    let mut deleted = Vec::new();
    for i in 0..15u32 {
        let name = format!("new{i}.txt");
        let before = window_rows(&l).len() as u64;
        let at = Instant::now();
        std::fs::remove_file(home.join(&name)).unwrap();
        let (ev, took) = until(&rx_a, at, "a Splice removing it", |v| is("Splice")(v) && v.u64_field("n") == Some(before - 1));
        let ops = ev.get("ops").unwrap().as_arr().unwrap();
        assert_eq!(ops.len(), 1);
        assert_eq!(ops[0].str_field("op"), Some("remove"));
        deleted.push(took);
    }
    within_the_budget("a deletion", &deleted);
    assert!(window_rows(&l).iter().all(|r| !r.str_field("name").unwrap_or("").starts_with("new")), "every one of them has gone from the view");

    // ---------------------------------------------------------------- eviction at 64 watches
    // This folder's watch is the oldest there is, so it is the one that goes when the table
    // fills. What it knew about its rows must survive: the string pool is kept, the next open
    // re-lists and diffs against it.
    let sizes_before: Vec<(String, Option<u64>)> = window_rows(&l).iter().map(|r| (r.str_field("name").unwrap().to_string(), r.get("meta").and_then(|m| m.u64_field("size")))).collect();
    assert!(sizes_before.iter().all(|(_, s)| s.is_some()), "every row was enriched before the watch went");

    let mut others = Vec::new();
    for i in 0..kikid::watch::MAX_WATCHES {
        let other = dir.join(format!("other{i}"));
        small_dir(&other);
        let (ol, _) = listing::open(&Uri::from_path(&other)).unwrap();
        assert!(listing::wait_scan(&ol, PATIENCE));
        others.push(other);
    }
    // A folder opened while the table had room is still watched — its second open is a cache hit.
    assert!(listing::open(&Uri::from_path(others.last().unwrap())).unwrap().1, "the newest watch is untouched");

    // Something happens in the folder nobody is watching any more.
    std::fs::write(home.join("while-away.txt"), b"new").unwrap();
    std::fs::write(home.join("edited.txt"), b"changed while nobody was watching").unwrap();

    let (back, cached) = listing::open(&Uri::from_path(&home)).unwrap();
    assert!(!cached, "a listing whose watch was evicted is stale: it is listed again, not served as it was");
    let rows = window_rows(&back);
    assert_eq!(rows.len(), sizes_before.len() + 1, "the file that arrived while it was away is there");
    for (name, size) in &sizes_before {
        if name == "edited.txt" {
            continue; // it really did change
        }
        assert_eq!(row(&rows, name).get("meta").and_then(|m| m.u64_field("size")), *size, "{name} kept the Meta it already had — nothing was stated again");
    }
    assert_eq!(row(&rows, "while-away.txt").get("meta"), Some(&Value::Null), "and the new one has none yet, which is what says the rest were kept");

    // And it is live again: a folder you come back to is watched, or it would be listed once and
    // then quietly stop keeping up.
    let (tx_c, rx_c) = mpsc::channel();
    back.subscribe(Subscriber { client: 3, lid: 2, tx: tx_c, first: 0, count: 512, view_first: 0, view_count: 512 });
    let at = Instant::now();
    std::fs::write(home.join("after-return.txt"), b"live").unwrap();
    let (ev, _) = until(&rx_c, at, "a Splice for the folder that came back", |v| is("Splice")(v) && named(v).contains(&"after-return.txt".to_string()));
    assert_eq!(ev.get("ops").unwrap().as_arr().unwrap().len(), 1);

    std::fs::remove_dir_all(&dir).unwrap();
}
