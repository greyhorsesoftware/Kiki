//! A location plugin over an in-memory tree, speaking docs/0.1.0/API-PLUGIN.md.
//! It has a two-field form (name, greeting) and a `stub://` scheme. Used by the
//! daemon's plugin contract tests and as the smallest example of a plugin.

use kikid::json::{self, Value};
use std::collections::BTreeMap;
use std::io::{self, Read, Write};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

struct Node {
    is_dir: bool,
    data: Vec<u8>,
    mtime: u64,
    /// The backend's own hidden flag (`Meta.hidden`), which a name says nothing about — SMB's DOS
    /// attribute, gio's `standard::is-hidden`. The daemon filters such an entry like a dot-file.
    hidden: bool,
    children: BTreeMap<String, Node>,
}

impl Node {
    fn dir() -> Node {
        Node { is_dir: true, data: Vec::new(), mtime: 1_700_000_000_000, hidden: false, children: BTreeMap::new() }
    }
    fn file(data: &[u8]) -> Node {
        Node { is_dir: false, data: data.to_vec(), mtime: 1_700_000_000_000, hidden: false, children: BTreeMap::new() }
    }
    /// A file as it is WRITTEN: dated by this server's own clock, because this one cannot be told
    /// a time (`setMtime: false`, as FTP cannot). The same file written twice therefore has two
    /// times, which is what lets a host tell the file it left from the one somebody replaced.
    fn written(data: &[u8]) -> Node {
        let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0);
        Node { mtime: now, ..Node::file(data) }
    }
    fn hidden(mut self) -> Node {
        self.hidden = true;
        self
    }
}

fn seed() -> Node {
    let mut root = Node::dir();
    let mut docs = Node::dir();
    docs.children.insert("readme.md".into(), Node::file(b"# stub\n"));
    docs.children.insert("notes.txt".into(), Node::file(b"hello"));
    root.children.insert("docs".into(), docs);
    root.children.insert("empty".into(), Node::dir());
    root.children.insert("data.bin".into(), Node::file(&[0u8; 4096]));
    // Read slowly, so a test can have a second transfer begin while this one is still streaming —
    // or kill the plugin part-way through one. Eight chunks at 60 ms is about half a second.
    root.children.insert("slow.bin".into(), Node::file(&[7u8; 8192]));
    // Hidden by the plugin, not by its name: the only way to prove the flag is what does it.
    root.children.insert("secret.bin".into(), Node::file(b"shh").hidden());
    root
}

fn lookup<'a>(root: &'a mut Node, path: &str) -> Option<&'a mut Node> {
    let mut cur = root;
    for seg in path.split('/').filter(|s| !s.is_empty()) {
        cur = cur.children.get_mut(seg)?;
    }
    Some(cur)
}

fn split(path: &str) -> (String, String) {
    let p = path.trim_end_matches('/');
    match p.rfind('/') {
        Some(i) => (p[..i].to_string(), p[i + 1..].to_string()),
        None => (String::new(), p.to_string()),
    }
}

// ---------------------------------------------------------------- framing (binary only)

fn read_frame(stdin: &mut impl Read) -> io::Result<Option<(u8, Vec<u8>)>> {
    let mut head = [0u8; 5];
    match stdin.read_exact(&mut head) {
        Ok(()) => {}
        Err(e) if e.kind() == io::ErrorKind::UnexpectedEof => return Ok(None),
        Err(e) => return Err(e),
    }
    let len = u32::from_le_bytes([head[0], head[1], head[2], head[3]]) as usize;
    let mut payload = vec![0u8; len];
    stdin.read_exact(&mut payload)?;
    Ok(Some((head[4], payload)))
}

fn write_json(out: &mut impl Write, v: &Value) -> io::Result<()> {
    let s = json::to_string(v);
    out.write_all(&(s.len() as u32).to_le_bytes())?;
    out.write_all(&[0u8])?;
    out.write_all(s.as_bytes())?;
    out.flush()
}

fn write_binary(out: &mut impl Write, b: &[u8]) -> io::Result<()> {
    out.write_all(&(b.len() as u32).to_le_bytes())?;
    out.write_all(&[1u8])?;
    out.write_all(b)?;
    out.flush()
}

/// stdout, shared: a big scan streams from a thread of its own while the main loop goes on
/// reading — which is the only way a `Cancel` can arrive in the middle of one.
type Out = Arc<Mutex<io::Stdout>>;

fn send(out: &Out, v: &Value) {
    let _ = write_json(&mut *out.lock().unwrap(), v);
}

fn send_binary(out: &Out, b: &[u8]) {
    let _ = write_binary(&mut *out.lock().unwrap(), b);
}

/// How many entries a recursive `Scan` should invent, when the tests ask for a big tree.
fn big_scan() -> Option<u64> {
    std::env::var("KIKI_STUB_BIG_SCAN").ok().and_then(|n| n.parse::<u64>().ok()).filter(|n| *n > 0)
}

/// A hundred entries at a time, with a breath between batches so the tree takes long enough to
/// be cancelled part-way. Answers how many entries it sent before it stopped.
fn stream_big_scan(out: &Out, id: u64, total: u64, cancel: &AtomicBool) -> u64 {
    let mut sent = 0u64;
    while sent < total {
        if cancel.load(Ordering::Relaxed) {
            break;
        }
        let batch: Vec<Value> = (sent..(sent + 100).min(total))
            .map(|i| {
                let rel = if i % 100 == 0 { format!("dir-{:04}", i / 100) } else { format!("dir-{:04}/file-{i:06}.txt", i / 100) };
                let is_dir = i % 100 == 0;
                let m = Value::obj().u("size", if is_dir { 0 } else { 32 }).u("mtime", 1_700_000_000_000u64).v("mode", Value::Null).v("owner", Value::Null).v("group", Value::Null).v("digest", Value::Null).done();
                Value::obj().s("rel", rel).s("name", format!("file-{i:06}.txt")).s("kind", if is_dir { "dir" } else { "file" }).v("meta", m).done()
            })
            .collect();
        sent += batch.len() as u64;
        send(out, &Value::obj().u("id", id).v("entries", Value::Arr(batch)).done());
        std::thread::sleep(std::time::Duration::from_millis(2));
    }
    sent
}

fn ok(id: u64, v: Value) -> Value {
    Value::obj().u("id", id).v("ok", v).done()
}
fn err(id: u64, code: &str, msg: &str) -> Value {
    Value::obj().u("id", id).v("err", Value::obj().s("code", code).s("message", msg).done()).done()
}

fn meta(n: &Node) -> Value {
    let mut o = Value::obj().u("size", n.data.len() as u64).u("mtime", n.mtime).v("mode", Value::Null).v("owner", Value::Null).v("group", Value::Null).v("digest", Value::Null);
    // Said only when it is true, as the SDK says it: a plugin that knows nothing about hiding
    // never mentions the field at all.
    if n.hidden {
        o = o.b("hidden", true);
    }
    o.done()
}

/// What this stub claims about detecting a changed file, per direction. Fixed at `sizeMtime`
/// unless a test asks for something else — which is how `pick_detector` is driven over the
/// answers only a device plugin gives (`sizeOnly`, `digest`) without one of those to hand.
fn detector(dir: &str, fallback: &str) -> String {
    std::env::var(format!("KIKI_STUB_DETECTOR_{}", dir.to_ascii_uppercase())).unwrap_or_else(|_| fallback.to_string())
}

fn main() {
    let mut root = seed();
    let mut stdin = io::stdin().lock();
    let out: Out = Arc::new(Mutex::new(io::stdout()));
    // Big scans in flight, so a `Cancel` can reach the thread that is streaming one.
    let scanning: Arc<Mutex<BTreeMap<u64, Arc<AtomicBool>>>> = Arc::new(Mutex::new(BTreeMap::new()));
    let mut connected: BTreeMap<String, bool> = BTreeMap::new();
    let mut pending_write: Option<(u64, String, Vec<u8>)> = None;
    // `Log` events go out only to a host that asked for them (API-PLUGIN): one that reads frames
    // strictly must not find an event in the middle of a stream it thought it knew.
    let logging = std::env::var("KIKI_PLUGIN_LOG").as_deref() == Ok("1");
    while let Ok(Some((kind, payload))) = read_frame(&mut stdin) {
        if kind == 1 {
            if let Some((id, path, buf)) = pending_write.as_mut() {
                if payload.is_empty() {
                    let (dir, name) = split(path);
                    let bytes = buf.len() as u64;
                    let reply = match lookup(&mut root, &dir) {
                        Some(d) if d.is_dir => {
                            d.children.insert(name, Node::written(buf));
                            ok(*id, Value::obj().u("bytes", bytes).done())
                        }
                        _ => err(*id, "NotFound", "no such directory"),
                    };
                    send(&out, &reply);
                    pending_write = None;
                } else {
                    buf.extend_from_slice(&payload);
                }
            }
            continue;
        }
        let v = match json::parse(&payload) {
            Ok(v) => v,
            Err(_) => continue,
        };
        let id = v.u64_field("id").unwrap_or(0);
        let t = v.str_field("type").unwrap_or("");
        let loc = v.str_field("location").unwrap_or("").to_string();
        let path = v.str_field("path").unwrap_or("/").to_string();
        let reply = match t {
            "Describe" => ok(
                id,
                Value::obj()
                    .s("scheme", "stub")
                    .s("displayName", "Stub")
                    .s("version", "1.0")
                    .v(
                        "form",
                        Value::Arr(vec![
                            Value::obj().s("key", "name").s("label", "Name").s("kind", "text").b("required", true).v("default", Value::Null).v("options", Value::Null).v("group", Value::Null).done(),
                            Value::obj().s("key", "greeting").s("label", "Greeting").s("kind", "text").b("required", false).s("default", "hi").v("options", Value::Null).v("group", Value::Null).done(),
                            // A select, as the SDK spells one: (value, label) pairs. The tests use it for
                            // the 0.2.0 migration of values stored as labels.
                            Value::obj()
                                .s("key", "flavour")
                                .s("label", "Flavour")
                                .s("kind", "select")
                                .b("required", false)
                                .s("default", "mild")
                                .v("options", Value::Arr(vec![Value::obj().s("value", "mild").s("label", "Mild").done(), Value::obj().s("value", "hot").s("label", "Hot").done()]))
                                .v("group", Value::Null)
                                .done(),
                        ]),
                    )
                    .v("defaults", Value::obj().s("greeting", "hi").done())
                    .v("secretFields", Value::Arr(vec![Value::Str("password".into())]))
                    .v("detector", Value::obj().s("upload", detector("upload", "sizeMtime")).s("download", detector("download", "sizeMtime")).done())
                    .v("features", Value::obj().b("setMtime", false).b("mode", false).b("realDirs", true).v("digestKind", Value::Null).s("separator", "/").b("metaInScan", true).b("pipelining", false).done())
                    .done(),
            ),
            "Validate" => {
                let cfg = v.get("config");
                if cfg.and_then(|c| c.str_field("name")).map(|n| n.is_empty()).unwrap_or(true) {
                    Value::obj().u("id", id).v("err", Value::obj().s("code", "Invalid").s("message", "name is required").s("field", "name").done()).done()
                } else {
                    ok(id, Value::obj().done())
                }
            }
            // A server that takes its time answering, for the tests that cancel while one is
            // being connected to: `KIKI_STUB_SLOW_CONNECT=<ms>` and a location whose name begins
            // with `slow`. It blocks this loop, as a plugin stuck in a TCP connect would.
            "Connect" if loc.starts_with("slow") => {
                if let Ok(ms) = std::env::var("KIKI_STUB_SLOW_CONNECT").map(|v| v.parse::<u64>().unwrap_or(0)) {
                    std::thread::sleep(std::time::Duration::from_millis(ms));
                }
                connected.insert(loc.clone(), true);
                ok(id, Value::obj().v("fingerprint", Value::Null).v("banner", Value::Null).done())
            }
            "Connect" => {
                connected.insert(loc.clone(), true);
                // A server that identifies itself with a key says so here; the stub does it when
                // its config asks, which is how the contract test drives kiki's verify flow.
                let fp = v.get("config").and_then(|c| c.str_field("fingerprint")).map(str::to_string);
                ok(id, Value::obj().opt_s("fingerprint", fp.as_deref()).v("banner", Value::Null).done())
            }
            "Disconnect" => {
                connected.remove(&loc);
                // Every real plugin says something as it lets a connection go (FTPS quits its
                // control connection, SFTP its channels), and the host files that line under
                // whichever jobs hold sessions on this process — which it has to ask the session
                // map for. A host that answered a Disconnect with that map locked would wait here
                // for ever, so the stub always speaks before it replies.
                if logging {
                    send(&out, &Value::obj().s("event", "Log").s("level", "info").s("target", "kiki").s("message", format!("letting {loc} go")).s("role", "").done());
                }
                ok(id, Value::obj().done())
            }
            "Capabilities" => {
                ok(id, Value::obj().b("trash", false).b("setMtime", false).b("mode", false).b("realDirs", true).v("digestKind", Value::Null).s("separator", "/").v("fastScan", Value::Null).b("partialRead", true).done())
            }
            "Ping" => ok(id, Value::obj().done()),
            "Cancel" => {
                // Told at once, not when the walk next looks up: the flag belongs to the request
                // being cancelled, and the thread streaming it reads it between batches.
                if let Some(f) = v.u64_field("target").and_then(|t| scanning.lock().unwrap().get(&t).cloned()) {
                    f.store(true, Ordering::Relaxed);
                }
                ok(id, Value::obj().done())
            }
            "Shutdown" => {
                send(&out, &ok(id, Value::obj().done()));
                return;
            }
            // A tree too big to answer in one breath, for the tests that cancel a compare:
            // `KIKI_STUB_BIG_SCAN=<entries>` makes a recursive Scan stream that many synthetic
            // entries, slowly, from a thread of its own — so this loop goes on reading and a
            // Cancel arrives in the middle of it, as it would against a real server.
            "Scan" if big_scan().is_some() && v.get("recursive").and_then(Value::as_bool).unwrap_or(false) => {
                let (total, id, out, scanning) = (big_scan().unwrap(), id, Arc::clone(&out), Arc::clone(&scanning));
                let flag = Arc::new(AtomicBool::new(false));
                scanning.lock().unwrap().insert(id, Arc::clone(&flag));
                std::thread::spawn(move || {
                    let sent = stream_big_scan(&out, id, total, &flag);
                    scanning.lock().unwrap().remove(&id);
                    // What the test reads: how far the plugin's own work got before it stopped.
                    if let Ok(p) = std::env::var("KIKI_STUB_SCAN_LOG") {
                        let _ = std::fs::write(p, format!("{sent} of {total}\n"));
                    }
                    let reply = if flag.load(Ordering::Relaxed) { err(id, "Cancelled", "cancelled") } else { ok(id, Value::obj().u("n", sent).done()) };
                    send(&out, &reply);
                });
                continue;
            }
            // API-PLUGIN: a plugin that cannot walk a whole tree in one request says so, and the
            // daemon lists directory by directory. Answering the top level and calling it
            // recursive would leave every mirror re-copying the files it never saw.
            "Scan" if v.get("recursive").and_then(Value::as_bool).unwrap_or(false) => err(id, "Unsupported", "the stub lists one directory at a time"),
            "Scan" => match lookup(&mut root, &path) {
                Some(d) if d.is_dir => {
                    let entries: Vec<Value> = d.children.iter().map(|(n, c)| Value::obj().s("name", n.clone()).s("kind", if c.is_dir { "dir" } else { "file" }).v("meta", meta(c)).done()).collect();
                    let n = entries.len() as u64;
                    send(&out, &Value::obj().u("id", id).v("entries", Value::Arr(entries)).done());
                    ok(id, Value::obj().u("n", n).done())
                }
                Some(_) => err(id, "Io", "not a directory"),
                None => err(id, "NotFound", "no such path"),
            },
            "Stat" => match lookup(&mut root, &path) {
                Some(n) => ok(id, meta(n)),
                None => err(id, "NotFound", "no such path"),
            },
            "Read" => match lookup(&mut root, &path) {
                Some(n) if !n.is_dir => {
                    let data = n.data.clone();
                    let slow = path.ends_with("slow.bin");
                    for chunk in data.chunks(1024) {
                        if slow {
                            std::thread::sleep(std::time::Duration::from_millis(60));
                        }
                        send_binary(&out, chunk);
                    }
                    send_binary(&out, &[]);
                    ok(id, Value::obj().u("bytes", data.len() as u64).done())
                }
                Some(_) => err(id, "Io", "is a directory"),
                None => err(id, "NotFound", "no such path"),
            },
            "Write" => {
                pending_write = Some((id, path, Vec::new()));
                continue;
            }
            "Mkdir" => {
                let (dir, name) = split(&path);
                match lookup(&mut root, &dir) {
                    Some(d) if d.is_dir => match d.children.entry(name) {
                        std::collections::btree_map::Entry::Occupied(_) => err(id, "Exists", "exists"),
                        std::collections::btree_map::Entry::Vacant(v) => {
                            v.insert(Node::dir());
                            ok(id, Value::obj().done())
                        }
                    },
                    _ => err(id, "NotFound", "no such directory"),
                }
            }
            "Rename" => {
                let from = v.str_field("from").unwrap_or("").to_string();
                let to = v.str_field("to").unwrap_or("").to_string();
                let (fd, fname) = split(&from);
                let (td, tname) = split(&to);
                let node = lookup(&mut root, &fd).and_then(|d| d.children.remove(&fname));
                match (node, lookup(&mut root, &td)) {
                    (Some(n), Some(d)) if d.is_dir => {
                        d.children.insert(tname, n);
                        ok(id, Value::obj().done())
                    }
                    _ => err(id, "NotFound", "no such path"),
                }
            }
            "Delete" => {
                let (dir, name) = split(&path);
                match lookup(&mut root, &dir) {
                    Some(d) => match d.children.get(&name) {
                        Some(n) if n.is_dir && !n.children.is_empty() => err(id, "NotEmpty", "directory not empty"),
                        Some(_) => {
                            d.children.remove(&name);
                            ok(id, Value::obj().done())
                        }
                        None => err(id, "NotFound", "no such path"),
                    },
                    None => err(id, "NotFound", "no such path"),
                }
            }
            "SetMtime" | "Chmod" => err(id, "Unsupported", "not supported"),
            _ => err(id, "Unsupported", "unknown request"),
        };
        send(&out, &reply);
    }
}
