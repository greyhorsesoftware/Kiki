//! The first screenful comes with the open (docs/0.5.0/10-faster-listings.md), seen from the
//! socket: a real `kikid` on a socket of its own, spoken to the way the window speaks.
//!
//! - A folder the daemon has not listed: `Open` answers at once with no rows, and the `Reset`
//!   that ends the scan carries the first `initial` rows, each with its metadata.
//! - A folder it has: `Open` answers with the rows itself, in the shape a `Window` answers in.
//! - A sort afterwards: its `Reset` carries the rows this connection holds.
//! - `initial: 0`, or none: nothing rides along, as before.

use kikid::json::Value;
use kikid::scratch::Scratch;
use std::collections::BTreeMap;
use std::io::{BufRead, BufReader, Write};
use std::path::Path;
use std::time::{Duration, Instant};

const PATIENCE: Duration = Duration::from_secs(20);

struct Client {
    stream: std::os::unix::net::UnixStream,
    lines: BufReader<std::os::unix::net::UnixStream>,
    next: u64,
    /// Events read past while waiting for a reply, in order, for `event` to hand out.
    held: Vec<Value>,
}

impl Client {
    fn connect(socket: &Path) -> Client {
        let start = Instant::now();
        loop {
            if let Ok(s) = std::os::unix::net::UnixStream::connect(socket) {
                s.set_read_timeout(Some(PATIENCE)).unwrap();
                let lines = BufReader::new(s.try_clone().unwrap());
                let mut c = Client { stream: s, lines, next: 1, held: Vec::new() };
                assert!(c.ask("Hello", Value::obj().done()).get("daemon").is_some(), "the daemon says who it is");
                return c;
            }
            assert!(start.elapsed() < PATIENCE, "kikid never came up on {}", socket.display());
            std::thread::sleep(Duration::from_millis(20));
        }
    }

    fn line(&mut self) -> Value {
        let mut text = String::new();
        assert!(self.lines.read_line(&mut text).unwrap() > 0, "the daemon closed");
        kikid::json::parse(text.trim().as_bytes()).unwrap_or_else(|e| panic!("{e} in {text}"))
    }

    /// One request and the reply to it; events on the way are kept for `event`.
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
            let v = self.line();
            if v.u64_field("id") != Some(id) {
                self.held.push(v);
                continue;
            }
            if let Some(err) = v.get("err") {
                panic!("{kind}: {} {}", err.str_field("code").unwrap_or(""), err.str_field("message").unwrap_or(""));
            }
            return v.get("ok").cloned().unwrap_or(Value::Null);
        }
    }

    /// The next event named `name` for `lid`, from what was read past or from the socket.
    fn event(&mut self, name: &str, lid: u64) -> Value {
        let wanted = |v: &Value| v.str_field("event") == Some(name) && v.u64_field("lid") == Some(lid);
        if let Some(i) = self.held.iter().position(wanted) {
            return self.held.remove(i);
        }
        let start = Instant::now();
        loop {
            assert!(start.elapsed() < PATIENCE, "no {name} for lid {lid}");
            let v = self.line();
            if wanted(&v) {
                return v;
            }
        }
    }
}

struct Daemon(std::process::Child);

impl Drop for Daemon {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

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
        .env("KIKI_EXIT_GRACE_MS", "0")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .expect("spawn kikid");
    Daemon(child)
}

fn rows_of(v: &Value) -> Vec<Value> {
    v.get("rows").and_then(Value::as_arr).map(|a| a.to_vec()).unwrap_or_default()
}

fn stated(rows: &[Value]) -> usize {
    rows.iter().filter(|r| r.get("meta").map(|m| m != &Value::Null).unwrap_or(false)).count()
}

#[test]
fn the_first_screenful_comes_with_the_open() {
    let sandbox = Scratch::new("open-initial");
    let folder = sandbox.join("folder");
    std::fs::create_dir_all(folder.join("a folder")).unwrap();
    // Enough that the scan cannot be over before `Open` has answered: a folder of forty is
    // listed in the microseconds between the reply being built and sent, and the cold path —
    // the one this test is for — is never seen.
    const N: usize = 20_000;
    for i in 0..N {
        std::fs::write(folder.join(format!("file{i:05}.txt")), vec![b'x'; i % 7]).unwrap();
    }
    let socket = sandbox.join("kiki.sock");
    let _daemon = start(&sandbox, &socket);
    let mut c = Client::connect(&socket);
    let uri = kikid::vfs::uri::Uri::from_path(&folder).to_string();

    // Cold: the reply says nothing of the rows, the scan's Reset says all of them.
    let r = c.ask("Open", Value::obj().u("lid", 1).s("uri", &uri).u("initial", 10).done());
    assert_eq!(r.get("cached").and_then(Value::as_bool), Some(false));
    assert!(r.get("rows").is_none(), "nothing listed yet, nothing to hand over: {r:?}");
    // Counts come as the scan goes; the one that says done is the last.
    while c.event("Count", 1).get("done").and_then(Value::as_bool) != Some(true) {}
    let reset = c.event("Reset", 1);
    assert_eq!(reset.u64_field("first"), Some(0));
    let rows = rows_of(&reset);
    assert_eq!(rows.len(), 10, "{reset:?}");
    assert_eq!(rows[0].str_field("name"), Some("a folder"));
    assert_eq!(rows[1].str_field("name"), Some("file00000.txt"));
    assert_eq!(stated(&rows), 10, "whole: every row with its metadata");
    assert_eq!(rows[3].get("meta").unwrap().u64_field("size"), Some(2));

    // Listed already: the rows are on the reply, the shape a Window answers in.
    let r = c.ask("Open", Value::obj().u("lid", 2).s("uri", &uri).u("initial", 5).done());
    assert_eq!(r.get("cached").and_then(Value::as_bool), Some(true));
    // The folder's device rides on the reply too (01-ui-cleanup.md, item 3): the one `stat` says.
    {
        use std::os::unix::fs::MetadataExt;
        assert_eq!(r.u64_field("device"), Some(std::fs::metadata(&folder).unwrap().dev()), "{r:?}");
    }
    assert_eq!(r.u64_field("first"), Some(0));
    assert_eq!(r.u64_field("n"), Some(N as u64 + 1));
    assert_eq!(r.get("done").and_then(Value::as_bool), Some(true));
    assert!(r.u64_field("gen").is_some());
    assert_eq!(rows_of(&r).len(), 5);
    assert_eq!(stated(&rows_of(&r)), 5);

    // A sort after: its Reset carries the rows this connection holds — the five it was handed,
    // since it has never asked a Window — in the new order.
    let _ = c.ask("Sort", Value::obj().u("lid", 2).s("role", "name").s("order", "desc").done());
    let reset = c.event("Reset", 2);
    assert_eq!(reset.u64_field("first"), Some(0));
    let rows = rows_of(&reset);
    assert_eq!(rows.len(), 5);
    assert_eq!(rows[0].str_field("name"), Some("a folder"), "folders first even descending");
    assert_eq!(rows[1].str_field("name"), Some("file19999.txt"));

    // And a Window asked later moves what the next Reset carries.
    let w = c.ask("Window", Value::obj().u("lid", 2).u("first", 20).u("count", 4).done());
    assert_eq!(rows_of(&w).len(), 4);
    let _ = c.ask("Sort", Value::obj().u("lid", 2).s("role", "name").s("order", "asc").done());
    let reset = c.event("Reset", 2);
    assert_eq!(reset.u64_field("first"), Some(20));
    assert_eq!(rows_of(&reset).len(), 4);

    // Asked for nothing, given nothing: the older shape, for a client that will ask.
    let r = c.ask("Open", Value::obj().u("lid", 3).s("uri", &uri).done());
    assert!(r.get("rows").is_none() && r.get("first").is_none(), "{r:?}");
    let _ = c.ask("Sort", Value::obj().u("lid", 3).s("role", "kind").s("order", "asc").done());
    let reset = c.event("Reset", 3);
    assert!(reset.get("rows").is_none(), "{reset:?}");
}
