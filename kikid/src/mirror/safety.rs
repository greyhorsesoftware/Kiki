//! Untrusted relative names, made safe before they are joined to a local root.
//!
//! A name in a plan is not kiki's. It comes from a plugin's `Scan` reply, which comes from a
//! server — which may be hostile, or may have been taken over since the location was saved, or
//! may simply be a third-party plugin with a bug. Joined straight to a root,
//! `../../.ssh/authorized_keys` is a write outside the folder the person chose. A parent
//! component that is a symlink is the same write by another route, and it can be *made* one
//! during a long transfer, after any check by path has already passed — which is why the checks
//! here end in an open file descriptor rather than a verdict about a string
//! (docs/1.0.0/01-from-the-fork.md item 3).
use super::*;
use std::fs::{File, OpenOptions};
use std::os::fd::AsRawFd;
use std::os::unix::fs::OpenOptionsExt;

/// Every component of a relative path must be an ordinary name: nothing empty (which is a
/// leading, trailing or doubled `/`), no `.`, no `..`, and no NUL. An absolute path is caught by
/// the same rule, its first component being empty.
pub(crate) fn validate_rel(rel: &str) -> Result<(), VfsError> {
    if rel.contains('\0') || rel.split('/').any(|c| c.is_empty() || c == "." || c == "..") {
        return Err(VfsError::said(1224, &[("path", &rel)], format!("Safety: refusing a path that is not inside the transfer root: {rel}")));
    }
    Ok(())
}

/// One entry from a listing, which is a single name and never a path.
pub(crate) fn validate_name(name: &str) -> Result<(), VfsError> {
    validate_rel(name)?;
    if name.contains('/') {
        return Err(VfsError::said(1224, &[("path", &name)], format!("Safety: refusing a path that is not inside the transfer root: {name}")));
    }
    Ok(())
}

/// A leaf to work on, with its parent directory held open for as long as the path is used.
///
/// `path` is `/proc/self/fd/<parent>/<leaf>`, which the kernel resolves through the open
/// descriptor — so it goes on meaning the same directory even if that directory is renamed, and
/// cannot be made to mean another one by replacing a component with a symlink halfway through a
/// copy. Holding `_parent` is what keeps that true: the path is only valid while this value is
/// alive, which is why it is returned rather than just a `PathBuf`.
pub(crate) struct LocalPath {
    _parent: File,
    path: PathBuf,
}

impl LocalPath {
    /// The anchored path, borrowed — which is the whole safety property, so it is deliberately
    /// not a `PathBuf` anybody can take away.
    ///
    /// Handing out an owned path made this easy to get wrong in a way nothing would catch at
    /// compile time: `&if … { root.clone() } else { local_path(p, rel)?.path }` drops the guard
    /// at the end of the `else` **block**, not at the end of the statement, so the descriptor
    /// closed and `/proc/self/fd/<n>` was gone by the time the path was used — ENOENT on a file
    /// that was plainly there. Borrowing forces the guard to be bound to a variable, which is
    /// exactly the lifetime the path needs.
    pub(crate) fn path(&self) -> &Path {
        &self.path
    }
}

/// Walk `rel` under `root` a component at a time, opening each directory without following
/// symlinks, and return the leaf anchored to its parent. Every path-taking call then works
/// unchanged and is confined: `create_new`, `remove_tree`, `create_dir`, `rename`, and
/// `ops::copy_file` with its `copy_file_range` fast path intact.
pub(crate) fn local_path(root: &Path, rel: &str) -> Result<LocalPath, VfsError> {
    validate_rel(rel)?;
    let directory = |p: &Path| OpenOptions::new().read(true).custom_flags(libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC).open(p);
    // The root itself is the person's choice, so it may be a symlink; nothing below it may be.
    let mut parent = OpenOptions::new().read(true).custom_flags(libc::O_DIRECTORY | libc::O_CLOEXEC).open(root)?;
    let mut parts = rel.split('/').peekable();
    while let Some(part) = parts.next() {
        let path = PathBuf::from(format!("/proc/self/fd/{}", parent.as_raw_fd())).join(part);
        if parts.peek().is_none() {
            return Ok(LocalPath { _parent: parent, path });
        }
        parent = directory(&path)?;
    }
    unreachable!("validate_rel refuses an empty path, so there is always a last component")
}

/// Open a file to be read as the source of a transfer: not through a symlink, and nothing that
/// is not a regular file. `O_NONBLOCK` so that a named pipe someone left in the tree cannot
/// block the open before the kind can be looked at.
pub(crate) fn open_source(path: &Path) -> Result<File, VfsError> {
    let file = OpenOptions::new().read(true).custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK | libc::O_CLOEXEC).open(path)?;
    if !file.metadata()?.is_file() {
        return Err(VfsError::said(1225, &[("path", &path.display())], format!("Safety: the transfer source is not a regular file: {}", path.display())));
    }
    Ok(file)
}

/// Create the file a download lands in, under `parent`, refusing anything already there that is
/// not ours to replace. `O_EXCL` will not open an existing symlink, so a part file planted ahead
/// of time cannot be written through; a stale one from a run that was killed is unlinked first,
/// and `unlink` never follows a symlink either.
pub(crate) fn create_part(path: &Path) -> Result<File, VfsError> {
    let _ = std::fs::remove_file(path);
    Ok(OpenOptions::new().write(true).create_new(true).custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC).open(path)?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    #[test]
    fn a_relative_name_must_be_ordinary_components() {
        for bad in ["", "/etc/passwd", "../x", "a/../../x", "a/./b", "a//b", "a/", "/", "..", ".", "a\0b"] {
            assert!(validate_rel(bad).is_err(), "{bad:?} must be refused");
        }
        for good in ["a", "a/b/c", "space and 'quotes'", "a\\b", "...", "..a", "a..", "-"] {
            assert!(validate_rel(good).is_ok(), "{good:?} is an ordinary name");
        }
        assert!(validate_name("a/b").is_err(), "a listing entry is one name, not a path");
        assert!(validate_name("a").is_ok());
    }

    /// The part that a check on the string cannot do: a component swapped for a symlink while
    /// the transfer is running. The fd keeps the write where it was aimed.
    #[test]
    fn a_symlinked_parent_is_refused_and_an_open_one_stays_put() {
        let root = crate::scratch::Scratch::new("safety-anchor");
        let outside = crate::scratch::Scratch::new("safety-outside");
        std::os::unix::fs::symlink(outside.path(), root.join("escape")).unwrap();
        assert!(local_path(root.path(), "escape/victim").is_err(), "a symlinked parent is not a way in");

        std::fs::create_dir(root.join("safe")).unwrap();
        let held = local_path(root.path(), "safe/file").unwrap();
        let held_path = held.path().to_path_buf();
        // Now move the directory out from under the name and put a symlink where it was.
        std::fs::rename(root.join("safe"), root.join("moved")).unwrap();
        std::os::unix::fs::symlink(outside.path(), root.join("safe")).unwrap();
        std::fs::write(held.path(), b"anchored").unwrap();
        assert!(!outside.join("file").exists(), "the write did not follow the new symlink");
        assert_eq!(std::fs::read(root.join("moved/file")).unwrap(), b"anchored", "it went to the directory that was opened");
        // And the guard is what keeps that path meaningful: once it goes, the path means nothing,
        // which is why `path()` borrows rather than handing out something detachable.
        drop(held);
        assert!(std::fs::symlink_metadata(&held_path).is_err(), "the anchored path is only valid while its guard lives");
    }

    #[test]
    fn a_source_is_a_regular_file_and_not_a_link_to_one() {
        let d = crate::scratch::Scratch::new("safety-source");
        std::fs::write(d.join("real"), b"x").unwrap();
        std::os::unix::fs::symlink(d.join("real"), d.join("link")).unwrap();
        std::fs::create_dir(d.join("dir")).unwrap();
        assert!(open_source(&d.join("real")).is_ok());
        assert!(open_source(&d.join("link")).is_err(), "a source is not reached through a symlink");
        assert!(open_source(&d.join("dir")).is_err(), "nor is a directory a source");
    }

    /// A part file planted as a symlink is not written through, and the real file it points at
    /// is left exactly as it was.
    #[test]
    fn a_planted_part_file_is_not_written_through() {
        let d = crate::scratch::Scratch::new("safety-part");
        let victim = d.join("victim");
        std::fs::write(&victim, b"keep").unwrap();
        std::os::unix::fs::symlink(&victim, d.join("notes.txt.kiki-part")).unwrap();
        let f = create_part(&d.join("notes.txt.kiki-part")).unwrap();
        assert_eq!(f.metadata().unwrap().permissions().mode() & 0o170000, 0o100000, "a regular file of our own");
        assert_eq!(std::fs::read(&victim).unwrap(), b"keep", "and the file it pointed at is untouched");
        // A stale part file from a killed run is replaceable, which is the case `O_EXCL` alone would break.
        std::fs::write(d.join("stale.kiki-part"), b"half").unwrap();
        assert!(create_part(&d.join("stale.kiki-part")).is_ok());
    }
}
