//! Drives the `kiki-plugin-gio` binary over its pipe protocol against a **real Samba server** and
//! the **real GVfs stack** — `dbus-daemon`, `gvfsd`, `gvfsd-smb` — because that stack is the whole
//! of what this plugin is (plan 25), and there is nothing under it worth mocking: a mock of
//! gvfsd-smb would have agreed with every wrong thing the plugin believed before this test was
//! written.
//!
//! Everything is the test's own and lives under one temp directory: an `smbd` on a high port as
//! the user running the tests, with its own passdb, and a **private session bus** with a `gvfsd`
//! of its own on it. The developer's own gvfs mounts are neither used nor touched, and nothing is
//! left behind — mounts live inside that gvfsd, and it goes when the test does.
//!
//! Skipped, by name, where `smbd`, `pdbedit`, `dbus-daemon` or `gvfsd` is not installed:
//!
//!     sudo pacman -S samba gvfs-smb
//!
//! Covers plan 29 B's list at the plugin's own level — list, stat, mkdir, rename, delete, upload,
//! download with offset, set mtime — and the four answers plan 25 promised and none of which the
//! plugin gave before today: a refused password is `Auth` (and comes back at all), a share that is
//! not there is `Invalid` on the `share` field, an unreachable server is `Network` naming the
//! host, and a server that offers only SMB1 is refused in those words.

use kiki_plugin_sdk::json::{self, Value};
use kiki_plugin_sdk::{read_frame, write_binary, write_json};
use std::io::{BufRead, BufReader, Write};
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

const PASSWORD: &str = "kiki-contract-smb";
const SHARE: &str = "files";
const GUEST_SHARE: &str = "pub";

fn which(names: &[&str]) -> Option<PathBuf> {
    let path = std::env::var("PATH").unwrap_or_default();
    for n in names {
        if n.starts_with('/') {
            if Path::new(n).exists() {
                return Some(PathBuf::from(n));
            }
            continue;
        }
        for dir in path.split(':') {
            let p = Path::new(dir).join(n);
            if p.exists() {
                return Some(p);
            }
        }
    }
    None
}

/// What this test cannot be run without, or why.
fn tools() -> Result<(PathBuf, PathBuf, PathBuf, PathBuf), String> {
    let smbd = which(&["smbd", "/usr/bin/smbd", "/usr/sbin/smbd"]).ok_or("smbd is not installed (sudo pacman -S samba)")?;
    let pdbedit = which(&["pdbedit", "/usr/bin/pdbedit", "/usr/sbin/pdbedit"]).ok_or("pdbedit is not installed (sudo pacman -S samba)")?;
    let dbus = which(&["dbus-daemon"]).ok_or("dbus-daemon is not installed")?;
    let gvfsd = which(&["/usr/lib/gvfsd", "/usr/libexec/gvfsd", "/usr/lib/gvfs/gvfsd"]).ok_or("gvfsd is not installed (sudo pacman -S gvfs gvfs-smb)")?;
    Ok((smbd, pdbedit, dbus, gvfsd))
}

fn free_port() -> u16 {
    TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port()
}

fn up(port: u16, deadline: Duration) -> bool {
    let end = Instant::now() + deadline;
    while Instant::now() < end {
        if std::net::TcpStream::connect(("127.0.0.1", port)).is_ok() {
            return true;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    false
}

/// A server, a bus and a gvfsd, all this test's own; every one of them dies with it.
struct Fixture {
    dir: PathBuf,
    root: PathBuf,
    guest: PathBuf,
    port: u16,
    smb1_port: u16,
    address: String,
    procs: Vec<Child>,
}

impl Fixture {
    fn start() -> Result<Fixture, String> {
        let (smbd, pdbedit, dbus, gvfsd) = tools()?;
        // Short, and it has to be: Samba's messaging socket is `<lock directory>/msg.lock/<pid>`,
        // and a unix socket path is 107 bytes. Under anything deeper `smbd` prints two lines and
        // exits with "messaging_dgm_ref failed: File name too long".
        let dir = std::env::temp_dir().join(format!("kiki-smb-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let (root, guest) = (dir.join("share"), dir.join("guest"));
        std::fs::create_dir_all(&root).map_err(|e| e.to_string())?;
        std::fs::create_dir_all(&guest).map_err(|e| e.to_string())?;

        let mut f = Fixture { dir, root, guest, port: 0, smb1_port: 0, address: String::new(), procs: Vec::new() };
        f.port = f.smbd(&smbd, &pdbedit, "smbd", "SMB2")?;
        f.smb1_port = f.smbd(&smbd, &pdbedit, "smbd-nt1", "NT1")?;

        // A session bus of this test's own. dbus-daemon prints its address and stays in the
        // foreground; gvfsd is started on it, and gvfsd-smb is activated from it when the plugin
        // first asks for a share.
        //
        // **With a runtime directory of its own, and that is not a detail.** Whatever this bus
        // activates inherits the bus's environment, and gvfsd-smb asks for `org.freedesktop.
        // secrets` before it asks the server anything. Started from the developer's environment,
        // that activation found the developer's own keyring and the mount never came back — a
        // test that hung for exactly as long as somebody left it, on the machine's real secrets.
        let runtime = f.dir.join("run");
        std::fs::create_dir_all(&runtime).map_err(|e| e.to_string())?;
        let mut bus = Command::new(&dbus)
            .args(["--session", "--nofork", "--print-address"])
            .env("XDG_RUNTIME_DIR", &runtime)
            .env("GVFS_DISABLE_FUSE", "1")
            .env_remove("DBUS_SESSION_BUS_ADDRESS")
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|e| format!("dbus-daemon: {e}"))?;
        let mut line = String::new();
        BufReader::new(bus.stdout.take().unwrap()).read_line(&mut line).map_err(|e| e.to_string())?;
        f.address = line.trim().to_string();
        f.procs.push(bus);
        if f.address.is_empty() {
            return Err("dbus-daemon printed no address".into());
        }
        f.procs.push(
            Command::new(&gvfsd)
                .arg("--no-fuse")
                .env("DBUS_SESSION_BUS_ADDRESS", &f.address)
                .env("XDG_RUNTIME_DIR", &runtime)
                .env("GVFS_DISABLE_FUSE", "1")
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
                .map_err(|e| format!("gvfsd: {e}"))?,
        );
        Ok(f)
    }

    /// One `smbd`, unprivileged: see `tests/e2e/servers.py` for the same recipe and the rest of
    /// what it took. Note the absence of `--no-process-group`, which would have `smbd` signal
    /// *this* process group on the way out.
    fn smbd(&mut self, smbd: &Path, pdbedit: &Path, tag: &str, protocol: &str) -> Result<u16, String> {
        let d = self.dir.join(tag);
        for sub in ["private", "lock", "state", "cache", "run", "log"] {
            std::fs::create_dir_all(d.join(sub)).map_err(|e| e.to_string())?;
        }
        let port = free_port();
        let me = std::env::var("USER").or_else(|_| std::env::var("LOGNAME")).map_err(|_| "no USER in the environment".to_string())?;
        let d = d.display().to_string();
        let max = if protocol == "SMB2" { String::new() } else { format!("   server max protocol = {protocol}\n") };
        let conf = format!(
            "[global]\n   workgroup = WORKGROUP\n   smb ports = {port}\n   bind interfaces only = yes\n   interfaces = 127.0.0.1\n\
             \x20  server min protocol = {protocol}\n{max}   security = user\n   store dos attributes = no\n   map hidden = yes\n\
             \x20  map archive = no\n   map system = no\n   create mask = 0755\n   map to guest = Bad User\n   guest account = {me}\n\
             \x20  passdb backend = tdbsam:{d}/private/passdb.tdb\n   private dir = {d}/private\n   lock directory = {d}/lock\n\
             \x20  state directory = {d}/state\n   cache directory = {d}/cache\n   pid directory = {d}/run\n   ncalrpc dir = {d}/run\n\
             \x20  log file = {d}/log/smbd.log\n   log level = 1\n   load printers = no\n   printing = bsd\n   printcap name = /dev/null\n\
             \x20  disable spoolss = yes\n   panic action = /bin/true\n\n\
             [{SHARE}]\n   path = {}\n   read only = no\n   guest ok = no\n   force user = {me}\n\n\
             [{GUEST_SHARE}]\n   path = {}\n   read only = no\n   guest ok = yes\n   force user = {me}\n",
            self.root.display(),
            self.guest.display()
        );
        let conf_path = format!("{d}/smb.conf");
        std::fs::write(&conf_path, conf).map_err(|e| e.to_string())?;
        // `smbpasswd -a` wants root and `smbpasswd -L` says so; pdbedit against a private passdb
        // does not. The unix account has to exist, which is why it is this one.
        let mut add = Command::new(pdbedit).args(["-s", &conf_path, "-a", "-u", &me, "-t"]).stdin(Stdio::piped()).stdout(Stdio::null()).stderr(Stdio::null()).spawn().map_err(|e| e.to_string())?;
        add.stdin.take().unwrap().write_all(format!("{PASSWORD}\n{PASSWORD}\n").as_bytes()).map_err(|e| e.to_string())?;
        if !add.wait().map_err(|e| e.to_string())?.success() {
            return Err("pdbedit could not make the account".into());
        }
        let log = std::fs::File::create(format!("{d}/smbd.out")).map_err(|e| e.to_string())?;
        self.procs.push(Command::new(smbd).args(["-F", "-s", &conf_path, "--debug-stdout"]).stdout(log).stderr(Stdio::null()).spawn().map_err(|e| format!("smbd: {e}"))?);
        if !up(port, Duration::from_secs(15)) {
            return Err(format!("smbd never listened: {}", std::fs::read_to_string(format!("{d}/smbd.out")).unwrap_or_default()));
        }
        Ok(port)
    }

    fn config(&self, share: &str, auth: &str) -> Value {
        self.config_on(self.port, share, auth)
    }

    fn config_on(&self, port: u16, share: &str, auth: &str) -> Value {
        // The port travels in the Host field — `smb://host:port/share` is what gvfsd-smb is given
        // and what it honours; there is no Port field in the SMB form (plan 25).
        let me = std::env::var("USER").unwrap_or_default();
        Value::obj().s("host", format!("127.0.0.1:{port}")).s("share", share).s("auth", auth).s("username", if auth == "guest" { String::new() } else { me }).s("domain", "").done()
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        // TERM before KILL, and it matters for `smbd`: it runs `samba-dcerpcd` and a pair of
        // `rpcd_*` workers beside itself and takes them with it by signalling its own process
        // group — which a KILL never gives it the chance to do. Killed outright, the master went
        // and its helpers stayed behind holding the fixture's config file open.
        for p in self.procs.iter_mut() {
            let _ = Command::new("kill").args(["-TERM", &p.id().to_string()]).stderr(Stdio::null()).status();
        }
        let end = Instant::now() + Duration::from_secs(5);
        while Instant::now() < end && self.procs.iter_mut().any(|p| matches!(p.try_wait(), Ok(None))) {
            std::thread::sleep(Duration::from_millis(50));
        }
        for p in self.procs.iter_mut() {
            let _ = p.kill();
            let _ = p.wait();
        }
        // And what `smbd` started beside itself: `samba-dcerpcd` and its `rpcd_classic` and
        // `rpcd_winreg` workers are not in its process group and outlive it. They are known by
        // this fixture's config path, which no process outside this test has.
        let mine = self.dir.display().to_string();
        if let Ok(procs) = std::fs::read_dir("/proc") {
            for e in procs.flatten() {
                let pid = e.file_name().to_string_lossy().to_string();
                if !pid.chars().all(|c| c.is_ascii_digit()) {
                    continue;
                }
                match std::fs::read(e.path().join("cmdline")) {
                    Ok(c) if String::from_utf8_lossy(&c).contains(&mine) => {
                        let _ = Command::new("kill").args(["-TERM", &pid]).stderr(Stdio::null()).status();
                    }
                    _ => {}
                }
            }
        }
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

// ---------------------------------------------------------------- plugin driver

struct Plugin {
    child: Child,
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
    next: u64,
    pid: u32,
}

impl Plugin {
    fn spawn(f: &Fixture) -> Plugin {
        let mut child = Command::new(env!("CARGO_BIN_EXE_kiki-plugin-gio"))
            .env("DBUS_SESSION_BUS_ADDRESS", &f.address)
            .env("XDG_RUNTIME_DIR", f.dir.join("run"))
            .env("GVFS_DISABLE_FUSE", "1")
            // A GLib critical is a bug in this plugin, not a line in a log: every ordinary file
            // in a listing used to print two of them.
            .env("G_DEBUG", "fatal-criticals")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .expect("spawn plugin");
        let pid = child.id();
        let stdin = child.stdin.take().unwrap();
        let stdout = BufReader::new(child.stdout.take().unwrap());
        Plugin { child, stdin, stdout, next: 1, pid }
    }

    /// Kills the plugin if the next request has not been answered within `secs`, so that a
    /// request that never comes back is a failed test and not a test suite that never ends. It is
    /// how the wrong-password loop below would show up again: it hung for ever.
    fn deadline(&self, secs: u64) -> Arc<AtomicBool> {
        let done = Arc::new(AtomicBool::new(false));
        let (flag, pid) = (done.clone(), self.pid);
        std::thread::spawn(move || {
            let end = Instant::now() + Duration::from_secs(secs);
            while Instant::now() < end {
                if flag.load(Ordering::SeqCst) {
                    return;
                }
                std::thread::sleep(Duration::from_millis(100));
            }
            eprintln!("the plugin did not answer within {secs} s — killing it");
            let _ = Command::new("kill").args(["-KILL", &pid.to_string()]).stderr(Stdio::null()).status();
        });
        done
    }

    fn req(&mut self, mut v: Value) -> Value {
        let id = self.next;
        self.next += 1;
        if let Value::Obj(m) = &mut v {
            m.insert("id".into(), Value::Uint(id));
        }
        write_json(&mut self.stdin, &v).unwrap();
        loop {
            let (kind, payload) = read_frame(&mut self.stdout).unwrap().expect("plugin closed its stdout");
            assert_eq!(kind, 0, "unexpected binary frame");
            let f = json::parse(&payload).unwrap();
            if f.get("ok").is_some() || f.get("err").is_some() {
                return f;
            }
        }
    }

    fn ok(&mut self, v: Value) -> Value {
        let r = self.req(v);
        r.get("ok").unwrap_or_else(|| panic!("expected ok, got {}", json::to_string(&r))).clone()
    }

    fn err(&mut self, v: Value) -> (String, String, String) {
        let r = self.req(v);
        let e = r.get("err").unwrap_or_else(|| panic!("expected an error, got {}", json::to_string(&r)));
        (e.str_field("code").unwrap_or("").into(), e.str_field("message").unwrap_or("").into(), e.str_field("field").unwrap_or("").into())
    }

    fn connect(&mut self, location: &str, config: Value) -> Value {
        self.req(Value::obj().s("type", "Connect").s("location", location).s("role", "browse").v("config", config).v("secrets", Value::obj().s("password", PASSWORD).done()).done())
    }

    fn scan(&mut self, location: &str, path: &str) -> Vec<Value> {
        let id = self.next;
        self.next += 1;
        write_json(&mut self.stdin, &Value::obj().u("id", id).s("type", "Scan").s("location", location).s("path", path).b("recursive", false).done()).unwrap();
        let mut entries = Vec::new();
        loop {
            let (_, payload) = read_frame(&mut self.stdout).unwrap().expect("eof");
            let f = json::parse(&payload).unwrap();
            if let Some(es) = f.get("entries").and_then(Value::as_arr) {
                entries.extend(es.iter().cloned());
                continue;
            }
            assert!(f.get("ok").is_some(), "scan failed: {}", json::to_string(&f));
            assert_eq!(f.get("ok").unwrap().u64_field("n"), Some(entries.len() as u64), "n must equal the streamed entry count");
            return entries;
        }
    }

    fn read(&mut self, location: &str, path: &str, offset: u64) -> Vec<u8> {
        let id = self.next;
        self.next += 1;
        write_json(&mut self.stdin, &Value::obj().u("id", id).s("type", "Read").s("location", location).s("path", path).u("offset", offset).done()).unwrap();
        let mut bytes = Vec::new();
        loop {
            let (kind, payload) = read_frame(&mut self.stdout).unwrap().expect("eof");
            assert_eq!(kind, 1, "{}", String::from_utf8_lossy(&payload));
            if payload.is_empty() {
                break;
            }
            bytes.extend_from_slice(&payload);
        }
        let (_, payload) = read_frame(&mut self.stdout).unwrap().expect("eof");
        let r = json::parse(&payload).unwrap();
        assert_eq!(r.get("ok").and_then(|o| o.u64_field("bytes")), Some(bytes.len() as u64), "{}", json::to_string(&r));
        bytes
    }

    fn write(&mut self, location: &str, path: &str, data: &[u8], mtime_ms: u64) -> Value {
        let id = self.next;
        self.next += 1;
        write_json(&mut self.stdin, &Value::obj().u("id", id).s("type", "Write").s("location", location).s("path", path).u("size", data.len() as u64).u("mtime", mtime_ms).done()).unwrap();
        for c in data.chunks(200_000) {
            write_binary(&mut self.stdin, c).unwrap();
        }
        write_binary(&mut self.stdin, &[]).unwrap();
        self.stdin.flush().unwrap();
        let (_, payload) = read_frame(&mut self.stdout).unwrap().expect("eof");
        json::parse(&payload).unwrap()
    }
}

impl Drop for Plugin {
    fn drop(&mut self) {
        let _ = write_json(&mut self.stdin, &Value::obj().u("id", 999_999).s("type", "Shutdown").done());
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn entry<'a>(entries: &'a [Value], name: &str) -> &'a Value {
    entries.iter().find(|e| e.str_field("name") == Some(name)).unwrap_or_else(|| panic!("no {name} in {}", json::to_string(&Value::Arr(entries.to_vec()))))
}

// ---------------------------------------------------------------- the test

#[test]
fn smb_through_gvfs_against_a_real_server() {
    let mut f = match Fixture::start() {
        Ok(f) => f,
        Err(why) => {
            eprintln!("smb_through_gvfs_against_a_real_server skipped: {why}");
            return;
        }
    };
    let mut p = Plugin::spawn(&f);

    // ------------------------------------------------ what it says it is
    let d = p.ok(Value::obj().s("type", "Describe").done());
    assert_eq!(d.str_field("scheme"), Some("smb"));
    assert_eq!(d.get("available").and_then(Value::as_bool), Some(true), "gvfsd is running and answers for smb: {}", json::to_string(&d));
    let features = d.get("features").unwrap();
    assert_eq!(features.get("setMtime").and_then(Value::as_bool), Some(true));
    assert_eq!(features.get("mode").and_then(Value::as_bool), Some(false));
    assert_eq!(features.get("metaInScan").and_then(Value::as_bool), Some(true));
    assert_eq!(features.get("partialRead").and_then(Value::as_bool), Some(true));

    // ------------------------------------------------ the credentials, before anything is mounted
    // First, because a mount is shared with everything else on the bus: once the share is
    // mounted, GIO answers AlreadyMounted and a wrong password would be accepted by a plugin that
    // never sent it. Before the fix this call did not come back at all — gvfsd-smb asks for the
    // password again after every refusal, for ever, and the plugin kept answering.
    let guard = p.deadline(45);
    let t = Instant::now();
    let r = p.req(
        Value::obj()
            .s("type", "Connect")
            .s("location", "wrong")
            .s("role", "browse")
            .v("config", f.config(SHARE, "password"))
            .v("secrets", Value::obj().s("password", "not the password").done())
            .done(),
    );
    guard.store(true, Ordering::SeqCst);
    let e = r.get("err").unwrap_or_else(|| panic!("a wrong password was accepted: {}", json::to_string(&r)));
    assert_eq!(e.str_field("code"), Some("Auth"), "{}", json::to_string(&r));
    assert!(e.str_field("message").unwrap_or("").contains("refused"), "in words a person can act on: {}", json::to_string(&r));
    assert!(t.elapsed() < Duration::from_secs(30), "and it answered at once, not eventually: {:?}", t.elapsed());

    // A share that is not there is the `share` field's problem, named — not "No such file or
    // directory" about a Windows share nobody mentioned.
    let (code, message, field) = p.err(
        Value::obj().s("type", "Connect").s("location", "gone").s("role", "browse").v("config", f.config("nosuchshare", "password")).v("secrets", Value::obj().s("password", PASSWORD).done()).done(),
    );
    assert_eq!((code.as_str(), field.as_str()), ("Invalid", "share"), "{message}");
    assert!(message.contains("nosuchshare"), "{message}");

    // A server that is not answering is the network's problem, and says which host.
    let (code, message, _) = p.err(
        Value::obj()
            .s("type", "Connect")
            .s("location", "off")
            .s("role", "browse")
            .v("config", f.config_on(free_port(), SHARE, "password"))
            .v("secrets", Value::obj().s("password", PASSWORD).done())
            .done(),
    );
    assert_eq!(code, "Network", "{message}");
    assert!(message.contains("127.0.0.1"), "naming the host: {message}");

    // A server that offers only SMB1 is refused in those words. Plan 25 wrote this message match
    // from Samba's source and never saw a server; what gvfsd-smb really hands over is an errno.
    let (code, message, _) = p.err(
        Value::obj().s("type", "Connect").s("location", "old").s("role", "browse").v("config", f.config_on(f.smb1_port, SHARE, "password")).v("secrets", Value::obj().s("password", PASSWORD).done()).done(),
    );
    assert_eq!((code.as_str(), message.as_str()), ("Network", "This server only offers SMB1, which kiki does not support."));

    // ------------------------------------------------ signed in
    let r = p.connect("lab", f.config(SHARE, "password"));
    assert!(r.get("ok").is_some(), "connect: {}", json::to_string(&r));
    let caps = p.ok(Value::obj().s("type", "Capabilities").s("location", "lab").done());
    assert_eq!(caps.get("trash").and_then(Value::as_bool), Some(false), "there is no trash on a share");
    assert_eq!(caps.str_field("separator"), Some("/"));

    // A guest share takes no credentials at all.
    std::fs::write(f.guest.join("open.txt"), b"anyone").unwrap();
    let r = p.req(Value::obj().s("type", "Connect").s("location", "open").s("role", "browse").v("config", f.config(GUEST_SHARE, "guest")).v("secrets", Value::obj().done()).done());
    assert!(r.get("ok").is_some(), "the guest share takes anybody: {}", json::to_string(&r));
    assert_eq!(p.scan("open", "/").len(), 1);

    // ------------------------------------------------ listing
    std::fs::create_dir_all(f.root.join("docs/deep")).unwrap();
    std::fs::write(f.root.join("docs/readme.md"), b"hello").unwrap();
    std::fs::write(f.root.join("secret.txt"), b"s").unwrap();
    std::fs::write(f.root.join("plain.txt"), b"p").unwrap();
    // The DOS Hidden attribute, which on this server is the other-execute bit (`map hidden`).
    std::fs::set_permissions(f.root.join("secret.txt"), std::os::unix::fs::PermissionsExt::from_mode(0o645)).unwrap();
    let when = 1_400_000_000u64;
    let ft = std::fs::FileTimes::new().set_modified(std::time::UNIX_EPOCH + Duration::from_secs(when));
    std::fs::File::options().write(true).open(f.root.join("docs/readme.md")).unwrap().set_times(ft).unwrap();

    let top = p.scan("lab", "/");
    let mut names: Vec<&str> = top.iter().filter_map(|e| e.str_field("name")).collect();
    names.sort();
    assert_eq!(names, vec!["docs", "plain.txt", "secret.txt"]);
    assert_eq!(entry(&top, "docs").str_field("kind"), Some("dir"));
    // metaInScan: the attributes came with the names, so nothing needs a Stat.
    assert_eq!(entry(&top, "plain.txt").get("meta").unwrap().u64_field("size"), Some(1));
    assert_eq!(entry(&top, "secret.txt").get("meta").unwrap().get("hidden").and_then(Value::as_bool), Some(true), "the DOS Hidden attribute reaches Meta.hidden");
    assert!(entry(&top, "plain.txt").get("meta").unwrap().get("hidden").is_none(), "and an ordinary file is not hidden");

    let docs = p.scan("lab", "/docs");
    assert_eq!(entry(&docs, "readme.md").get("meta").unwrap().u64_field("mtime"), Some(when * 1000), "the server's own time, to the millisecond");
    let stat = p.ok(Value::obj().s("type", "Stat").s("location", "lab").s("path", "/docs/readme.md").done());
    assert_eq!((stat.u64_field("size"), stat.u64_field("mtime")), (Some(5), Some(when * 1000)));

    // ------------------------------------------------ reading
    let payload: Vec<u8> = (0..3 * 1024 * 1024u32).map(|i| (i.wrapping_mul(2_654_435_761) >> 13) as u8).collect();
    std::fs::write(f.root.join("data.bin"), &payload).unwrap();
    assert_eq!(p.read("lab", "/data.bin", 0), payload, "every byte of it");
    // partialRead: a read that starts in the middle, which is what a resumed transfer does.
    assert_eq!(p.read("lab", "/data.bin", 3_000_000), payload[3_000_000..], "and from wherever it is asked to start");

    // ------------------------------------------------ writing
    let r = p.write("lab", "/docs/new.txt", b"written by kiki", when * 1000);
    assert_eq!(r.get("ok").and_then(|o| o.u64_field("bytes")), Some(15), "{}", json::to_string(&r));
    assert_eq!(std::fs::read(f.root.join("docs/new.txt")).unwrap(), b"written by kiki");
    let stat = p.ok(Value::obj().s("type", "Stat").s("location", "lab").s("path", "/docs/new.txt").done());
    assert_eq!(stat.u64_field("mtime"), Some(when * 1000), "an upload keeps the time it was given — which is why smb declares setMtime");
    // And on its own, which is what the mirror asks for after the fact.
    p.ok(Value::obj().s("type", "SetMtime").s("location", "lab").s("path", "/docs/new.txt").u("mtime", 1_500_000_000_000u64).done());
    assert_eq!(f.root.join("docs/new.txt").metadata().unwrap().modified().unwrap().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs(), 1_500_000_000);

    // A name with the characters SMB does allow. (`?`, `*`, `<`, `>`, `|`, `"` and `\` it does
    // not: Samba hands back an 8.3 name instead, and no client can do anything about that.)
    let odd = "a name with spaces, üñí, 100% and a #.txt";
    assert!(p.write("lab", &format!("/{odd}"), b"odd", 0).get("ok").is_some());
    assert_eq!(std::fs::read(f.root.join(odd)).unwrap(), b"odd");
    assert_eq!(p.read("lab", &format!("/{odd}"), 0), b"odd");

    // ------------------------------------------------ making, moving, removing
    p.ok(Value::obj().s("type", "Mkdir").s("location", "lab").s("path", "/made").done());
    assert!(f.root.join("made").is_dir());
    p.ok(Value::obj().s("type", "Rename").s("location", "lab").s("from", "/made").s("to", "/moved").done());
    assert!(!f.root.join("made").exists() && f.root.join("moved").is_dir());
    p.ok(Value::obj().s("type", "Delete").s("location", "lab").s("path", "/moved").done());
    assert!(!f.root.join("moved").exists());
    assert_eq!(p.err(Value::obj().s("type", "Delete").s("location", "lab").s("path", "/docs").done()).0, "NotEmpty");
    assert_eq!(p.err(Value::obj().s("type", "Delete").s("location", "lab").s("path", "/nothing-here").done()).0, "NotFound");
    assert_eq!(p.err(Value::obj().s("type", "Stat").s("location", "lab").s("path", "/nothing-here").done()).0, "NotFound");
    // A rename onto a name that is taken is refused rather than silently swallowing it; the
    // daemon takes the old one away itself when that is what was asked for (mirror::put).
    assert_eq!(p.err(Value::obj().s("type", "Rename").s("location", "lab").s("from", "/plain.txt").s("to", "/secret.txt").done()).0, "Exists");

    // ------------------------------------------------ a job's session beside the browser's
    // A transfer gets a session of its own (`role: job-<id>`), and when it ends the daemon
    // disconnects that one. It used to disconnect the only one there was: this plugin kept its
    // sessions under the location's name alone, so a job found the browser's entry, reused it,
    // and took it away on the way out. Everything on the share answered "not connected" from the
    // first copy until kiki was restarted.
    p.ok(Value::obj().s("type", "Connect").s("location", "lab").s("role", "job-1").v("config", f.config(SHARE, "password")).v("secrets", Value::obj().s("password", PASSWORD).done()).done());
    p.ok(Value::obj().s("type", "Stat").s("location", "lab").s("role", "job-1").s("path", "/docs/readme.md").done());
    p.req(Value::obj().s("type", "Disconnect").s("location", "lab").s("role", "job-1").done());
    p.ok(Value::obj().s("type", "Stat").s("location", "lab").s("path", "/docs/readme.md").done());
    assert_eq!(
        p.err(Value::obj().s("type", "Stat").s("location", "lab").s("role", "job-1").s("path", "/docs/readme.md").done()).0,
        "Network",
        "and the job's own session really is gone with it"
    );

    // ------------------------------------------------ what the dialog's Browse… offers
    let shares = p.ok(Value::obj().s("type", "Browse").s("field", "share").v("config", f.config(SHARE, "password")).v("secrets", Value::obj().s("password", PASSWORD).done()).done());
    let offered: Vec<&str> = shares.get("options").and_then(Value::as_arr).unwrap().iter().filter_map(|o| o.str_field("value")).collect();
    assert!(offered.contains(&SHARE) && offered.contains(&GUEST_SHARE), "{offered:?}");
    assert!(!offered.iter().any(|s| s.ends_with('$')), "and not the admin shares: {offered:?}");

    // ------------------------------------------------ the server goes away under us
    // Whatever is asked next must say so and come back, not wait on a socket nobody is listening
    // to. (The `smbd` for this share is the first of the fixture's processes.)
    let _ = f.procs[0].kill();
    let _ = f.procs[0].wait();
    let guard = p.deadline(45);
    let t = Instant::now();
    let (code, message, _) = p.err(Value::obj().s("type", "Stat").s("location", "lab").s("path", "/docs/readme.md").done());
    guard.store(true, Ordering::SeqCst);
    assert_eq!(code, "Network", "{message}");
    assert!(t.elapsed() < Duration::from_secs(30), "and it said so at once: {:?}", t.elapsed());
}
