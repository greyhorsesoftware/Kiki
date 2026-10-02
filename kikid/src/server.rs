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
use std::collections::{HashMap, VecDeque};
use std::io::Write;
use std::os::unix::net::{UnixListener, UnixStream};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{SendError, Sender};
use std::sync::{Arc, Condvar, Mutex};
use std::thread;
use std::time::{Duration, Instant};

static NEXT_CLIENT: AtomicU64 = AtomicU64::new(1);

// ---------------------------------------------------------------- the way out to a client

/// How many frames a connection may have waiting to be written before the daemon stops keeping
/// them. A window reads its socket every frame; this is minutes of events for one that has
/// stopped (docs/0.5.0/02-daemon-bounds.md).
pub const QUEUE_BOUND: usize = 4096;
/// How long a frame that must arrive waits for room before the connection is given up on.
const ROOM_WAIT: Duration = Duration::from_secs(1);

/// What everything in the daemon holds on a connection to write by — a listing's subscriber, a
/// job's owner, a handler answering late. It looks like the `Sender` it replaced and is sent to
/// the same way, and inside it is the bound the audit found missing: a client that stops
/// reading used to grow the heap with every event the daemon produced, for as long as the
/// socket stayed open. Now the queue holds `QUEUE_BOUND` frames. When it is full, a frame a
/// later one supersedes — a job's progress, a scan's growing count, an enrich's tally — pushes
/// the oldest such frame out and is counted; a frame that must arrive — a reply, a job's end,
/// a `Reset`, rows — waits a second for room and, finding none, closes the connection: a
/// window that far behind has to come back anyway, and coming back resubscribes
/// (`Daemon.qml`, `reconnected`). The count of what was dropped is said once, when the
/// connection ends.
#[derive(Clone)]
pub struct ClientTx(Arc<TxInner>);

enum TxInner {
    /// A connection: the queue its writer thread drains, and the stream to shut when the queue
    /// has nowhere to put something that must arrive.
    Socket { id: u64, q: Mutex<Queue>, cv: Condvar, stream: UnixStream },
    /// A test's or the bench's channel: everything through, nothing bounded.
    Plain(Sender<Value>),
}

struct Queue {
    items: VecDeque<Value>,
    /// No more is taken; the writer drains what is here and shuts the stream.
    closed: bool,
    dropped: u64,
}

/// A frame a later frame of the same kind replaces, so losing it loses nothing.
fn superseded(v: &Value) -> bool {
    match v.str_field("event") {
        Some("Progress") => true,
        Some("Count") => v.get("done").and_then(Value::as_bool) != Some(true),
        Some("JobEvent") => matches!(v.get("job").and_then(|j| j.str_field("state")), Some("running") | Some("queued")),
        _ => false,
    }
}

impl From<Sender<Value>> for ClientTx {
    fn from(tx: Sender<Value>) -> ClientTx {
        ClientTx(Arc::new(TxInner::Plain(tx)))
    }
}

impl ClientTx {
    /// A connection's: the writer thread is started here and ends when the queue is closed and
    /// drained, or the stream will not take what it has.
    pub(crate) fn socket(id: u64, stream: UnixStream, framing: Framing) -> ClientTx {
        let writer = stream.try_clone().expect("clone for the writer");
        let tx = ClientTx(Arc::new(TxInner::Socket { id, q: Mutex::new(Queue { items: VecDeque::new(), closed: false, dropped: 0 }), cv: Condvar::new(), stream }));
        let me = tx.clone();
        thread::Builder::new().name(format!("writer-{id}")).spawn(move || me.drain(writer, framing)).expect("spawn writer");
        tx
    }

    fn drain(&self, mut writer: UnixStream, framing: Framing) {
        let TxInner::Socket { q, cv, stream, .. } = &*self.0 else { return };
        loop {
            let v = {
                let mut q = q.lock().unwrap();
                loop {
                    if let Some(v) = q.items.pop_front() {
                        // Room was made: a sender waiting for it may go on.
                        cv.notify_all();
                        break v;
                    }
                    if q.closed {
                        // Everything written: the other end may now see the end of it.
                        let _ = stream.shutdown(std::net::Shutdown::Both);
                        return;
                    }
                    q = cv.wait(q).unwrap();
                }
            };
            if proto::write_json(&mut writer, framing, &v).is_err() || writer.flush().is_err() {
                self.shut("the socket would not take a frame");
                return;
            }
        }
    }

    /// Give up on the connection now: nothing more is queued, what is queued is dropped, and
    /// the stream is shut so the reading side ends as well.
    fn shut(&self, why: &str) {
        let TxInner::Socket { id, q, cv, stream } = &*self.0 else { return };
        let mut g = q.lock().unwrap();
        if !g.closed {
            g.closed = true;
            g.items.clear();
            self.say_dropped(*id, g.dropped);
            eprintln!("client {id}: closed, {why}");
        }
        let _ = stream.shutdown(std::net::Shutdown::Both);
        cv.notify_all();
    }

    /// The connection has ended on the reading side: the writer finishes what it has and goes.
    pub(crate) fn close(&self) {
        let TxInner::Socket { id, q, cv, .. } = &*self.0 else { return };
        let mut g = q.lock().unwrap();
        if !g.closed {
            g.closed = true;
            self.say_dropped(*id, g.dropped);
        }
        cv.notify_all();
    }

    fn say_dropped(&self, id: u64, dropped: u64) {
        if dropped > 0 {
            eprintln!("client {id}: {dropped} progress frames were dropped unread (the window was not reading its socket)");
        }
    }

    pub fn is_open(&self) -> bool {
        match &*self.0 {
            TxInner::Socket { q, .. } => !q.lock().unwrap().closed,
            TxInner::Plain(tx) => tx.send(Value::Null).is_ok(),
        }
    }

    /// How many frames were dropped unread so far.
    pub fn dropped(&self) -> u64 {
        match &*self.0 {
            TxInner::Socket { q, .. } => q.lock().unwrap().dropped,
            TxInner::Plain(_) => 0,
        }
    }

    /// How many frames are waiting to be written.
    pub fn queued(&self) -> usize {
        match &*self.0 {
            TxInner::Socket { q, .. } => q.lock().unwrap().items.len(),
            TxInner::Plain(_) => 0,
        }
    }

    /// `Err` means the connection is gone and the frame with it, the way a `Sender` says it.
    pub fn send(&self, v: Value) -> Result<(), SendError<Value>> {
        let (q, cv) = match &*self.0 {
            TxInner::Plain(tx) => return tx.send(v),
            TxInner::Socket { q, cv, .. } => (q, cv),
        };
        let mut g = q.lock().unwrap();
        if g.closed {
            return Err(SendError(v));
        }
        if g.items.len() >= QUEUE_BOUND {
            // Room is made by losing the oldest frame a later one makes good, whatever is being
            // sent; a frame of that kind with nothing older to lose is itself the one lost.
            if let Some(i) = g.items.iter().position(superseded) {
                g.items.remove(i);
                g.dropped += 1;
            } else if superseded(&v) {
                g.dropped += 1;
                return Ok(());
            } else {
                // Full of frames that must arrive and nobody reading: a second's grace, then
                // the connection is given up rather than the daemon's memory.
                let deadline = Instant::now() + ROOM_WAIT;
                while g.items.len() >= QUEUE_BOUND && !g.closed {
                    let left = deadline.saturating_duration_since(Instant::now());
                    if left.is_zero() {
                        break;
                    }
                    g = cv.wait_timeout(g, left).unwrap().0;
                }
                if g.closed || g.items.len() >= QUEUE_BOUND {
                    drop(g);
                    self.shut("it had not read its socket in a second with the queue full");
                    return Err(SendError(v));
                }
            }
        }
        g.items.push_back(v);
        cv.notify_all();
        Ok(())
    }
}

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
    pub(crate) tx: ClientTx,
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
        let tx = ClientTx::socket(id, stream, framing);
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
        // The writer finishes what it has and goes; whoever else holds this connection (a job
        // it started, the jobs' subscriber list) finds it closed on their next send.
        client.tx.close();
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Read;

    fn progress() -> Value {
        proto::event("JobEvent").v("job", Value::obj().u("id", 1).s("state", "running").done()).done()
    }
    fn ended(n: u64) -> Value {
        proto::event("JobEvent").v("job", Value::obj().u("id", n).s("state", "done").done()).done()
    }

    /// Everything the other end was sent, as lines, until it is closed.
    fn read_all(mut s: UnixStream) -> String {
        s.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
        let mut out = Vec::new();
        let mut buf = [0u8; 65536];
        loop {
            match s.read(&mut buf) {
                Ok(0) => break,
                Ok(n) => out.extend_from_slice(&buf[..n]),
                Err(e) => panic!("the stream was not closed: {e}"),
            }
        }
        String::from_utf8_lossy(&out).into_owned()
    }

    #[test]
    fn a_client_that_never_reads_is_bounded_and_then_let_go() {
        let (a, b) = UnixStream::pair().unwrap();
        let tx = ClientTx::socket(7, a, Framing::Text);
        // Ten thousand progress frames into a socket nobody reads: the kernel takes a few
        // thousand bytes' worth, the queue its bound, and the rest is dropped — the heap does
        // not grow with them.
        for _ in 0..10_000 {
            tx.send(progress()).unwrap();
        }
        assert!(tx.queued() <= QUEUE_BOUND, "the queue is bounded: {}", tx.queued());
        assert!(tx.dropped() > 0, "what did not fit was dropped, not kept");
        assert!(tx.is_open());
        // Frames that must arrive push the progress out first: every one of them is taken,
        // until the queue holds nothing but frames that must arrive.
        for n in 1..=QUEUE_BOUND as u64 {
            tx.send(ended(n)).unwrap();
        }
        assert!(tx.is_open());
        // One more finds no room within a second: the connection is given up, not the memory.
        let t = Instant::now();
        assert!(tx.send(ended(0)).is_err());
        assert!(t.elapsed() < Duration::from_secs(3), "gave up in about a second, not more: {:?}", t.elapsed());
        assert!(!tx.is_open());
        assert!(tx.send(progress()).is_err(), "nothing more is taken");
        // And the other end sees the end of the stream, not a hang.
        let got = read_all(b);
        assert!(got.contains("JobEvent"), "what the kernel had taken was delivered first");
    }

    #[test]
    fn a_slow_reader_gets_every_frame_that_matters() {
        let (a, b) = UnixStream::pair().unwrap();
        let tx = ClientTx::socket(8, a, Framing::Text);
        // Far more than the bound, with twenty ends among the progress, nobody reading yet.
        for n in 1..=20 {
            for _ in 0..300 {
                tx.send(progress()).unwrap();
            }
            tx.send(ended(n)).unwrap();
        }
        assert!(tx.queued() <= QUEUE_BOUND);
        tx.close();
        let got = read_all(b);
        let ends: Vec<u64> = got.lines().filter(|l| l.contains("\"done\"")).filter_map(|l| crate::json::parse(l.as_bytes()).ok()).filter_map(|v| v.get("job").and_then(|j| j.u64_field("id"))).collect();
        assert_eq!(ends, (1..=20).collect::<Vec<u64>>(), "every end arrived, in order; only progress was dropped");
        assert!(tx.dropped() > 0);
    }
}
