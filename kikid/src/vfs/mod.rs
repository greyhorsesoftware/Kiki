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
    /// Refused, and why — a server's own account of it where there is one. Empty is "no reason
    /// given"; a plugin that has words for it (SMB: "The server refused this username and
    /// password.") had them thrown away here, and the dialog said only "Denied".
    Denied(String),
    Exists,
    NotEmpty,
    Unsupported,
    /// Refused because doing it would be dangerous: an archive entry that escapes its folder.
    Unsafe(String),
    Io(String),
    /// A failure the user is told about, by number: the window says it in its own language
    /// from `n` and `params` (docs/0.2.0/02-localization.md, L5 — owner, 2026-09-25: "the daemon
    /// should return numeric values and localization should happen in the UI"). The English
    /// `message` travels too, for logs and for a window that has no words for the number.
    Said {
        n: u16,
        params: Vec<(String, String)>,
        message: String,
    },
}

impl VfsError {
    pub fn code(&self) -> &'static str {
        match self {
            VfsError::NotFound => "NotFound",
            VfsError::Denied(_) => "Denied",
            VfsError::Exists => "Exists",
            VfsError::NotEmpty => "NotEmpty",
            VfsError::Unsupported => "Unsupported",
            VfsError::Unsafe(_) => "Unsafe",
            VfsError::Io(_) | VfsError::Said { .. } => "Io",
        }
    }
    pub fn message(&self) -> String {
        match self {
            VfsError::Io(m) | VfsError::Unsafe(m) | VfsError::Said { message: m, .. } => m.clone(),
            VfsError::Denied(m) if !m.is_empty() => m.clone(),
            other => other.code().to_string(),
        }
    }
}

impl VfsError {
    /// A numbered failure: `VfsError::said(1201, &[("host", host)], "a server has no trash…")`.
    pub fn said(n: u16, params: &[(&str, &dyn std::fmt::Display)], message: impl Into<String>) -> VfsError {
        VfsError::Said { n, params: params.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect(), message: message.into() }
    }
    /// The number and params, for the wire; none for an error without a number.
    pub fn said_json(&self) -> Option<(u16, crate::json::Value)> {
        match self {
            VfsError::Said { n, params, .. } => {
                let mut o = crate::json::Value::obj();
                for (k, v) in params {
                    o = o.s(k, v.clone());
                }
                Some((*n, o.done()))
            }
            _ => None,
        }
    }
}

impl From<io::Error> for VfsError {
    fn from(e: io::Error) -> Self {
        match e.kind() {
            io::ErrorKind::NotFound => VfsError::NotFound,
            io::ErrorKind::PermissionDenied => VfsError::Denied(e.to_string()),
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
