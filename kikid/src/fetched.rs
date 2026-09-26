//! Files brought to the cache to be looked at (docs/0.2.0/05-quicklook.md, decision 4).
//!
//! A file on a server has nothing on this machine a window can show, so it is first brought to
//! a folder of its own under the cache — `<cache>/open/<moment>/<name>` (`config::cache_dir`) — by an
//! ordinary copy job (the orb shows it, it can be cancelled, its wire is logged), and the window
//! shows the copy once the job is done. Nothing watches the copy and nothing goes back: a look
//! writes nothing. (For a day in 0.2.0 this also opened the copy in an application and sent a
//! save back to the server; the owner withdrew that on 2026-09-25 — "double click should just
//! open the quicklook window for remote" — and remote editing is a plan of its own,
//! docs/0.3.0/01-remote-edit.md.) The copies are for the session: at start the daemon empties
//! the folder.

use crate::json::Value;
use crate::vfs::uri::Uri;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

/// Where fetched copies go: `<cache>/open` — `~/.cache/kiki/open`, or wherever `KIKI_CACHE_DIR`
/// says (a checkout's run keeps its own).
pub fn cache_dir() -> Result<PathBuf, String> {
    let dir = crate::config::cache_dir().join("open");
    std::fs::create_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    Ok(dir)
}

/// Brings `uris` (remote, all of them) to a folder of the moment: the copy job's id, and where
/// each will be once it is done, in the order given.
pub fn bring(uris: &[Uri]) -> Result<(u64, Vec<PathBuf>), String> {
    let moment = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis()).unwrap_or(0);
    let mut dir = cache_dir()?.join(moment.to_string());
    let mut n = 0;
    while dir.exists() {
        n += 1;
        dir = dir.with_file_name(format!("{moment}-{n}"));
    }
    std::fs::create_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let locals: Vec<PathBuf> = uris.iter().map(|u| dir.join(u.name())).collect();
    // Hidden: the window that asked shows the fetch's progress itself, and a copy into the
    // cache is nothing to toast about or to undo (it showed as "Copy X to 1790392334311 · Undo").
    let op = Value::obj().s("op", "copy").v("items", Value::Arr(uris.iter().map(|u| Value::Str(u.to_string())).collect())).s("dest", Uri::from_path(&dir).to_string()).b("_silent", true).done();
    let job = crate::jobs::submit(op, None).map_err(|(_, m)| m)?;
    Ok((job, locals))
}

/// At the daemon's start: whatever a previous life fetched goes.
pub fn clear() {
    let Ok(dir) = cache_dir() else { return };
    let Ok(entries) = std::fs::read_dir(&dir) else { return };
    for e in entries.flatten() {
        let _ = std::fs::remove_dir_all(e.path());
    }
}

#[cfg(test)]
mod tests {
    /// `KIKI_CACHE_DIR` moves the copies with the rest of the cache: what `make run` sets so a
    /// checkout never writes into the installed kiki's folders.
    #[test]
    fn the_copies_follow_the_cache_directory() {
        let d = std::env::temp_dir().join(format!("kiki-fetched-{}", std::process::id()));
        std::env::set_var("KIKI_CACHE_DIR", &d);
        assert_eq!(super::cache_dir().unwrap(), d.join("open"));
        std::env::remove_var("KIKI_CACHE_DIR");
        let _ = std::fs::remove_dir_all(&d);
    }
}
