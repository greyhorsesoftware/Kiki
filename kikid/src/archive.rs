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
    run(cmd, cancel, progress).map_err(|e| {
        let _ = std::fs::remove_file(archive);
        e
    })
}

/// Extracts `archive` into `dest` (created if missing). Refuses entries that would escape `dest`.
pub fn extract(archive: &Path, dest: &Path, cancel: &AtomicBool, progress: &mut dyn FnMut(&str)) -> Result<Vec<String>> {
    // Inspect the member list first so a hostile archive never writes anything.
    let members = list(archive)?;
    for m in &members {
        let p = Path::new(&m.name);
        if p.is_absolute() || p.components().any(|c| matches!(c, std::path::Component::ParentDir)) {
            return Err(VfsError::Io(format!("refusing unsafe archive entry {}", m.name)));
        }
    }
    std::fs::create_dir_all(dest)?;
    let mut cmd = Command::new("bsdtar");
    cmd.arg("-xvf").arg(archive).arg("-C").arg(dest).arg("--no-same-owner");
    run(cmd, cancel, progress)?;
    // Top-level names created, for the journal's inverse.
    let mut top: Vec<String> = members.iter().filter_map(|m| m.name.trim_end_matches('/').split('/').next().map(str::to_string)).collect();
    top.sort();
    top.dedup();
    Ok(top)
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
    let out = Command::new("bsdtar").arg("-tvf").arg(archive).stderr(Stdio::piped()).output().map_err(|e| if e.kind() == std::io::ErrorKind::NotFound { VfsError::Io("bsdtar is not installed".into()) } else { e.into() })?;
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
    Ok(Value::obj()
        .v("members", Value::Arr(m.into_iter().take(200).map(|x| Value::obj().s("name", x.name).u("size", x.size).b("isDir", x.is_dir).done()).collect()))
        .u("n", n as u64)
        .done())
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
        let top = extract(&d.join("src.tar.gz"), &d.join("out"), &cancel, &mut |_| {}).unwrap();
        assert_eq!(top, vec!["src".to_string()]);
        assert_eq!(std::fs::read(d.join("out/src/sub/b c.txt")).unwrap(), b"world!");
        assert_eq!(format_from_name("x.tar.zst"), Some("tar.zst"));
        assert_eq!(format_from_name("x.ZIP"), Some("zip"));
        assert_eq!(format_from_name("x.txt"), None);
        std::fs::remove_dir_all(&d).unwrap();
    }
}
