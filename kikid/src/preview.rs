//! `Preview { uri }`: text head, image or frame path, folder children, archive members (plan 05).

use crate::json::Value;
use crate::kinds::Kind;
use crate::thumbs;
use crate::vfs::uri::Uri;
use crate::vfs::{EntryType, Result, VfsError};
use std::io::Read;

pub const TEXT_CAP: usize = 64 * 1024;
pub const TEXT_LINES: usize = 40;

pub fn preview(uri: &Uri) -> Result<Value> {
    if !uri.is_local() {
        return Err(VfsError::Unsupported);
    }
    let path = uri.to_path();
    let md = std::fs::symlink_metadata(&path)?;
    if md.is_dir() {
        let mut names: Vec<String> = std::fs::read_dir(&path)?.filter_map(|e| e.ok()).map(|e| e.file_name().to_string_lossy().into_owned()).collect();
        let n = names.len();
        names.sort_by_key(|s| s.to_lowercase());
        names.truncate(50);
        return Ok(Value::obj().v("children", Value::Arr(names.into_iter().map(Value::Str).collect())).u("n", n as u64).done());
    }
    let name = uri.name().as_bytes().to_vec();
    let kind = Kind::guess(EntryType::File, &name);
    let mtime_ms = md.modified().ok().and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok()).map(|d| d.as_millis() as u64).unwrap_or(0);
    match kind {
        Kind::Image | Kind::Video | Kind::Pdf => {
            let p = thumbs::generate(uri, kind, thumbs::Size::Large, mtime_ms).ok_or(VfsError::Unsupported)?;
            let (w, h) = image::image_dimensions(&p).unwrap_or((0, 0));
            Ok(Value::obj().s("path", p.to_string_lossy()).u("width", w as u64).u("height", h as u64).done())
        }
        Kind::Archive => crate::archive::members_json(&path),
        _ => text_head(&path),
    }
}

fn text_head(path: &std::path::Path) -> Result<Value> {
    let mut f = std::fs::File::open(path)?;
    let mut buf = vec![0u8; TEXT_CAP];
    let mut read = 0;
    while read < TEXT_CAP {
        let n = f.read(&mut buf[read..])?;
        if n == 0 {
            break;
        }
        read += n;
    }
    buf.truncate(read);
    if buf.iter().take(8192).any(|&b| b == 0) {
        return Err(VfsError::Unsupported); // binary
    }
    let text = String::from_utf8_lossy(&buf);
    let mut lines: Vec<&str> = text.lines().collect();
    let truncated = lines.len() > TEXT_LINES || read >= TEXT_CAP;
    lines.truncate(TEXT_LINES);
    Ok(Value::obj().s("text", lines.join("\n")).u("bytesRead", read as u64).b("truncated", truncated).done())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn text_head_is_capped() {
        let dir = std::env::temp_dir().join(format!("kiki-preview-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let big = dir.join("big.txt");
        let line = "x".repeat(100) + "\n";
        std::fs::write(&big, line.repeat(2000)).unwrap();
        let v = preview(&Uri::from_path(&big)).unwrap();
        assert!(v.u64_field("bytesRead").unwrap() <= TEXT_CAP as u64);
        assert_eq!(v.str_field("text").unwrap().lines().count(), TEXT_LINES);
        assert_eq!(v.get("truncated").unwrap().as_bool(), Some(true));
        let v = preview(&Uri::from_path(&dir)).unwrap();
        assert_eq!(v.u64_field("n"), Some(1));
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
