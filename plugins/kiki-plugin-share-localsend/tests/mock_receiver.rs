//! Drives the LocalSend plugin binary against an in-process mock receiver speaking protocol v2
//! over plain HTTP (the announce's `protocol: http` variant), including the PIN handshake.

use kiki_plugin_sdk::json::{self, Value};
use kiki_plugin_sdk::{read_frame, write_json};
use std::collections::HashMap;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpListener;
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex};

#[derive(Default)]
struct Received {
    prepare: Option<Value>,
    pin_seen: Option<String>,
    uploads: HashMap<String, Vec<u8>>,
    cancelled: bool,
}

fn start(require_pin: Option<&'static str>, accept: &'static [&'static str]) -> (u16, Arc<Mutex<Received>>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let state = Arc::new(Mutex::new(Received::default()));
    let st = state.clone();
    std::thread::spawn(move || {
        for conn in listener.incoming() {
            let Ok(mut c) = conn else { break };
            let mut r = BufReader::new(c.try_clone().unwrap());
            let mut line = String::new();
            if r.read_line(&mut line).unwrap() == 0 {
                continue;
            }
            let mut parts = line.split_whitespace();
            let (method, target) = (parts.next().unwrap_or("").to_string(), parts.next().unwrap_or("").to_string());
            let mut len = 0usize;
            loop {
                line.clear();
                r.read_line(&mut line).unwrap();
                let l = line.trim_end();
                if l.is_empty() {
                    break;
                }
                if let Some(v) = l.to_ascii_lowercase().strip_prefix("content-length:") {
                    len = v.trim().parse().unwrap();
                }
            }
            let mut body = vec![0u8; len];
            r.read_exact(&mut body).unwrap();
            let (path, query) = target.split_once('?').unwrap_or((&target, ""));
            let q: HashMap<String, String> = query.split('&').filter(|s| !s.is_empty()).filter_map(|kv| kv.split_once('=')).map(|(k, v)| (k.to_string(), v.to_string())).collect();
            let mut s = st.lock().unwrap();
            let (status, reply) = match (method.as_str(), path) {
                ("POST", "/api/localsend/v2/prepare-upload") => {
                    s.pin_seen = q.get("pin").cloned();
                    if let Some(p) = require_pin {
                        if q.get("pin").map(String::as_str) != Some(p) {
                            (401, String::new())
                        } else {
                            let v = json::parse(&body).unwrap();
                            s.prepare = Some(v.clone());
                            let mut files = Value::obj();
                            for id in accept {
                                files = files.s(id, format!("tok-{id}"));
                            }
                            (200, json::to_string(&Value::obj().s("sessionId", "s1").v("files", files.done()).done()))
                        }
                    } else {
                        let v = json::parse(&body).unwrap();
                        s.prepare = Some(v.clone());
                        let mut files = Value::obj();
                        for id in accept {
                            files = files.s(id, format!("tok-{id}"));
                        }
                        (200, json::to_string(&Value::obj().s("sessionId", "s1").v("files", files.done()).done()))
                    }
                }
                ("POST", "/api/localsend/v2/upload") => {
                    let ok = q.get("sessionId").map(String::as_str) == Some("s1") && q.get("token").map(String::as_str) == Some(format!("tok-{}", q.get("fileId").cloned().unwrap_or_default()).as_str());
                    if ok {
                        s.uploads.insert(q["fileId"].clone(), body.clone());
                        (200, String::new())
                    } else {
                        (403, String::new())
                    }
                }
                ("POST", "/api/localsend/v2/cancel") => {
                    s.cancelled = true;
                    (200, String::new())
                }
                _ => (404, String::new()),
            };
            drop(s);
            let _ = c.write_all(format!("HTTP/1.1 {status} X\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{reply}", reply.len()).as_bytes());
            let _ = c.flush();
        }
    });
    (port, state)
}

fn share(port: u16, pin: &str, names: &[(&str, &[u8])]) -> (Value, Vec<Value>) {
    let dir = std::env::temp_dir().join(format!("kiki-localsend-{}-{port}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let mut uris = Vec::new();
    for (n, b) in names {
        let f = dir.join(n);
        std::fs::write(&f, b).unwrap();
        uris.push(Value::Str(format!("file://{}", f.to_string_lossy().replace(' ', "%20"))));
    }
    let mut child = Command::new(env!("CARGO_BIN_EXE_kiki-plugin-share-localsend")).stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::inherit()).spawn().unwrap();
    let mut stdin = child.stdin.take().unwrap();
    let mut stdout = BufReader::new(child.stdout.take().unwrap());
    let req = Value::obj()
        .u("id", 1)
        .s("type", "Share")
        .v("config", Value::obj().done())
        .v("secrets", Value::obj().done())
        .v("uris", Value::Arr(uris))
        .s("target", format!("http://127.0.0.1:{port}"))
        .v("compose", Value::obj().s("pin", pin).done())
        .done();
    write_json(&mut stdin, &req).unwrap();
    let mut reply = Value::Null;
    let mut events = Vec::new();
    while let Some((_, payload)) = read_frame(&mut stdout).unwrap() {
        let v = json::parse(&payload).unwrap();
        if v.get("ok").is_some() || v.get("err").is_some() {
            reply = v;
            break;
        }
        events.push(v);
    }
    let _ = write_json(&mut stdin, &Value::obj().u("id", 2).s("type", "Shutdown").done());
    let _ = child.wait();
    let _ = std::fs::remove_dir_all(&dir);
    (reply, events)
}

#[test]
fn sends_two_files_with_progress() {
    let (port, state) = start(None, &["f0", "f1"]);
    let big = vec![7u8; 700_000];
    let (r, events) = share(port, "", &[("photo one.jpg", &big), ("notes.md", b"# hi")]);
    assert_eq!(r.get("ok").map(|o| o.str_field("result")), Some(Some("sent")), "{}", json::to_string(&r));
    let s = state.lock().unwrap();
    let prep = s.prepare.as_ref().unwrap();
    assert_eq!(prep.get("info").unwrap().str_field("alias"), Some("kiki"));
    let files = prep.get("files").unwrap();
    assert_eq!(files.get("f0").unwrap().str_field("fileName"), Some("photo one.jpg"));
    assert_eq!(files.get("f0").unwrap().u64_field("size"), Some(700_000));
    assert_eq!(files.get("f0").unwrap().str_field("fileType"), Some("image/jpeg"));
    assert_eq!(s.uploads.get("f0").map(Vec::len), Some(700_000));
    assert_eq!(s.uploads.get("f1").map(|v| v.as_slice()), Some(&b"# hi"[..]));
    let last = events.last().unwrap();
    assert_eq!(last.u64_field("done"), Some(2));
    assert_eq!(last.u64_field("bytes"), Some(700_004));
    assert!(events.iter().any(|e| e.str_field("status").map(|s| s.starts_with("sending photo one.jpg")).unwrap_or(false)), "per-file progress");
}

#[test]
fn pin_handshake_and_partial_acceptance() {
    let (port, state) = start(Some("4242"), &["f1"]);
    let (r, _) = share(port, "", &[("a.txt", b"a"), ("b.txt", b"b")]);
    let e = r.get("err").expect("401 without a pin is an error");
    assert_eq!((e.str_field("code"), e.str_field("field")), (Some("Invalid"), Some("pin")));
    let (r, _) = share(port, "4242", &[("a.txt", b"a"), ("b.txt", b"b")]);
    assert_eq!(r.get("ok").map(|o| o.str_field("result")), Some(Some("sent")), "{}", json::to_string(&r));
    assert!(r.get("ok").unwrap().str_field("detail").unwrap().contains("1 file(s) not accepted"));
    let s = state.lock().unwrap();
    assert_eq!(s.pin_seen.as_deref(), Some("4242"));
    assert!(s.uploads.contains_key("f1") && !s.uploads.contains_key("f0"));
}
