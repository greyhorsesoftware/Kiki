//! The daemon is the window's engine, not a service (docs/0.3.0/01-daemon-on-demand.md): it
//! serves one socket, it will not start a second time on a socket that is already served, and it
//! leaves a moment after the last window has gone. A real `kikid` proves each of the three —
//! a grace of a few hundred milliseconds rather than the ten seconds a person would wait for.

use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

struct Daemon(Child);

impl Drop for Daemon {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

impl Daemon {
    fn start(sandbox: &Path, socket: &Path, grace_ms: u64) -> Daemon {
        let child = Command::new(env!("CARGO_BIN_EXE_kikid"))
            .env("KIKI_SOCKET", socket)
            .env("KIKI_EXIT_GRACE_MS", grace_ms.to_string())
            .env("KIKI_STATE_DIR", sandbox.join("state"))
            .env("KIKI_CONFIG_DIR", sandbox.join("config"))
            .env("KIKI_DATA_DIR", sandbox.join("data"))
            .env("KIKI_CACHE_DIR", sandbox.join("cache"))
            .env("KIKI_TRASH_DIR", sandbox.join("trash"))
            .env("KIKI_THUMB_DIR", sandbox.join("thumbs"))
            .env("KIKI_PLUGIN_DIR", sandbox.join("plugins"))
            .env("KIKI_SECRET_TOOL", sandbox.join("no-secret-tool"))
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("spawn kikid");
        Daemon(child)
    }

    /// Whether the process has ended, and with what, without waiting for it.
    fn ended(&mut self) -> Option<i32> {
        self.0.try_wait().ok().flatten().map(|s| s.code().unwrap_or(-1))
    }

    fn gone_within(&mut self, timeout: Duration) -> Option<i32> {
        let start = Instant::now();
        while start.elapsed() < timeout {
            if let Some(code) = self.ended() {
                return Some(code);
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        None
    }
}

fn sandbox(tag: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("kiki-lifetime-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

/// A connection that says Hello as a window does, so the daemon counts it as one.
struct Window(UnixStream);

impl Window {
    fn open(socket: &Path, timeout: Duration) -> Option<Window> {
        let start = Instant::now();
        while start.elapsed() < timeout {
            if let Ok(s) = UnixStream::connect(socket) {
                let mut w = Window(s);
                w.hello()?;
                return Some(w);
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        None
    }

    fn hello(&mut self) -> Option<String> {
        self.0.write_all(b"{\"id\":1,\"type\":\"Hello\",\"version\":1,\"client\":\"kiki\"}\n").ok()?;
        self.0.flush().ok()?;
        let mut line = String::new();
        BufReader::new(self.0.try_clone().ok()?).read_line(&mut line).ok()?;
        Some(line)
    }
}

#[test]
fn one_daemon_a_socket_and_it_leaves_when_the_last_window_has_gone() {
    let dir = sandbox("serve");
    let socket = dir.join("kiki-test.sock");
    let grace = 400;

    // Up, and answering as itself.
    let mut first = Daemon::start(&dir, &socket, grace);
    let mut window = Window::open(&socket, Duration::from_secs(10)).expect("the daemon answered");
    assert!(window.hello().expect("a second hello").contains("\"kikid\""), "Hello names the daemon's version");

    // A second daemon on the same socket has nothing to do: it says so and leaves with 0, and
    // the one already serving is untouched.
    let mut second = Daemon::start(&dir, &socket, grace);
    assert_eq!(second.gone_within(Duration::from_secs(10)), Some(0), "the second daemon stands down");
    assert!(first.ended().is_none(), "the first is still serving");

    // Two windows, and the second is *held* — a connection opened and dropped in the same breath
    // would leave one window, not two, which is the thing this half of the test is about.
    let other = Window::open(&socket, Duration::from_secs(5)).expect("a second window");

    // One of the two closing is not the last one out: nothing happens for well over the grace.
    drop(window);
    std::thread::sleep(Duration::from_millis(grace * 3));
    assert!(first.ended().is_none(), "a daemon with a window left does not leave");

    // The last one out: gone within the grace.
    drop(other);
    let code = first.gone_within(Duration::from_millis(grace * 10 + 2000));
    assert_eq!(code, Some(0), "the daemon leaves after the last window, quietly");

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_grace_of_zero_is_a_daemon_that_stays() {
    // What the test harness sets: there the daemon's life is the harness's to manage, and a
    // gap between flows must not take it down.
    let dir = sandbox("stay");
    let socket = dir.join("kiki-test.sock");
    let mut daemon = Daemon::start(&dir, &socket, 0);
    let window = Window::open(&socket, Duration::from_secs(10)).expect("the daemon answered");
    drop(window);
    std::thread::sleep(Duration::from_millis(600));
    assert!(daemon.ended().is_none(), "a grace of zero never leaves");
    assert!(Window::open(&socket, Duration::from_secs(5)).is_some(), "and is still there to answer");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_leftover_socket_file_does_not_stop_the_next_daemon() {
    // What a daemon that was killed leaves behind. The next one removes it and binds.
    let dir = sandbox("stale");
    let socket = dir.join("kiki-test.sock");
    std::fs::write(&socket, b"not a socket").unwrap();
    let mut daemon = Daemon::start(&dir, &socket, 0);
    assert!(Window::open(&socket, Duration::from_secs(10)).is_some(), "bound over the leftover");
    assert!(daemon.ended().is_none());
    let _ = std::fs::remove_dir_all(&dir);
}
