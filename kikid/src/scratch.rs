//! A folder under the temp dir that goes when its guard does — on success, on failure and on a
//! panic alike (owner, 2026-10-02: "there are a bunch of kiki-* folders in tmp right now. they
//! should be deleted on exit of test"). Every test that wants a folder takes one of these
//! instead of a path and a `remove_dir_all` it may never reach; the share job and the bench use
//! it for their own scratch for the same reason. Standard library only, and no `use crate::`:
//! the plugins' tests include this file by path, since they do not link the daemon.
#![allow(dead_code)]

use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

pub struct Scratch(PathBuf);

impl Scratch {
    /// `temp_dir()/kiki-<tag>-<pid>-<nth>`, made now. The count is what keeps two callers that
    /// choose the same tag apart, which they do: `temp_tree(3)` is asked for twice in
    /// `listing/tests.rs` and `cargo test` runs those two tests on different threads of one
    /// process, so the tag and the pid alone named one folder for both. Not a near miss —
    /// `named` below *begins* by clearing what is there, so the test that started second wiped
    /// the first one's tree out from under it, and `Drop` deletes, so whichever finished first
    /// took the other's files with it; the symptom is a listing test failing with a wrong count
    /// now and then on a machine with enough cores. Unique by construction beats unique because
    /// every tag was picked carefully (2026-10-24, docs/1.0.0/01-from-the-fork.md item 8).
    pub fn new(tag: &str) -> Scratch {
        static NTH: AtomicUsize = AtomicUsize::new(0);
        Scratch::named(&format!("{tag}-{}-{}", std::process::id(), NTH.fetch_add(1, Ordering::Relaxed)))
    }

    /// `temp_dir()/kiki-<name>` exactly, for a name that is already unique and may be worked
    /// out by somebody else (a job's id: `share.rs`, which `tests/share_contract.rs` spells by
    /// hand). No count here — the exact name *is* the contract.
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

#[cfg(test)]
mod tests {
    use super::*;

    /// Two callers that choose the same tag get two folders. Before the count they got one
    /// between them, and since `named` opens with `remove_dir_all` and `Drop` closes with one,
    /// each was deleting the other's tree while it was still being read
    /// (docs/1.0.0/01-from-the-fork.md item 8).
    #[test]
    fn the_same_tag_twice_is_two_folders() {
        let (a, b) = (Scratch::new("same-tag"), Scratch::new("same-tag"));
        assert_ne!(a.path(), b.path(), "two scratches with one tag must not share a folder");
        std::fs::write(a.join("mine"), b"a").unwrap();
        assert!(!b.join("mine").exists(), "and neither can see what the other put in");
        let kept = a.path().to_path_buf();
        drop(b);
        assert!(kept.exists(), "nor does one being dropped take the other's folder with it");
    }
}
