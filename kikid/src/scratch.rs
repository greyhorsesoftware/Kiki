//! A folder under the temp dir that goes when its guard does — on success, on failure and on a
//! panic alike (owner, 2026-10-02: "there are a bunch of kiki-* folders in tmp right now. they
//! should be deleted on exit of test"). Every test that wants a folder takes one of these
//! instead of a path and a `remove_dir_all` it may never reach; the share job and the bench use
//! it for their own scratch for the same reason. Standard library only, and no `use crate::`:
//! the plugins' tests include this file by path, since they do not link the daemon.
#![allow(dead_code)]

use std::ffi::OsStr;
use std::path::{Path, PathBuf};

pub struct Scratch(PathBuf);

impl Scratch {
    /// `temp_dir()/kiki-<tag>-<pid>`, made now, a stale one from an earlier run cleared first.
    pub fn new(tag: &str) -> Scratch {
        Scratch::named(&format!("{tag}-{}", std::process::id()))
    }

    /// `temp_dir()/kiki-<name>` exactly, for a name that is already unique (a job's id).
    pub fn named(name: &str) -> Scratch {
        let p = std::env::temp_dir().join(format!("kiki-{name}"));
        let _ = std::fs::remove_dir_all(&p);
        let _ = std::fs::remove_file(&p);
        std::fs::create_dir_all(&p).expect("a scratch folder under the temp dir");
        // Canonical, so a test comparing it with a path the code resolved (`repo_root`, a cwd)
        // compares like with like when the temp dir is itself a link.
        let p = p.canonicalize().unwrap_or(p);
        Scratch(p)
    }

    pub fn path(&self) -> &Path {
        &self.0
    }
}

impl std::ops::Deref for Scratch {
    type Target = Path;
    fn deref(&self) -> &Path {
        &self.0
    }
}

impl AsRef<Path> for Scratch {
    fn as_ref(&self) -> &Path {
        &self.0
    }
}

impl AsRef<OsStr> for Scratch {
    fn as_ref(&self) -> &OsStr {
        self.0.as_os_str()
    }
}

impl std::fmt::Debug for Scratch {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

impl std::fmt::Display for Scratch {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.display().fmt(f)
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        // Whatever is there by now: the tree it was made as, or a file a test put in its place.
        if std::fs::remove_dir_all(&self.0).is_err() {
            let _ = std::fs::remove_file(&self.0);
        }
    }
}
