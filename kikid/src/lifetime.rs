//! How long the daemon lives (docs/0.3.0/01-daemon-on-demand.md, decisions 1 and 2).
//!
//! `kikid` is not a service and nothing but a window starts it: it is the window's engine,
//! started by the window when the socket does not answer and gone a moment after the last window
//! has closed. Two rules, both here:
//!
//! **One daemon per socket.** Before binding, `claim` tries to connect to the socket. Something
//! answered means another daemon of this version is already serving — two windows opened at once
//! and one lost the race — and this one has nothing to do: it says so and leaves with 0, because
//! a window that started it will find the winner. Nothing answering means the file is a leftover
//! of a daemon that died, and it is removed before binding.
//!
//! **The last window out closes the door.** Every connection counts; when the count reaches zero
//! the daemon waits `KIKI_EXIT_GRACE_MS` (10 s — a window closed and reopened does not pay for a
//! cold start) and exits if nothing has connected since. Jobs are already stopped by then: the
//! window's quit prompt cancels them before it goes, and `jobs::shell_went`'s own shorter grace
//! catches a window that was killed. A grace of 0 means "never leave", which is what the test
//! harness sets: there the daemon's life is the harness's to manage.

use std::os::unix::net::UnixStream;
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Condvar, Mutex};
use std::time::Duration;

/// How long the daemon waits, with nothing connected, before it exits. 0 never exits.
pub fn exit_grace() -> Duration {
    Duration::from_millis(std::env::var("KIKI_EXIT_GRACE_MS").ok().and_then(|v| v.parse().ok()).unwrap_or(10_000))
}

/// (connected now, how many have ever connected). The second is how a grace that has slept knows
/// that somebody came and went while it slept, rather than seeing the same zero and taking it for
/// the same silence.
fn clients() -> &'static (Mutex<(usize, u64)>, Condvar) {
    static C: std::sync::OnceLock<(Mutex<(usize, u64)>, Condvar)> = std::sync::OnceLock::new();
    C.get_or_init(|| (Mutex::new((0, 0)), Condvar::new()))
}

/// Set while the process is leaving, so nothing starts work it cannot finish.
static LEAVING: AtomicU64 = AtomicU64::new(0);

pub fn leaving() -> bool {
    LEAVING.load(Ordering::Relaxed) != 0
}

/// A client connected.
pub fn came() {
    let (m, cv) = clients();
    let mut c = m.lock().unwrap();
    c.0 += 1;
    c.1 += 1;
    cv.notify_all();
}

/// A client went. When it was the last one, the grace starts; if nothing has connected by the end
/// of it, the daemon exits.
pub fn went() {
    let (m, cv) = clients();
    let mut c = m.lock().unwrap();
    c.0 = c.0.saturating_sub(1);
    cv.notify_all();
    if c.0 > 0 {
        return;
    }
    let grace = exit_grace();
    if grace.is_zero() {
        return; // the harness's daemon: its life is not ours to end
    }
    let seen = c.1;
    drop(c);
    let spawned = std::thread::Builder::new().name("exit-grace".into()).spawn(move || {
        let (m, cv) = clients();
        let mut c = m.lock().unwrap();
        // Woken early by anything that changes the count; only the whole grace with nobody
        // arriving is silence. `wait_timeout_while` re-checks after a spurious wake of its own.
        let (c2, timed_out) = cv.wait_timeout_while(c, grace, |c| *c == (0, seen)).unwrap();
        c = c2;
        if timed_out.timed_out() && *c == (0, seen) {
            LEAVING.store(1, Ordering::Relaxed);
            drop(c);
            eprintln!("kikid: no windows for {} ms; leaving", grace.as_millis());
            // Straight out: everything the daemon owns is either finished (jobs, above) or is a
            // cache that is rebuilt on demand. An orderly unwind would only be a slower exit.
            std::process::exit(0);
        }
    });
    if let Err(e) = spawned {
        eprintln!("kikid: exit grace: {e}");
    }
}

/// How many clients are connected, for a test to watch.
pub fn connected() -> usize {
    clients().0.lock().unwrap().0
}

/// Whether this process should serve `path`, taking the socket if it is free.
pub enum Claim {
    /// Nobody was there; the stale file (if any) is gone and the path is ready to bind.
    Free,
    /// A daemon of this version is already serving: this process has nothing to do.
    Taken,
}

/// Decide whether to serve: a socket that answers belongs to a daemon already up. A socket file
/// that refuses is a leftover and is removed, which is the only case where the file is unlinked —
/// never one that something is listening on.
pub fn claim(path: &Path) -> Claim {
    match UnixStream::connect(path) {
        Ok(_) => Claim::Taken,
        Err(_) if path.exists() => {
            let _ = std::fs::remove_file(path);
            Claim::Free
        }
        Err(_) => Claim::Free,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dir(tag: &str) -> std::path::PathBuf {
        let d = std::env::temp_dir().join(format!("kiki-lifetime-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn a_socket_nobody_answers_is_a_leftover_and_one_that_answers_is_taken() {
        let d = dir("claim");
        let path = d.join("kiki.sock");
        assert!(matches!(claim(&path), Claim::Free), "no file at all");

        // A file that is not a socket, or a socket nobody is listening on: a daemon died here.
        std::fs::write(&path, b"stale").unwrap();
        assert!(matches!(claim(&path), Claim::Free));
        assert!(!path.exists(), "the leftover is removed before the bind");

        // Something listening: this process is not the one to serve, and the socket is untouched.
        let l = std::os::unix::net::UnixListener::bind(&path).unwrap();
        assert!(matches!(claim(&path), Claim::Taken));
        assert!(path.exists(), "a live socket is never unlinked");
        drop(l);
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn the_grace_is_read_from_the_environment_and_zero_means_stay() {
        std::env::set_var("KIKI_EXIT_GRACE_MS", "250");
        assert_eq!(exit_grace(), Duration::from_millis(250));
        std::env::set_var("KIKI_EXIT_GRACE_MS", "0");
        assert!(exit_grace().is_zero());
        // A grace of zero never starts the thread that would exit: `went` with nobody connected
        // returns, and the process is still here to say so.
        came();
        went();
        std::thread::sleep(Duration::from_millis(50));
        std::env::remove_var("KIKI_EXIT_GRACE_MS");
    }
}
