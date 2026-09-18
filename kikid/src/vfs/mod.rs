//! Backends and the URI resolver.

pub mod local;
pub mod remote;
pub mod uri;

use std::io;

/// One entry's phase-2 metadata.
#[derive(Clone, Debug, PartialEq)]
pub struct Meta {
    pub size: u64,
    /// Milliseconds since the epoch; 0 when unknown.
    pub mtime_ms: u64,
    /// Last access, milliseconds since the epoch; 0 when unknown. Only as fresh as the
    /// filesystem keeps it (`relatime` updates it at most once a day unless the file changed).
    pub atime_ms: u64,
    /// `Meta::NONE` when unknown (kept as plain u32s so 200k entries stay small).
    pub mode: u32,
    pub uid: u32,
    pub gid: u32,
    /// Hidden by the backend's own convention (SMB's DOS attribute); dot-files are hidden by name.
    pub hidden: bool,
}

impl Meta {
    pub const NONE: u32 = u32::MAX;
    pub fn opt(v: u32) -> Option<u32> {
        if v == Meta::NONE {
            None
        } else {
            Some(v)
        }
    }
    pub fn mode(&self) -> Option<u32> {
        Meta::opt(self.mode)
    }
}

impl Default for Meta {
    fn default() -> Self {
        Meta { size: 0, mtime_ms: 0, atime_ms: 0, mode: Meta::NONE, uid: Meta::NONE, gid: Meta::NONE, hidden: false }
    }
}

/// Phase-1 entry type, from the directory entry alone.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
#[repr(u8)]
pub enum EntryType {
    File = 0,
    Dir = 1,
    Link = 2,
    Other = 3,
    /// Type unknown from the dirent (some filesystems); resolved by stat.
    Unknown = 4,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Capabilities {
    pub trash: bool,
    pub set_mtime: bool,
    pub mode: bool,
    pub real_dirs: bool,
}

#[derive(Debug)]
pub enum VfsError {
    NotFound,
    Denied,
    Exists,
    NotEmpty,
    Unsupported,
    Io(String),
}

impl VfsError {
    pub fn code(&self) -> &'static str {
        match self {
            VfsError::NotFound => "NotFound",
            VfsError::Denied => "Denied",
            VfsError::Exists => "Exists",
            VfsError::NotEmpty => "NotEmpty",
            VfsError::Unsupported => "Unsupported",
            VfsError::Io(_) => "Io",
        }
    }
    pub fn message(&self) -> String {
        match self {
            VfsError::Io(m) => m.clone(),
            other => other.code().to_string(),
        }
    }
}

impl From<io::Error> for VfsError {
    fn from(e: io::Error) -> Self {
        match e.kind() {
            io::ErrorKind::NotFound => VfsError::NotFound,
            io::ErrorKind::PermissionDenied => VfsError::Denied,
            io::ErrorKind::AlreadyExists => VfsError::Exists,
            _ => {
                #[cfg(unix)]
                {
                    if e.raw_os_error() == Some(libc::ENOTEMPTY) {
                        return VfsError::NotEmpty;
                    }
                }
                VfsError::Io(e.to_string())
            }
        }
    }
}

pub type Result<T> = std::result::Result<T, VfsError>;

/// What a listing needs from a backend: enumerate names and kinds, and stat one child.
pub trait Source: Send + Sync {
    fn scan(&self, sink: &mut dyn FnMut(Vec<local::RawEntry>)) -> Result<usize>;
    fn stat_child(&self, name: &std::ffi::OsStr) -> Result<(Meta, EntryType)>;
    /// True when inotify can watch it (local directories only).
    fn watchable(&self) -> bool;
    /// Whether this handle still refers to what lives at `path`. A directory deleted and
    /// recreated with the same name is a different directory, and a handle opened on the old one
    /// reads the old one — which is empty — for ever. Sources that cannot tell say yes.
    fn still_at(&self, _path: &std::path::Path) -> bool {
        true
    }
    /// Stop an in-progress scan (a remote listing nobody is looking at any more).
    fn cancel(&self) {}
}
