//! A double-click on a file (0.2.0): the default application for its type — a local file as it
//! is, a remote file brought to the cache first by a copy job and opened from there — and a
//! save of the fetched copy sent back to where it came from.

mod common;

use kikid::json::Value;
use kikid::plugin::Msg;
use kikid::vfs::uri::Uri;
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
fn a_local_file_opens_as_it_is_and_a_remote_one_is_fetched_first() {
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

    // Local: opened where it is, no job.
    let local = dir.join("note.txt");
    std::fs::write(&local, "here").unwrap();
    let job = kikid::desktop::open_default(&Uri::from_path(&local)).unwrap();
    assert_eq!(job, None, "a local file needs no job");
    assert_eq!(wait_for_line(&log, Duration::from_secs(5)).as_deref(), Some(local.to_string_lossy().as_ref()));

    // Remote: a copy job to a folder under the cache, then opened from there, bytes intact.
    let job = kikid::desktop::open_default(&Uri::parse("stub://lab/docs/notes.txt").unwrap()).unwrap();
    assert!(job.is_some(), "a remote file is fetched by a job");
    let start = Instant::now();
    let opened = loop {
        if let Some(l) = wait_for_line(&log, Duration::from_secs(1)) {
            if l != local.to_string_lossy() {
                break l;
            }
        }
        assert!(start.elapsed() < Duration::from_secs(20), "the fetched file was never opened");
    };
    let opened = Path::new(&opened);
    assert!(opened.starts_with(dir.join("cache/kiki/open")), "opened from the cache: {}", opened.display());
    assert_eq!(opened.file_name().unwrap(), "notes.txt");
    assert_eq!(std::fs::read(opened).unwrap(), b"hello", "the stub's bytes, whole");

    // Saved: the copy goes back by a job of its own, named for what it is, replacing the
    // original without a question (there is nobody to ask). An editor's save is a write and a
    // close, or a rename over the file: both are seen. The job is waited for — the save is
    // asynchronous — and then the server's file is read once.
    std::fs::write(opened, b"changed!").unwrap();
    let start = Instant::now();
    let done = loop {
        let jobs = kikid::jobs::list();
        let save = jobs.as_arr().into_iter().flatten().find(|j| j.str_field("title") == Some("Save notes.txt back to lab")).cloned();
        if let Some(j) = save {
            if j.str_field("state") != Some("queued") && j.str_field("state") != Some("running") {
                break j;
            }
        }
        assert!(start.elapsed() < Duration::from_secs(20), "no save job finished: {}", kikid::json::to_string(&jobs));
        std::thread::sleep(Duration::from_millis(50));
    };
    assert_eq!(done.str_field("state"), Some("done"), "{}", kikid::json::to_string(&done));
    let session = kikid::locations::resolve(&Uri::parse("stub://lab/").unwrap()).unwrap().0;
    let mut got = Vec::new();
    let _ = session.plugin.read_stream(Value::obj().s("type", "Read").s("location", "lab").s("path", "/docs/notes.txt").done(), |m| {
        if let Msg::Binary(b) = m {
            got.extend_from_slice(&b)
        }
    });
    assert_eq!(got, b"changed!", "the server has the saved bytes");

    // A selection with both kinds in it: the local ones as they are, the remote ones fetched,
    // the application given the lot in the order asked.
    let (tx, rx) = std::sync::mpsc::channel();
    let job = kikid::openback::localise(&[Uri::from_path(&local), Uri::parse("stub://lab/docs/readme.md").unwrap()], move |all| {
        let _ = tx.send(all);
    })
    .unwrap();
    assert!(job.is_some());
    let all = rx.recv_timeout(Duration::from_secs(20)).expect("the application was given its files");
    assert_eq!(all.len(), 2);
    assert_eq!(all[0].to_path(), local, "the local file, as it is");
    assert!(all[1].to_path().starts_with(dir.join("cache/kiki/open")) && all[1].name() == "readme.md", "the remote one, fetched: {}", all[1]);
    assert_eq!(std::fs::read(all[1].to_path()).unwrap(), b"# stub\n");

    std::env::remove_var("KIKI_OPEN_WITH");
    std::env::remove_var("XDG_CACHE_HOME");
    let _ = std::fs::remove_dir_all(&dir);
}
