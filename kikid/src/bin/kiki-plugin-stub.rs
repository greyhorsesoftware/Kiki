//! A location plugin over an in-memory tree, speaking docs/0.1.0/API-PLUGIN.md.
//! It has a two-field form (name, greeting) and a `stub://` scheme. Used by the
//! daemon's plugin contract tests and as the smallest example of a plugin.

use kikid::json::{self, Value};
use std::collections::BTreeMap;
use std::io::{self, Read, Write};

struct Node {
    is_dir: bool,
    data: Vec<u8>,
    mtime: u64,
    children: BTreeMap<String, Node>,
}

impl Node {
    fn dir() -> Node {
        Node { is_dir: true, data: Vec::new(), mtime: 1_700_000_000_000, children: BTreeMap::new() }
    }
    fn file(data: &[u8]) -> Node {
        Node { is_dir: false, data: data.to_vec(), mtime: 1_700_000_000_000, children: BTreeMap::new() }
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
    // Read slowly, so a test can have a second transfer begin while this one is still streaming.
    root.children.insert("slow.bin".into(), Node::file(&[7u8; 3072]));
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

fn ok(id: u64, v: Value) -> Value {
    Value::obj().u("id", id).v("ok", v).done()
}
fn err(id: u64, code: &str, msg: &str) -> Value {
    Value::obj().u("id", id).v("err", Value::obj().s("code", code).s("message", msg).done()).done()
}

fn meta(n: &Node) -> Value {
    Value::obj().u("size", n.data.len() as u64).u("mtime", n.mtime).v("mode", Value::Null).v("owner", Value::Null).v("group", Value::Null).v("digest", Value::Null).done()
}

fn main() {
    let mut root = seed();
    let mut stdin = io::stdin().lock();
    let mut stdout = io::stdout().lock();
    let mut connected: BTreeMap<String, bool> = BTreeMap::new();
    let mut pending_write: Option<(u64, String, Vec<u8>)> = None;
    while let Ok(Some((kind, payload))) = read_frame(&mut stdin) {
        if kind == 1 {
            if let Some((id, path, buf)) = pending_write.as_mut() {
                if payload.is_empty() {
                    let (dir, name) = split(path);
                    let bytes = buf.len() as u64;
                    let reply = match lookup(&mut root, &dir) {
                        Some(d) if d.is_dir => {
                            d.children.insert(name, Node::file(buf));
                            ok(*id, Value::obj().u("bytes", bytes).done())
                        }
                        _ => err(*id, "NotFound", "no such directory"),
                    };
                    let _ = write_json(&mut stdout, &reply);
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
                        ]),
                    )
                    .v("defaults", Value::obj().s("greeting", "hi").done())
                    .v("secretFields", Value::Arr(vec![]))
                    .v("detector", Value::obj().s("upload", "sizeMtime").s("download", "sizeMtime").done())
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
            "Connect" => {
                connected.insert(loc.clone(), true);
                // A server that identifies itself with a key says so here; the stub does it when
                // its config asks, which is how the contract test drives kiki's verify flow.
                let fp = v.get("config").and_then(|c| c.str_field("fingerprint")).map(str::to_string);
                ok(id, Value::obj().opt_s("fingerprint", fp.as_deref()).v("banner", Value::Null).done())
            }
            "Disconnect" => {
                connected.remove(&loc);
                ok(id, Value::obj().done())
            }
            "Capabilities" => {
                ok(id, Value::obj().b("trash", false).b("setMtime", false).b("mode", false).b("realDirs", true).v("digestKind", Value::Null).s("separator", "/").v("fastScan", Value::Null).b("partialRead", true).done())
            }
            "Ping" | "Cancel" => ok(id, Value::obj().done()),
            "Shutdown" => {
                let _ = write_json(&mut stdout, &ok(id, Value::obj().done()));
                return;
            }
            // API-PLUGIN: a plugin that cannot walk a whole tree in one request says so, and the
            // daemon lists directory by directory. Answering the top level and calling it
            // recursive would leave every mirror re-copying the files it never saw.
            "Scan" if v.get("recursive").and_then(Value::as_bool).unwrap_or(false) => err(id, "Unsupported", "the stub lists one directory at a time"),
            "Scan" => match lookup(&mut root, &path) {
                Some(d) if d.is_dir => {
                    let entries: Vec<Value> = d.children.iter().map(|(n, c)| Value::obj().s("name", n.clone()).s("kind", if c.is_dir { "dir" } else { "file" }).v("meta", meta(c)).done()).collect();
                    let n = entries.len() as u64;
                    let _ = write_json(&mut stdout, &Value::obj().u("id", id).v("entries", Value::Arr(entries)).done());
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
                        let _ = write_binary(&mut stdout, chunk);
                    }
                    let _ = write_binary(&mut stdout, &[]);
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
        let _ = write_json(&mut stdout, &reply);
    }
}
