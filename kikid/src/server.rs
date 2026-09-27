//! The Unix socket server: one thread per client for reading, one for writing.
//!
//! This file is the connection and the wire — `Hello`, the reply and its number, the one way
//! slow work answers (`later`) — and nothing about any particular verb: those are in
//! `handlers/`, by area, and `handle` asks them in turn (docs/0.3.0/01-daemon-on-demand.md,
//! decision 6). A connection answers its requests one after another, in the order they arrive;
//! `handlers/mod.rs` says why, and what a handler that would be slow does about it.

use crate::handlers;
use crate::json::Value;
use crate::listing::Listing;
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

/// One connection, and everything it holds: what the handlers see as `Cx`.
pub struct Client {
    pub(crate) id: u64,
    pub(crate) tx: Sender<Value>,
    /// A kiki window, by its own Hello: its jobs stop when the last one has gone.
    pub(crate) shell: bool,
    pub(crate) listings: HashMap<u64, Arc<Listing>>,
    /// Mirror plans served as windowed views: lid -> (job, reason filter)
    pub(crate) plans: HashMap<u64, (u64, String)>,
    /// Search results served as windowed views: lid -> rows
    pub(crate) searches: HashMap<u64, Vec<Value>>,
    pub(crate) trees: HashMap<u64, crate::tree::Tree>,
    /// The id of the request being handled, for a handler whose reply is sent later.
    pub(crate) req_id: u64,
}

/// What a handler is given: the connection. The name says what it is to a handler — context —
/// rather than what it is to the server.
pub type Cx = Client;

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
        let mut client = Client { id, tx, shell: false, listings: HashMap::new(), plans: HashMap::new(), searches: HashMap::new(), trees: HashMap::new(), req_id: 0 };
        // Counted from the first frame to the last: the daemon is the window's engine and leaves
        // a moment after the last one (docs/0.3.0/01-daemon-on-demand.md).
        crate::lifetime::came();
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
        if client.shell {
            crate::jobs::shell_went();
        }
        // A window that went away was showing these: nobody will close them now.
        for (_, (job, _)) in client.plans.drain() {
            crate::mirror::unview(job);
        }
        // Last of all, and after the jobs and the views: the count that may end the process.
        crate::lifetime::went();
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
        let said = SAID.with(|s| s.borrow_mut().take());
        let _ = self.tx.send(answer(id, r, said));
    }

    /// Slow work, answered from a thread of its own: the request's thread returns at once with
    /// `Ok(None)`, `f` runs on a named thread, and the reply for THIS request goes out from there
    /// — with its number, if `f` said one: `vfs_err` leaves the number on the thread that called
    /// it, so it is read there, after `f`, and not on the connection's thread where there is
    /// nothing to read. This is the one way a handler that can take longer than a listing
    /// answers (`handlers/mod.rs`); nothing else spawns a thread to reply.
    pub(crate) fn later(&self, name: &str, f: impl FnOnce() -> Result<Value, (&'static str, String)> + Send + 'static) -> handlers::Reply {
        let tx = self.tx.clone();
        let id = self.req_id;
        thread::Builder::new()
            .name(name.into())
            .spawn(move || {
                let r = f();
                let said = SAID.with(|s| s.borrow_mut().take());
                let _ = tx.send(answer(id, r, said));
            })
            .expect("spawn later");
        Ok(None)
    }

    /// For work that has a thread of its own already (the thumber's queue) and calls back when
    /// it is done: what to call with the result, and the reply for THIS request is built and
    /// sent the one way replies are. A number said on the calling thread goes with it.
    pub(crate) fn answer_later(&self) -> impl FnOnce(Result<Value, (&'static str, String)>) + Send + 'static {
        let tx = self.tx.clone();
        let id = self.req_id;
        move |r| {
            let said = SAID.with(|s| s.borrow_mut().take());
            let _ = tx.send(answer(id, r, said));
        }
    }

    /// The answer to anything that starts a job — and the job is this window's, if a window asked.
    pub(crate) fn started(&self, r: Result<u64, (&'static str, String)>) -> Result<Option<Value>, (&'static str, String)> {
        r.map(|j| {
            if self.shell {
                crate::jobs::own(j);
            }
            Some(Value::obj().u("job", j).done())
        })
    }

    fn handle(&mut self, req: Request) {
        SAID.with(|s| *s.borrow_mut() = None);
        self.req_id = req.id;
        static TRACE: std::sync::LazyLock<bool> = std::sync::LazyLock::new(|| std::env::var_os("KIKI_TRACE").is_some_and(|v| v == "1"));
        let id = req.id;
        let b = &req.body;
        // `KIKI_TRACE=1`: every request, by client, on stderr — for the stray request nobody can
        // otherwise place (a remote file listed as a folder at a daemon start, 2026-09-24).
        if *TRACE {
            let t = crate::json::to_string(b);
            eprintln!("[req] client {} {} {}", self.id, req.kind, &t[..t.len().min(240)]);
        }
        let result: Result<Option<Value>, (&str, String)> = match req.kind.as_str() {
            "Hello" => {
                // A client built for another protocol is told so, in words it can show, instead of
                // being answered as if all were well and failing later on a request that has
                // changed shape. One that names no version is taken at its word (scripts, tests).
                if let Some(theirs) = b.u64_field("version") {
                    if theirs != proto::PROTOCOL_VERSION {
                        return self
                            .reply(id, Err(("Version", format!("this kikid speaks protocol {}, the client {theirs}: restart kiki after an upgrade (systemctl --user restart kikid.service)", proto::PROTOCOL_VERSION))));
                    }
                }
                // A window, saying so once: its jobs stop when the last window has gone.
                if b.str_field("client") == Some("kiki") && !self.shell {
                    self.shell = true;
                    crate::jobs::shell_came();
                }
                Ok(Some(
                    Value::obj()
                        .u("version", proto::PROTOCOL_VERSION)
                        .s("daemon", format!("kikid {}", env!("CARGO_PKG_VERSION")))
                        // The build itself, for the window to compare with its own: a window and
                        // a daemon are a pair, and one from another version is refused.
                        .s("kikid", env!("CARGO_PKG_VERSION"))
                        .v("plugins", Value::Arr(crate::plugin::available().into_iter().map(Value::Str).collect()))
                        .done(),
                ))
            }
            // Every other verb belongs to an area in `handlers/`: the first to say it is theirs
            // answers it, and one nobody claims is refused in the words it always was.
            other => handlers::ALL.iter().find_map(|h| h(self, other, b)).unwrap_or_else(|| Err(("Unsupported", format!("unknown request type {other}")))),
        };
        match result {
            Ok(Some(v)) => self.reply(id, Ok(v)),
            Ok(None) => {} // the reply is sent later by whoever holds the waiter
            Err(e) => self.reply(id, Err(e)),
        }
    }

    pub(crate) fn lid(&self, b: &Value) -> Result<(u64, Arc<Listing>), (&'static str, String)> {
        let lid = b.u64_field("lid").ok_or(("Protocol", "missing lid".to_string()))?;
        let l = self.listings.get(&lid).cloned().ok_or(("NotFound", format!("no listing {lid}")))?;
        Ok((lid, l))
    }
}

/// A reply on the wire: the value, or the error with its number beside it when one was said.
fn answer(id: u64, r: Result<Value, (&str, String)>, said: Option<(u16, Value)>) -> Value {
    match (r, said) {
        (Ok(v), _) => proto::ok(id, v),
        (Err((code, msg)), Some((n, params))) => proto::err_said(id, code, msg, n, params),
        (Err((code, msg)), None) => proto::err(id, code, msg),
    }
}

pub(crate) fn parse_uri(b: &Value, key: &str) -> Result<Uri, (&'static str, String)> {
    let s = b.str_field(key).ok_or(("Protocol", format!("missing {key}")))?;
    Uri::parse(s).map_err(|e| ("Protocol", format!("bad uri: {}", e.0)))
}

// A numbered error keeps its number across the `(code, message)` tuple every handler returns:
// `vfs_err` leaves it here, on the thread that called it, and the reply — the next thing that
// runs on that thread: `reply`, or `later`'s and `answer_later`'s send — picks it up and puts
// `n` and `params` beside the message. Nothing else sets or reads it; a reply with no number
// finds it empty.
thread_local! {
    static SAID: std::cell::RefCell<Option<(u16, Value)>> = const { std::cell::RefCell::new(None) };
}
pub(crate) fn vfs_err(e: VfsError) -> (&'static str, String) {
    SAID.with(|s| *s.borrow_mut() = e.said_json());
    (e.code(), e.message())
}
