//! The local filesystem backend: raw directory enumeration and metadata.

use super::{EntryType, Meta, Result, VfsError};
use std::ffi::OsString;
use std::fs::File;
use std::path::Path;

/// A phase-1 entry: name bytes and type, nothing else.
pub struct RawEntry {
    pub name: OsString,
    pub kind: EntryType,
    pub meta: Option<Meta>,
}

/// A directory opened for scanning and later relative stats.
pub struct DirHandle {
    #[allow(dead_code)]
    file: File,
}

impl DirHandle {
    pub fn open(path: &Path) -> Result<DirHandle> {
        let file = File::open(path)?;
        if !file.metadata()?.is_dir() {
            return Err(VfsError::Io("not a directory".into()));
        }
        Ok(DirHandle { file })
    }

    /// Enumerate without stat, calling `sink` with chunks of entries.
    /// Returns the total count. `.` and `..` are skipped.
    pub fn scan(&self, mut sink: impl FnMut(Vec<RawEntry>)) -> Result<usize> {
        scan_impl(self, &mut sink)
    }

    /// Metadata for one child, relative to this directory, never following symlinks.
    pub fn stat_child(&self, name: &std::ffi::OsStr) -> Result<(Meta, EntryType)> {
        stat_impl(self, name)
    }
}

impl super::Source for DirHandle {
    fn scan(&self, sink: &mut dyn FnMut(Vec<RawEntry>)) -> Result<usize> {
        scan_impl(self, sink)
    }
    fn stat_child(&self, name: &std::ffi::OsStr) -> Result<(Meta, EntryType)> {
        stat_impl(self, name)
    }
    fn watchable(&self) -> bool {
        true
    }
}

pub fn capabilities() -> super::Capabilities {
    super::Capabilities { trash: true, set_mtime: true, mode: true, real_dirs: true }
}

// ---------------------------------------------------------------- linux: getdents64 + statx

#[cfg(target_os = "linux")]
fn scan_impl(h: &DirHandle, sink: &mut dyn FnMut(Vec<RawEntry>)) -> Result<usize> {
    use std::os::unix::ffi::OsStringExt;
    use std::os::unix::io::AsRawFd;
    const BUF: usize = 1 << 20; // 1 MiB: a handful of syscalls for 100k entries
    const CHUNK: usize = 1024;
    let fd = h.file.as_raw_fd();
    let mut buf = vec![0u8; BUF];
    let mut total = 0usize;
    let mut chunk: Vec<RawEntry> = Vec::with_capacity(CHUNK);
    loop {
        let n = unsafe { libc::syscall(libc::SYS_getdents64, fd, buf.as_mut_ptr(), BUF) };
        if n < 0 {
            return Err(std::io::Error::last_os_error().into());
        }
        if n == 0 {
            break;
        }
        let n = n as usize;
        let mut off = 0usize;
        while off < n {
            // struct linux_dirent64 { u64 d_ino; i64 d_off; u16 d_reclen; u8 d_type; char d_name[]; }
            let reclen = u16::from_ne_bytes([buf[off + 16], buf[off + 17]]) as usize;
            let d_type = buf[off + 18];
            let name_start = off + 19;
            let name_end = buf[name_start..off + reclen].iter().position(|&b| b == 0).map(|p| name_start + p).unwrap_or(off + reclen);
            let name = &buf[name_start..name_end];
            off += reclen;
            if name == b"." || name == b".." {
                continue;
            }
            let kind = match d_type {
                libc::DT_REG => EntryType::File,
                libc::DT_DIR => EntryType::Dir,
                libc::DT_LNK => EntryType::Link,
                libc::DT_UNKNOWN => EntryType::Unknown,
                _ => EntryType::Other,
            };
            chunk.push(RawEntry { name: OsString::from_vec(name.to_vec()), kind, meta: None });
            total += 1;
            if chunk.len() >= CHUNK {
                sink(std::mem::replace(&mut chunk, Vec::with_capacity(CHUNK)));
            }
        }
    }
    if !chunk.is_empty() {
        sink(chunk);
    }
    Ok(total)
}

#[cfg(target_os = "linux")]
fn stat_impl(h: &DirHandle, name: &std::ffi::OsStr) -> Result<(Meta, EntryType)> {
    use rustix::fs::{statx, AtFlags, StatxFlags};
    let mask = StatxFlags::SIZE | StatxFlags::MTIME | StatxFlags::MODE | StatxFlags::TYPE | StatxFlags::UID | StatxFlags::GID;
    let st = statx(&h.file, name, AtFlags::SYMLINK_NOFOLLOW | AtFlags::STATX_DONT_SYNC, mask).map_err(|e| std::io::Error::from_raw_os_error(e.raw_os_error()))?;
    let mtime_ms = if st.stx_mtime.tv_sec <= 0 { 0 } else { st.stx_mtime.tv_sec as u64 * 1000 + (st.stx_mtime.tv_nsec / 1_000_000) as u64 };
    let mode = st.stx_mode as u32;
    let kind = match mode & libc::S_IFMT {
        m if m == libc::S_IFREG => EntryType::File,
        m if m == libc::S_IFDIR => EntryType::Dir,
        m if m == libc::S_IFLNK => EntryType::Link,
        _ => EntryType::Other,
    };
    Ok((Meta { size: st.stx_size, mtime_ms, mode: Some(mode & 0o7777), uid: Some(st.stx_uid), gid: Some(st.stx_gid) }, kind))
}

// ---------------------------------------------------------------- portable fallback (used for native tests on macOS)

#[cfg(not(target_os = "linux"))]
fn scan_impl(h: &DirHandle, sink: &mut dyn FnMut(Vec<RawEntry>)) -> Result<usize> {
    let _ = &h.file;
    let path = path_of(h)?;
    let mut total = 0;
    let mut chunk = Vec::with_capacity(1024);
    for e in std::fs::read_dir(path)? {
        let e = e?;
        let kind = match e.file_type() {
            Ok(t) if t.is_dir() => EntryType::Dir,
            Ok(t) if t.is_symlink() => EntryType::Link,
            Ok(t) if t.is_file() => EntryType::File,
            Ok(_) => EntryType::Other,
            Err(_) => EntryType::Unknown,
        };
        chunk.push(RawEntry { name: e.file_name(), kind, meta: None });
        total += 1;
        if chunk.len() >= 1024 {
            sink(std::mem::replace(&mut chunk, Vec::with_capacity(1024)));
        }
    }
    if !chunk.is_empty() {
        sink(chunk);
    }
    Ok(total)
}

#[cfg(not(target_os = "linux"))]
fn stat_impl(h: &DirHandle, name: &std::ffi::OsStr) -> Result<(Meta, EntryType)> {
    use std::os::unix::fs::MetadataExt;
    let p = path_of(h)?.join(name);
    let md = std::fs::symlink_metadata(&p)?;
    let ft = md.file_type();
    let kind = if ft.is_dir() {
        EntryType::Dir
    } else if ft.is_symlink() {
        EntryType::Link
    } else if ft.is_file() {
        EntryType::File
    } else {
        EntryType::Other
    };
    let mtime_ms = if md.mtime() <= 0 { 0 } else { md.mtime() as u64 * 1000 + (md.mtime_nsec() / 1_000_000) as u64 };
    Ok((Meta { size: md.size(), mtime_ms, mode: Some(md.mode() & 0o7777), uid: Some(md.uid()), gid: Some(md.gid()) }, kind))
}

#[cfg(not(target_os = "linux"))]
fn path_of(h: &DirHandle) -> Result<std::path::PathBuf> {
    // macOS: recover the path from the descriptor for the portable fallback.
    use std::os::unix::io::AsRawFd;
    let mut buf = vec![0u8; libc::PATH_MAX as usize];
    let r = unsafe { libc::fcntl(h.file.as_raw_fd(), libc::F_GETPATH, buf.as_mut_ptr()) };
    if r < 0 {
        return Err(std::io::Error::last_os_error().into());
    }
    let end = buf.iter().position(|&b| b == 0).unwrap_or(buf.len());
    Ok(std::path::PathBuf::from(String::from_utf8_lossy(&buf[..end]).into_owned()))
}

// ---------------------------------------------------------------- owner and group names

pub fn user_name(uid: u32) -> Option<String> {
    unsafe {
        let p = libc::getpwuid(uid);
        if p.is_null() {
            return None;
        }
        Some(std::ffi::CStr::from_ptr((*p).pw_name).to_string_lossy().into_owned())
    }
}

pub fn group_name(gid: u32) -> Option<String> {
    unsafe {
        let g = libc::getgrgid(gid);
        if g.is_null() {
            return None;
        }
        Some(std::ffi::CStr::from_ptr((*g).gr_name).to_string_lossy().into_owned())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scans_and_stats_temp_tree() {
        let dir = std::env::temp_dir().join(format!("kiki-local-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("sub")).unwrap();
        std::fs::write(dir.join("a.txt"), b"hello").unwrap();
        std::fs::write(dir.join("b c.md"), b"# hi").unwrap();
        let h = DirHandle::open(&dir).unwrap();
        let mut names = Vec::new();
        let n = h.scan(|chunk| names.extend(chunk.into_iter().map(|e| (e.name, e.kind)))).unwrap();
        assert_eq!(n, 3);
        names.sort();
        assert_eq!(names[0].0, "a.txt");
        assert!(names.iter().any(|(nm, k)| nm == "sub" && *k == EntryType::Dir));
        let (m, k) = h.stat_child(std::ffi::OsStr::new("a.txt")).unwrap();
        assert_eq!(m.size, 5);
        assert_eq!(k, EntryType::File);
        assert!(m.mtime_ms > 0);
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
