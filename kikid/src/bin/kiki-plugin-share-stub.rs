//! A share plugin over a log file, speaking the share half of docs/0.1.0/API-PLUGIN.md. It sends
//! nothing anywhere: it writes down what the daemon handed it, which is what `share::run` has to
//! be tested against — the files it fetched, zipped and passed on.
//!
//! Built only under the `stub` feature, which is the daemon's own test build (see Cargo.toml); a
//! shipped kiki has no such plugin.
//!
//! Like the gio plugin, it takes its **id from the name it is run by** — `kiki-plugin-share-x` is
//! the share plugin `x` — so one binary put down under several names is several plugins, which is
//! how the cases that differ only in a `Describe` are covered without a second source file:
//! `folders` takes folders as they are, `needy` claims to need a program that is not installed,
//! and anything else is the plain one. `KIKI_SHARE_STUB_LOG` is where it writes what it was given.

use kikid::json::{self, Value};
use std::io::{self, Read, Write};

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

fn ok(id: u64, v: Value) -> Value {
    Value::obj().u("id", id).v("ok", v).done()
}

fn err(id: u64, code: &str, msg: &str) -> Value {
    Value::obj().u("id", id).v("err", Value::obj().s("code", code).s("message", msg).done()).done()
}

/// The name this copy was run by, after `kiki-plugin-share-`.
fn my_id() -> String {
    std::env::args().next().unwrap_or_default().rsplit('/').next().unwrap_or("").rsplit("kiki-plugin-share-").next().unwrap_or("stub").to_string()
}

/// One line per request worth remembering, so a test can say what the daemon did before the
/// plugin was reached. Every copy writes to the one file and says which of them it is.
fn note(line: Value) {
    let Value::Obj(mut m) = line else { return };
    m.insert("plugin".into(), Value::Str(my_id()));
    if let Ok(p) = std::env::var("KIKI_SHARE_STUB_LOG") {
        if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(p) {
            let _ = writeln!(f, "{}", json::to_string(&Value::Obj(m)));
        }
    }
}

fn field(key: &str, label: &str, kind: &str) -> Value {
    Value::obj().s("key", key).s("label", label).s("kind", kind).b("required", false).v("default", Value::Null).v("options", Value::Null).v("group", Value::Null).done()
}

fn main() {
    let me = my_id();
    let mut stdin = io::stdin().lock();
    let mut stdout = io::stdout().lock();
    while let Ok(Some((kind, payload))) = read_frame(&mut stdin) {
        if kind != 0 {
            continue;
        }
        let Ok(v) = json::parse(&payload) else { continue };
        let id = v.u64_field("id").unwrap_or(0);
        let reply = match v.str_field("type").unwrap_or("") {
            "Describe" => ok(
                id,
                Value::obj()
                    .s("kind", "share")
                    .s("id", me.clone())
                    .s("name", "Stub")
                    .s("icon", "share")
                    .s("version", "1.0")
                    .v("accepts", Value::obj().b("files", true).b("folders", me == "folders").b("multiple", true).v("maxBytes", Value::Null).done())
                    .s("targets", "list")
                    .v("form", Value::Arr(vec![field("note", "Note", "text")]))
                    .v("secretFields", Value::Arr(vec![Value::Str("token".into())]))
                    .v("compose", Value::Arr(vec![field("subject", "Subject", "text")]))
                    .v("requires",Value::Arr(if me == "needy" { vec![Value::Str("definitely-not-a-program".into())] } else { vec![] }))
                    .done(),
            ),
            "Ping" | "Cancel" => ok(id, Value::obj().done()),
            "Shutdown" => {
                let _ = write_json(&mut stdout, &ok(id, Value::obj().done()));
                return;
            }
            "Configure" => {
                note(Value::obj().s("call", "Configure").v("config", v.get("config").cloned().unwrap_or(Value::Null)).v("secrets", v.get("secrets").cloned().unwrap_or(Value::Null)).done());
                ok(id, Value::obj().done())
            }
            "Targets" => {
                let query = v.str_field("query").unwrap_or("");
                let all = [("near", "Near phone", true), ("far", "Far laptop", false)];
                let targets: Vec<Value> = all
                    .iter()
                    .filter(|(_, name, _)| query.is_empty() || name.to_lowercase().contains(&query.to_lowercase()))
                    .map(|(tid, name, online)| Value::obj().s("id", *tid).s("name", *name).s("detail", "stub").b("online", *online).s("icon", "server").done())
                    .collect();
                ok(id, Value::obj().v("targets", Value::Arr(targets)).done())
            }
            "Share" => {
                let uris = v.get("uris").cloned().unwrap_or(Value::Null);
                let n = uris.as_arr().map(<[Value]>::len).unwrap_or(0) as u64;
                note(
                    Value::obj()
                        .s("call", "Share")
                        .v("uris", uris)
                        .opt_s("target", v.str_field("target"))
                        .v("compose", v.get("compose").cloned().unwrap_or(Value::Null))
                        .v("secrets", v.get("secrets").cloned().unwrap_or(Value::Null))
                        .done(),
                );
                if v.str_field("target") == Some("fail") {
                    err(id, "Network", "the stub was told to fail")
                } else {
                    // Progress, as a real one reports it, so the job's bar is driven by the plugin.
                    for done in 1..=n {
                        let _ = write_json(&mut stdout, &Value::obj().u("id", id).s("event", "Progress").u("done", done).u("total", n).u("bytes", done * 100).u("bytesTotal", n * 100).s("status", "sending").done());
                    }
                    ok(id, Value::obj().s("result", "sent").s("detail", format!("{n} to {}", v.str_field("target").unwrap_or("nobody"))).done())
                }
            }
            _ => err(id, "Unsupported", "unknown request"),
        };
        let _ = write_json(&mut stdout, &reply);
    }
}
