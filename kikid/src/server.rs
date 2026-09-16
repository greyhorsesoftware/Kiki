//! The Unix socket server: one thread per client for reading, one for writing.

use crate::json::Value;
use crate::listing::{self, Listing, SortRole, Subscriber};
use crate::proto::{self, Frame, Framing, Reader, Request};
use crate::vfs::uri::Uri;
use crate::vfs::VfsError;
use std::collections::HashMap;
use std::io::Write;
use std::os::unix::net::{UnixListener, UnixStream};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{self, Sender};
use std::sync::Arc;
use std::thread;

static NEXT_CLIENT: AtomicU64 = AtomicU64::new(1);

pub fn serve(listener: UnixListener) -> std::io::Result<()> {
    for stream in listener.incoming() {
        match stream {
            Ok(s) => {
                let id = NEXT_CLIENT.fetch_add(1, Ordering::Relaxed);
                thread::Builder::new().name(format!("client-{id}")).spawn(move || Client::run(id, s)).expect("spawn client");
            }
            Err(e) => eprintln!("accept: {e}"),
        }
    }
    Ok(())
}

struct Client {
    id: u64,
    tx: Sender<Value>,
    listings: HashMap<u64, Arc<Listing>>,
}

impl Client {
    fn run(id: u64, stream: UnixStream) {
        let reader_stream = match stream.try_clone() {
            Ok(s) => s,
            Err(e) => {
                eprintln!("client {id}: clone: {e}");
                return;
            }
        };
        let mut reader = Reader::new(reader_stream);
        // The writer needs the framing, which is known after the first frame.
        let first = match reader.next() {
            Ok(Some(f)) => f,
            Ok(None) => return,
            Err(e) => {
                eprintln!("client {id}: {e}");
                return;
            }
        };
        let framing = reader.framing().unwrap_or(Framing::Binary);
        let (tx, rx) = mpsc::channel::<Value>();
        let mut writer = stream;
        thread::Builder::new()
            .name(format!("writer-{id}"))
            .spawn(move || {
                while let Ok(v) = rx.recv() {
                    if proto::write_json(&mut writer, framing, &v).is_err() {
                        break;
                    }
                    if writer.flush().is_err() {
                        break;
                    }
                }
            })
            .expect("spawn writer");
        let mut client = Client { id, tx, listings: HashMap::new() };
        client.handle_frame(first);
        loop {
            match reader.next() {
                Ok(Some(f)) => client.handle_frame(f),
                Ok(None) => break,
                Err(e) => {
                    let _ = client.tx.send(proto::err(0, "Protocol", e.to_string()));
                    break;
                }
            }
        }
        for (lid, l) in client.listings.drain() {
            l.unsubscribe(id, lid);
        }
    }

    fn handle_frame(&mut self, f: Frame) {
        match f {
            Frame::Binary(_) => {
                let _ = self.tx.send(proto::err(0, "Protocol", "unexpected binary frame"));
            }
            Frame::Json(v) => match proto::parse_request(v) {
                Ok(req) => self.handle(req),
                Err(m) => {
                    let _ = self.tx.send(proto::err(0, "Protocol", m));
                }
            },
        }
    }

    fn reply(&self, id: u64, r: Result<Value, (&str, String)>) {
        let v = match r {
            Ok(v) => proto::ok(id, v),
            Err((code, msg)) => proto::err(id, code, msg),
        };
        let _ = self.tx.send(v);
    }

    fn handle(&mut self, req: Request) {
        let id = req.id;
        let b = &req.body;
        let result: Result<Option<Value>, (&str, String)> = match req.kind.as_str() {
            "Hello" => Ok(Some(
                Value::obj().u("version", proto::PROTOCOL_VERSION).s("daemon", format!("kikid {}", env!("CARGO_PKG_VERSION"))).v("plugins", Value::Arr(vec![])).done(),
            )),
            "Ping" => Ok(Some(Value::obj().done())),
            "Version" => Ok(Some(Value::obj().s("version", env!("CARGO_PKG_VERSION")).done())),
            "Open" => self.open(b),
            "Window" => self.window(b),
            "Sort" => self.sort(b, id),
            "Filter" => self.filter(b),
            "Enrich" => self.enrich(b, id),
            "Close" => self.close(b),
            "Refresh" => self.refresh(b),
            "Prefetch" => match parse_uri(b, "uri") {
                Ok(u) => listing::open(&u).map(|_| Some(Value::obj().done())).map_err(vfs_err),
                Err(e) => Err(e),
            },
            "Favorites" => Ok(Some(Value::obj().v("items", crate::config::favorites()).done())),
            "SetFavorites" => match b.get("items").and_then(Value::as_arr) {
                Some(items) => crate::config::set_favorites(items).map(|_| Some(Value::obj().done())).map_err(|e| ("Io", e.to_string())),
                None => Err(("Protocol", "missing items".into())),
            },
            "Volumes" => Ok(Some(Value::obj().v("items", crate::config::volumes()).done())),
            "Settings" => Ok(Some(crate::config::settings())),
            "SetSettings" => match b.get("patch") {
                Some(p) => crate::config::set_settings(p).map(|_| Some(Value::obj().done())).map_err(|e| ("Io", e.to_string())),
                None => Err(("Protocol", "missing patch".into())),
            },
            "Stat" => match parse_uri(b, "uri") {
                Ok(u) => Listing::stat_uri(&u).map(Some).map_err(vfs_err),
                Err(e) => Err(e),
            },
            other => Err(("Unsupported", format!("unknown request type {other}"))),
        };
        match result {
            Ok(Some(v)) => self.reply(id, Ok(v)),
            Ok(None) => {} // the reply is sent later by whoever holds the waiter
            Err(e) => self.reply(id, Err(e)),
        }
    }

    fn lid(&self, b: &Value) -> Result<(u64, Arc<Listing>), (&'static str, String)> {
        let lid = b.u64_field("lid").ok_or(("Protocol", "missing lid".to_string()))?;
        let l = self.listings.get(&lid).cloned().ok_or(("NotFound", format!("no listing {lid}")))?;
        Ok((lid, l))
    }

    fn open(&mut self, b: &Value) -> Result<Option<Value>, (&'static str, String)> {
        let lid = b.u64_field("lid").ok_or(("Protocol", "missing lid".to_string()))?;
        let uri = parse_uri(b, "uri")?;
        let (l, cached) = listing::open(&uri).map_err(vfs_err)?;
        if let Some(old) = self.listings.insert(lid, Arc::clone(&l)) {
            old.unsubscribe(self.id, lid);
        }
        l.subscribe(Subscriber { client: self.id, lid, tx: self.tx.clone(), first: 0, count: 0 });
        let (n, done) = l.count();
        // The first Count arrives with the reply so a client can size its model immediately.
        let _ = self.tx.send(proto::event("Count").u("lid", lid).u("n", n).b("done", done).done());
        if let Some(e) = l.error() {
            return Err(("Io", e));
        }
        Ok(Some(Value::obj().b("cached", cached).done()))
    }

    fn window(&mut self, b: &Value) -> Result<Option<Value>, (&'static str, String)> {
        let (lid, l) = self.lid(b)?;
        let first = b.u64_field("first").unwrap_or(0) as u32;
        let count = b.u64_field("count").unwrap_or(60) as u32;
        Ok(Some(l.window(self.id, lid, first, count)))
    }

    fn sort(&mut self, b: &Value, id: u64) -> Result<Option<Value>, (&'static str, String)> {
        let (_, l) = self.lid(b)?;
        let role = SortRole::parse(b.str_field("role").unwrap_or("name")).ok_or(("Protocol", "bad role".to_string()))?;
        let asc = b.str_field("order").unwrap_or("asc") != "desc";
        l.sort(role, asc, Some((self.tx.clone(), id)));
        Ok(None)
    }

    fn filter(&mut self, b: &Value) -> Result<Option<Value>, (&'static str, String)> {
        let (_, l) = self.lid(b)?;
        let n = l.filter(b.str_field("text").unwrap_or(""));
        Ok(Some(Value::obj().u("n", n).done()))
    }

    fn enrich(&mut self, b: &Value, id: u64) -> Result<Option<Value>, (&'static str, String)> {
        let (_, l) = self.lid(b)?;
        l.enrich(Some((self.tx.clone(), id)));
        Ok(None)
    }

    fn close(&mut self, b: &Value) -> Result<Option<Value>, (&'static str, String)> {
        let lid = b.u64_field("lid").ok_or(("Protocol", "missing lid".to_string()))?;
        if let Some(l) = self.listings.remove(&lid) {
            l.unsubscribe(self.id, lid);
        }
        Ok(Some(Value::obj().done()))
    }

    fn refresh(&mut self, b: &Value) -> Result<Option<Value>, (&'static str, String)> {
        let (_, l) = self.lid(b)?;
        l.rescan();
        Ok(Some(Value::obj().done()))
    }
}

fn parse_uri(b: &Value, key: &str) -> Result<Uri, (&'static str, String)> {
    let s = b.str_field(key).ok_or(("Protocol", format!("missing {key}")))?;
    Uri::parse(s).map_err(|e| ("Protocol", format!("bad uri: {}", e.0)))
}

fn vfs_err(e: VfsError) -> (&'static str, String) {
    (e.code(), e.message())
}
