//! Backends and the URI resolver.

pub mod local;
pub mod uri;

use std::io;

/// One entry's phase-2 metadata.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Meta {
    pub size: u64,
    /// Milliseconds since the epoch; 0 when unknown.
    pub mtime_ms: u64,
    pub mode: Option<u32>,
    pub uid: Option<u32>,
    pub gid: Option<u32>,
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
