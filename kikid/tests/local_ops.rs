//! The small local operations, and what the daemon says about a file's permissions.
//!
//! `mkdir`, `rename`, `delete` and `set_mtime` are one or two lines each in `jobs.rs`, which is
//! exactly why nothing tested them: what they refuse matters as much as what they do, and a
//! rename that takes a name with a `/` in it writes outside the folder the user was looking at.
//!
//! The permission bits are checked against **`stat -c`**, not against what the daemon computed
//! them from: `stat` is a second opinion from outside kiki, and setuid, setgid and the sticky bit
//! are the three that a `& 0o777` somewhere would quietly drop.
//!
//! Also the URI resolver, which every one of these goes through: `~`, and a scheme with nothing
//! behind it, which must come back as an error with a code on it rather than as a path on this
//! machine.

use kikid::json::Value;
use kikid::listing::Listing;
use kikid::vfs::uri::Uri;
use kikid::vfs::VfsError;
use kikid::{jobs, ops};
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::time::{Duration, Instant, SystemTime};

const PATIENCE: Duration = Duration::from_secs(20);

fn job(id: u64) -> Value {
    jobs::list().as_arr().unwrap().iter().find(|j| j.u64_field("id") == Some(id)).cloned().expect("the job is in the list")
}

/// Runs an op and hands back how it ended and what it said.
fn run(op: Value) -> (String, String) {
    let (tx, _rx) = mpsc::channel();
    let id = jobs::submit(op, Some(tx)).expect("submitted");
    let start = Instant::now();
    loop {
        let j = job(id);
        match j.str_field("state") {
            Some("running") | Some("queued") => {
                assert!(start.elapsed() < PATIENCE, "the job never finished");
                std::thread::sleep(Duration::from_millis(2));
            }
            other => return (other.unwrap_or("").to_string(), j.str_field("error").unwrap_or("").to_string()),
        }
    }
}

fn did(op: Value) {
    let (state, why) = run(op);
    assert_eq!(state, "done", "{why}");
}

fn refused(op: Value, because: &str) {
    let (state, why) = run(op);
    assert_eq!(state, "failed", "it should have been refused: {because}");
    assert!(why.contains(because), "the reason should say {because:?}: {why}");
}

fn uri(p: &Path) -> String {
    Uri::from_path(p).to_string()
}

fn one(p: &Path) -> Value {
    Value::Arr(vec![Value::Str(uri(p))])
}

/// `stat -c <format>` over a path, as a second opinion from outside kiki.
fn stat(fmt: &str, p: &Path) -> String {
    let out = std::process::Command::new("stat").arg("-c").arg(fmt).arg(p).output().expect("stat is in coreutils");
    assert!(out.status.success(), "stat {fmt} {}: {}", p.display(), String::from_utf8_lossy(&out.stderr));
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

/// What the daemon says about a file, the way a client asks (`Stat`).
fn meta(p: &Path) -> Value {
    Listing::stat_uri(&Uri::from_path(p)).expect("stat")
}

#[test]
fn make_rename_delete_and_stamp_a_local_file() {
    let _guard = kikid::ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let dir = std::env::temp_dir().join(format!("kiki-localops-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::env::set_var("KIKI_CONFIG_DIR", dir.join("config"));
    std::env::set_var("KIKI_STATE_DIR", dir.join("state"));
    std::env::set_var("KIKI_DATA_DIR", dir.join("data"));
    std::env::set_var("KIKI_TRASH_DIR", dir.join("trash"));
    let work = dir.join("work");
    std::fs::create_dir_all(&work).unwrap();

    // ---------------------------------------------------------------- mkdir
    did(Value::obj().s("op", "mkdir").s("uri", uri(&work.join("new"))).done());
    assert!(work.join("new").is_dir());
    // A name that is already taken, and a parent that is not there: neither is a folder made.
    refused(Value::obj().s("op", "mkdir").s("uri", uri(&work.join("new"))).done(), "Exists");
    refused(Value::obj().s("op", "mkdir").s("uri", uri(&work.join("nowhere/deep"))).done(), "NotFound");
    assert!(!work.join("nowhere").exists(), "and nothing was made on the way");
    // A name with a space and one outside ASCII: the URI carries them and the folder gets them.
    for name in ["two words", "Été", "a#b&c", "n%20ot-decoded-twice"] {
        did(Value::obj().s("op", "mkdir").s("uri", uri(&work.join(name))).done());
        assert!(work.join(name).is_dir(), "{name}");
    }

    // ---------------------------------------------------------------- rename
    std::fs::write(work.join("a.txt"), b"a").unwrap();
    did(Value::obj().s("op", "rename").s("uri", uri(&work.join("a.txt"))).s("name", "b.txt").done());
    assert!(work.join("b.txt").exists() && !work.join("a.txt").exists());
    // Onto something that is there, and two names that would take the file out of its folder.
    std::fs::write(work.join("c.txt"), b"c").unwrap();
    refused(Value::obj().s("op", "rename").s("uri", uri(&work.join("b.txt"))).s("name", "c.txt").done(), "Exists");
    assert_eq!(std::fs::read(work.join("c.txt")).unwrap(), b"c", "and what was there is untouched");
    for bad in ["", "../escaped.txt", "sub/deeper.txt", "/etc/passwd"] {
        refused(Value::obj().s("op", "rename").s("uri", uri(&work.join("b.txt"))).s("name", bad).done(), "bad name");
    }
    assert!(work.join("b.txt").exists() && !dir.join("escaped.txt").exists());
    // A folder renames like a file, and takes what is in it.
    std::fs::write(work.join("new/inside.txt"), b"in").unwrap();
    did(Value::obj().s("op", "rename").s("uri", uri(&work.join("new"))).s("name", "renamed").done());
    assert_eq!(std::fs::read(work.join("renamed/inside.txt")).unwrap(), b"in");

    // ---------------------------------------------------------------- delete
    // `delete` is Shift+Del: it does not journal and it does not go through the trash.
    let journal = || std::fs::read_to_string(dir.join("state/journal.json")).unwrap_or_default();
    let before_delete = journal();
    assert!(!before_delete.is_empty(), "the mkdir and the rename above did journal");
    std::os::unix::fs::symlink(work.join("c.txt"), work.join("link")).unwrap();
    did(Value::obj().s("op", "delete").v("items", one(&work.join("link"))).done());
    assert!(!work.join("link").exists() && work.join("c.txt").exists(), "a symlink is deleted, never what it points at");
    did(Value::obj().s("op", "delete").v("items", Value::Arr(vec![Value::Str(uri(&work.join("renamed"))), Value::Str(uri(&work.join("c.txt")))])).done());
    assert!(!work.join("renamed").exists(), "a folder goes with everything under it");
    assert!(!work.join("c.txt").exists());
    assert_eq!(journal(), before_delete, "and nothing was written down to take it back with");
    assert!(ops::trash_infos().is_empty(), "nor did any of it go to the trash");
    refused(Value::obj().s("op", "delete").v("items", one(&work.join("never-was"))).done(), "NotFound");

    // ---------------------------------------------------------------- set_mtime
    // What a copy uses to give the new file the old one's time, and what a mirror stamps with.
    let f = work.join("stamped.txt");
    std::fs::write(&f, b"x").unwrap();
    let when = SystemTime::UNIX_EPOCH + Duration::from_millis(1_234_567_891_000);
    ops::set_mtime(&f, when).unwrap();
    assert_eq!(std::fs::metadata(&f).unwrap().modified().unwrap(), when);
    assert_eq!(stat("%Y", &f), "1234567891", "and the filesystem agrees");
    assert_eq!(meta(&f).u64_field("mtime"), Some(1_234_567_891_000), "as does what the daemon tells a client");
    // A copy carries the time across, which is the whole reason it exists.
    let copied = work.join("copied.txt");
    did(Value::obj().s("op", "copy").v("items", one(&f)).s("dest", uri(&work.join("dest").tap_create())).done());
    std::fs::rename(work.join("dest/stamped.txt"), &copied).unwrap();
    assert_eq!(std::fs::metadata(&copied).unwrap().modified().unwrap(), when);
    // Something that is not there, and a folder (which cannot be opened for writing): errors, not panics.
    assert!(ops::set_mtime(&work.join("never-was"), when).is_err());
    assert!(ops::set_mtime(&work, when).is_err());

    // ---------------------------------------------------------------- permissions, against stat
    // Every bit that is not in 0o777 is one that a mask somewhere could drop: setuid, setgid,
    // sticky. They are set here and read back both ways.
    let bits = work.join("bits.txt");
    std::fs::write(&bits, b"bits").unwrap();
    for mode in [0o644u32, 0o600, 0o755, 0o700, 0o444, 0o4755, 0o2755, 0o1777, 0o7777, 0o000] {
        did(Value::obj().s("op", "chmod").v("items", one(&bits)).u("mode", mode as u64).b("recursive", false).done());
        assert_eq!(stat("%a", &bits), format!("{mode:o}"), "stat -c %a after chmod {mode:o}");
        assert_eq!(meta(&bits).u64_field("mode"), Some(mode as u64), "what the daemon reports after chmod {mode:o}");
        assert_eq!(std::fs::metadata(&bits).unwrap().permissions().mode() & 0o7777, mode);
    }
    // Readable again, or the tidy-up at the end cannot read it.
    did(Value::obj().s("op", "chmod").v("items", one(&bits)).u("mode", 0o644).b("recursive", false).done());
    // Owner and group are names, and they are the ones `stat` gives.
    let m = meta(&bits);
    assert_eq!(m.str_field("owner").unwrap_or(""), stat("%U", &bits));
    assert_eq!(m.str_field("group").unwrap_or(""), stat("%G", &bits));
    assert_eq!(m.u64_field("size"), Some(stat("%s", &bits).parse().unwrap()));
    // A folder keeps its own bits, including the sticky bit a shared folder wants.
    let shared = work.join("shared");
    std::fs::create_dir_all(&shared).unwrap();
    did(Value::obj().s("op", "chmod").v("items", one(&shared)).u("mode", 0o1777).b("recursive", false).done());
    assert_eq!(stat("%a", &shared), "1777");
    assert_eq!(meta(&shared).u64_field("mode"), Some(0o1777));

    // ---------------------------------------------------------------- the resolver
    let home = std::env::var("HOME").expect("HOME");
    assert_eq!(Uri::parse("~").unwrap().path, home);
    assert_eq!(Uri::parse("~/Projects/kiki").unwrap().path, format!("{home}/Projects/kiki"));
    assert!(Uri::parse("~").unwrap().is_local());
    assert_eq!(Uri::parse("~/x").unwrap().display(), "~/x", "and it goes back the way it came");
    // Without a HOME there is nothing to expand, and that is an error rather than a path of "".
    std::env::remove_var("HOME");
    assert!(Uri::parse("~/x").is_err());
    std::env::set_var("HOME", &home);

    // A scheme nothing serves resolves to an error with a code on it — never to a path on this
    // machine, which is what dropping the scheme would give (`/srv/www` for `nosuch://a/srv/www`).
    let unknown = Uri::parse("nosuch://server/srv/www").unwrap();
    assert!(!unknown.is_local());
    let e = ops::local_path(&unknown).unwrap_err();
    assert_eq!(e.code(), "Unsupported", "{}", e.message());
    let e = kikid::locations::resolve(&unknown).map(|_| ()).unwrap_err();
    assert_eq!(e.code(), "Io");
    assert!(e.message().contains("no location for nosuch://server"), "{}", e.message());
    let e = kikid::listing::open(&unknown).map(|_| ()).unwrap_err();
    assert!(matches!(e, VfsError::Io(_)), "{}", e.message());
    // And a job over one is refused rather than run against a local path of the same name.
    refused(Value::obj().s("op", "mkdir").s("uri", "nosuch://server/made-up").done(), "no location for nosuch://server");
    assert!(!PathBuf::from("/made-up").exists());
    // What is not a URI at all.
    for bad in ["nope", "relative/path", "ht tp://x/", "file://host/x"] {
        assert!(Uri::parse(bad).is_err(), "{bad}");
    }

    std::env::remove_var("KIKI_TRASH_DIR");
    std::env::remove_var("KIKI_DATA_DIR");
    std::env::remove_var("KIKI_STATE_DIR");
    std::env::remove_var("KIKI_CONFIG_DIR");
    std::fs::remove_dir_all(&dir).unwrap();
}

/// `work.join("dest")`, made if it is not there — a copy needs somewhere to land.
trait TapCreate {
    fn tap_create(self) -> PathBuf;
}

impl TapCreate for PathBuf {
    fn tap_create(self) -> PathBuf {
        std::fs::create_dir_all(&self).unwrap();
        self
    }
}
