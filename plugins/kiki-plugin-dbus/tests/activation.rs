//! The listener, started by the bus (docs/0.3.0/01-daemon-on-demand.md, decision 4).
//!
//! This is the path nothing has ever tested: another application asks the session bus for
//! `org.freedesktop.FileManager1` or the file-chooser portal, the bus starts kiki's listener from
//! a service file, and the listener puts the request to a kiki window — starting one if there is
//! none. Here the bus is private (`dbus-run-session`), the service file is in the test's own
//! `XDG_DATA_DIRS` (which is also how a user asks for the FileManager1 name without touching the
//! package that owns it system-wide), and the window is a script that records what it was asked
//! and answers as the shell's IPC does.

#[path = "../../../kikid/src/scratch.rs"]
mod scratch;
use scratch::Scratch;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};

/// The state letter of a process — R, S, Z … — or None when there is no such process.
fn process_state(pid: &str) -> Option<char> {
    let stat = std::fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
    // "pid (comm) S …": the comm may hold spaces and parentheses, so the state follows the LAST ')'.
    stat.rsplit(')').next()?.trim_start().chars().next()
}

fn tools() -> bool {
    ["dbus-run-session", "busctl"].iter().all(|t| Command::new("which").arg(t).output().map(|o| o.status.success()).unwrap_or(false))
}

fn sandbox(tag: &str) -> Scratch {
    Scratch::new(&format!("dbus-{tag}"))
}

fn script(path: &Path, body: &str) {
    std::fs::write(path, format!("#!/bin/sh\n{body}\n")).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
}

/// A service file for `name`, run through a wrapper that points the listener at the stub window.
fn service(dir: &Path, name: &str, wrapper: &Path) {
    let services = dir.join("dbus-1/services");
    std::fs::create_dir_all(&services).unwrap();
    std::fs::write(services.join(format!("{name}.service")), format!("[D-BUS Service]\nName={name}\nExec={}\n", wrapper.display())).unwrap();
}

/// Everything the listener needs to stand up on a private bus: the stub window, the wrapper the
/// bus activates, and the log the window writes.
struct Fixture {
    dir: Scratch,
    log: PathBuf,
}

impl Fixture {
    fn new(tag: &str, window_answer: &str) -> Fixture {
        let dir = sandbox(tag);
        let log = dir.join("asked.log");
        // The window: records the call and answers on stdout as the shell's IPC does.
        let window = dir.join("window");
        script(&window, &format!("printf '%s\\n' \"$*\" >> '{}'\n{window_answer}", log.display()));
        // What the bus starts: the listener, told where its window is.
        let wrapper = dir.join("listener");
        // `$$` before `exec`: the shell's pid becomes the listener's, so the test watches the
        // one THIS fixture started rather than any listener of the same binary (the tests share
        // one, and one winding down in another test is not this one still running).
        script(
            &wrapper,
            &format!(
                "echo $$ > '{}'\nexport KIKI_SHELL_CMD='{}'\nexport KIKI_DBUS_IDLE_MS=700\nexec '{}' >>'{}' 2>&1",
                dir.join("listener.pid").display(),
                window.display(),
                env!("CARGO_BIN_EXE_kiki-plugin-dbus"),
                dir.join("listener.log").display()
            ),
        );
        service(&dir, "org.freedesktop.FileManager1", &wrapper);
        service(&dir, "org.freedesktop.impl.portal.desktop.kiki", &wrapper);
        Fixture { dir, log }
    }

    /// One `busctl` call on a private bus that can only see this fixture's service files —
    /// XDG_DATA_HOME as well as XDG_DATA_DIRS, since the developer's own ~/.local/share outranks
    /// the latter and, with kiki's "Show in folder" row on, would have the bus start the
    /// INSTALLED listener and this test quietly proving nothing about the built one.
    fn call(&self, args: &[&str]) -> std::process::Output {
        let home = self.dir.join("home");
        std::fs::create_dir_all(&home).unwrap();
        let mut cmd = Command::new("dbus-run-session");
        cmd.arg("--").arg("busctl").arg("--user").args(args).env("XDG_DATA_DIRS", &self.dir).env("XDG_DATA_HOME", &home).env("KIKI_SHELL_CMD", self.dir.join("window")).current_dir(&self.dir);
        cmd.output().expect("busctl")
    }

    /// Whether the listener THIS fixture started has gone, by the pid its wrapper wrote. Gone
    /// means exited: a zombie counts, because it is one. The listener is dbus-daemon's child
    /// and outlives the bus by its idle grace, so it is reparented to pid 1 — and in a container
    /// pid 1 is often not an init that reaps, so the corpse stays in the table and `kill -0`
    /// keeps saying yes (CI, 2026-09-26). The state letter in /proc/<pid>/stat is the truth.
    fn listener_gone(&self, timeout: Duration) -> bool {
        let pid = match std::fs::read_to_string(self.dir.join("listener.pid")) {
            Ok(s) => s.trim().to_string(),
            Err(_) => return true, // never started: nothing to be still running
        };
        let start = Instant::now();
        while start.elapsed() < timeout {
            match process_state(&pid) {
                None | Some('Z') | Some('X') => return true,
                _ => std::thread::sleep(Duration::from_millis(100)),
            }
        }
        eprintln!("listener {pid} is in state {:?}; its log:\n{}", process_state(&pid), std::fs::read_to_string(self.dir.join("listener.log")).unwrap_or_default());
        false
    }

    fn asked(&self, timeout: Duration) -> String {
        let start = Instant::now();
        while start.elapsed() < timeout {
            if let Ok(s) = std::fs::read_to_string(&self.log) {
                if !s.trim().is_empty() {
                    return s;
                }
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        String::new()
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {}
}

#[test]
fn show_items_with_nothing_running_starts_the_listener_and_reaches_a_window() {
    if !tools() {
        eprintln!("dbus-run-session or busctl missing; skipped");
        return;
    }
    let f = Fixture::new("show", "exit 0");
    // Nothing is running: no listener, no window, no daemon. The bus starts the listener from the
    // service file because somebody asked for the name.
    let out = f.call(&["call", "org.freedesktop.FileManager1", "/org/freedesktop/FileManager1", "org.freedesktop.FileManager1", "ShowItems", "ass", "1", "file:///tmp/a.txt", ""]);
    assert!(out.status.success(), "the bus could not start the listener: {}", String::from_utf8_lossy(&out.stderr));

    let asked = f.asked(Duration::from_secs(10));
    assert!(!asked.is_empty(), "the listener never put the request to a window");
    assert!(asked.contains("file:///tmp/a.txt"), "the window was asked about the wrong thing: {asked}");

    // And then it goes. The listener is not a daemon: the bus starts one for the next request,
    // and one that sat about would be a process nobody asked for, holding a name and (in a test)
    // a private bus open. It did, until 2026-09-26 — `main` ended in a future that never
    // finished, and every run left one behind.
    assert!(f.listener_gone(Duration::from_secs(10)), "the listener is still running with nothing to do");
}

#[test]
fn a_chooser_asks_the_window_and_carries_its_answer_back_to_the_bus() {
    if !tools() {
        eprintln!("dbus-run-session or busctl missing; skipped");
        return;
    }
    // The window answers as the shell's IPC does: a chooser is two steps, because the person
    // has not chosen when the first call returns. A token, then the answer when it is collected.
    let f = Fixture::new(
        "chooser",
        r#"case "$1" in
  ShowChooser) printf '{"token":"tok-1"}' ;;
  ChooserPoll) printf '{"uris":["file:///tmp/chosen.txt"]}' ;;
  *) printf '{}' ;;
esac"#,
    );
    let out = f.call(&[
        "call",
        "org.freedesktop.impl.portal.desktop.kiki",
        "/org/freedesktop/portal/desktop",
        "org.freedesktop.impl.portal.FileChooser",
        "OpenFile",
        "osssa{sv}",
        "/org/freedesktop/portal/desktop/request/1",
        "app.test",
        "",
        "Open a file",
        "0",
    ]);
    assert!(out.status.success(), "the chooser call failed: {}", String::from_utf8_lossy(&out.stderr));

    let asked = f.asked(Duration::from_secs(10));
    assert!(asked.contains("ShowChooser"), "the listener never put the chooser to a window: {asked}");
    assert!(asked.contains("ChooserPoll"), "the listener never collected the answer: {asked}");
    assert!(asked.contains("tok-1"), "it collected without the window's token: {asked}");

    // And what the window chose is what the bus was told: response 0 (success) with the uri.
    let reply = String::from_utf8_lossy(&out.stdout);
    assert!(reply.contains("chosen.txt"), "the answer never came back: {reply}");
}
