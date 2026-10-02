//! The index keeps up with what is on screen (docs/0.5.0/06-index-live.md): a folder a window
//! is showing is watched, and what the watch sees reaches Search everywhere without waiting for
//! the ten-minute walk.
//!
//! Its own test binary, like `watch.rs`: the watch table and the index are one per process.
//! One test, in the order a session meets it: a file made in a watched folder is found within a
//! second and gone when removed; a folder made there is listed one level, and renamed takes
//! its children with it; what lies deeper waits for the refresh, which finds it by the mtime
//! the live patch put down as "not yet"; and a burst of five thousand files costs the index a
//! handful of batches, not five thousand lookups.

use kikid::index::{self, Mode};
use kikid::json::Value;
use kikid::listing::{self, Subscriber};
use kikid::vfs::uri::Uri;
use std::path::PathBuf;
use std::sync::mpsc;
use std::time::{Duration, Instant};

/// The plan's bound: what the watch sees is searchable within a second.
const WITHIN: Duration = Duration::from_secs(1);

fn setup(tag: &str) -> PathBuf {
    let _guard = kikid::ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let dir = std::env::temp_dir().join(format!("kiki-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("home")).unwrap();
    std::env::set_var("KIKI_CONFIG_DIR", dir.join("config"));
    std::env::set_var("KIKI_STATE_DIR", dir.join("state"));
    std::env::set_var("KIKI_DATA_DIR", dir.join("data"));
    std::env::set_var("KIKI_CACHE_DIR", dir.join("cache"));
    std::env::set_var("KIKI_THUMB_DIR", dir.join("thumbs"));
    dir
}

/// Every path the index knows whose name starts with `prefix`.
fn found(prefix: &str) -> Vec<PathBuf> {
    index::with_index(|ix| index::query(ix, prefix, Mode::Prefix).0.iter().map(|h| ix.path(h.entry)).collect())
}

/// Polls until `pred` holds, saying how long it took; panics past `limit`.
fn soon(what: &str, limit: Duration, pred: impl Fn() -> bool) -> Duration {
    let t0 = Instant::now();
    while !pred() {
        assert!(t0.elapsed() < limit, "{what}: not within {limit:?}");
        std::thread::sleep(Duration::from_millis(20));
    }
    t0.elapsed()
}

fn status() -> Value {
    index::status_json()
}

#[test]
fn a_watched_folder_feeds_the_index_as_it_changes() {
    let dir = setup("index-live");
    let home = dir.join("home");
    std::fs::write(home.join("already.txt"), b"").unwrap();
    kikid::config::set_settings(&Value::obj().v("index", Value::obj().v("roots", Value::Arr(vec![Value::Str(Uri::from_path(&home).to_string())])).done()).done()).unwrap();
    index::rebuild_async();
    soon("the index builds", Duration::from_secs(10), || status().u64_field("entries").unwrap_or(0) > 0 && status().get("refreshing").and_then(Value::as_bool) == Some(false));
    assert_eq!(found("already").len(), 1, "the build found what was there");

    // A window on the folder: that is what makes it watched.
    let (l, _) = listing::open(&Uri::from_path(&home)).unwrap();
    let (tx, _rx) = mpsc::channel();
    l.subscribe(Subscriber { client: 1, lid: 1, tx: tx.into(), first: 0, count: 512, view_first: 0, view_count: 512, initial: 0 });
    let _ = l.window(1, 1, 0, 512, None);
    std::thread::sleep(Duration::from_millis(100));

    // ---------------------------------------------------------------- a file, made and unmade
    std::fs::write(home.join("notes.txt"), b"").unwrap();
    let took = soon("a new file is found", WITHIN, || found("notes.txt").len() == 1);
    eprintln!("found {took:?} after it was written");
    assert!(status().u64_field("live").unwrap_or(0) >= 1, "IndexStatus counts what came in live: {:?}", status());
    std::fs::remove_file(home.join("notes.txt")).unwrap();
    soon("a removed file is gone", WITHIN, || found("notes.txt").is_empty());

    // ---------------------------------------------------------------- a folder, and its rename
    std::fs::create_dir(home.join("alpha")).unwrap();
    std::fs::write(home.join("alpha/inside.txt"), b"").unwrap();
    soon("a new folder is listed one level", WITHIN, || found("inside.txt").len() == 1);
    // Deeper than one level, made while nobody watches `alpha`: the refresh's to find.
    std::fs::create_dir(home.join("alpha/deep")).unwrap();
    std::fs::write(home.join("alpha/deep/x.txt"), b"").unwrap();
    std::thread::sleep(Duration::from_millis(200));
    std::fs::rename(home.join("alpha"), home.join("beta")).unwrap();
    soon("the renamed folder is found under its new name", WITHIN, || found("beta").len() == 1 && found("alpha").is_empty());
    soon("and its children with it", WITHIN, || found("inside.txt").iter().any(|p| p.ends_with("beta/inside.txt")) && found("inside.txt").len() == 1);
    assert!(found("x.txt").is_empty(), "two levels down is the refresh's, not the patch's");
    index::refresh_walk();
    assert!(found("x.txt").iter().any(|p| p.ends_with("beta/deep/x.txt")), "the refresh walks the subfolder the patch put down as not yet listed: {:?}", found("x.txt"));
    assert_eq!(status().u64_field("live").unwrap_or(9), 0, "a refresh folds the live entries in");

    // ---------------------------------------------------------------- a burst
    let (patches0, relists0) = index::work();
    for i in 0..5_000 {
        std::fs::write(home.join(format!("burst-{i:05}.txt")), b"").unwrap();
    }
    soon("five thousand files are all found", Duration::from_secs(10), || found("burst-").len() == 5_000);
    let (patches, relists) = index::work();
    eprintln!("the burst cost {} patch batches and {} re-lists", patches - patches0, relists - relists0);
    assert!(patches - patches0 <= 20 && relists - relists0 <= 1, "a burst is a few batches, not a lookup per name: {} patches, {} re-lists", patches - patches0, relists - relists0);

    drop(l);
    let _ = std::fs::remove_dir_all(&dir);
}
