//! File operations on the local backend. Each returns the inverse action for the journal.

use crate::vfs::uri::Uri;
use crate::vfs::{Result, VfsError};
use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

pub const COPY_BUF: usize = 1024 * 1024;

pub struct Progress<'a> {
    pub cancel: &'a AtomicBool,
    pub bytes: &'a mut dyn FnMut(u64),
    /// Told as each file of a tree is begun and finished: what the activity view names as the
    /// file being copied, and what lets a folder copy count files rather than end at "1 of 458".
    pub file: Option<&'a mut dyn FnMut(FileEvent)>,
    /// With somewhere to note them, a file or folder that cannot be copied is noted and the rest
    /// of the tree goes on — one unreadable file does not stop the other four hundred — and the
    /// caller fails the job at the end, saying which. Without, the first error ends the copy.
    pub failed: Option<&'a mut Vec<(PathBuf, String)>>,
}

pub enum FileEvent<'a> {
    Start(&'a Path, u64),
    Done,
}

impl Progress<'_> {
    /// Note a failure and carry on, or hand it back to end the copy. A cancel always ends it.
    fn excuse(&mut self, at: &Path, e: VfsError) -> Result<()> {
        if matches!(&e, VfsError::Io(m) if m == "cancelled") {
            return Err(e);
        }
        match self.failed.as_mut() {
            Some(list) => {
                list.push((at.to_path_buf(), e.message()));
                Ok(())
            }
            None => Err(e),
        }
    }

    fn failures(&self) -> usize {
        self.failed.as_ref().map(|f| f.len()).unwrap_or(0)
    }

    fn tell(&mut self, e: FileEvent) {
        if let Some(f) = self.file.as_mut() {
            f(e);
        }
    }
}

fn cancelled(c: &AtomicBool) -> bool {
    c.load(Ordering::Relaxed)
}

/// Copies one file, streaming with progress; `copy_file_range` on Linux with a partial-write loop.
pub fn copy_file(src: &Path, dst: &Path, p: &mut Progress) -> Result<u64> {
    let mut input = fs::File::open(src)?;
    let md = input.metadata()?;
    #[cfg(target_os = "linux")]
    let total = md.len();
    let mut output = fs::OpenOptions::new().write(true).create_new(true).open(dst)?;
    #[cfg(target_os = "linux")]
    {
        use std::os::unix::io::AsRawFd;
        if total > 0 {
            let _ = unsafe { libc::fallocate(output.as_raw_fd(), 0, 0, total as libc::off_t) };
            let _ = unsafe { libc::posix_fadvise(input.as_raw_fd(), 0, 0, libc::POSIX_FADV_SEQUENTIAL) };
        }
        let mut done = 0u64;
        let mut fast_failed = false;
        while done < total {
            if cancelled(p.cancel) {
                drop(output);
                let _ = fs::remove_file(dst);
                return Err(VfsError::Io("cancelled".into()));
            }
            // 64 MiB at a time: the kernel does each call in one go, so this is how often a cancel
            // is looked for and progress is reported. It was 1 GiB, which on a 4 GB file was four
            // updates and a cancel that could take a long while to be noticed.
            let want = (total - done).min(64 << 20) as usize;
            let n = unsafe { libc::copy_file_range(input.as_raw_fd(), std::ptr::null_mut(), output.as_raw_fd(), std::ptr::null_mut(), want, 0) };
            if n < 0 {
                let err = std::io::Error::last_os_error();
                match err.raw_os_error() {
                    Some(libc::EXDEV) | Some(libc::ENOSYS) | Some(libc::EINVAL) | Some(libc::EOPNOTSUPP) if done == 0 => {
                        fast_failed = true;
                        break;
                    }
                    _ => {
                        drop(output);
                        let _ = fs::remove_file(dst);
                        return Err(err.into());
                    }
                }
            } else if n == 0 {
                break;
            } else {
                done += n as u64;
                (p.bytes)(n as u64);
            }
        }
        if !fast_failed {
            let _ = unsafe { libc::posix_fadvise(input.as_raw_fd(), 0, 0, libc::POSIX_FADV_DONTNEED) };
            finish_copy(&md, dst)?;
            return Ok(done);
        }
    }
    let n = buffered_copy(&mut input, &mut output, p).inspect_err(|_| {
        let _ = fs::remove_file(dst);
    })?;
    finish_copy(&md, dst)?;
    Ok(n)
}

fn buffered_copy(input: &mut fs::File, output: &mut fs::File, p: &mut Progress) -> Result<u64> {
    let mut buf = vec![0u8; COPY_BUF];
    let mut done = 0u64;
    loop {
        if cancelled(p.cancel) {
            return Err(VfsError::Io("cancelled".into()));
        }
        let n = input.read(&mut buf)?;
        if n == 0 {
            break;
        }
        output.write_all(&buf[..n])?;
        done += n as u64;
        (p.bytes)(n as u64);
    }
    Ok(done)
}

fn finish_copy(md: &fs::Metadata, dst: &Path) -> Result<()> {
    let _ = fs::set_permissions(dst, md.permissions());
    if let Ok(m) = md.modified() {
        let _ = set_mtime(dst, m);
    }
    Ok(())
}

pub fn set_mtime(path: &Path, t: std::time::SystemTime) -> std::io::Result<()> {
    let f = fs::OpenOptions::new().write(true).open(path)?;
    f.set_modified(t)
}

/// Copies a file or directory tree. Returns the paths created at the top level.
pub fn copy_tree(src: &Path, dst: &Path, p: &mut Progress) -> Result<()> {
    let md = fs::symlink_metadata(src)?;
    if md.file_type().is_symlink() {
        let target = fs::read_link(src)?;
        std::os::unix::fs::symlink(target, dst)?;
        p.tell(FileEvent::Done);
        return Ok(());
    }
    if md.is_dir() {
        fs::create_dir(dst)?;
        let entries = match fs::read_dir(src) {
            Ok(rd) => rd,
            // A folder that cannot be opened is one thing that did not copy, not the end of it.
            Err(e) => return p.excuse(src, e.into()),
        };
        for e in entries {
            let e = e?;
            copy_tree(&e.path(), &dst.join(e.file_name()), p)?;
        }
        let _ = fs::set_permissions(dst, md.permissions());
        return Ok(());
    }
    p.tell(FileEvent::Start(src, md.len()));
    match copy_file(src, dst, p) {
        Ok(_) => p.tell(FileEvent::Done),
        Err(e) => p.excuse(src, e)?,
    }
    Ok(())
}

pub fn remove_tree(path: &Path) -> Result<()> {
    let md = fs::symlink_metadata(path)?;
    if md.is_dir() && !md.file_type().is_symlink() {
        fs::remove_dir_all(path)?;
    } else {
        fs::remove_file(path)?;
    }
    Ok(())
}

pub fn tree_size(path: &Path) -> (u64, u64) {
    // (files, bytes)
    let md = match fs::symlink_metadata(path) {
        Ok(m) => m,
        Err(_) => return (0, 0),
    };
    if md.is_dir() && !md.file_type().is_symlink() {
        let mut f = 0;
        let mut b = 0;
        if let Ok(rd) = fs::read_dir(path) {
            for e in rd.flatten() {
                let (ef, eb) = tree_size(&e.path());
                f += ef;
                b += eb;
            }
        }
        (f, b)
    } else {
        (1, md.len())
    }
}

/// Move: rename when possible, else copy then delete.
pub fn move_path(src: &Path, dst: &Path, p: &mut Progress) -> Result<()> {
    match fs::rename(src, dst) {
        Ok(()) => Ok(()),
        Err(e) if e.raw_os_error() == Some(libc::EXDEV) => {
            // Copy, then delete — and the delete only if ALL of it copied: with anything missing
            // the original stays whole, and the caller reports what did not make it.
            let before = p.failures();
            copy_tree(src, dst, p)?;
            if p.failures() == before {
                remove_tree(src)
            } else {
                Ok(())
            }
        }
        Err(e) => Err(e.into()),
    }
}

pub fn chmod(path: &Path, mode: u32, recursive: bool, out: &mut Vec<(PathBuf, u32)>) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    let md = fs::symlink_metadata(path)?;
    if md.file_type().is_symlink() {
        return Ok(());
    }
    out.push((path.to_path_buf(), md.permissions().mode() & 0o7777));
    fs::set_permissions(path, fs::Permissions::from_mode(mode))?;
    if recursive && md.is_dir() {
        for e in fs::read_dir(path)? {
            chmod(&e?.path(), mode, true, out)?;
        }
    }
    Ok(())
}

// ---------------------------------------------------------------- trash (freedesktop.org Trash spec)

pub fn trash_dir() -> PathBuf {
    if let Ok(d) = std::env::var("KIKI_TRASH_DIR") {
        return PathBuf::from(d);
    }
    std::env::var("XDG_DATA_HOME").map(PathBuf::from).unwrap_or_else(|_| crate::config::home().join(".local/share")).join("Trash")
}

/// Moves `path` into the trash; returns the name it got in `files/`.
pub fn trash(path: &Path) -> Result<String> {
    let td = trash_dir();
    fs::create_dir_all(td.join("files"))?;
    fs::create_dir_all(td.join("info"))?;
    let base = path.file_name().ok_or(VfsError::NotFound)?.to_string_lossy().into_owned();
    let mut name = base.clone();
    let mut n = 1;
    while td.join("files").join(&name).exists() || td.join("info").join(format!("{name}.trashinfo")).exists() {
        n += 1;
        name = format!("{base}.{n}");
    }
    let now = unix_now();
    let info = format!("[Trash Info]\nPath={}\nDeletionDate={}\n", percent_path(path), iso_local(now));
    fs::write(td.join("info").join(format!("{name}.trashinfo")), info)?;
    let mut dummy = AtomicBool::new(false);
    let mut p = Progress { cancel: &mut dummy, bytes: &mut |_| {}, file: None, failed: None };
    if let Err(e) = move_path(path, &td.join("files").join(&name), &mut p) {
        let _ = fs::remove_file(td.join("info").join(format!("{name}.trashinfo")));
        return Err(e);
    }
    Ok(name)
}

/// Restores a trashed item to its original path (fails if something is there now).
pub fn restore(name: &str) -> Result<PathBuf> {
    let td = trash_dir();
    let info = fs::read_to_string(td.join("info").join(format!("{name}.trashinfo")))?;
    let orig = info.lines().find_map(|l| l.strip_prefix("Path=")).ok_or(VfsError::Io("bad trashinfo".into()))?;
    let orig = PathBuf::from(percent_decode(orig));
    if orig.exists() {
        return Err(VfsError::Exists);
    }
    let mut dummy = AtomicBool::new(false);
    let mut p = Progress { cancel: &mut dummy, bytes: &mut |_| {}, file: None, failed: None };
    move_path(&td.join("files").join(name), &orig, &mut p)?;
    let _ = fs::remove_file(td.join("info").join(format!("{name}.trashinfo")));
    Ok(orig)
}

fn percent_path(p: &Path) -> String {
    let mut out = String::new();
    for &b in p.to_string_lossy().as_bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'/' | b'-' | b'_' | b'.' | b'~') {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

fn percent_decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' && i + 2 < b.len() {
            if let Ok(v) = u8::from_str_radix(&s[i + 1..i + 3], 16) {
                out.push(v);
                i += 3;
                continue;
            }
        }
        out.push(b[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

pub fn unix_now() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

fn iso_local(secs: u64) -> String {
    let mut tm: libc::tm = unsafe { std::mem::zeroed() };
    let t = secs as libc::time_t;
    unsafe { libc::localtime_r(&t, &mut tm) };
    format!("{:04}-{:02}-{:02}T{:02}:{:02}:{:02}", tm.tm_year + 1900, tm.tm_mon + 1, tm.tm_mday, tm.tm_hour, tm.tm_min, tm.tm_sec)
}

pub fn local_path(uri: &Uri) -> Result<PathBuf> {
    if uri.scheme == "trash" {
        // trash:///name → the trashed file itself
        return Ok(trash_dir().join("files").join(uri.path.trim_start_matches('/')));
    }
    if !uri.is_local() {
        return Err(VfsError::Unsupported);
    }
    Ok(uri.to_path())
}

/// Every `.trashinfo`: (name in files/, original path, deletion date ISO).
pub fn trash_infos() -> Vec<(String, String, String)> {
    let mut out = Vec::new();
    let Ok(rd) = fs::read_dir(trash_dir().join("info")) else { return out };
    for e in rd.flatten() {
        let Some(name) = e.file_name().to_str().and_then(|n| n.strip_suffix(".trashinfo")).map(str::to_string) else { continue };
        let Ok(text) = fs::read_to_string(e.path()) else { continue };
        let path = text.lines().find_map(|l| l.strip_prefix("Path=")).map(percent_decode).unwrap_or_default();
        let date = text.lines().find_map(|l| l.strip_prefix("DeletionDate=")).unwrap_or("").to_string();
        out.push((name, path, date));
    }
    out.sort();
    out
}

/// How many things are in the trash: the total an "Empty Trash" counts towards.
pub fn trash_count() -> u64 {
    fs::read_dir(trash_dir().join("files")).map(|rd| rd.flatten().count() as u64).unwrap_or(0)
}

/// Delete everything in the trash for good; `each` is told as every item goes.
pub fn empty_trash(cancel: &AtomicBool, each: &mut dyn FnMut()) -> Result<u64> {
    let td = trash_dir();
    let mut n = 0;
    for sub in ["files", "info"] {
        let Ok(rd) = fs::read_dir(td.join(sub)) else { continue };
        for e in rd.flatten() {
            if cancelled(cancel) {
                return Err(VfsError::Io("cancelled".into()));
            }
            let p = e.path();
            if p.is_dir() && !p.is_symlink() {
                remove_tree(&p)?;
            } else {
                fs::remove_file(&p)?;
            }
            if sub == "files" {
                n += 1;
                each();
            }
        }
    }
    Ok(n)
}

/// A unique destination name: "name", "name (2)", "name (3)" … keeping the extension.
pub fn unique_name(dir: &Path, name: &str) -> String {
    if !dir.join(name).exists() {
        return name.to_string();
    }
    let (stem, ext) = match name.rfind('.') {
        Some(i) if i > 0 => (&name[..i], &name[i..]),
        _ => (name, ""),
    };
    let mut n = 2;
    loop {
        let cand = format!("{stem} ({n}){ext}");
        if !dir.join(&cand).exists() {
            return cand;
        }
        n += 1;
    }
}

pub type Cancel = Arc<AtomicBool>;

#[cfg(test)]
mod tests {
    use super::*;

    fn temp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("kiki-ops-{}-{name}", std::process::id()));
        let _ = fs::remove_dir_all(&d);
        fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn copy_move_trash_restore() {
        let _guard = crate::ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let d = temp("a");
        std::env::set_var("KIKI_TRASH_DIR", d.join("trash"));
        fs::create_dir_all(d.join("src/sub")).unwrap();
        fs::write(d.join("src/a.txt"), vec![b'a'; 3 * 1024 * 1024]).unwrap();
        fs::write(d.join("src/sub/b.txt"), b"bb").unwrap();
        let cancel = AtomicBool::new(false);
        let mut bytes = 0u64;
        {
            let mut p = Progress { cancel: &cancel, bytes: &mut |n| bytes += n, file: None, failed: None };
            copy_tree(&d.join("src"), &d.join("copy"), &mut p).unwrap();
        }
        assert_eq!(bytes, 3 * 1024 * 1024 + 2);
        assert_eq!(fs::read(d.join("copy/sub/b.txt")).unwrap(), b"bb");
        assert_eq!(tree_size(&d.join("copy")), (2, 3 * 1024 * 1024 + 2));
        let mut p = Progress { cancel: &cancel, bytes: &mut |_| {}, file: None, failed: None };
        move_path(&d.join("copy"), &d.join("moved"), &mut p).unwrap();
        assert!(!d.join("copy").exists() && d.join("moved/a.txt").exists());
        let name = trash(&d.join("moved")).unwrap();
        assert!(!d.join("moved").exists());
        assert!(trash_dir().join("files").join(&name).is_dir());
        let back = restore(&name).unwrap();
        assert_eq!(back, d.join("moved"));
        assert!(d.join("moved/a.txt").exists());
        assert_eq!(unique_name(&d.join("moved"), "a.txt"), "a (2).txt");
        let mut undo = Vec::new();
        chmod(&d.join("moved"), 0o700, true, &mut undo).unwrap();
        assert_eq!(undo.len(), 4); // moved/, a.txt, sub/, sub/b.txt
        std::env::remove_var("KIKI_TRASH_DIR");
        fs::remove_dir_all(&d).unwrap();
    }

    #[test]
    fn cancel_leaves_no_partial_file() {
        let d = temp("b");
        fs::write(d.join("big"), vec![0u8; 8 * 1024 * 1024]).unwrap();
        let cancel = AtomicBool::new(false);
        let mut seen = 0u64;
        let r = {
            let mut p = Progress {
                cancel: &cancel,
                bytes: &mut |n| {
                    seen += n;
                    if seen >= 1024 * 1024 {
                        cancel.store(true, Ordering::Relaxed)
                    }
                },
                file: None,
                failed: None,
            };
            copy_file(&d.join("big"), &d.join("out"), &mut p)
        };
        // On Linux copy_file_range may finish in one call before the flag is checked; either way no partial file may remain on error.
        if r.is_err() {
            assert!(!d.join("out").exists());
        }
        fs::remove_dir_all(&d).unwrap();
    }
}
