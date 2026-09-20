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

/// A row that leaves every window before its stat job runs is skipped by that job — and used to
/// stay marked as queued for ever. `enrich_all` passes over queued rows, so the listing could
/// never finish enriching, and a sort by size or date (which waits for that) never replied:
/// scroll fast through a big folder, sort it, and the sort hung.
#[test]
fn a_row_skipped_by_its_stat_job_can_still_be_enriched() {
    let dir = temp_tree(SMALL_DIR + 100); // too big to be enriched whole after the scan
    let (l, _) = open(&Uri::from_path(&dir)).unwrap();
    assert!(wait_scan(&l, Duration::from_secs(10)));
    // What a window request does for the rows it shows…
    let rows: Vec<u32> = {
        let mut inner = l.inner.lock().unwrap();
        let rows: Vec<u32> = (0..inner.meta.len() as u32).filter(|&i| inner.meta[i as usize].is_none()).take(40).collect();
        assert_eq!(rows.len(), 40, "a folder this size is not enriched up front");
        for &i in &rows {
            inner.queued[i as usize] = true;
        }
        rows
    };
    // …and what a stat worker does once nobody is looking at them any more (no subscriber).
    l.run_stats(rows.clone(), 0, false);
    {
        let inner = l.inner.lock().unwrap();
        assert!(rows.iter().all(|&i| !inner.queued[i as usize]), "skipped means no longer queued");
    }
    let (tx, rx) = mpsc::channel();
    l.sort(SortRole::Size, false, Some((tx, 9)));
    assert!(rx.recv_timeout(Duration::from_secs(20)).is_ok(), "the sort is answered once everything is enriched");
    assert!(l.inner.lock().unwrap().meta.iter().all(Option::is_some));
    std::fs::remove_dir_all(&dir).unwrap();
}

/// A rescan deals the pool's indexes again. What is known about a name has to cross it by name:
/// the thumbnail files were always still on disk, and every picture in view lost its thumbnail
/// anyway, to be asked for again one by one.
#[test]
fn a_rescan_keeps_the_thumbnails_it_had() {
    let dir = temp_tree(40);
    let (l, _) = open(&Uri::from_path(&dir)).unwrap();
    assert!(wait_scan(&l, Duration::from_secs(5)));
    {
        let mut inner = l.inner.lock().unwrap();
        let a = inner.pool.find(b"file3.txt").unwrap();
        let b = inner.pool.find(b"file4.txt").unwrap();
        inner.thumb.insert(a, "/cache/three.png".into());
        inner.thumb.insert(b, String::new()); // one that failed
    }
    // New names that sort before the old ones, and one gone: every index moves.
    std::fs::write(dir.join("aaa.txt"), b"1").unwrap();
    std::fs::remove_file(dir.join("file0.txt")).unwrap();
    l.rescan();
    let w = l.window(1, 1, 0, 100);
    let rows = w.get("rows").unwrap().as_arr().unwrap();
    let thumb = |name: &str| rows.iter().find(|r| r.str_field("name") == Some(name)).unwrap().get("thumb").unwrap().clone();
    assert_eq!(thumb("file3.txt"), Value::Str("/cache/three.png".into()), "carried by name");
    assert_eq!(thumb("file4.txt"), Value::Null, "a failure is asked about again");
    assert_eq!(thumb("file5.txt"), Value::Null, "and nobody else was given one");
    assert_eq!(thumb("aaa.txt"), Value::Null);
    std::fs::remove_dir_all(&dir).unwrap();
}

/// A thumbnail asked for before a rescan and finished after it belongs to the name it was asked
/// for, wherever that name now is — not to whatever file has taken its old index.
#[test]
fn a_thumbnail_that_lands_after_a_rescan_lands_on_its_own_file() {
    let dir = temp_tree(41);
    let (l, _) = open(&Uri::from_path(&dir)).unwrap();
    assert!(wait_scan(&l, Duration::from_secs(5)));
    let old_idx = l.inner.lock().unwrap().pool.find(b"file7.txt").unwrap();
    for i in 0..5 {
        std::fs::remove_file(dir.join(format!("file{i}.txt"))).unwrap();
    }
    l.rescan();
    // Not a picture, so the job fails and records "" — which is all this needs: where it lands.
    l.inner.lock().unwrap().thumb_queued[old_idx as usize] = true;
    l.submit_thumb(old_idx, crate::kinds::Kind::Image, 1, "file7.txt");
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let inner = l.inner.lock().unwrap();
        if !inner.thumb.is_empty() {
            let at = inner.pool.find(b"file7.txt").unwrap();
            assert_eq!(inner.thumb.keys().copied().collect::<Vec<_>>(), vec![at]);
            break;
        }
        drop(inner);
        assert!(Instant::now() < deadline, "the thumbnail job never answered");
        thread::sleep(Duration::from_millis(5));
    }
    std::fs::remove_dir_all(&dir).unwrap();
}

/// One file arriving or leaving is told as what happened (`Splice`), not as "forget everything
/// and ask again" (`Reset`); and every answer says which state of the view it describes.
#[test]
fn a_small_change_is_spliced_not_reset() {
    let dir = temp_tree(6);
    let (l, _) = open(&Uri::from_path(&dir)).unwrap();
    assert!(wait_scan(&l, Duration::from_secs(5)));
    let (tx, rx) = mpsc::channel();
    l.subscribe(Subscriber { client: 3, lid: 9, tx, first: 0, count: 50 });
    let before = l.window(3, 9, 0, 50).u64_field("gen").unwrap();
    let _ = drain(&rx);

    std::fs::write(dir.join("file1b.txt"), b"1").unwrap();
    std::fs::remove_file(dir.join("file0.txt")).unwrap();
    l.patch(&[b"file1b.txt".to_vec()], &[b"file0.txt".to_vec()], &[]);
    let events = drain(&rx);
    assert!(!events.iter().any(|e| e.str_field("event") == Some("Reset")), "{events:?}");
    let s = events.iter().find(|e| e.str_field("event") == Some("Splice")).expect("a Splice");
    assert_eq!(s.u64_field("lid"), Some(9));
    assert_eq!(s.u64_field("n"), Some(7)); // sub + six files, one gone, one new
    assert_eq!(s.u64_field("gen"), Some(before + 1));
    let ops = s.get("ops").unwrap().as_arr().unwrap();
    // [sub, file0, file1, …]: file0 leaves position 1; file1b then lands after file1, at 2.
    assert_eq!((ops[0].str_field("op"), ops[0].u64_field("pos")), (Some("remove"), Some(1)));
    assert_eq!((ops[1].str_field("op"), ops[1].u64_field("pos")), (Some("insert"), Some(2)));
    assert_eq!(ops[1].get("row").unwrap().str_field("name"), Some("file1b.txt"));
    let w = l.window(3, 9, 0, 50);
    assert_eq!(w.u64_field("gen"), Some(before + 1));
    let names: Vec<&str> = w.get("rows").unwrap().as_arr().unwrap().iter().map(|r| r.str_field("name").unwrap()).collect();
    assert_eq!(&names[..4], ["sub", "file1.txt", "file1b.txt", "file2.txt"]);

    // A dot-file arriving where they are hidden changes nothing anyone can see.
    std::fs::write(dir.join(".hidden"), b"1").unwrap();
    l.patch(&[b".hidden".to_vec()], &[], &[]);
    let events = drain(&rx);
    assert!(!events.iter().any(|e| matches!(e.str_field("event"), Some("Reset" | "Splice"))), "{events:?}");
    assert_eq!(l.window(3, 9, 0, 50).u64_field("n"), Some(7));

    // A filtered view cannot be spliced (the daemon rebuilds it): that is still a Reset.
    l.filter("file");
    let _ = drain(&rx);
    std::fs::write(dir.join("file9.txt"), b"1").unwrap();
    l.patch(&[b"file9.txt".to_vec()], &[], &[]);
    let events = drain(&rx);
    assert!(events.iter().any(|e| e.str_field("event") == Some("Reset") && e.u64_field("gen").is_some()), "{events:?}");
    std::fs::remove_dir_all(&dir).unwrap();
}
