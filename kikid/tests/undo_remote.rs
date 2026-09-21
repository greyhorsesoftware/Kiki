//! `Ctrl+Z` after a copy whose destination was a server (plan 07).
//!
//! A server has no trash, so the inverse of such a copy is a delete that cannot itself be taken
//! back. Everything here is about that being exact: it deletes what the copy created and nothing
//! else — not the folder it landed in, not a file somebody has changed since, not a folder that
//! has something new in it — and it says what it did.
//!
//! The server is the stub plugin (`stub://`), asked for its tree directly, so what is checked is
//! what is really on the other side and not what the daemon thinks it put there.

mod common;

use kikid::json::Value;
use kikid::locations::{self, Session};
use kikid::plugin::Msg;
use kikid::vfs::uri::Uri;
use kikid::{jobs, json};
use std::collections::BTreeMap;
use std::path::Path;
use std::sync::mpsc::{self, Receiver};
use std::time::{Duration, Instant};

const PATIENCE: Duration = Duration::from_secs(20);

// ---------------------------------------------------------------- driving jobs

fn job_json(id: u64) -> Value {
    jobs::list().as_arr().unwrap().iter().find(|j| j.u64_field("id") == Some(id)).cloned().unwrap_or_else(|| panic!("job {id} is not in the list"))
}

/// Polls until the job has stopped, and returns it.
fn stopped(id: u64) -> Value {
    let start = Instant::now();
    loop {
        let j = job_json(id);
        if !matches!(j.str_field("state"), Some("running") | Some("queued")) {
            return j;
        }
        assert!(start.elapsed() < PATIENCE, "the job never stopped: {}", json::to_string(&j));
        std::thread::sleep(Duration::from_millis(2));
    }
}

fn run(op: Value, what: &str) {
    // No client: a name that is taken is answered "skip" rather than waiting ten minutes for a
    // window that is not there. Nothing below is meant to collide.
    let id = jobs::submit(op, None).unwrap_or_else(|e| panic!("{what}: {} {}", e.0, e.1));
    let j = stopped(id);
    assert_eq!(j.str_field("state"), Some("done"), "{what}: {}", j.str_field("error").unwrap_or(""));
}

fn undo() {
    let id = jobs::undo(None).expect("there is something to undo");
    let j = stopped(id);
    assert_eq!(j.str_field("state"), Some("done"), "the undo: {}", j.str_field("error").unwrap_or(""));
}

/// The last thing a toast said, of all that have been said since the last look.
fn toast(rx: &Receiver<Value>) -> String {
    let mut last = String::new();
    while let Ok(e) = rx.try_recv() {
        if e.str_field("event") == Some("Toast") {
            last = e.str_field("text").unwrap_or("").to_string();
        }
    }
    last
}

// ---------------------------------------------------------------- the server's tree, as it says it is

/// Everything under `path` on the stub, relative to it: a folder is `"d"`, a file is its bytes.
/// What `diff -r` would compare, asked of the plugin rather than of the daemon.
fn tree(s: &Session, path: &str) -> BTreeMap<String, String> {
    let mut out = BTreeMap::new();
    walk(s, path, "", &mut out);
    out
}

fn walk(s: &Session, path: &str, prefix: &str, out: &mut BTreeMap<String, String>) {
    let mut here: Vec<(String, bool)> = Vec::new();
    let listed = s.plugin.request_stream(s.req("Scan").s("path", path).done(), |m| {
        if let Msg::Json(v) = m {
            for e in v.get("entries").and_then(Value::as_arr).into_iter().flatten() {
                here.push((e.str_field("name").unwrap_or("").to_string(), e.str_field("kind") == Some("dir")));
            }
        }
    });
    if listed.is_err() {
        return; // not there at all
    }
    for (name, is_dir) in here {
        let rel = if prefix.is_empty() { name.clone() } else { format!("{prefix}/{name}") };
        let full = format!("{}/{name}", path.trim_end_matches('/'));
        if is_dir {
            out.insert(rel.clone(), "d".to_string());
            walk(s, &full, &rel, out);
        } else {
            out.insert(rel, read(s, &full));
        }
    }
}

fn read(s: &Session, path: &str) -> String {
    let mut got = Vec::new();
    s.plugin
        .read_stream(s.req("Read").s("path", path).done(), |m| {
            if let Msg::Binary(b) = m {
                got.extend_from_slice(&b);
            }
        })
        .unwrap_or_else(|e| panic!("reading {path}: {}", e.message()));
    String::from_utf8_lossy(&got).into_owned()
}

/// Somebody else changing a file on the server, behind kiki's back.
fn put(s: &Session, path: &str, data: &[u8]) {
    let mut once = Some(data.to_vec());
    s.plugin.write_stream(s.req("Write").s("path", path).u("size", data.len() as u64).u("mtime", 0).done(), || once.take()).unwrap_or_else(|e| panic!("writing {path}: {}", e.message()));
}

fn gone(s: &Session, path: &str) {
    s.plugin.request(s.req("Delete").s("path", path).done()).unwrap_or_else(|e| panic!("deleting {path}: {}", e.message()));
}

fn uri(p: &Path) -> String {
    Uri::from_path(p).to_string()
}

fn copy_to(what: &[&Path], dest: &str) -> Value {
    Value::obj().s("op", "copy").v("items", Value::Arr(what.iter().map(|p| Value::Str(uri(p))).collect())).s("dest", dest).done()
}

// ================================================================ the whole story, one stub

/// One test and not six: the plugin registry is one per process and keyed by scheme, so every
/// test in this binary would share one stub process and one in-memory tree (`common`).
#[test]
fn a_copy_to_a_server_is_taken_back_exactly_and_only_where_it_is_still_the_copy() {
    let dir = common::setup("undo-remote");
    let state = dir.join("state");
    std::env::set_var("KIKI_STATE_DIR", &state);
    common::save_location("lab");
    let (s, _) = locations::resolve(&Uri::parse("stub://lab/").unwrap()).expect("the stub location resolves");
    let (tx, rx) = mpsc::channel();
    jobs::subscribe(tx);

    let work = dir.join("work");
    std::fs::create_dir_all(work.join("site/img")).unwrap();
    std::fs::write(work.join("site/index.html"), b"<h1>hi</h1>").unwrap();
    std::fs::write(work.join("site/img/logo.bin"), b"logo").unwrap();
    for (name, text) in [("a.txt", "aaa"), ("b.txt", "bbb"), ("c.txt", "ccc")] {
        std::fs::write(work.join(name), text).unwrap();
    }

    // ---------------------------------------------------------------- a move to a server: nothing to undo
    // It is not journalled and never was: taking a move back means putting an original back, and
    // there is no trash on the other side to take it from.
    std::fs::write(work.join("moved.txt"), b"moved").unwrap();
    run(Value::obj().s("op", "move").v("items", Value::Arr(vec![Value::Str(uri(&work.join("moved.txt")))])).s("dest", "stub://lab/empty").done(), "a move to the server");
    assert_eq!(read(&s, "/empty/moved.txt"), "moved", "the move did arrive");
    assert!(!work.join("moved.txt").exists(), "and took the original away");
    let e = jobs::undo(None).unwrap_err();
    assert_eq!((e.0, e.1.as_str()), ("NotFound", "nothing to undo"), "a move that touched a server journals nothing");
    gone(&s, "/empty/moved.txt");

    // ---------------------------------------------------------------- a folder into a folder that was there
    // `/docs` is the stub's own, with two files in it. The copy adds one folder to it; the undo
    // takes that folder back and leaves `/docs` and everything that was in it alone.
    let docs_before = tree(&s, "/docs");
    run(copy_to(&[&work.join("site")], "stub://lab/docs"), "a copy into /docs");
    assert_eq!(read(&s, "/docs/site/img/logo.bin"), "logo", "the copy arrived");
    let offered = toast(&rx);
    assert_eq!(offered, "Copy site to docs — Undo deletes it from lab, permanently", "the toast that offers the undo says what it would do");

    // What was written down for it — on disk, because the next daemon may be the one that runs it.
    let saved = json::parse(&std::fs::read(state.join("journal.json")).unwrap()).unwrap();
    let entry = saved.as_arr().unwrap().last().expect("the copy is on the journal").clone();
    let inv = entry.get("inverse").unwrap();
    assert_eq!(inv.str_field("op"), Some("deleteCopies"));
    assert_eq!(inv.str_field("dest"), Some("stub://lab/docs"));
    assert_eq!(inv.str_field("guard"), Some("sizeMtime"), "the stub dates what is written to it, and its Describe says so");
    assert_eq!(
        inv.get("dirs").and_then(Value::as_arr).map(<[Value]>::to_vec),
        Some(vec![Value::Str("stub://lab/docs/site".into()), Value::Str("stub://lab/docs/site/img".into())]),
        "the folders it made — and not the one it landed in"
    );
    let files = inv.get("files").and_then(Value::as_arr).unwrap().to_vec();
    assert_eq!(files.len(), 2);
    for f in &files {
        let a = f.as_arr().unwrap();
        let u = a[0].as_str().unwrap();
        let (size, mtime) = (a[1].as_u64().unwrap(), a[2].as_u64().unwrap());
        let stat = s.plugin.request(s.req("Stat").s("path", u.trim_start_matches("stub://lab")).done()).unwrap();
        assert_eq!((Some(size), Some(mtime)), (stat.u64_field("size"), stat.u64_field("mtime")), "{u}: what is written down is what the SERVER says about it");
        assert!(mtime > 1_700_000_000_000, "{u}: dated by the server as it was written, not by the file it came from");
    }
    assert_eq!(entry.get("redo").and_then(|r| r.str_field("op")), Some("copy"), "and the redo is the copy itself");

    undo();
    assert_eq!(toast(&rx), "Undid copy — 4 items deleted from lab (permanently)", "two files and the two folders it made");
    assert_eq!(tree(&s, "/docs"), docs_before, "/docs is as it was: the copy's folder gone, the server's own files untouched");

    // ---------------------------------------------------------------- and put back again
    let again = jobs::redo(None).expect("there is something to redo");
    assert_eq!(stopped(again).str_field("state"), Some("done"));
    assert_eq!(read(&s, "/docs/site/index.html"), "<h1>hi</h1>", "redo copies it again");
    undo();
    assert_eq!(tree(&s, "/docs"), docs_before);

    // ---------------------------------------------------------------- files into a folder, two changed since
    // Three files straight into `/empty`, the stub's own folder: the inverse names the three files
    // and not the folder they landed in. Then somebody edits two of them — one to the same length,
    // which only the time can tell, one to a different length.
    run(copy_to(&[&work.join("a.txt"), &work.join("b.txt"), &work.join("c.txt")], "stub://lab/empty"), "three files into /empty");
    std::thread::sleep(Duration::from_millis(20)); // the server dates what it writes by its own clock
    put(&s, "/empty/b.txt", b"BBB");
    put(&s, "/empty/c.txt", b"ccccccc");

    undo();
    assert_eq!(toast(&rx), "Undid copy — 1 item deleted from lab (permanently), 2 items left because they had changed");
    let left = tree(&s, "/empty");
    assert_eq!(left.get("a.txt"), None, "the one that was still the copy's is gone");
    assert_eq!(
        (left.get("b.txt").map(String::as_str), left.get("c.txt").map(String::as_str)),
        (Some("BBB"), Some("ccccccc")),
        "the two that changed are left exactly as they were changed to"
    );
    gone(&s, "/empty/b.txt");
    gone(&s, "/empty/c.txt");

    // ---------------------------------------------------------------- a folder somebody has put something in
    run(copy_to(&[&work.join("site")], "stub://lab/empty"), "a copy into /empty");
    put(&s, "/empty/site/img/mine.txt", b"mine");

    undo();
    assert_eq!(toast(&rx), "Undid copy — 2 items deleted from lab (permanently), 2 items left because they had changed", "the two files go; neither folder does");
    let left = tree(&s, "/empty");
    assert_eq!(left.get("site").map(String::as_str), Some("d"), "the folder stays, because what is in it now is not the copy's");
    assert_eq!(left.get("site/img/mine.txt").map(String::as_str), Some("mine"));
    assert_eq!(left.get("site/index.html"), None, "but what the copy put there is gone");
    assert_eq!(left.get("site/img/logo.bin"), None);
    gone(&s, "/empty/site/img/mine.txt");
    gone(&s, "/empty/site/img");
    gone(&s, "/empty/site");
    assert!(tree(&s, "/empty").is_empty(), "and the stub's own folder is empty again");

    // ---------------------------------------------------------------- a copy that stopped part-way
    // One file that cannot be read does not stop the other: it arrives, the job fails at the end
    // saying which did not, and what did arrive is as undoable as if the job had finished.
    use std::os::unix::fs::PermissionsExt;
    std::fs::write(work.join("locked.txt"), b"locked").unwrap();
    std::fs::set_permissions(work.join("locked.txt"), std::fs::Permissions::from_mode(0o000)).unwrap();
    if std::fs::read(work.join("locked.txt")).is_err() {
        // (Running as root, nothing is unreadable, and there is nothing here to test.)
        let id = jobs::submit(copy_to(&[&work.join("b.txt"), &work.join("locked.txt")], "stub://lab/empty"), None).unwrap();
        let j = stopped(id);
        assert_eq!(j.str_field("state"), Some("failed"), "the file it could not read fails the job");
        assert_eq!(read(&s, "/empty/b.txt"), "bbb", "the other one arrived all the same");
        undo();
        assert_eq!(toast(&rx), "Undid copy — 1 item deleted from lab (permanently)", "and the part that arrived can be taken back");
        assert!(tree(&s, "/empty").is_empty());
    }
    std::fs::set_permissions(work.join("locked.txt"), std::fs::Permissions::from_mode(0o644)).unwrap();

    // ---------------------------------------------------------------- a redo with nothing to copy
    run(copy_to(&[&work.join("a.txt")], "stub://lab/empty"), "one file into /empty");
    undo();
    std::fs::remove_file(work.join("a.txt")).unwrap();
    let again = jobs::redo(None).expect("there is something to redo");
    let j = stopped(again);
    assert_eq!(j.str_field("state"), Some("failed"), "a redo whose source has gone fails, and says so");
    assert!(tree(&s, "/empty").is_empty(), "leaving the server as the undo left it");

    locations::remove("lab").unwrap();
    std::env::remove_var("KIKI_STATE_DIR");
    std::fs::remove_dir_all(&dir).unwrap();
}

// ================================================================ a daemon that was restarted

/// The journal is a file so that `Ctrl+Z` still works after a crash or an upgrade, and this
/// inverse is no exception: a real `kikid` copies to the stub, is killed outright, and a second
/// daemon on the same state directory is asked to undo it.
///
/// The stub's tree lives in the plugin process, so the second daemon's stub starts from the seed
/// and knows nothing of the first one's files. What the copy made is therefore put back — same
/// names, same sizes — by a MOVE, which journals nothing, and these daemons' stub is told to
/// answer `sizeOnly` (as the FTPS plugin does, having no way to keep a file's time), so the check
/// the undo makes is the size, on files this daemon never saw written.
#[test]
fn a_copy_to_a_server_can_be_undone_by_the_daemon_that_comes_after() {
    let _guard = kikid::ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let sandbox = std::env::temp_dir().join(format!("kiki-undo-remote-restart-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&sandbox);
    let work = sandbox.join("work");
    std::fs::create_dir_all(work.join("site")).unwrap();
    std::fs::create_dir_all(sandbox.join("plugins")).unwrap();
    std::fs::copy(env!("CARGO_BIN_EXE_kiki-plugin-stub"), sandbox.join("plugins/kiki-plugin-stub")).unwrap();
    std::fs::write(work.join("site/one.txt"), b"one").unwrap();
    std::fs::write(work.join("site/two.txt"), b"twotwo").unwrap();
    let socket = sandbox.join("kiki.sock");

    {
        let _first = Daemon::start(&sandbox, &socket);
        let mut c = Client::connect(&socket);
        c.add_location();
        let copy = c.ask("Submit", Value::obj().v("op", Value::obj().s("op", "copy").v("items", Value::Arr(vec![Value::Str(uri(&work.join("site")))])).s("dest", "stub://lab/empty").done()).done());
        c.finished(copy.u64_field("job").unwrap());
        assert_eq!(c.names("stub://lab/empty/site"), vec!["one.txt", "two.txt"], "the copy arrived on the server");
        let saved = std::fs::read_to_string(sandbox.join("state/journal.json")).unwrap();
        assert!(saved.contains("deleteCopies") && saved.contains(r#""guard":"size""#), "the inverse is on disk, with the check to make: {saved}");
    }
    // Killed, not asked to leave: the drop above is a SIGKILL.

    let _second = Daemon::start(&sandbox, &socket);
    let mut c = Client::connect(&socket);
    std::fs::create_dir_all(work.join("again/site")).unwrap();
    std::fs::write(work.join("again/site/one.txt"), b"ONE").unwrap();
    std::fs::write(work.join("again/site/two.txt"), b"TWOTWO").unwrap();
    let mv = c.ask("Submit", Value::obj().v("op", Value::obj().s("op", "move").v("items", Value::Arr(vec![Value::Str(uri(&work.join("again/site")))])).s("dest", "stub://lab/empty").done()).done());
    c.finished(mv.u64_field("job").unwrap());
    assert_eq!(c.names("stub://lab/empty/site"), vec!["one.txt", "two.txt"], "the same tree is there again");

    let u = c.ask("Undo", Value::obj().done()).u64_field("job").unwrap();
    let said = c.toast(u);
    c.finished(u);
    assert_eq!(said, "Undid copy — 3 items deleted from lab (permanently)", "the two files and the folder the copy made");
    assert!(c.names("stub://lab/empty").is_empty(), "the server is as it was before a copy this daemon never ran");
    assert_eq!(c.error("Undo").str_field("code"), Some("NotFound"), "and that was the only thing on the journal");

    drop(_second);
    std::fs::remove_dir_all(&sandbox).unwrap();
}

/// A client on the daemon's socket, speaking the text framing (one JSON object per line), as
/// `undo.rs`'s does — with the little it needs of a listing and of what a job says.
struct Client {
    stream: std::os::unix::net::UnixStream,
    lines: std::io::BufReader<std::os::unix::net::UnixStream>,
    events: Vec<Value>,
    next: u64,
}

impl Client {
    fn connect(socket: &Path) -> Client {
        use std::io::BufReader;
        let start = Instant::now();
        loop {
            if let Ok(s) = std::os::unix::net::UnixStream::connect(socket) {
                s.set_read_timeout(Some(PATIENCE)).unwrap();
                let lines = BufReader::new(s.try_clone().unwrap());
                let mut c = Client { stream: s, lines, events: Vec::new(), next: 1 };
                assert!(c.ask("Hello", Value::obj().done()).get("daemon").is_some(), "the daemon says who it is");
                c.ask("JobEvents", Value::obj().done()); // toasts among them, which is what a window does
                return c;
            }
            assert!(start.elapsed() < PATIENCE, "kikid never came up on {}", socket.display());
            std::thread::sleep(Duration::from_millis(20));
        }
    }

    /// The reply to one request; events on the way are kept (the toasts are read from them).
    fn reply(&mut self, kind: &str, body: Value) -> Value {
        use std::io::{BufRead, Write};
        let id = self.next;
        self.next += 1;
        let mut o = match body {
            Value::Obj(m) => m,
            _ => BTreeMap::new(),
        };
        o.insert("id".into(), Value::Uint(id));
        o.insert("type".into(), Value::Str(kind.to_string()));
        let mut line = json::to_string(&Value::Obj(o));
        line.push('\n');
        self.stream.write_all(line.as_bytes()).unwrap();
        self.stream.flush().unwrap();
        loop {
            let mut text = String::new();
            assert!(self.lines.read_line(&mut text).unwrap() > 0, "the daemon closed while answering {kind}");
            let v = json::parse(text.trim().as_bytes()).unwrap_or_else(|e| panic!("{kind}: {e} in {text}"));
            if v.u64_field("id") != Some(id) {
                self.events.push(v);
                continue;
            }
            return v;
        }
    }

    fn ask(&mut self, kind: &str, body: Value) -> Value {
        let v = self.reply(kind, body);
        if let Some(err) = v.get("err") {
            panic!("{kind}: {} {}", err.str_field("code").unwrap_or(""), err.str_field("message").unwrap_or(""));
        }
        v.get("ok").cloned().unwrap_or(Value::Null)
    }

    fn error(&mut self, kind: &str) -> Value {
        self.reply(kind, Value::obj().done()).get("err").cloned().unwrap_or(Value::Null)
    }

    fn add_location(&mut self) {
        let loc = Value::obj().s("name", "lab").s("plugin", "stub").s("remoteUri", "stub://lab/").v("config", Value::obj().s("name", "lab").done()).done();
        self.ask("AddLocation", Value::obj().v("location", loc).v("secrets", Value::obj().done()).done());
    }

    /// What the server has in a folder, by name; empty when there is no such folder.
    fn names(&mut self, uri: &str) -> Vec<String> {
        let lid = 4242;
        if self.reply("Open", Value::obj().u("lid", lid).s("uri", uri).done()).get("ok").is_none() {
            return Vec::new();
        }
        let start = Instant::now();
        let mut rows: Vec<String> = Vec::new();
        loop {
            let w = self.ask("Window", Value::obj().u("lid", lid).u("first", 0).u("count", 50).done());
            rows = w.get("rows").and_then(Value::as_arr).map(|a| a.iter().filter_map(|r| r.str_field("name").map(str::to_string)).collect()).unwrap_or(rows);
            if w.get("done").and_then(Value::as_bool).unwrap_or(false) || start.elapsed() > Duration::from_secs(5) {
                break;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        self.ask("Close", Value::obj().u("lid", lid).done());
        rows.sort();
        rows
    }

    /// What a job said when it was over, waited for.
    fn toast(&mut self, job: u64) -> String {
        let start = Instant::now();
        loop {
            if let Some(t) = self.events.iter().find(|e| e.str_field("event") == Some("Toast") && e.u64_field("job") == Some(job)) {
                return t.str_field("text").unwrap_or("").to_string();
            }
            assert!(start.elapsed() < PATIENCE, "job {job} never said anything");
            self.ask("Jobs", Value::obj().done()); // which reads whatever else is waiting on the socket
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    /// Waits for a job the daemon is running to finish, and insists it finished well.
    fn finished(&mut self, id: u64) {
        let start = Instant::now();
        loop {
            let jobs = self.ask("Jobs", Value::obj().done());
            let j = jobs.get("jobs").and_then(Value::as_arr).and_then(|a| a.iter().find(|j| j.u64_field("id") == Some(id)).cloned());
            let (state, why) = (j.as_ref().and_then(|j| j.str_field("state").map(str::to_string)), j.as_ref().and_then(|j| j.str_field("error").map(str::to_string)).unwrap_or_default());
            match state.as_deref() {
                Some("done") => return,
                Some("queued") | Some("running") | None => {
                    assert!(start.elapsed() < PATIENCE, "job {id} never finished");
                    std::thread::sleep(Duration::from_millis(10));
                }
                other => panic!("job {id} ended {other:?}: {why}"),
            }
        }
    }
}

/// A `kikid` of its own: its own socket, state, config, trash and plugin directories, and nothing
/// of the developer's. Killed by the guard however the test ends.
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
            // As FTPS answers, having no way to set a file's time.
            .env("KIKI_STUB_DETECTOR_UPLOAD", "sizeOnly")
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .expect("spawn kikid");
        Daemon(child)
    }
}
