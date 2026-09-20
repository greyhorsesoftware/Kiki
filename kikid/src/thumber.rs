//! The daemon's side of `kiki-thumber`: keep one alive, ask it for thumbnails, and survive it.
//!
//! Decoding a picture is the one thing kiki does to a file's contents, and the one thing a
//! malformed file can turn against it. It happens in a child process so that the worst a file can
//! do is end that process.
//!
//! The child is run through `Plugin::spawn_path` and speaks the framing of `API-PLUGIN.md`, the
//! one way kiki runs a child — it is not a location plugin and has no `Describe` or `Connect`, but
//! there is no reason for the daemon to own a second way of talking down a pipe. What the plugin
//! host does not have, and what is here, is the supervision a decoder needs and a file server does
//! not:
//!
//! - **A wedged child is killed.** `Plugin::request_within` gives a thumbnail `STUCK` rather than
//!   the two minutes a transfer may fairly take; a decoder looping on a malformed file would
//!   otherwise hold a worker for ever.
//! - **A file that kills the child twice is given up on**, so a crash cannot become a loop of
//!   restarts. The first death is forgiven: it may have been the machine, or the file beside it.
//! - **The child is started again** on the next request, rather than waiting — as the location
//!   registry does — for someone to ask for that location again.
//!
//! A cache hit never leaves the daemon: `thumbs::lookup` reads a PNG we wrote ourselves, so the
//! common case costs no pipe and no process. The workers are what limit how much is in flight —
//! one blocking request each — which is also what lets `STUCK` mean "stuck" and not "busy".

use crate::json::Value;
use crate::kinds::Kind;
use crate::plugin::Plugin;
use crate::thumbs::{self, Size};
use crate::vfs::uri::Uri;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::mpsc::{self, Sender};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;

/// A single file has this long to be decoded before the child is taken to be wedged on it.
const STUCK: Duration = Duration::from_secs(20);
/// Deaths with a file in flight before that file is given up on.
const STRIKES: u8 = 2;

pub struct Job {
    pub uri: Uri,
    pub kind: Kind,
    pub mtime_ms: u64,
    pub size: Size,
    pub done: Box<dyn FnOnce(Option<PathBuf>) + Send>,
}

/// Where the thumbnailer is: beside this binary first, which is what a checkout and an install
/// both look like, then the places plugins live, then the path.
fn find_binary() -> Option<PathBuf> {
    if let Ok(p) = std::env::var("KIKI_THUMBER") {
        let p = PathBuf::from(p);
        return p.is_file().then_some(p);
    }
    let mut dirs: Vec<PathBuf> = Vec::new();
    if let Ok(exe) = std::env::current_exe() {
        if let Some(d) = exe.parent() {
            dirs.push(d.to_path_buf());
            // `cargo test` runs from target/<profile>/deps; the binaries are one up.
            if let Some(up) = d.parent() {
                dirs.push(up.to_path_buf());
            }
        }
    }
    dirs.extend(crate::plugin::plugin_dirs());
    dirs.push(PathBuf::from("/usr/lib/kiki"));
    if let Some(p) = dirs.iter().map(|d| d.join("kiki-thumber")).find(|p| p.is_file()) {
        return Some(p);
    }
    std::env::var_os("PATH").map(|p| std::env::split_paths(&p).map(|d| d.join("kiki-thumber")).find(|p| p.is_file())).unwrap_or(None)
}

#[derive(Default)]
struct Supervisor {
    /// The child every worker is using, until one of them finds it wanting.
    current: Mutex<Option<Arc<Plugin>>>,
    /// How many times the child has died or wedged with this file in flight.
    strikes: Mutex<HashMap<String, u8>>,
    missing_logged: Mutex<bool>,
}

fn sup() -> &'static Supervisor {
    static S: OnceLock<Supervisor> = OnceLock::new();
    S.get_or_init(Supervisor::default)
}

impl Supervisor {
    /// The running child, started if there is none. `None` means there is no thumbnailer to run,
    /// which is a packaging fault: it is said once, and every request is answered "no thumbnail".
    fn plugin(&self) -> Option<Arc<Plugin>> {
        let mut cur = self.current.lock().unwrap();
        if let Some(p) = cur.as_ref() {
            if p.alive() {
                return Some(Arc::clone(p));
            }
            *cur = None;
        }
        let Some(bin) = find_binary() else {
            let mut said = self.missing_logged.lock().unwrap();
            if !*said {
                *said = true;
                eprintln!("no kiki-thumber beside kikid or on the path: no thumbnails will be made");
            }
            return None;
        };
        match Plugin::spawn_path(&bin, "thumb") {
            Ok(p) => {
                *cur = Some(Arc::clone(&p));
                Some(p)
            }
            Err(e) => {
                eprintln!("cannot start {}: {}", bin.display(), e.message());
                None
            }
        }
    }

    /// This child failed a request: end it, and everything it started, so the next request gets a
    /// fresh one. Only if it is still the current child — another worker may have got here first.
    fn condemn(&self, bad: &Arc<Plugin>) {
        {
            let mut cur = self.current.lock().unwrap();
            if cur.as_ref().is_some_and(|p| Arc::ptr_eq(p, bad)) {
                *cur = None;
            }
        }
        bad.kill_group();
    }

    /// One more death with this file in flight. `true` once it has had its chances.
    fn strike(&self, uri: &str) -> bool {
        let mut s = self.strikes.lock().unwrap();
        let n = s.entry(uri.to_string()).or_insert(0);
        *n += 1;
        *n >= STRIKES
    }

    /// It answered about this file, whatever the answer was: the file did not kill it.
    fn forgive(&self, uri: &str) {
        self.strikes.lock().unwrap().remove(uri);
    }
}

fn workers() -> &'static Sender<Job> {
    static W: OnceLock<Sender<Job>> = OnceLock::new();
    W.get_or_init(|| {
        let (tx, rx) = mpsc::channel::<Job>();
        let rx = Arc::new(Mutex::new(rx));
        let n = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(2).clamp(1, 4);
        for i in 0..n {
            let rx = Arc::clone(&rx);
            std::thread::Builder::new()
                .name(format!("thumber-{i}"))
                .spawn(move || loop {
                    let job = { rx.lock().unwrap().recv() };
                    match job {
                        Ok(j) => run(j),
                        Err(_) => return,
                    }
                })
                .expect("spawn thumber worker");
        }
        tx
    })
}

/// One thumbnail, start to finish, on one worker thread.
///
/// The child says "no thumbnail for this one" as an `ok` with a null path, which is why an `Err`
/// here can only mean the child died on this file or never answered about it. Those are the same
/// thing: this child is finished, and this file is a suspect. Asking the child whether it is still
/// alive instead would be a race — it has exited, and whether the kernel has said so yet is not
/// something the answer should turn on.
fn run(job: Job) {
    let uri = job.uri.to_string();
    let req = Value::obj().s("type", "Thumb").s("uri", uri.clone()).s("kind", kind_str(job.kind)).u("size", job.size as u64).u("mtime", job.mtime_ms).done();
    for _ in 0..STRIKES {
        let Some(p) = sup().plugin() else {
            (job.done)(None);
            return;
        };
        match p.request_within(req.clone(), STUCK) {
            Ok(v) => {
                sup().forgive(&uri);
                (job.done)(v.str_field("path").map(PathBuf::from));
                return;
            }
            Err(_) => {
                eprintln!("thumbnailer died or hung on {uri}; starting it again");
                sup().condemn(&p);
                if sup().strike(&uri) {
                    (job.done)(None);
                    return;
                }
            }
        }
    }
    (job.done)(None);
}

fn kind_str(k: Kind) -> &'static str {
    match k {
        Kind::Video => "video",
        Kind::Pdf => "pdf",
        _ => "image",
    }
}

/// Drop the child that is up, so the next request starts a fresh one. Only a test wants this: it
/// is how a test swaps the thumbnailer under a supervisor that is deliberately long-lived.
#[cfg(test)]
pub(crate) fn restart_for_test() {
    let p = sup().current.lock().unwrap().take();
    if let Some(p) = p {
        p.kill_group();
    }
    sup().strikes.lock().unwrap().clear();
}

/// Ask for a thumbnail. A cache hit is answered here and now; anything else goes to the child.
pub fn submit(job: Job) {
    if let Some(p) = thumbs::lookup(&job.uri, job.size, job.mtime_ms) {
        (job.done)(Some(p));
        return;
    }
    if !job.uri.is_local() || !thumbs::thumbable(job.kind) {
        (job.done)(None);
        return;
    }
    let _ = workers().send(job);
}

/// The same, for a caller that has to have the answer before it can reply — the inspector's
/// preview. Waits, because the request it is serving is already on a worker of its own.
pub fn blocking(uri: &Uri, kind: Kind, size: Size, mtime_ms: u64) -> Option<PathBuf> {
    let (tx, rx) = mpsc::channel();
    submit(Job {
        uri: uri.clone(),
        kind,
        mtime_ms,
        size,
        done: Box::new(move |p| {
            let _ = tx.send(p);
        }),
    });
    // Longer than a worker's own patience, twice over: a child stuck on this very file is given
    // up on at `STUCK`, tried once more, and the answer that follows is the one to return.
    rx.recv_timeout(STUCK * STRIKES as u32 + Duration::from_secs(5)).ok().flatten()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc::RecvTimeoutError;

    /// A stand-in thumbnailer: speaks the framing, and misbehaves to order. Written in Python and
    /// in one process, because the misbehaviour has to be the real thing — a shell pipeline would
    /// leave its other half holding stdout open, and the host would see a hang where the test
    /// meant a death.
    ///
    /// `mode` is what it does with a file whose name contains "poison": `die` exits at once,
    /// `hang` never answers and leaves a child of its own holding stdout, which is what a wedged
    /// `ffmpeg` looks like. Everything else is answered with a made-up cache path.
    fn fake(dir: &std::path::Path, mode: &str, deaths: &std::path::Path) -> PathBuf {
        let p = dir.join("kiki-thumber");
        let src = format!(
            r#"#!/usr/bin/env python3
import json, os, struct, subprocess, sys
MODE, DEATHS = {mode:?}, {deaths:?}
r, w = sys.stdin.buffer, sys.stdout.buffer
def send(obj):
    b = json.dumps(obj).encode()
    w.write(struct.pack("<I", len(b)) + b"\x00" + b); w.flush()
while True:
    head = r.read(5)
    if len(head) < 5: break
    n = struct.unpack("<I", head[:4])[0]
    req = json.loads(r.read(n))
    if "poison" in req.get("uri", ""):
        open(DEATHS, "a").write("x\n")
        if MODE == "die":
            os._exit(1)
        # A helper of our own, holding this stdout: killing us alone would leave it, and the
        # daemon would wait on a pipe that never closes.
        subprocess.Popen(["sleep", "600"])
        while True: __import__("time").sleep(600)
    send({{"id": req.get("id", 0), "ok": {{"path": "/cache/%s.png" % req.get("id", 0)}}}})
"#
        );
        std::fs::write(&p, src).unwrap();
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o755)).unwrap();
        p
    }

    fn ask(uri: &str) -> mpsc::Receiver<Option<PathBuf>> {
        let (tx, rx) = mpsc::channel();
        submit(Job {
            uri: Uri::parse(uri).unwrap(),
            kind: Kind::Image,
            mtime_ms: 1,
            size: Size::Normal,
            done: Box::new(move |p| {
                let _ = tx.send(p);
            }),
        });
        rx
    }

    /// The whole point of the child: a file that kills the decoder must cost that file its
    /// thumbnail and nothing else. It must not be retried for ever, and the files asked for
    /// beside it must still be answered.
    #[test]
    fn a_file_that_kills_the_decoder_costs_only_itself() {
        let _g = crate::ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let dir = std::env::temp_dir().join(format!("kiki-thumber-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let count = dir.join("deaths");
        let bin = fake(&dir, "die", &count);
        std::env::set_var("KIKI_THUMBER", &bin);
        restart_for_test();

        // The request number is the child's, so it is the shape of the answer that is asserted.
        let good = ask("file:///tmp/a.jpg").recv_timeout(Duration::from_secs(25)).unwrap().expect("an ordinary file is answered");
        assert!(good.to_string_lossy().starts_with("/cache/"), "{good:?}");

        // The poison file: the child dies with it in flight, is forgiven once, dies again, and
        // then the file — not the daemon, and not the thumbnailer for good — is given up on.
        let bad = ask("file:///tmp/poison.jpg");
        assert_eq!(bad.recv_timeout(Duration::from_secs(30)).unwrap(), None, "the file that kills it gets no thumbnail");
        assert_eq!(std::fs::read_to_string(&count).unwrap().lines().count(), STRIKES as usize, "tried twice, then given up on");

        // And the thumbnailer is alive again for everyone else.
        let after = ask("file:///tmp/b.jpg");
        assert!(after.recv_timeout(Duration::from_secs(25)).unwrap().is_some(), "the next file is thumbnailed as if nothing happened");

        std::env::remove_var("KIKI_THUMBER");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A decoder that never returns holds a worker for ever. It is killed and the file is given up
    /// on, rather than the folder quietly never finishing. The script leaves a `sleep` running with
    /// stdout inherited, which is what a wedged `ffmpeg` looks like: killing the child alone would
    /// leave that grandchild holding the pipe, and the host waiting on it for ever.
    #[test]
    #[ignore = "waits out the 20s watchdog twice; run with --ignored"]
    fn a_decoder_that_hangs_is_killed() {
        let _g = crate::ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let dir = std::env::temp_dir().join(format!("kiki-thumber-hang-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let bin = fake(&dir, "hang", &dir.join("deaths"));
        std::env::set_var("KIKI_THUMBER", &bin);
        restart_for_test();
        let r = ask("file:///tmp/poison-hangs.jpg");
        assert_eq!(r.recv_timeout(STUCK * 3 + Duration::from_secs(20)), Ok(None));
        std::env::remove_var("KIKI_THUMBER");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// No thumbnailer installed is a packaging fault, not a reason for every picture in the
    /// window to sit for ever waiting for an answer that is not coming.
    #[test]
    fn no_thumbnailer_answers_rather_than_hangs() {
        let _g = crate::ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        std::env::set_var("KIKI_THUMBER", "/nonexistent/kiki-thumber");
        restart_for_test();
        let r = ask("file:///tmp/whatever.jpg");
        let got = r.recv_timeout(Duration::from_secs(10));
        std::env::remove_var("KIKI_THUMBER");
        assert!(matches!(got, Ok(None)), "expected a prompt 'no thumbnail', got {got:?}");
        assert_ne!(got, Err(RecvTimeoutError::Timeout));
    }

    /// A remote file never reaches the child at all: the daemon answers it here.
    #[test]
    fn a_remote_file_is_refused_without_asking() {
        let r = ask("sftp://host/pics/far.jpg");
        assert_eq!(r.recv_timeout(Duration::from_secs(2)).unwrap(), None);
    }
}
