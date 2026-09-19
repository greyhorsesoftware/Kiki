//! The listing's tests: sorting, filtering, windows, watching and the cache.

use crate::listing::*;
use std::time::Duration;

fn temp_tree(n: usize) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("kiki-listing-{}-{}", std::process::id(), n));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("sub")).unwrap();
    for i in 0..n {
        std::fs::write(dir.join(format!("file{i}.txt")), vec![b'x'; i % 7]).unwrap();
    }
    dir
}

/// A folder deleted and recreated with the same name is a different folder: the cached
/// listing holds a handle on the old inode, and reusing it shows the old contents (or
/// nothing at all) for ever.
#[test]
fn a_recreated_folder_is_listed_afresh() {
    let dir = std::env::temp_dir().join(format!("kiki-recreate-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("before.txt"), b"before").unwrap();
    let uri = Uri::from_path(&dir);

    let (l, _) = open(&uri).unwrap();
    assert!(wait_scan(&l, Duration::from_secs(5)));
    assert_eq!(l.window(1, 1, 0, 10).u64_field("n"), Some(1));

    std::fs::remove_dir_all(&dir).unwrap();
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("after-one.txt"), b"1").unwrap();
    std::fs::write(dir.join("after-two.txt"), b"2").unwrap();

    let (l2, cached) = open(&uri).unwrap();
    assert!(!cached, "a replaced directory must not be served from the cache");
    assert!(wait_scan(&l2, Duration::from_secs(5)));
    let w = l2.window(1, 2, 0, 10);
    assert_eq!(w.u64_field("n"), Some(2));
    let rows = w.get("rows").unwrap().as_arr().unwrap();
    assert_eq!(rows[0].str_field("name"), Some("after-one.txt"));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn open_window_and_stats() {
    let dir = temp_tree(50);
    let uri = Uri::from_path(&dir);
    let (l, cached) = open(&uri).unwrap();
    assert!(!cached);
    assert!(wait_scan(&l, Duration::from_secs(5)));
    let (tx, rx) = mpsc::channel();
    l.subscribe(Subscriber { client: 1, lid: 7, tx, first: 0, count: 10 });
    let w = l.window(1, 7, 0, 10);
    assert_eq!(w.u64_field("n"), Some(51));
    let rows = w.get("rows").unwrap().as_arr().unwrap();
    assert_eq!(rows.len(), 10);
    assert_eq!(rows[0].str_field("name"), Some("sub")); // folders first
    assert_eq!(rows[1].str_field("name"), Some("file0.txt")); // natural order
    assert_eq!(rows[2].str_field("name"), Some("file1.txt"));
    // small dir: enrichment lands soon; wait for a Rows event or metadata
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let w = l.window(1, 7, 0, 10);
        let rows = w.get("rows").unwrap().as_arr().unwrap();
        if rows[1].get("meta").map(|m| m != &Value::Null).unwrap_or(false) {
            assert_eq!(rows[1].get("meta").unwrap().u64_field("size"), Some(0));
            break;
        }
        assert!(Instant::now() < deadline, "metadata never arrived");
        thread::sleep(Duration::from_millis(5));
    }
    let _ = drain(&rx);
    // second open is served from the cache
    let (_l2, cached) = open(&uri).unwrap();
    assert!(cached);
    // filter
    assert_eq!(l.filter("file1"), 11); // file1, file10..file19
    assert_eq!(l.filter(""), 51);
    // sort by size descending waits for enrichment then resets
    let (tx2, rx2) = mpsc::channel();
    l.sort(SortRole::Size, false, Some((tx2, 99)));
    let reply = rx2.recv_timeout(Duration::from_secs(5)).unwrap();
    assert_eq!(reply.u64_field("id"), Some(99));
    let w = l.window(1, 7, 0, 3);
    let rows = w.get("rows").unwrap().as_arr().unwrap();
    assert_eq!(rows[0].str_field("name"), Some("sub"));
    assert_eq!(rows[1].get("meta").unwrap().u64_field("size"), Some(6));
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn rescan_keeps_meta_for_unchanged() {
    let dir = temp_tree(5);
    let uri = Uri::from_path(&dir);
    let (l, _) = open(&uri).unwrap();
    assert!(wait_scan(&l, Duration::from_secs(5)));
    let (tx, rx) = mpsc::channel();
    l.subscribe(Subscriber { client: 2, lid: 1, tx, first: 0, count: 10 });
    l.enrich(None);
    let deadline = Instant::now() + Duration::from_secs(5);
    while l.inner.lock().unwrap().meta.iter().any(Option::is_none) {
        assert!(Instant::now() < deadline);
        thread::sleep(Duration::from_millis(5));
    }
    std::fs::write(dir.join("new.txt"), b"1").unwrap();
    l.rescan();
    let w = l.window(2, 1, 0, 20);
    let rows = w.get("rows").unwrap().as_arr().unwrap();
    assert_eq!(rows.len(), 7);
    let kept = rows.iter().find(|r| r.str_field("name") == Some("file1.txt")).unwrap();
    assert!(kept.get("meta").unwrap() != &Value::Null);
    let fresh = rows.iter().find(|r| r.str_field("name") == Some("new.txt")).unwrap();
    assert_eq!(fresh.get("meta").unwrap(), &Value::Null);
    let events = drain(&rx);
    assert!(events.iter().any(|e| e.str_field("event") == Some("Reset")));
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn dot_files_hidden_until_asked() {
    let _g = crate::ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let dir = std::env::temp_dir().join(format!("kiki-hidden-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("a.txt"), b"a").unwrap();
    std::fs::write(dir.join(".secret"), b"s").unwrap();
    std::fs::create_dir(dir.join(".git")).unwrap();
    let (l, _) = open(&Uri::from_path(&dir)).unwrap();
    assert!(wait_scan(&l, Duration::from_secs(5)));
    assert_eq!(l.count().0, 1, "dot-files hidden by default");
    assert_eq!(l.set_hidden(true), 3);
    let w = l.window(1, 1, 0, 10);
    let names: Vec<&str> = w.get("rows").unwrap().as_arr().unwrap().iter().map(|r| r.str_field("name").unwrap()).collect();
    assert_eq!(names, vec![".git", ".secret", "a.txt"]); // folders first, then dot-files sort with the rest
    assert_eq!(l.set_hidden(false), 1);
    // type-ahead: prefix search over the view with wrap-around
    std::fs::write(dir.join("Banana.txt"), b"b").unwrap();
    std::fs::write(dir.join("apple.txt"), b"a").unwrap();
    l.rescan();
    assert_eq!(l.seek("ba", None), Some(2)); // a.txt, apple.txt, Banana.txt sorted naturally
    assert_eq!(l.seek("a", None), Some(0));
    assert_eq!(l.seek("a", Some(0)), Some(1));
    assert_eq!(l.seek("a", Some(1)), Some(0), "wraps");
    assert_eq!(l.seek("zz", None), None);
    std::fs::remove_dir_all(&dir).unwrap();
}

// ------------------------------------------------------------ the view

/// Sorting, filtering, hidden files and type-ahead all act on the same view, and each one
/// renumbers it. These are the operations every keystroke in the window goes through.
#[test]
fn the_view_sorts_filters_and_seeks() {
    let dir = std::env::temp_dir().join(format!("kiki-view-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("Zed")).unwrap();
    std::fs::write(dir.join("apple.txt"), vec![b'x'; 300]).unwrap();
    std::fs::write(dir.join("Banana.md"), vec![b'x'; 10]).unwrap();
    std::fs::write(dir.join("cherry.txt"), vec![b'x'; 100]).unwrap();
    std::fs::write(dir.join(".hidden.txt"), b"h").unwrap();
    let uri = Uri::from_path(&dir);
    let (l, _) = open(&uri).unwrap();
    assert!(wait_scan(&l, Duration::from_secs(5)));

    let names = |l: &std::sync::Arc<Listing>| -> Vec<String> {
        l.window(1, 1, 0, 50).get("rows").unwrap().as_arr().unwrap().iter().map(|r| r.str_field("name").unwrap_or("").to_string()).collect()
    };

    // Folders first, then case-insensitive natural order; dot-files are out of the way.
    assert_eq!(names(&l), vec!["Zed", "apple.txt", "Banana.md", "cherry.txt"]);

    // Hidden files come and go, and the count follows.
    assert_eq!(l.set_hidden(true), 5);
    assert_eq!(names(&l)[1], ".hidden.txt");
    assert_eq!(l.set_hidden(true), 5, "asking twice changes nothing");
    assert_eq!(l.set_hidden(false), 4);

    // Type-ahead searches the whole view, case-insensitively, and wraps.
    let pos = l.seek("ban", None).expect("Banana.md");
    assert_eq!(names(&l)[pos as usize], "Banana.md");
    assert_eq!(l.seek("z", None), Some(0));
    assert_eq!(l.seek("z", Some(0)), Some(0), "one match: searching on wraps back to it");
    assert_eq!(l.seek("nothing", None), None);
    assert_eq!(l.seek("", None), None);

    // Filtering narrows the view; sorting then applies to what is left.
    assert_eq!(l.filter("txt"), 2);
    assert_eq!(names(&l), vec!["apple.txt", "cherry.txt"]);
    let (tx, rx) = mpsc::channel();
    l.sort(SortRole::Size, false, Some((tx, 5)));
    let _ = rx.recv_timeout(Duration::from_secs(5)).unwrap();
    assert_eq!(names(&l), vec!["apple.txt", "cherry.txt"], "300 bytes before 100");
    let (tx, rx) = mpsc::channel();
    l.sort(SortRole::Size, true, Some((tx, 6)));
    let _ = rx.recv_timeout(Duration::from_secs(5)).unwrap();
    assert_eq!(names(&l), vec!["cherry.txt", "apple.txt"]);

    // Clearing the filter brings everything back, under the sort that is now in force.
    assert_eq!(l.filter(""), 4);
    assert_eq!(names(&l)[0], "Zed", "folders stay first whatever the sort");

    // Sorting by kind groups the two text files together.
    let (tx, rx) = mpsc::channel();
    l.sort(SortRole::Kind, true, Some((tx, 7)));
    let _ = rx.recv_timeout(Duration::from_secs(5)).unwrap();
    let k = names(&l);
    assert_eq!(k[0], "Zed");
    assert!(k.contains(&"apple.txt".to_string()) && k.len() == 4);

    let _ = std::fs::remove_dir_all(&dir);
}

/// A window is answered from whatever the listing has; asking past the end is not an error,
/// and a count above the cap is trimmed rather than refused.
#[test]
fn windows_are_clamped_not_refused() {
    let dir = temp_tree(12);
    let uri = Uri::from_path(&dir);
    let (l, _) = open(&uri).unwrap();
    assert!(wait_scan(&l, Duration::from_secs(5)));

    let w = l.window(1, 1, 0, WINDOW_MAX + 100);
    assert_eq!(w.get("rows").unwrap().as_arr().unwrap().len(), 13, "the folder is shorter than the cap");
    let w = l.window(1, 1, 11, 10);
    assert_eq!(w.u64_field("first"), Some(11));
    assert_eq!(w.get("rows").unwrap().as_arr().unwrap().len(), 2);
    let w = l.window(1, 1, 500, 10);
    assert!(w.get("rows").unwrap().as_arr().unwrap().is_empty(), "past the end is empty, not an error");
    assert_eq!(w.u64_field("n"), Some(13));
    assert_eq!(l.count().0, 13);
    assert!(l.error().is_none());

    // A subscriber that leaves stops being sent to.
    let (tx, rx) = mpsc::channel();
    l.subscribe(Subscriber { client: 9, lid: 3, tx, first: 0, count: 5 });
    l.unsubscribe(9, 3);
    l.filter("file1");
    assert!(drain(&rx).is_empty(), "no events after unsubscribing");

    let _ = std::fs::remove_dir_all(&dir);
}

/// Opening something that is not a directory reports it rather than hanging.
#[test]
fn opening_what_is_not_a_folder_is_an_error() {
    let dir = temp_tree(1);
    let f = dir.join("file0.txt");
    let e = open(&Uri::from_path(&f));
    assert!(e.is_err(), "a file is not a listing");
    assert!(open(&Uri::from_path(&dir.join("no-such-folder"))).is_err());
    let _ = std::fs::remove_dir_all(&dir);
}

/// `mark_stale` and `gone` are what the watcher calls when a folder changes underneath a
/// listing; both have to survive being called for a path nothing has open.
#[test]
fn cache_invalidation_is_safe_for_paths_nobody_has_open() {
    let dir = temp_tree(3);
    let uri = Uri::from_path(&dir);
    let (l, _) = open(&uri).unwrap();
    assert!(wait_scan(&l, Duration::from_secs(5)));
    assert!(find(&dir).is_some());

    mark_stale(&dir);
    assert!(l.inner.lock().unwrap().stale, "the listing knows it is out of date");
    gone(&dir);
    assert!(find(&dir).is_none(), "and then it is dropped from the cache");

    let unknown = dir.join("never-opened");
    mark_stale(&unknown);
    gone(&unknown);
    forget(&Uri::from_path(&unknown));
    assert!(find(&unknown).is_none());
    let _ = std::fs::remove_dir_all(&dir);
}
