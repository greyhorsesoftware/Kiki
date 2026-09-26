//! A double-click on a file (0.2.0): the default application for its type, for a file on this
//! machine; a file on a server is not opened in an application — Quick Look shows it (owner,
//! 2026-09-25) — and asked anyway the daemon says so by number, starting nothing.

mod common;

use kikid::vfs::uri::Uri;
use kikid::vfs::VfsError;
use std::path::Path;
use std::time::{Duration, Instant};

fn wait_for_line(log: &Path, timeout: Duration) -> Option<String> {
    let start = Instant::now();
    while start.elapsed() < timeout {
        if let Ok(s) = std::fs::read_to_string(log) {
            if let Some(l) = s.lines().last() {
                return Some(l.to_string());
            }
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    None
}

#[test]
fn a_local_file_opens_as_it_is_and_a_remote_one_is_refused_by_number() {
    let dir = common::setup("open-default");
    common::save_location("lab");
    // What "the default application" runs in this test: a script that writes down the path.
    let log = dir.join("opened.log");
    let opener = dir.join("opener");
    std::fs::write(&opener, format!("#!/bin/sh\nprintf '%s\\n' \"$1\" >> '{}'\n", log.display())).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&opener, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
    std::env::set_var("KIKI_OPEN_WITH", &opener);
    std::env::set_var("XDG_CACHE_HOME", dir.join("cache"));

    // Local: opened where it is.
    let local = dir.join("note.txt");
    std::fs::write(&local, "here").unwrap();
    kikid::desktop::open_default(&Uri::from_path(&local)).unwrap();
    assert_eq!(wait_for_line(&log, Duration::from_secs(5)).as_deref(), Some(local.to_string_lossy().as_ref()));

    // Remote: 1330 with the name, no job, nothing fetched, nothing opened.
    let jobs_before = kikid::jobs::list().as_arr().map(|a| a.len()).unwrap_or(0);
    let err = kikid::desktop::open_default(&Uri::parse("stub://lab/docs/notes.txt").unwrap()).unwrap_err();
    match err {
        VfsError::Said { n, .. } => assert_eq!(n, 1330),
        other => panic!("{other:?}"),
    }
    assert_eq!(err.said_json().and_then(|(_, p)| p.str_field("name").map(str::to_string)).as_deref(), Some("notes.txt"));
    std::thread::sleep(Duration::from_millis(300));
    assert_eq!(kikid::jobs::list().as_arr().map(|a| a.len()).unwrap_or(0), jobs_before, "no fetch job");
    assert!(!dir.join("cache/kiki/open").exists() || std::fs::read_dir(dir.join("cache/kiki/open")).map(|d| d.count()).unwrap_or(0) == 0, "nothing fetched");
    assert_eq!(std::fs::read_to_string(&log).unwrap().lines().count(), 1, "nothing else opened");

    std::env::remove_var("KIKI_OPEN_WITH");
    std::env::remove_var("XDG_CACHE_HOME");
    let _ = std::fs::remove_dir_all(&dir);
}
