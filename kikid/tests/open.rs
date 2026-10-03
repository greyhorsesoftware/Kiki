//! `Open`, the one verb for opening things elsewhere (docs/0.3.0/01-daemon-on-demand.md,
//! decision 5), and its rule table in `handlers::open::open_request`: every `with` against a
//! file on this machine, a folder, and a file on a server. The rule about a server's file — 1330,
//! Quick Look is the way — is the one that used to live in each of six verbs, and the reason
//! there is now one table.

mod common;

use kikid::handlers::open::open_request;
use kikid::scratch::Scratch;
use std::path::PathBuf;

/// A folder with a file in it, and a script standing in for every application: it writes what it
/// was asked to open, which is how `default` is seen to have opened the right thing.
fn fixture(tag: &str) -> (Scratch, PathBuf) {
    let d = common::setup(tag);
    std::fs::write(d.join("note.txt"), "hello").unwrap();
    let log = d.join("opened.log");
    let app = d.join("app");
    std::fs::write(&app, format!("#!/bin/sh\nprintf '%s\\n' \"$*\" >> '{}'\n", log.display())).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&app, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
    std::env::set_var("KIKI_OPEN_WITH", &app);
    (d, log)
}

fn file_uri(p: &std::path::Path) -> String {
    kikid::vfs::uri::Uri::from_path(p).to_string()
}

fn wait_for_log(log: &std::path::Path) -> String {
    let end = std::time::Instant::now() + std::time::Duration::from_secs(5);
    while std::time::Instant::now() < end {
        if let Ok(s) = std::fs::read_to_string(log) {
            if !s.trim().is_empty() {
                return s;
            }
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    String::new()
}

#[test]
fn default_opens_a_local_file_with_its_application_and_refuses_a_servers_file() {
    let (d, log) = fixture("open-default");
    let note = d.join("note.txt");
    let r = open_request(&[file_uri(&note)], "default", None, None).expect("a local file opens");
    assert_eq!(kikid::json::to_string(&r), "{}", "the reply `default` has always given");
    assert!(wait_for_log(&log).contains(note.to_str().unwrap()), "the application was handed the file");

    // A server's file: 1330, Quick Look is the way — said before anything is tried.
    // A numbered error goes out as `Io` with its number beside it (`vfs_err`); the words are
    // the proof that it is 1330 and not some other refusal.
    let (code, msg) = open_request(&["stub://lab/docs/notes.txt".into()], "default", None, None).unwrap_err();
    assert_eq!(code, "Io", "{msg}");
    assert!(msg.contains("Quick Look"), "{msg}");
    // And the ask is in the access log either way, as it always was.
    assert!(kikid::access::opened("stub://lab/docs/notes.txt").is_some(), "recorded even though refused");
    assert!(kikid::access::opened(&file_uri(&note)).is_some());

    // Nothing at all is a protocol error, not a launch of nothing.
    assert_eq!(open_request(&[], "default", None, None).unwrap_err().0, "Protocol");
}

#[test]
fn app_launches_the_chosen_entry_and_refuses_a_servers_file_first() {
    let (d, _log) = fixture("open-app");
    let note = d.join("note.txt");
    // A desktop entry that does not exist is logged, not an error: the menu offered what the
    // desktop said opens this, and the desktop is where a broken entry is fixed.
    let r = open_request(&[file_uri(&note)], "app:no.such.desktop", None, None).expect("a failed launch is not an error");
    assert_eq!(kikid::json::to_string(&r), "{}");
    let (code, msg) = open_request(&[file_uri(&note), "stub://lab/x.png".into()], "app:imv.desktop", None, None).unwrap_err();
    assert_eq!(code, "Io");
    assert!(msg.contains("Quick Look"), "one server's file among local ones refuses the lot: {msg}");
}

#[test]
fn tool_answers_as_open_in_did_and_refuses_a_servers_file_before_looking_the_tool_up() {
    let (d, _log) = fixture("open-tool");
    let note = d.join("note.txt");
    // A tool nobody configured: Invalid, in the tool's own words.
    let (code, msg) = open_request(&[file_uri(&note)], "tool:nothing", Some(3), None).unwrap_err();
    assert_eq!(code, "Invalid");
    assert!(msg.contains("no tool"), "{msg}");
    // A server's file: refused as 1330 before the tool is even looked up — so a missing tool is
    // not what the person is told about a file Quick Look would show.
    let (code, msg) = open_request(&["stub://lab/a.rs".into()], "tool:nothing", None, None).unwrap_err();
    assert_eq!(code, "Io", "a number, not the tool's `Invalid`: {msg}");
    assert!(msg.contains("Quick Look"), "{msg}");
}

#[test]
fn terminal_and_ai_open_in_a_folder_and_keep_their_own_words_for_a_server() {
    let (d, log) = fixture("open-terminal");
    // The terminal is a script here: what it was started with says where it was started. It
    // has to be an `xdg-terminal-exec`, first on PATH, and not only `$TERMINAL`: open_terminal
    // prefers xdg-terminal-exec whenever one is on PATH, and on Omarchy one always is — so a
    // `$TERMINAL` stub was skipped and this test opened a real terminal on the developer's
    // desktop with every `cargo test`, and left it there (owner, 2026-09-26).
    let xdg = d.join("xdg-terminal-exec");
    std::fs::copy(d.join("app"), &xdg).unwrap();
    std::env::set_var("TERMINAL", d.join("app"));
    std::env::set_var("PATH", format!("{}:{}", d.display(), std::env::var("PATH").unwrap_or_default()));
    let r = open_request(&[], "terminal", None, Some(&file_uri(&d))).expect("a terminal opens in a local folder");
    assert_eq!(kikid::json::to_string(&r), "{}");
    // And it was the stub that ran, in the folder asked for — not something real.
    let started = wait_for_log(&log);
    assert!(started.contains(&format!("--dir={}", d.display())), "the stub terminal was started in the folder: {started:?}");
    // A folder on a server: a terminal's own number (1251), which says a folder, not Quick Look.
    let (_, msg) = open_request(&[], "terminal", None, Some("stub://lab/")).unwrap_err();
    assert!(msg.contains("folder on this machine"), "{msg}");
    // Without `dir`, the first uri is the place; without either, nothing to open in.
    assert_eq!(open_request(&[], "terminal", None, None).unwrap_err().0, "Protocol");

    // The AI on a server's folder or files: its own number (1252), its own words.
    let (_, msg) = open_request(&["stub://lab/a.rs".into()], "ai", None, Some(&file_uri(&d))).unwrap_err();
    assert!(msg.contains("files on this machine"), "{msg}");
    // And a `with` nobody knows is a protocol error, not a silent nothing.
    assert_eq!(open_request(&[file_uri(&d)], "elsewhere", None, None).unwrap_err().0, "Protocol");
    std::env::remove_var("TERMINAL");
}
