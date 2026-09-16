//! kikid: the kiki file manager daemon. See docs/0.1.0/01-daemon-and-listing.md.

mod json;
mod kinds;
mod listing;
mod proto;
mod server;
mod string_pool;
mod vfs;
mod watch;

use std::os::unix::net::UnixListener;
use std::path::PathBuf;

fn main() {
    tune_allocator();
    let args: Vec<String> = std::env::args().collect();
    match args.get(1).map(String::as_str) {
        Some("bench") => bench(args.get(2).map(PathBuf::from)),
        Some("--version") | Some("-V") => println!("kikid {}", env!("CARGO_PKG_VERSION")),
        Some("--help") | Some("-h") => println!("usage: kikid [bench <dir>]"),
        _ => serve(),
    }
}

fn serve() {
    let listener = match systemd_socket() {
        Some(l) => l,
        None => {
            let path = socket_path();
            if let Some(dir) = path.parent() {
                let _ = std::fs::create_dir_all(dir);
            }
            let _ = std::fs::remove_file(&path);
            let l = UnixListener::bind(&path).unwrap_or_else(|e| {
                eprintln!("bind {}: {e}", path.display());
                std::process::exit(1)
            });
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let _ = std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600));
            }
            eprintln!("kikid listening on {}", path.display());
            l
        }
    };
    if let Err(e) = server::serve(listener) {
        eprintln!("serve: {e}");
        std::process::exit(1);
    }
}

fn socket_path() -> PathBuf {
    if let Ok(p) = std::env::var("KIKI_SOCKET") {
        return PathBuf::from(p);
    }
    let dir = std::env::var("XDG_RUNTIME_DIR").unwrap_or_else(|_| format!("/tmp/kiki-{}", unsafe { libc::getuid() }));
    PathBuf::from(dir).join("kiki.sock")
}

/// systemd socket activation: LISTEN_FDS=1 with the listener on fd 3.
fn systemd_socket() -> Option<UnixListener> {
    use std::os::unix::io::FromRawFd;
    let pid: u32 = std::env::var("LISTEN_PID").ok()?.parse().ok()?;
    if pid != std::process::id() {
        return None;
    }
    let n: u32 = std::env::var("LISTEN_FDS").ok()?.parse().ok()?;
    if n < 1 {
        return None;
    }
    Some(unsafe { UnixListener::from_raw_fd(3) })
}

/// Plan 01, allocator tuning: pin the mmap threshold, cap arenas, trim promptly.
fn tune_allocator() {
    #[cfg(all(target_os = "linux", target_env = "gnu"))]
    unsafe {
        libc::mallopt(libc::M_MMAP_THRESHOLD, 131072);
        libc::mallopt(libc::M_ARENA_MAX, 2);
        libc::mallopt(libc::M_TRIM_THRESHOLD, 1024 * 1024);
    }
}

/// `kikid bench <dir>`: time phase 1 and a full enrichment of one directory.
fn bench(dir: Option<PathBuf>) {
    use std::time::{Duration, Instant};
    let dir = dir.unwrap_or_else(|| std::env::var("HOME").map(PathBuf::from).unwrap_or_else(|_| PathBuf::from("/")));
    let uri = vfs::uri::Uri::from_path(&dir);
    let t0 = Instant::now();
    let (l, _) = match listing::open(&uri) {
        Ok(x) => x,
        Err(e) => {
            eprintln!("open: {}", e.message());
            std::process::exit(1)
        }
    };
    let (tx, rx) = std::sync::mpsc::channel();
    l.subscribe(listing::Subscriber { client: 0, lid: 1, tx, first: 0, count: 60 });
    let mut first_chunk = None;
    let mut done = None;
    while done.is_none() {
        match rx.recv_timeout(Duration::from_secs(60)) {
            Ok(ev) => {
                if ev.str_field("event") == Some("Count") {
                    if first_chunk.is_none() {
                        first_chunk = Some(t0.elapsed());
                    }
                    if ev.get("done").and_then(|d| d.as_bool()) == Some(true) {
                        done = Some(t0.elapsed());
                    }
                }
            }
            Err(_) => break,
        }
    }
    let (n, _) = l.count();
    let t1 = Instant::now();
    let w = l.window(0, 1, 0, 60);
    let window_ms = t1.elapsed();
    let rows = w.get("rows").and_then(|r| r.as_arr()).map(|a| a.len()).unwrap_or(0);
    let t2 = Instant::now();
    let (etx, erx) = std::sync::mpsc::channel();
    l.enrich(Some((etx, 1)));
    let _ = erx.recv_timeout(Duration::from_secs(600));
    let enrich = t2.elapsed();
    println!("dir            {}", dir.display());
    println!("entries        {n}");
    println!("first chunk    {:?}", first_chunk.unwrap_or_default());
    println!("phase 1 done   {:?}", done.unwrap_or_default());
    println!("window(0,60)   {:?} ({rows} rows)", window_ms);
    println!("enrich all     {:?}", enrich);
}
