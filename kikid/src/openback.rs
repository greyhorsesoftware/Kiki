//! Files fetched to be opened, and written back when saved (0.2.0).
//!
//! A remote file has nothing an application on this machine can open, so it is first brought to
//! a folder of its own under the cache — `$XDG_CACHE_HOME/kiki/open/<moment>/<name>` — by an
//! ordinary copy job (the orb shows it, it can be cancelled, its wire is logged), and the
//! application is started on the copy once the job is done (`localise`). The folder is watched
//! for as long as the daemon runs: a save — `IN_CLOSE_WRITE`, or the rename an editor finishes
//! with — sends the copy back to where it came from by another copy job, replacing without
//! asking (owner, 2026-09-24: "can we detect save events, so on save we can reupload?"). The
//! copies are for the editing session and nothing more (owner, 2026-09-25): where each came from
//! is kept in memory, and at start the daemon empties the folder — nothing from a previous life
//! is watched or kept.

use crate::json::Value;
use crate::vfs::uri::Uri;
use std::collections::HashMap;
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};
use std::os::unix::ffi::OsStrExt;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

/// A save is often several writes and a rename within a moment; one upload for the lot.
const SETTLE: Duration = Duration::from_millis(400);

struct State {
    fd: OwnedFd,
    by_wd: HashMap<i32, PathBuf>,
    /// Where each fetched copy came from.
    origin: HashMap<PathBuf, Uri>,
    /// The last save seen of each copy: the upload waits for the saves to stop.
    last: HashMap<PathBuf, Instant>,
}

fn state() -> &'static Mutex<State> {
    static S: OnceLock<Mutex<State>> = OnceLock::new();
    S.get_or_init(|| {
        // The raw descriptor is ours alone; the `OwnedFd` closes it, if ever.
        let fd = unsafe {
            let raw = libc::inotify_init1(libc::IN_CLOEXEC);
            if raw < 0 {
                panic!("inotify_init: {}", std::io::Error::last_os_error());
            }
            OwnedFd::from_raw_fd(raw)
        };
        let raw = fd.as_raw_fd();
        std::thread::Builder::new().name("openback".into()).spawn(move || reader(raw)).expect("spawn openback");
        Mutex::new(State { fd, by_wd: HashMap::new(), origin: HashMap::new(), last: HashMap::new() })
    })
}

/// Where fetched copies go: `$XDG_CACHE_HOME/kiki/open`.
pub fn cache_dir() -> Result<PathBuf, String> {
    let base = std::env::var("XDG_CACHE_HOME").map(PathBuf::from).unwrap_or_else(|_| crate::config::home().join(".cache"));
    let dir = base.join("kiki").join("open");
    std::fs::create_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    Ok(dir)
}

/// Brings `uris` (remote, all of them) to a folder of the moment: the copy job's id, and where
/// each will be once it is done, in the order given.
pub fn fetch(uris: &[Uri]) -> Result<(u64, Vec<PathBuf>), String> {
    let moment = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis()).unwrap_or(0);
    let mut dir = cache_dir()?.join(moment.to_string());
    let mut n = 0;
    while dir.exists() {
        n += 1;
        dir = dir.with_file_name(format!("{moment}-{n}"));
    }
    std::fs::create_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let locals: Vec<PathBuf> = uris.iter().map(|u| dir.join(u.name())).collect();
    watch_dir(&dir, uris.iter().cloned().zip(locals.iter().cloned()));
    let op = Value::obj().s("op", "copy").v("items", Value::Arr(uris.iter().map(|u| Value::Str(u.to_string())).collect())).s("dest", Uri::from_path(&dir).to_string()).done();
    let job = crate::jobs::submit(op, None).map_err(|(_, m)| m)?;
    Ok((job, locals))
}

/// Runs `then` on a thread of its own once the job has ended, with whether it finished.
pub fn when_done(job: u64, then: impl FnOnce(bool) + Send + 'static) {
    let spawned = std::thread::Builder::new().name("openback-wait".into()).spawn(move || {
        let ok = matches!(crate::jobs::wait(job, Duration::from_secs(6 * 3600)), Some(crate::jobs::State::Done));
        then(ok)
    });
    if let Err(e) = spawned {
        eprintln!("openback: {e}");
    }
}

/// The URIs an application can be given: local ones as they are, remote ones fetched first.
/// With nothing remote `then` runs at once and there is no job; otherwise it runs when the
/// fetch is done, and the job's id comes back so the window can follow it.
pub fn localise(uris: &[Uri], then: impl FnOnce(Vec<Uri>) + Send + 'static) -> Result<Option<u64>, String> {
    let remote: Vec<Uri> = uris.iter().filter(|u| !u.is_local()).cloned().collect();
    if remote.is_empty() {
        then(uris.to_vec());
        return Ok(None);
    }
    let (job, locals) = fetch(&remote)?;
    let mut fetched = locals.into_iter();
    let all: Vec<Uri> = uris.iter().map(|u| if u.is_local() { u.clone() } else { Uri::from_path(&fetched.next().expect("one copy per remote uri")) }).collect();
    when_done(job, move |ok| {
        if ok {
            then(all)
        } else {
            eprintln!("openback: the fetch did not finish; nothing opened");
        }
    });
    Ok(Some(job))
}

/// Watches a fetched folder, remembering where each copy in it came from.
fn watch_dir(dir: &Path, origins: impl Iterator<Item = (Uri, PathBuf)>) {
    let mut s = state().lock().unwrap();
    for (remote, local) in origins {
        s.origin.insert(local, remote);
    }
    let c_path = match std::ffi::CString::new(dir.as_os_str().as_bytes()) {
        Ok(c) => c,
        Err(_) => return,
    };
    // NUL-terminated, and the descriptor lives as long as the state does.
    let wd = unsafe { libc::inotify_add_watch(s.fd.as_raw_fd(), c_path.as_ptr(), libc::IN_CLOSE_WRITE | libc::IN_MOVED_TO) };
    if wd >= 0 {
        s.by_wd.insert(wd, dir.to_path_buf());
    } else {
        eprintln!("openback: cannot watch {}: {}", dir.display(), std::io::Error::last_os_error());
    }
}

/// At the daemon's start: whatever a previous life fetched goes. The copies were for that
/// session's editing, and nothing watches them now.
pub fn clear() {
    let Ok(dir) = cache_dir() else { return };
    let Ok(entries) = std::fs::read_dir(&dir) else { return };
    for e in entries.flatten() {
        let _ = std::fs::remove_dir_all(e.path());
    }
}

fn reader(fd: i32) {
    let mut buf = vec![0u8; 64 * 1024];
    loop {
        // The descriptor stays open for the daemon's life; the buffer is ours and sized.
        let n = unsafe { libc::read(fd, buf.as_mut_ptr() as *mut libc::c_void, buf.len()) };
        if n <= 0 {
            std::thread::sleep(Duration::from_millis(50));
            continue;
        }
        let mut at = 0usize;
        while at + std::mem::size_of::<libc::inotify_event>() <= n as usize {
            // Aligned as the kernel laid it out, in the buffer we handed it.
            let ev = unsafe { &*(buf.as_ptr().add(at) as *const libc::inotify_event) };
            let head = std::mem::size_of::<libc::inotify_event>();
            let name_bytes = &buf[at + head..at + head + ev.len as usize];
            let name = String::from_utf8_lossy(name_bytes.split(|b| *b == 0).next().unwrap_or(&[])).into_owned();
            at += head + ev.len as usize;
            if ev.mask & (libc::IN_CLOSE_WRITE | libc::IN_MOVED_TO) == 0 || name.is_empty() {
                continue;
            }
            let (path, remote) = {
                let s = state().lock().unwrap();
                let Some(dir) = s.by_wd.get(&ev.wd) else { continue };
                let path = dir.join(&name);
                let Some(remote) = s.origin.get(&path).cloned() else { continue };
                (path, remote)
            };
            saved(path, remote);
        }
    }
}

/// A save seen: send the copy back once the saves have stopped for a moment.
fn saved(local: PathBuf, remote: Uri) {
    let now = Instant::now();
    state().lock().unwrap().last.insert(local.clone(), now);
    let spawned = std::thread::Builder::new().name("openback-save".into()).spawn(move || {
        std::thread::sleep(SETTLE);
        if state().lock().unwrap().last.get(&local) != Some(&now) {
            return; // saved again since: that one uploads
        }
        write_back(&local, &remote);
    });
    if let Err(e) = spawned {
        eprintln!("openback: {e}");
    }
}

fn write_back(local: &Path, remote: &Uri) {
    let Some(dest) = remote.parent() else { return };
    let op = Value::obj()
        .s("op", "copy")
        .v("items", Value::Arr(vec![Value::Str(Uri::from_path(local).to_string())]))
        .s("dest", dest.to_string())
        .s("policy", "replace")
        .s("title", format!("Save {} back to {}", remote.name(), remote.authority))
        .done();
    match crate::jobs::submit(op, None) {
        Ok(_) => {}
        Err((_, m)) => eprintln!("openback: {}: {m}", local.display()),
    }
}
