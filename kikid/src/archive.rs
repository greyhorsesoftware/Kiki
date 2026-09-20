//! Archives through `bsdtar` (libarchive, in Arch base): compress, extract, list members.
//! Spawning keeps the compression libraries out of the daemon; progress comes from
//! bsdtar's verbose output, one line per entry.

use crate::vfs::{Result, VfsError};
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};

pub fn format_from_name(name: &str) -> Option<&'static str> {
    let n = name.to_ascii_lowercase();
    if n.ends_with(".tar.gz") || n.ends_with(".tgz") {
        Some("tar.gz")
    } else if n.ends_with(".tar.xz") || n.ends_with(".txz") {
        Some("tar.xz")
    } else if n.ends_with(".tar.zst") || n.ends_with(".tzst") {
        Some("tar.zst")
    } else if n.ends_with(".tar.bz2") || n.ends_with(".tbz2") {
        Some("tar.bz2")
    } else if n.ends_with(".tar") {
        Some("tar")
    } else if n.ends_with(".zip") {
        Some("zip")
    } else if n.ends_with(".7z") {
        Some("7z")
    } else {
        None
    }
}

fn format_args(format: &str) -> Result<Vec<&'static str>> {
    Ok(match format {
        "zip" => vec!["--format", "zip"],
        "tar" => vec!["--format", "ustar"],
        "tar.gz" => vec!["--format", "ustar", "--gzip"],
        "tar.xz" => vec!["--format", "ustar", "--xz"],
        "tar.zst" => vec!["--format", "ustar", "--zstd"],
        "tar.bz2" => vec!["--format", "ustar", "--bzip2"],
        "7z" => vec!["--format", "7zip"],
        _ => return Err(VfsError::Unsupported),
    })
}

/// Creates `archive` from `items` (paths sharing a parent directory); `progress` gets each entry name.
pub fn compress(items: &[PathBuf], archive: &Path, format: &str, cancel: &AtomicBool, progress: &mut dyn FnMut(&str)) -> Result<()> {
    if items.is_empty() {
        return Err(VfsError::Io("nothing to compress".into()));
    }
    let parent = items[0].parent().ok_or(VfsError::NotFound)?;
    let mut cmd = Command::new("bsdtar");
    cmd.arg("-cvf").arg(archive).args(format_args(format)?).arg("-C").arg(parent);
    for it in items {
        if it.parent() != Some(parent) {
            return Err(VfsError::Io("items must share a parent".into()));
        }
        cmd.arg(it.file_name().ok_or(VfsError::NotFound)?);
    }
    run(cmd, cancel, progress).inspect_err(|_| {
        let _ = std::fs::remove_file(archive);
    })
}

/// Every member's name, uncapped (`bsdtar -tf`): what the safety check reads. `list` stops at
/// 10,000 entries because it is for showing, and a check that stops there is no check.
fn names(archive: &Path) -> Result<Vec<String>> {
    let out = Command::new("bsdtar")
        .arg("-tf")
        .arg(archive)
        .stderr(Stdio::piped())
        .output()
        .map_err(|e| if e.kind() == std::io::ErrorKind::NotFound { VfsError::Io("bsdtar is not installed".into()) } else { e.into() })?;
    if !out.status.success() {
        return Err(VfsError::Io(String::from_utf8_lossy(&out.stderr).trim().to_string()));
    }
    Ok(String::from_utf8_lossy(&out.stdout).lines().map(str::to_string).collect())
}

/// An archive's name without its archive extension: `site.tar.gz` -> `site`.
pub fn stem(name: &str) -> &str {
    let lower = name.to_ascii_lowercase();
    for ext in [".tar.gz", ".tar.xz", ".tar.zst", ".tar.bz2", ".tgz", ".txz", ".tzst", ".tbz2", ".tar", ".zip", ".7z"] {
        if lower.ends_with(ext) && name.len() > ext.len() {
            return &name[..name.len() - ext.len()];
        }
    }
    name
}

/// Extracts `archive` into `dest` (created if missing) and returns the ONE thing it made there,
/// or `None` for an empty archive.
///
/// - An archive holding a single file or folder puts that in `dest`.
/// - An archive holding several puts them in a new folder named after the archive — thirty
///   files loose among the ones already there is never what was wanted (plan 05).
/// - Either way the name is a free one ("site", then "site (2)"): nothing in `dest` is merged
///   into or overwritten. That is also what makes undo exact — it removes what is returned
///   here, and nothing that was there before can be inside it.
///
/// The members are unpacked in a staging folder inside `dest` (same filesystem, so taking their
/// place is a rename), and an archive with an entry that would escape it is refused before
/// anything is written.
pub fn extract(archive: &Path, dest: &Path, cancel: &AtomicBool, progress: &mut dyn FnMut(&str)) -> Result<Option<PathBuf>> {
    for name in names(archive)? {
        let p = Path::new(&name);
        if p.is_absolute() || p.components().any(|c| matches!(c, std::path::Component::ParentDir)) {
            return Err(VfsError::Unsafe(format!("refusing unsafe archive entry {name}")));
        }
    }
    std::fs::create_dir_all(dest)?;
    let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.subsec_nanos()).unwrap_or(0);
    let staging = dest.join(format!(".kiki-extract-{}-{nanos}", std::process::id()));
    std::fs::create_dir(&staging)?;
    let placed = (|| {
        let mut cmd = Command::new("bsdtar");
        cmd.arg("-xvf").arg(archive).arg("-C").arg(&staging).arg("--no-same-owner");
        run(cmd, cancel, progress)?;
        let mut top: Vec<std::ffi::OsString> = std::fs::read_dir(&staging)?.filter_map(|e| e.ok()).map(|e| e.file_name()).collect();
        top.sort();
        match top.as_slice() {
            [] => Ok(None),
            [only] => {
                let to = dest.join(crate::ops::unique_name(dest, &only.to_string_lossy()));
                std::fs::rename(staging.join(only), &to)?;
                Ok(Some(to))
            }
            _ => {
                let folder = archive.file_name().map(|n| stem(&n.to_string_lossy()).to_string()).unwrap_or_else(|| "archive".into());
                let to = dest.join(crate::ops::unique_name(dest, &folder));
                std::fs::rename(&staging, &to)?;
                Ok(Some(to))
            }
        }
    })();
    // Gone already when it became the folder; otherwise empty, or what a failure left behind.
    let _ = std::fs::remove_dir_all(&staging);
    placed
}

fn run(mut cmd: Command, cancel: &AtomicBool, progress: &mut dyn FnMut(&str)) -> Result<()> {
    cmd.stdout(Stdio::null()).stderr(Stdio::piped()).stdin(Stdio::null());
    let mut child = cmd.spawn().map_err(|e| if e.kind() == std::io::ErrorKind::NotFound { VfsError::Io("bsdtar is not installed".into()) } else { e.into() })?;
    let stderr = child.stderr.take().unwrap();
    let mut errors = String::new();
    for line in BufReader::new(stderr).lines() {
        if cancel.load(Ordering::Relaxed) {
            let _ = child.kill();
            let _ = child.wait();
            return Err(VfsError::Io("cancelled".into()));
        }
        let line = match line {
            Ok(l) => l,
            Err(_) => break,
        };
        // Verbose listing lines start with "x " or "a "; anything else is an error message.
        if let Some(name) = line.strip_prefix("x ").or_else(|| line.strip_prefix("a ")) {
            progress(name);
        } else if !line.trim().is_empty() {
            errors.push_str(&line);
            errors.push('\n');
        }
    }
    let status = child.wait()?;
    if !status.success() {
        return Err(VfsError::Io(if errors.is_empty() { format!("bsdtar exited with {status}") } else { errors.trim().to_string() }));
    }
    Ok(())
}

pub struct Member {
    pub name: String,
    pub size: u64,
    pub is_dir: bool,
}

/// Lists members with sizes (`bsdtar -tvf`), capped at 10,000 entries.
pub fn list(archive: &Path) -> Result<Vec<Member>> {
    let out = Command::new("bsdtar")
        .arg("-tvf")
        .arg(archive)
        .stderr(Stdio::piped())
        .output()
        .map_err(|e| if e.kind() == std::io::ErrorKind::NotFound { VfsError::Io("bsdtar is not installed".into()) } else { e.into() })?;
    if !out.status.success() {
        return Err(VfsError::Io(String::from_utf8_lossy(&out.stderr).trim().to_string()));
    }
    let text = String::from_utf8_lossy(&out.stdout);
    let mut members = Vec::new();
    for line in text.lines().take(10_000) {
        // "-rw-r--r--  0 david david   1126 Sep 12 14:02 kiki/README.md"
        let mut parts = line.split_whitespace();
        let mode = parts.next().unwrap_or("");
        let _links = parts.next();
        let _user = parts.next();
        let _group = parts.next();
        let size: u64 = parts.next().and_then(|s| s.parse().ok()).unwrap_or(0);
        let _m = parts.next();
        let _d = parts.next();
        let _t = parts.next();
        let name: String = parts.collect::<Vec<_>>().join(" ");
        if name.is_empty() {
            continue;
        }
        members.push(Member { name, size, is_dir: mode.starts_with('d') });
    }
    Ok(members)
}

pub fn members_json(archive: &Path) -> Result<crate::json::Value> {
    use crate::json::Value;
    let m = list(archive)?;
    let n = m.len();
    Ok(Value::obj().v("members", Value::Arr(m.into_iter().take(200).map(|x| Value::obj().s("name", x.name).u("size", x.size).b("isDir", x.is_dir).done()).collect())).u("n", n as u64).done())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compress_list_extract_round_trip() {
        let d = std::env::temp_dir().join(format!("kiki-archive-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(d.join("src/sub")).unwrap();
        std::fs::write(d.join("src/a.txt"), b"hello").unwrap();
        std::fs::write(d.join("src/sub/b c.txt"), b"world!").unwrap();
        let cancel = AtomicBool::new(false);
        let mut seen = Vec::new();
        compress(&[d.join("src")], &d.join("src.tar.gz"), "tar.gz", &cancel, &mut |n| seen.push(n.to_string())).unwrap();
        assert!(seen.iter().any(|s| s.contains("b c.txt")));
        let members = list(&d.join("src.tar.gz")).unwrap();
        assert!(members.iter().any(|m| m.name.ends_with("sub/b c.txt") && m.size == 6));
        let made = extract(&d.join("src.tar.gz"), &d.join("out"), &cancel, &mut |_| {}).unwrap();
        assert_eq!(made, Some(d.join("out/src")), "one top-level folder lands in the destination as itself");
        assert_eq!(std::fs::read(d.join("out/src/sub/b c.txt")).unwrap(), b"world!");
        assert_eq!(format_from_name("x.tar.zst"), Some("tar.zst"));
        assert_eq!(format_from_name("x.ZIP"), Some("zip"));
        assert_eq!(format_from_name("x.txt"), None);
        std::fs::remove_dir_all(&d).unwrap();
    }

    fn scratch(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("kiki-archive-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    fn only_staging_left_is_none(dir: &Path) {
        let left: Vec<String> = std::fs::read_dir(dir).unwrap().filter_map(|e| e.ok()).map(|e| e.file_name().to_string_lossy().into_owned()).filter(|n| n.starts_with(".kiki-extract")).collect();
        assert!(left.is_empty(), "staging folder left behind: {left:?}");
    }

    #[test]
    fn several_top_level_entries_go_into_a_folder_named_after_the_archive() {
        let d = scratch("many");
        std::fs::create_dir_all(d.join("in/docs")).unwrap();
        std::fs::write(d.join("in/a.txt"), b"a").unwrap();
        std::fs::write(d.join("in/b.txt"), b"b").unwrap();
        std::fs::write(d.join("in/docs/c.txt"), b"c").unwrap();
        let cancel = AtomicBool::new(false);
        compress(&[d.join("in/a.txt"), d.join("in/b.txt"), d.join("in/docs")], &d.join("site.v2.tar.gz"), "tar.gz", &cancel, &mut |_| {}).unwrap();
        let out = d.join("out");
        let made = extract(&d.join("site.v2.tar.gz"), &out, &cancel, &mut |_| {}).unwrap();
        assert_eq!(made, Some(out.join("site.v2")));
        assert_eq!(std::fs::read(out.join("site.v2/docs/c.txt")).unwrap(), b"c");
        assert_eq!(std::fs::read_dir(&out).unwrap().count(), 1, "nothing loose beside the folder");
        only_staging_left_is_none(&out);
        std::fs::remove_dir_all(&d).unwrap();
    }

    /// The hazard this replaced: extraction merged into a folder that was already there, and undo
    /// then deleted the folder — the user's own files with it.
    #[test]
    fn extracting_never_merges_into_what_is_there_so_undo_cannot_take_it() {
        let d = scratch("merge");
        std::fs::create_dir_all(d.join("src")).unwrap();
        std::fs::write(d.join("src/new.txt"), b"from the archive").unwrap();
        let cancel = AtomicBool::new(false);
        compress(&[d.join("src")], &d.join("src.zip"), "zip", &cancel, &mut |_| {}).unwrap();
        // The folder of the same name that is already here, with something of the user's in it.
        std::fs::write(d.join("src/mine.txt"), b"mine").unwrap();
        let made = extract(&d.join("src.zip"), &d, &cancel, &mut |_| {}).unwrap().unwrap();
        assert_eq!(made, d.join("src (2)"), "a free name, not a merge");
        assert!(made.join("new.txt").exists());
        assert!(!made.join("mine.txt").exists());
        // Undo is "delete what extract returned".
        std::fs::remove_dir_all(&made).unwrap();
        assert_eq!(std::fs::read(d.join("src/mine.txt")).unwrap(), b"mine", "what was there before is untouched");
        only_staging_left_is_none(&d);
        std::fs::remove_dir_all(&d).unwrap();
    }

    #[test]
    fn an_entry_that_escapes_is_refused_before_anything_is_written() {
        let d = scratch("evil");
        std::fs::create_dir_all(d.join("deep/er")).unwrap();
        std::fs::write(d.join("evil.txt"), b"x").unwrap();
        // bsdtar will happily store a `..` path when asked to (-P); unpacking it is what we refuse.
        let made = Command::new("bsdtar").current_dir(d.join("deep/er")).args(["-cPf", "../../evil.tar", "../../evil.txt"]).status().unwrap();
        assert!(made.success());
        assert!(names(&d.join("evil.tar")).unwrap().iter().any(|n| n.contains("..")), "the fixture really holds a .. entry");
        let out = d.join("out");
        let cancel = AtomicBool::new(false);
        let r = extract(&d.join("evil.tar"), &out, &cancel, &mut |_| {});
        assert!(matches!(r, Err(VfsError::Unsafe(_))), "a typed refusal");
        assert_eq!(r.unwrap_err().code(), "Unsafe");
        assert!(!out.exists(), "and nothing was created, not even the destination");
        std::fs::remove_dir_all(&d).unwrap();
    }

    #[test]
    fn stems() {
        assert_eq!(stem("site.tar.gz"), "site");
        assert_eq!(stem("Photos.ZIP"), "Photos");
        assert_eq!(stem("site.v2.tgz"), "site.v2");
        assert_eq!(stem("notes.txt"), "notes.txt");
        assert_eq!(stem(".zip"), ".zip");
    }
}
