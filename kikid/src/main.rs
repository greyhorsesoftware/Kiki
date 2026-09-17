//! kikid: the kiki file manager daemon. See docs/0.1.0/01-daemon-and-listing.md.

use kikid::server;

use std::os::unix::net::UnixListener;
use std::path::PathBuf;

fn main() {
    tune_allocator();
    let args: Vec<String> = std::env::args().collect();
    match args.get(1).map(String::as_str) {
        Some("bench") => bench(&args[2..]),
        Some("--version") | Some("-V") => println!("kikid {}", env!("CARGO_PKG_VERSION")),
        Some("--help") | Some("-h") => println!("usage: kikid [bench gen|run|compare …]"),
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
    kikid::dbus::start();
    kikid::plugin::start_reaper();
    kikid::devices::start();
    std::thread::spawn(|| {
        std::thread::sleep(std::time::Duration::from_secs(30));
        kikid::index::start();
    });
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

/// `kikid bench gen <profile> <dir>` | `bench run <dir> [--json out]` | `bench compare <base> <new> [--tolerance pct]` | `bench <dir>`
fn bench(args: &[String]) {
    let usage = || {
        eprintln!("usage: kikid bench gen <flat10k|flat200k|deep100k|photos|all> <dir>\n       kikid bench run <dir> [--json <out>]\n       kikid bench compare <baseline.json> <results.json> [--tolerance <pct>]");
        std::process::exit(2)
    };
    match args.first().map(String::as_str) {
        Some("gen") => {
            let (Some(profile), Some(dir)) = (args.get(1), args.get(2)) else { usage() };
            if let Err(e) = kikid::bench::gen(profile, &PathBuf::from(dir)) {
                eprintln!("gen: {e}");
                std::process::exit(1)
            }
        }
        Some("run") | Some(_) | None => {
            let dir = if args.first().map(String::as_str) == Some("run") { args.get(1) } else { args.first() };
            let dir = dir.map(PathBuf::from).unwrap_or_else(|| std::env::var("HOME").map(PathBuf::from).unwrap_or_else(|_| PathBuf::from("/")));
            if args.first().map(String::as_str) == Some("compare") {
                let (Some(b), Some(n)) = (args.get(1), args.get(2)) else { usage() };
                let tol: f64 = args.iter().position(|a| a == "--tolerance").and_then(|i| args.get(i + 1)).and_then(|t| t.parse().ok()).unwrap_or(25.0);
                let read = |p: &String| {
                    kikid::json::parse(&std::fs::read(p).unwrap_or_else(|e| {
                        eprintln!("{p}: {e}");
                        std::process::exit(1)
                    }))
                    .unwrap_or_else(|_| {
                        eprintln!("{p}: not JSON");
                        std::process::exit(1)
                    })
                };
                let regressions = kikid::bench::compare(&read(b), &read(n), tol);
                for (p, k, bv, nv) in &regressions {
                    println!("REGRESSION {p}.{k}: {bv} -> {nv} (+{:.0}%)", (nv / bv - 1.0) * 100.0);
                }
                if regressions.is_empty() {
                    println!("no regressions beyond {tol}%");
                } else {
                    std::process::exit(1)
                }
                return;
            }
            let v = kikid::bench::run(&dir);
            kikid::bench::print_table(&v);
            if let Some(out) = args.iter().position(|a| a == "--json").and_then(|i| args.get(i + 1)) {
                if let Err(e) = std::fs::write(out, kikid::json::to_string(&v)) {
                    eprintln!("{out}: {e}");
                    std::process::exit(1)
                }
                eprintln!("wrote {out}");
            }
        }
    }
}
