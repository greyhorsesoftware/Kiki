//! `Ctrl+Z` for every local operation that journals one, and `Ctrl+Shift+Z` after it.
//!
//! Each op is run against a tree that has been written down first: what it did is checked, the
//! undo is run, and the **whole tree** — names, bytes, permission bits, modification times — is
//! compared with the one it started from. Then the redo, which has to put it back exactly as the
//! op left it, and one more undo to hand the next op a clean tree.
//!
//! Comparing the tree rather than the one file the op names is the point: a `chmod -R` whose
//! inverse forgets a subdirectory, an extract whose inverse deletes the wrong folder, and a move
//! that comes back under a new name all look right if you only look where the op was aimed.
//!
//! And **undo after the daemon has been restarted**, which is the whole reason the journal is a
//! file: a real `kikid` is started on a socket of its own, told to do something, killed outright,
//! started again on the same state directory, and told `Undo`.

use kikid::jobs;
use kikid::json::Value;
use kikid::ops;
use kikid::vfs::uri::Uri;
use std::collections::BTreeMap;
use std::io::{BufRead, BufReader, Write};
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::time::{Duration, Instant};

const PATIENCE: Duration = Duration::from_secs(20);

// ---------------------------------------------------------------- the tree, written down

/// One entry of a tree: enough of it that a wrong undo cannot pass.
#[derive(Clone, Debug, PartialEq)]
enum Entry {
    /// Directories carry their permission bits and not their time: adding and removing a child
    /// changes a directory's mtime, and an op that is undone has done both.
    Dir(u32),
    File {
        bytes: Vec<u8>,
        mode: u32,
        mtime: u64,
    },
    Link(PathBuf),
}

/// What an entry is and what it may do, without its contents or its time: what can be compared
/// about something an op has just made for the second time. A `tar.gz` carries the moment it was
/// compressed in its own header, and a file made again is made now — neither says anything about
/// whether the redo did the right thing, which is what `check` below is for.
fn shape(e: &Entry) -> (&'static str, u32, Option<&PathBuf>) {
    match e {
        Entry::Dir(m) => ("dir", *m, None),
        Entry::File { mode, .. } => ("file", *mode, None),
        Entry::Link(t) => ("link", 0, Some(t)),
    }
}

/// Every path under `root`, relative to it, in order.
fn tree(root: &Path) -> BTreeMap<String, Entry> {
    let mut out = BTreeMap::new();
    walk(root, root, &mut out);
    out
}

fn walk(root: &Path, at: &Path, out: &mut BTreeMap<String, Entry>) {
    let Ok(rd) = std::fs::read_dir(at) else { return };
    for e in rd.flatten() {
        let p = e.path();
        let rel = p.strip_prefix(root).unwrap().to_string_lossy().into_owned();
        let md = std::fs::symlink_metadata(&p).unwrap();
        let mode = md.permissions().mode() & 0o7777;
        if md.file_type().is_symlink() {
            out.insert(rel, Entry::Link(std::fs::read_link(&p).unwrap()));
        } else if md.is_dir() {
            out.insert(rel, Entry::Dir(mode));
            walk(root, &p, out);
        } else {
            let mtime = md.modified().ok().and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok()).map(|d| d.as_millis() as u64).unwrap_or(0);
            out.insert(rel, Entry::File { bytes: std::fs::read(&p).unwrap(), mode, mtime });
        }
    }
}

// ---------------------------------------------------------------- driving jobs

fn job_json(id: u64) -> Value {
    jobs::list().as_arr().unwrap().iter().find(|j| j.u64_field("id") == Some(id)).cloned().unwrap_or_else(|| panic!("job {id} is not in the list"))
}

/// Polls until the job has stopped, and insists it finished.
fn done(id: u64, what: &str) {
    let start = Instant::now();
    loop {
        let j = job_json(id);
        match j.str_field("state") {
            Some("running") | Some("queued") => {
                assert!(start.elapsed() < PATIENCE, "{what} never finished");
                std::thread::sleep(Duration::from_millis(2));
            }
            Some("done") => return,
            other => panic!("{what} ended {other:?}: {}", j.str_field("error").unwrap_or("")),
        }
    }
}

fn uri(p: &Path) -> String {
    Uri::from_path(p).to_string()
}

fn items(paths: &[PathBuf]) -> Value {
    Value::Arr(paths.iter().map(|p| Value::Str(uri(p))).collect())
}

#[test]
fn every_local_op_can_be_taken_back_and_put_back() {
    let _guard = kikid::ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let sandbox = std::env::temp_dir().join(format!("kiki-undo-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&sandbox);
    let work = sandbox.join("work");
    std::fs::create_dir_all(&work).unwrap();
    std::env::set_var("KIKI_CONFIG_DIR", sandbox.join("config"));
    std::env::set_var("KIKI_STATE_DIR", sandbox.join("state"));
    std::env::set_var("KIKI_DATA_DIR", sandbox.join("data"));
    std::env::set_var("KIKI_TRASH_DIR", sandbox.join("trash"));

    // A tree with something of everything in it: nested folders, a file with odd permissions,
    // a symlink, an empty folder.
    std::fs::create_dir_all(work.join("site/img")).unwrap();
    std::fs::create_dir_all(work.join("site/empty")).unwrap();
    std::fs::create_dir_all(work.join("dest")).unwrap();
    std::fs::write(work.join("site/index.html"), b"<h1>hi</h1>").unwrap();
    std::fs::write(work.join("site/app.js"), b"console.log(1)").unwrap();
    std::fs::write(work.join("site/img/logo.bin"), vec![7u8; 4096]).unwrap();
    std::fs::set_permissions(work.join("site/app.js"), std::fs::Permissions::from_mode(0o640)).unwrap();
    // Folders with bits of their own, so that a recursive chmod really changes them and its
    // inverse really has to put them back.
    std::fs::set_permissions(work.join("site"), std::fs::Permissions::from_mode(0o750)).unwrap();
    std::fs::set_permissions(work.join("site/img"), std::fs::Permissions::from_mode(0o770)).unwrap();
    std::fs::set_permissions(work.join("site/empty"), std::fs::Permissions::from_mode(0o700)).unwrap();
    std::os::unix::fs::symlink("index.html", work.join("site/home.html")).unwrap();
    std::fs::write(work.join("notes.txt"), b"notes").unwrap();

    let (tx, _rx) = mpsc::channel();
    let submit = |op: Value, what: &str| {
        let id = jobs::submit(op, Some(tx.clone())).unwrap_or_else(|e| panic!("{what}: {} {}", e.0, e.1));
        done(id, what);
    };

    // Two trees, entry by entry. Everything that was there before the op is compared whole —
    // bytes, permission bits, modification time; what the op itself made is compared by what it
    // is and what it may do, because a second run of the same op makes it a second time, now.
    let compare = |what: &str, got: &BTreeMap<String, Entry>, want: &BTreeMap<String, Entry>, whole: &BTreeMap<String, Entry>| {
        let (g, w): (Vec<&String>, Vec<&String>) = (got.keys().collect(), want.keys().collect());
        assert_eq!(g, w, "{what}: the tree holds different things");
        for (name, want) in want {
            let got = &got[name];
            if whole.contains_key(name) {
                assert_eq!(got, want, "{what}: {name}");
            } else {
                assert_eq!(shape(got), shape(want), "{what}: {name}");
            }
        }
    };

    // The one shape every case below follows: what the op did is checked, the undo is checked
    // against the whole tree, the redo against the tree the op made, and a last undo leaves the
    // tree as it was found.
    let round_trip = |what: &str, op: Value, check: &dyn Fn()| {
        let before = tree(&work);
        submit(op, what);
        check();
        let after = tree(&work);
        assert_ne!(after, before, "{what} changed nothing: there is nothing to undo");

        let u = jobs::undo(Some(tx.clone())).unwrap_or_else(|e| panic!("{what}: undo refused: {} {}", e.0, e.1));
        done(u, &format!("the undo of {what}"));
        compare(&format!("{what}, undone"), &tree(&work), &before, &before);

        let r = jobs::redo(Some(tx.clone())).unwrap_or_else(|e| panic!("{what}: redo refused: {} {}", e.0, e.1));
        done(r, &format!("the redo of {what}"));
        check();
        compare(&format!("{what}, redone"), &tree(&work), &after, &before);

        let u = jobs::undo(Some(tx.clone())).unwrap_or_else(|e| panic!("{what}: the second undo was refused: {} {}", e.0, e.1));
        done(u, &format!("the second undo of {what}"));
        compare(&format!("{what}, taken back again"), &tree(&work), &before, &before);
    };

    // ---------------------------------------------------------------- rename
    round_trip("a rename", Value::obj().s("op", "rename").s("uri", uri(&work.join("notes.txt"))).s("name", "renamed.md").done(), &|| {
        assert!(work.join("renamed.md").exists() && !work.join("notes.txt").exists());
    });
    // A folder, which is the case that moves a whole subtree with one call.
    round_trip("a folder rename", Value::obj().s("op", "rename").s("uri", uri(&work.join("site"))).s("name", "www").done(), &|| {
        assert_eq!(std::fs::read(work.join("www/img/logo.bin")).unwrap().len(), 4096);
    });

    // ---------------------------------------------------------------- mkdir
    round_trip("a new folder", Value::obj().s("op", "mkdir").s("uri", uri(&work.join("site/fresh"))).done(), &|| {
        assert!(work.join("site/fresh").is_dir());
    });

    // ---------------------------------------------------------------- chmod, recursive
    round_trip("a recursive chmod", Value::obj().s("op", "chmod").v("items", items(&[work.join("site")])).u("mode", 0o711).b("recursive", true).done(), &|| {
        // Every folder and file under it, not just the one named — and the symlink left alone.
        for p in ["site", "site/img", "site/empty", "site/index.html", "site/app.js", "site/img/logo.bin"] {
            assert_eq!(std::fs::symlink_metadata(work.join(p)).unwrap().permissions().mode() & 0o7777, 0o711, "{p}");
        }
    });
    // One file, one mode, no recursion — the Permissions tab's own case.
    round_trip("a chmod of one file", Value::obj().s("op", "chmod").v("items", items(&[work.join("site/index.html")])).u("mode", 0o600).b("recursive", false).done(), &|| {
        assert_eq!(std::fs::metadata(work.join("site/index.html")).unwrap().permissions().mode() & 0o7777, 0o600);
        assert_eq!(std::fs::metadata(work.join("site/app.js")).unwrap().permissions().mode() & 0o7777, 0o640, "and nothing beside it");
    });

    // ---------------------------------------------------------------- move, same filesystem
    round_trip("a move within one disk", Value::obj().s("op", "move").v("items", items(&[work.join("site/app.js"), work.join("site/img")])).s("dest", uri(&work.join("dest"))).done(), &|| {
        assert!(!work.join("site/app.js").exists() && !work.join("site/img").exists());
        assert_eq!(std::fs::read(work.join("dest/app.js")).unwrap(), b"console.log(1)");
        assert_eq!(std::fs::metadata(work.join("dest/app.js")).unwrap().permissions().mode() & 0o7777, 0o640, "a move keeps what it moved");
        assert_eq!(std::fs::read(work.join("dest/img/logo.bin")).unwrap().len(), 4096);
    });

    // ---------------------------------------------------------------- restore, from the trash
    // The op under test is the restore, so the file is in the trash before the tree is written
    // down: undoing a restore puts it back there.
    submit(Value::obj().s("op", "trash").v("items", items(&[work.join("notes.txt")])).done(), "the trash that sets it up");
    let trashed: Vec<String> = ops::trash_infos().into_iter().map(|(n, _, _)| n).collect();
    assert_eq!(trashed.len(), 1);
    round_trip("a restore", Value::obj().s("op", "restore").v("names", Value::Arr(vec![Value::Str(trashed[0].clone())])).done(), &|| {
        assert_eq!(std::fs::read(work.join("notes.txt")).unwrap(), b"notes");
        assert!(ops::trash_infos().is_empty(), "and it is out of the trash");
    });
    assert_eq!(ops::trash_infos().len(), 1, "undoing a restore puts it back in the trash");
    // Put it back for the cases below, and forget the journal entry that did it.
    submit(Value::obj().s("op", "restore").v("names", Value::Arr(vec![Value::Str(ops::trash_infos()[0].0.clone())])).done(), "the restore that puts it back");

    // ---------------------------------------------------------------- compress
    for (format, name) in [("zip", "site.zip"), ("tar.gz", "site.tar.gz")] {
        round_trip(&format!("a {format}"), Value::obj().s("op", "compress").v("items", items(&[work.join("site")])).s("archive", uri(&work.join(name))).s("format", format).done(), &|| {
            assert!(work.join(name).metadata().unwrap().len() > 0);
        });
    }

    // ---------------------------------------------------------------- extract
    // Made outside the tree the comparison watches, so that the archive itself is not the change.
    let archive = sandbox.join("site.zip");
    let cancel = std::sync::atomic::AtomicBool::new(false);
    kikid::archive::compress(&[work.join("site")], &archive, "zip", &cancel, &mut |_| {}).unwrap();
    round_trip("an extract", Value::obj().s("op", "extract").s("archive", uri(&archive)).s("dest", uri(&work.join("dest"))).done(), &|| {
        assert_eq!(std::fs::read(work.join("dest/site/index.html")).unwrap(), b"<h1>hi</h1>");
    });

    // ---------------------------------------------------------------- and the end of the journal
    // Everything above was undone, so the journal is empty and `Ctrl+Z` says so rather than
    // taking back something older than this session.
    // Each is waited for: an undo is a job, and one still running when the sandbox's variables
    // are taken away below runs against the developer's own home — its failure was being written
    // into the real `~/.local/state/kiki/failed-jobs.log`, and the trash it looked in was the
    // real one.
    while let Ok(id) = jobs::undo(Some(tx.clone())) {
        let start = Instant::now();
        while matches!(job_json(id).str_field("state"), Some("running") | Some("queued")) {
            assert!(start.elapsed() < PATIENCE, "an undo at the end of the journal never finished");
            std::thread::sleep(Duration::from_millis(2));
        }
    }
    let e = jobs::undo(Some(tx.clone())).unwrap_err();
    assert_eq!(e.0, "NotFound");
    assert_eq!(e.1, "nothing to undo");

    std::env::remove_var("KIKI_TRASH_DIR");
    std::env::remove_var("KIKI_STATE_DIR");
    std::env::remove_var("KIKI_DATA_DIR");
    std::env::remove_var("KIKI_CONFIG_DIR");
    std::fs::remove_dir_all(&sandbox).unwrap();
}

// ================================================================ a daemon that was restarted

/// A client on the daemon's socket, speaking the text framing (one JSON object per line), which
/// is what the shell uses.
struct Client {
    stream: std::os::unix::net::UnixStream,
    lines: BufReader<std::os::unix::net::UnixStream>,
    next: u64,
}

impl Client {
    fn connect(socket: &Path) -> Client {
        let start = Instant::now();
        loop {
            if let Ok(s) = std::os::unix::net::UnixStream::connect(socket) {
                s.set_read_timeout(Some(PATIENCE)).unwrap();
                let lines = BufReader::new(s.try_clone().unwrap());
                let mut c = Client { stream: s, lines, next: 1 };
                assert!(c.ask("Hello", Value::obj().done()).get("daemon").is_some(), "the daemon says who it is");
                return c;
            }
            assert!(start.elapsed() < PATIENCE, "kikid never came up on {}", socket.display());
            std::thread::sleep(Duration::from_millis(20));
        }
    }

    /// One request, and the reply to it — events on the way are skipped.
    fn ask(&mut self, kind: &str, body: Value) -> Value {
        let id = self.next;
        self.next += 1;
        let mut o = match body {
            Value::Obj(m) => m,
            _ => BTreeMap::new(),
        };
        o.insert("id".into(), Value::Uint(id));
        o.insert("type".into(), Value::Str(kind.to_string()));
        let mut line = kikid::json::to_string(&Value::Obj(o));
        line.push('\n');
        self.stream.write_all(line.as_bytes()).unwrap();
        self.stream.flush().unwrap();
        loop {
            let mut text = String::new();
            assert!(self.lines.read_line(&mut text).unwrap() > 0, "the daemon closed while answering {kind}");
            let v = kikid::json::parse(text.trim().as_bytes()).unwrap_or_else(|e| panic!("{kind}: {e} in {text}"));
            if v.u64_field("id") != Some(id) {
                continue; // an event, or a reply to something else
            }
            if let Some(err) = v.get("err") {
                panic!("{kind}: {} {}", err.str_field("code").unwrap_or(""), err.str_field("message").unwrap_or(""));
            }
            return v.get("ok").cloned().unwrap_or(Value::Null);
        }
    }

    /// Waits for a job the daemon is running to finish, and insists it finished well.
    fn finished(&mut self, id: u64) {
        let start = Instant::now();
        loop {
            let jobs = self.ask("Jobs", Value::obj().done());
            let j = jobs.get("jobs").and_then(Value::as_arr).and_then(|a| a.iter().find(|j| j.u64_field("id") == Some(id)).cloned());
            let state = j.as_ref().and_then(|j| j.str_field("state")).map(str::to_string);
            match state.as_deref() {
                Some("done") => return,
                Some("queued") | Some("running") | None => {
                    assert!(start.elapsed() < PATIENCE, "job {id} never finished");
                    std::thread::sleep(Duration::from_millis(10));
                }
                other => panic!("job {id} ended {other:?}: {}", j.and_then(|j| j.str_field("error").map(str::to_string)).unwrap_or_default()),
            }
        }
    }
}

/// A `kikid` of its own: its own socket, its own state, config, trash and plugin directories, and
/// nothing of the developer's. Killed by the guard however the test ends.
struct Daemon(std::process::Child);

impl Drop for Daemon {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

impl Daemon {
    fn start(sandbox: &Path, socket: &Path) -> Daemon {
        let _ = std::fs::remove_file(socket);
        let child = std::process::Command::new(env!("CARGO_BIN_EXE_kikid"))
            .env("KIKI_SOCKET", socket)
            .env("KIKI_STATE_DIR", sandbox.join("state"))
            .env("KIKI_CONFIG_DIR", sandbox.join("config"))
            .env("KIKI_DATA_DIR", sandbox.join("data"))
            .env("KIKI_TRASH_DIR", sandbox.join("trash"))
            .env("KIKI_THUMB_DIR", sandbox.join("thumbs"))
            .env("KIKI_PLUGIN_DIR", sandbox.join("plugins"))
            .env("KIKI_SECRET_TOOL", sandbox.join("no-secret-tool"))
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .expect("spawn kikid");
        Daemon(child)
    }
}

/// The journal is a file so that `Ctrl+Z` still works after a reboot, an upgrade or a crash —
/// and nothing proved it did. A real daemon does the work, is killed outright (not asked to stop:
/// what is on disk when it dies is all the next one gets), and a second daemon on the same state
/// directory is asked to undo it.
#[test]
fn a_job_can_be_undone_by_the_daemon_that_comes_after() {
    let _guard = kikid::ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let sandbox = std::env::temp_dir().join(format!("kiki-undo-restart-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&sandbox);
    let work = sandbox.join("work");
    std::fs::create_dir_all(work.join("site")).unwrap();
    std::fs::create_dir_all(sandbox.join("plugins")).unwrap();
    std::fs::write(work.join("site/index.html"), b"<h1>hi</h1>").unwrap();
    std::fs::write(work.join("keep.txt"), b"keep").unwrap();
    let socket = sandbox.join("kiki.sock");
    let before = tree(&work);

    {
        let _first = Daemon::start(&sandbox, &socket);
        let mut c = Client::connect(&socket);
        // Two of them, so that the journal has to be more than the last thing anybody did.
        let rename = c.ask("Submit", Value::obj().v("op", Value::obj().s("op", "rename").s("uri", uri(&work.join("keep.txt"))).s("name", "gone.txt").done()).done());
        c.finished(rename.u64_field("job").unwrap());
        let trash = c.ask("Submit", Value::obj().v("op", Value::obj().s("op", "trash").v("items", items(&[work.join("site")])).done()).done());
        c.finished(trash.u64_field("job").unwrap());
        assert!(!work.join("site").exists() && work.join("gone.txt").exists());
        assert!(sandbox.join("state/journal.json").is_file(), "the journal is on disk, not only in the daemon");
    }
    // Killed, not asked to leave: the drop above is a SIGKILL.
    assert!(!work.join("site").exists(), "and it stayed done");

    let _second = Daemon::start(&sandbox, &socket);
    let mut c = Client::connect(&socket);
    let jobs = c.ask("Jobs", Value::obj().done());
    assert_eq!(jobs.get("jobs").and_then(Value::as_arr).map(<[Value]>::len), Some(0), "a fresh daemon is running nothing: only the journal came across");

    let u = c.ask("Undo", Value::obj().done());
    c.finished(u.u64_field("job").unwrap());
    assert!(work.join("site/index.html").is_file(), "the trash was taken back by a daemon that never did it");
    let u = c.ask("Undo", Value::obj().done());
    c.finished(u.u64_field("job").unwrap());
    assert_eq!(tree(&work), before, "and so was the rename before it: the whole journal survived");

    // A journal that has been played out says so, rather than reaching further back.
    let mut line = kikid::json::to_string(&Value::obj().u("id", 99).s("type", "Undo").done());
    line.push('\n');
    c.stream.write_all(line.as_bytes()).unwrap();
    c.stream.flush().unwrap();
    let err = loop {
        let mut text = String::new();
        assert!(c.lines.read_line(&mut text).unwrap() > 0);
        let v = kikid::json::parse(text.trim().as_bytes()).unwrap();
        if v.u64_field("id") == Some(99) {
            break v;
        }
    };
    assert_eq!(err.get("err").unwrap().str_field("code"), Some("NotFound"));

    drop(_second);
    std::fs::remove_dir_all(&sandbox).unwrap();
}
