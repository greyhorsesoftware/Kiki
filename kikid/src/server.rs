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
    /// Mirror plans served as windowed views: lid -> (job, reason filter)
    plans: HashMap<u64, (u64, String)>,
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
        let mut client = Client { id, tx, listings: HashMap::new(), plans: HashMap::new() };
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
                Value::obj().u("version", proto::PROTOCOL_VERSION).s("daemon", format!("kikid {}", env!("CARGO_PKG_VERSION"))).v("plugins", Value::Arr(crate::plugin::available().into_iter().map(Value::Str).collect())).done(),
            )),
            "Ping" => Ok(Some(Value::obj().done())),
            "Version" => Ok(Some(Value::obj().s("version", env!("CARGO_PKG_VERSION")).done())),
            "Open" => self.open(b),
            "Window" => {
                if let Some(lid) = b.u64_field("lid") {
                    if self.plans.contains_key(&lid) {
                        return self.reply(id, self.plan_window(lid, b.u64_field("first").unwrap_or(0) as usize, b.u64_field("count").unwrap_or(60) as usize));
                    }
                }
                self.window(b)
            }
            "MirrorPlan" => self.mirror_plan(b),
            "MirrorFilter" => self.mirror_filter(b),
            "MirrorCheck" => self.mirror_check(b),
            "MirrorReport" => match b.u64_field("job").and_then(crate::mirror::stored) {
                Some(s) => Ok(Some(Value::obj().s("text", crate::mirror::report(&s.spec, &s.plan.lock().unwrap())).done())),
                None => Err(("NotFound", "no such plan".into())),
            },
            "Filters" => Ok(Some(Value::obj().v("rules", Value::Arr(crate::mirror::load_filters().iter().map(|r| match r { crate::mirror::Rule::Contains(v) => Value::obj().s("kind", "contains").s("value", v.clone()).done(), crate::mirror::Rule::StartsWith(v) => Value::obj().s("kind", "startsWith").s("value", v.clone()).done(), crate::mirror::Rule::EndsWith(v) => Value::obj().s("kind", "endsWith").s("value", v.clone()).done(), crate::mirror::Rule::Matches(v) => Value::obj().s("kind", "matches").s("value", v.clone()).done() }).collect())).done())),
            "SetFilters" => match b.get("rules").and_then(Value::as_arr) {
                Some(rules) => {
                    let mut m = std::collections::BTreeMap::new();
                    m.insert("rule".to_string(), Value::Arr(rules.to_vec()));
                    crate::config::write_named("filters.toml", &Value::Obj(m)).map(|_| Some(Value::obj().done())).map_err(|e| ("Io", e.to_string()))
                }
                None => Err(("Protocol", "missing rules".into())),
            },
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
            "Plugins" => Ok(Some(Value::obj().v("plugins", Value::Arr(crate::plugin::describe_all())).done())),
            "Locations" => Ok(Some(Value::obj().v("locations", crate::locations::json_list()).done())),
            "TestLocation" => match b.get("location") {
                Some(loc) => crate::locations::test(loc, b.get("secrets").unwrap_or(&Value::Null)).map(|_| Some(Value::obj().done())).map_err(vfs_err),
                None => Err(("Protocol", "missing location".into())),
            },
            "AddLocation" | "UpdateLocation" => match b.get("location") {
                Some(loc) => crate::locations::save(loc.clone(), b.get("secrets").unwrap_or(&Value::Null)).map(|_| {
                    let _ = self.tx.send(proto::event("LocationsChanged").done());
                    Some(Value::obj().done())
                }).map_err(vfs_err),
                None => Err(("Protocol", "missing location".into())),
            },
            "RemoveLocation" => match b.str_field("name") {
                Some(n) => {
                    if let Some(l) = crate::locations::find(n) { if let Some(p) = l.str_field("plugin") { crate::listing::invalidate_authority(p, n); } }
                    crate::locations::remove(n).map(|_| { let _ = self.tx.send(proto::event("LocationsChanged").done()); Some(Value::obj().done()) }).map_err(|e| ("Io", e.to_string()))
                }
                None => Err(("Protocol", "missing name".into())),
            },
            "Disconnect" => match b.str_field("name") {
                Some(n) => {
                    if let Some(l) = crate::locations::find(n) { if let Some(p) = l.str_field("plugin") { crate::listing::invalidate_authority(p, n); } }
                    crate::locations::disconnect(n);
                    Ok(Some(Value::obj().done()))
                }
                None => Err(("Protocol", "missing name".into())),
            },
            "Preview" => match parse_uri(b, "uri") {
                Ok(u) => crate::preview::preview(&u).map(Some).map_err(vfs_err),
                Err(e) => Err(e),
            },
            "Thumbnail" => match parse_uri(b, "uri") {
                Ok(u) => {
                    let size = if b.u64_field("size") == Some(256) { crate::thumbs::Size::Large } else { crate::thumbs::Size::Normal };
                    let tx = self.tx.clone();
                    let kind = crate::kinds::Kind::guess(crate::vfs::EntryType::File, u.name().as_bytes());
                    let mtime = std::fs::metadata(u.to_path()).ok().and_then(|m| m.modified().ok()).and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok()).map(|d| d.as_millis() as u64).unwrap_or(0);
                    crate::thumbs::submit(crate::thumbs::ThumbJob { uri: u, kind, mtime_ms: mtime, size, done: Box::new(move |p| {
                        let _ = tx.send(match p {
                            Some(p) => proto::ok(id, Value::obj().s("path", p.to_string_lossy()).done()),
                            None => proto::err(id, "Unsupported", "no thumbnail"),
                        });
                    }) });
                    Ok(None)
                }
                Err(e) => Err(e),
            },
            "Submit" => match b.get("op") {
                Some(op) => crate::jobs::submit(op.clone(), Some(self.tx.clone())).map(|j| Some(Value::obj().u("job", j).done())),
                None => Err(("Protocol", "missing op".into())),
            },
            "Cancel" => match b.u64_field("job") {
                Some(j) if crate::jobs::cancel(j) => Ok(Some(Value::obj().done())),
                Some(_) => Err(("NotFound", "no such job".into())),
                None => Err(("Protocol", "missing job".into())),
            },
            "Jobs" => Ok(Some(Value::obj().v("jobs", crate::jobs::list()).done())),
            "Undo" => crate::jobs::undo(Some(self.tx.clone())).map(|j| Some(Value::obj().u("job", j).done())),
            "Redo" => crate::jobs::redo(Some(self.tx.clone())).map(|j| Some(Value::obj().u("job", j).done())),
            "JobEvents" => {
                crate::jobs::subscribe(self.tx.clone());
                Ok(Some(Value::obj().done()))
            }
            "PromptReply" => match (b.u64_field("job"), b.str_field("choice")) {
                (Some(j), Some(c)) => {
                    let all = b.get("applyToAll").and_then(Value::as_bool).unwrap_or(false);
                    if crate::jobs::prompt_reply(j, c, all) { Ok(Some(Value::obj().done())) } else { Err(("NotFound", "no prompt pending".into())) }
                }
                _ => Err(("Protocol", "missing job or choice".into())),
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

impl Client {
    fn plan_rows(&self, lid: u64) -> Result<(Arc<crate::mirror::Stored>, Vec<usize>), (&'static str, String)> {
        let (job, reason) = self.plans.get(&lid).cloned().ok_or(("NotFound", "no plan view".to_string()))?;
        let stored = crate::mirror::stored(job).ok_or(("NotFound", "no such plan".to_string()))?;
        let idx: Vec<usize> = {
            let p = stored.plan.lock().unwrap();
            p.actions
                .iter()
                .enumerate()
                .filter(|(_, a)| match reason.as_str() {
                    "new" => a.reason == crate::mirror::Reason::New,
                    "changed" => a.reason == crate::mirror::Reason::Changed,
                    "equal" => a.reason == crate::mirror::Reason::Equal,
                    "delete" => matches!(a.kind, crate::mirror::ActionKind::Delete | crate::mirror::ActionKind::Rmdir),
                    _ => true,
                })
                .map(|(i, _)| i)
                .collect()
        };
        Ok((stored, idx))
    }

    fn plan_window(&self, lid: u64, first: usize, count: usize) -> Result<Value, (&'static str, String)> {
        let (stored, idx) = self.plan_rows(lid)?;
        let p = stored.plan.lock().unwrap();
        let rows: Vec<Value> = idx.iter().skip(first).take(count.min(512)).map(|&i| crate::mirror::action_json(&p.actions[i])).collect();
        Ok(Value::obj().u("first", first as u64).u("n", idx.len() as u64).b("done", true).v("rows", Value::Arr(rows)).done())
    }

    fn mirror_plan(&mut self, b: &Value) -> Result<Option<Value>, (&'static str, String)> {
        let job = b.u64_field("job").ok_or(("Protocol", "missing job".to_string()))?;
        let lid = b.u64_field("lid").ok_or(("Protocol", "missing lid".to_string()))?;
        let stored = crate::mirror::stored(job).ok_or(("NotFound", "no such plan".to_string()))?;
        self.plans.insert(lid, (job, "all".into()));
        let (counts, offset, n) = {
            let p = stored.plan.lock().unwrap();
            (p.counts_json(), p.clock_offset_ms, p.actions.len() as u64)
        };
        let _ = self.tx.send(proto::event("Count").u("lid", lid).u("n", n).b("done", true).done());
        Ok(Some(Value::obj().v("counts", counts).i("clockOffsetMs", offset).u("n", n).done()))
    }

    fn mirror_filter(&mut self, b: &Value) -> Result<Option<Value>, (&'static str, String)> {
        let lid = b.u64_field("lid").ok_or(("Protocol", "missing lid".to_string()))?;
        let reason = b.str_field("reason").unwrap_or("all").to_string();
        let entry = self.plans.get_mut(&lid).ok_or(("NotFound", "no plan view".to_string()))?;
        entry.1 = reason;
        let (_, idx) = self.plan_rows(lid)?;
        let _ = self.tx.send(proto::event("Reset").u("lid", lid).u("n", idx.len() as u64).done());
        Ok(Some(Value::obj().u("n", idx.len() as u64).done()))
    }

    fn mirror_check(&mut self, b: &Value) -> Result<Option<Value>, (&'static str, String)> {
        let lid = b.u64_field("lid").ok_or(("Protocol", "missing lid".to_string()))?;
        let first = b.u64_field("first").unwrap_or(0) as usize;
        let count = b.u64_field("count").unwrap_or(1) as usize;
        let checked = b.get("checked").and_then(Value::as_bool).unwrap_or(true);
        let (stored, idx) = self.plan_rows(lid)?;
        let mut p = stored.plan.lock().unwrap();
        for &i in idx.iter().skip(first).take(count) {
            if p.actions[i].kind != crate::mirror::ActionKind::Skip {
                p.actions[i].checked = checked;
            }
        }
        let counts = p.counts_json();
        Ok(Some(Value::obj().v("counts", counts).done()))
    }
}

fn parse_uri(b: &Value, key: &str) -> Result<Uri, (&'static str, String)> {
    let s = b.str_field(key).ok_or(("Protocol", format!("missing {key}")))?;
    Uri::parse(s).map_err(|e| ("Protocol", format!("bad uri: {}", e.0)))
}

fn vfs_err(e: VfsError) -> (&'static str, String) {
    (e.code(), e.message())
}
