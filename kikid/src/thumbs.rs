//! Thumbnails per the freedesktop thumbnail spec (the decoding itself is `decode.rs`): `~/.cache/thumbnails/{normal,large}/<md5 of uri>.png`
//! with `Thumb::URI` and `Thumb::MTime` keys. JPEGs try the embedded EXIF thumbnail first;
//! video frames come from `ffmpeg`; PDF pages from `pdftoppm`. Failures are recorded under
//! `fail/kiki/` so they are not retried on every scroll.

use crate::decode::{self, Why};
use crate::kinds::Kind;
use crate::vfs::uri::Uri;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Size {
    Normal = 128,
    Large = 256,
}

impl Size {
    fn dir(self) -> &'static str {
        match self {
            Size::Normal => "normal",
            Size::Large => "large",
        }
    }
    fn px(self) -> u32 {
        self as u32
    }
}

pub fn cache_root() -> PathBuf {
    if let Ok(d) = std::env::var("KIKI_THUMB_DIR") {
        return PathBuf::from(d);
    }
    std::env::var("XDG_CACHE_HOME").map(PathBuf::from).unwrap_or_else(|_| crate::config::home().join(".cache")).join("thumbnails")
}

fn key(uri: &Uri) -> String {
    crate::md5::hex(uri.to_string().as_bytes())
}

pub fn cached_path(uri: &Uri, size: Size) -> PathBuf {
    cache_root().join(size.dir()).join(format!("{}.png", key(uri)))
}

fn fail_path(uri: &Uri) -> PathBuf {
    cache_root().join("fail").join("kiki").join(format!("{}.png", key(uri)))
}

/// Returns the cached thumbnail path if it exists and matches the file's mtime.
pub fn lookup(uri: &Uri, size: Size, mtime_ms: u64) -> Option<PathBuf> {
    let p = cached_path(uri, size);
    let meta = fs::metadata(&p).ok()?;
    if meta.len() == 0 {
        return None;
    }
    // Cheap validation: compare the recorded mtime key when we can read it.
    if let Some(recorded) = read_mtime_key(&p) {
        if recorded != mtime_ms / 1000 {
            return None;
        }
    }
    Some(p)
}

/// How long a failure stands before the file is tried again. A marker used to stand for ever
/// (while the file's mtime was unchanged), and the decoders cannot tell a bad file from a bad
/// moment — ffmpeg or pdftoppm not answering under load, a decode starved of memory — so one
/// bad hour marked hundreds of good files as never-to-be-thumbnailed: 892 markers on one day
/// on the owner's machine, and "the gallery sometimes loses its thumbnails" (2026-09-28). A day
/// is long enough that a truly bad file is not retried on every scroll, and short enough that
/// a photograph is back tomorrow. `KIKI_THUMB_FAIL_TTL_S` for the tests.
fn fail_ttl() -> std::time::Duration {
    std::time::Duration::from_secs(std::env::var("KIKI_THUMB_FAIL_TTL_S").ok().and_then(|v| v.parse().ok()).unwrap_or(24 * 60 * 60))
}

/// Whether a recent failure stands for this file: the marker names the same mtime and is
/// younger than `fail_ttl`. An older marker is simply written over by the next attempt.
fn failed(uri: &Uri, mtime_ms: u64) -> bool {
    let p = fail_path(uri);
    let fresh = fs::metadata(&p).and_then(|m| m.modified()).ok().and_then(|t| t.elapsed().ok()).map(|age| age < fail_ttl()).unwrap_or(false);
    fresh && read_mtime_key(&p) == Some(mtime_ms / 1000)
}

fn read_mtime_key(png: &Path) -> Option<u64> {
    let f = fs::File::open(png).ok()?;
    let dec = png::Decoder::new(std::io::BufReader::new(f));
    let reader = dec.read_info().ok()?;
    for t in &reader.info().uncompressed_latin1_text {
        if t.keyword == "Thumb::MTime" {
            return t.text.trim().parse().ok();
        }
    }
    None
}

/// Generate (or fetch from cache) the thumbnail for a local file. Blocking; call from a worker.
pub fn generate(uri: &Uri, kind: Kind, size: Size, mtime_ms: u64) -> Option<PathBuf> {
    if let Some(p) = lookup(uri, size, mtime_ms) {
        return Some(p);
    }
    // A cached thumbnail of a remote file is served above; making one is not this function's —
    // `to_path` drops the scheme and the host, so what follows would read this machine's copy of
    // that path, and a fail marker would then be recorded against the remote name for ever.
    if !uri.is_local() {
        return None;
    }
    if failed(uri, mtime_ms) {
        return None;
    }
    let src = uri.to_path();
    let img = decode::decode(&src, kind, size.px());
    match img {
        Ok(img) => {
            let out = cached_path(uri, size);
            if write_png(&out, &img, uri, mtime_ms).is_ok() {
                Some(out)
            } else {
                None
            }
        }
        // The file's fault: marked, with the reason written into the marker where somebody
        // wondering why a picture has no thumbnail can read it (`kiki:Reason`, beside the
        // spec's own keys), and not tried again until the marker has aged (`fail_ttl`).
        Err(Why::BadFile(reason)) => {
            let fp = fail_path(uri);
            let _ = fs::create_dir_all(fp.parent().unwrap());
            let _ = write_fail(&fp, uri, mtime_ms, &reason);
            None
        }
        // Not the file's fault — a tool missing or killed, a read that failed: nothing is
        // written, and the next look tries again. (Before 2026-09-28 this was marked like a
        // bad file, and one bad hour cost hundreds of good files their thumbnails for ever.)
        Err(Why::CouldNotRun(reason)) => {
            eprintln!("thumbnail {}: not made, not marked: {reason}", uri);
            None
        }
    }
}

/// The marker: a 1 × 1 PNG with the spec's keys and the reason.
fn write_fail(fp: &Path, uri: &Uri, mtime_ms: u64, reason: &str) -> std::io::Result<()> {
    write_png_with(fp, &image::RgbaImage::new(1, 1), uri, mtime_ms, Some(reason))
}

/// What a marker says went wrong, for whoever is looking.
pub fn read_reason(png: &Path) -> Option<String> {
    let decoder = png::Decoder::new(std::io::BufReader::new(fs::File::open(png).ok()?));
    let reader = decoder.read_info().ok()?;
    reader.info().uncompressed_latin1_text.iter().find(|t| t.keyword == "kiki:Reason").map(|t| t.text.clone())
}

fn write_png(out: &Path, img: &image::RgbaImage, uri: &Uri, mtime_ms: u64) -> std::io::Result<()> {
    write_png_with(out, img, uri, mtime_ms, None)
}

fn write_png_with(out: &Path, img: &image::RgbaImage, uri: &Uri, mtime_ms: u64, reason: Option<&str>) -> std::io::Result<()> {
    fs::create_dir_all(out.parent().unwrap())?;
    let tmp = out.with_extension("tmp");
    {
        let f = fs::File::create(&tmp)?;
        let w = std::io::BufWriter::new(f);
        let mut enc = png::Encoder::new(w, img.width(), img.height());
        enc.set_color(png::ColorType::Rgba);
        enc.set_depth(png::BitDepth::Eight);
        enc.add_text_chunk("Thumb::URI".into(), uri.to_string()).map_err(io)?;
        enc.add_text_chunk("Thumb::MTime".into(), (mtime_ms / 1000).to_string()).map_err(io)?;
        if let Some(r) = reason {
            enc.add_text_chunk("kiki:Reason".into(), r.to_string()).map_err(io)?;
        }
        enc.add_text_chunk("Software".into(), "kiki".into()).map_err(io)?;
        let mut w = enc.write_header().map_err(io)?;
        w.write_image_data(img.as_raw()).map_err(io)?;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = fs::set_permissions(&tmp, fs::Permissions::from_mode(0o600));
    }
    fs::rename(tmp, out)
}

fn io<E: std::fmt::Display>(e: E) -> std::io::Error {
    std::io::Error::other(e.to_string())
}

pub fn thumbable(kind: Kind) -> bool {
    matches!(kind, Kind::Image | Kind::Video | Kind::Pdf)
}

// The pool that used to be here is now a process of its own: `thumber.rs` in the daemon and
// `bin/kiki-thumber.rs` at the other end. Everything above runs there, not here — the daemon
// keeps only `lookup`, which reads a PNG it wrote itself.

#[cfg(test)]
mod tests {
    /// A failure is retried once it is old enough: the marker keeps a bad file from being tried
    /// on every scroll, but a bad moment does not cost the file its thumbnail for ever.
    /// A file that reads but does not decode is marked, and the marker says why.
    #[test]
    fn a_bad_file_is_marked_with_its_reason() {
        let d = std::env::temp_dir().join(format!("kiki-thumb-why-{}", std::process::id()));
        let _ = fs::remove_dir_all(&d);
        fs::create_dir_all(&d).unwrap();
        std::env::set_var("KIKI_THUMB_DIR", &d);
        let bad = d.join("not-really.png");
        fs::write(&bad, b"this is not a picture").unwrap();
        let uri = Uri::from_path(&bad);
        assert!(generate(&uri, Kind::Image, Size::Normal, 5000).is_none());
        let fp = fail_path(&uri);
        assert!(fp.exists(), "a bad file is marked");
        assert_eq!(read_reason(&fp).as_deref(), Some("the image could not be decoded"));
        assert_eq!(read_mtime_key(&fp), Some(5));
        // And a file that cannot be read at all is nobody's verdict: no marker.
        let gone = Uri::from_path(&d.join("missing.png"));
        assert!(generate(&gone, Kind::Image, Size::Normal, 5000).is_none());
        assert!(!fail_path(&gone).exists(), "not the file's fault: nothing written");
        std::env::remove_var("KIKI_THUMB_DIR");
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn a_fail_marker_expires() {
        let d = std::env::temp_dir().join(format!("kiki-thumb-fail-{}", std::process::id()));
        let _ = fs::remove_dir_all(&d);
        fs::create_dir_all(&d).unwrap();
        std::env::set_var("KIKI_THUMB_DIR", &d);
        std::env::set_var("KIKI_THUMB_FAIL_TTL_S", "3600");
        let uri = Uri::from_path(&d.join("bad.jpg"));
        let fp = fail_path(&uri);
        fs::create_dir_all(fp.parent().unwrap()).unwrap();
        write_png(&fp, &image::RgbaImage::new(1, 1), &uri, 7000).unwrap();
        assert!(failed(&uri, 7000), "a fresh marker for this mtime stands");
        assert!(!failed(&uri, 8000), "but not for a file that has changed since");
        // The same marker, two hours old: it no longer stands, and the next attempt writes over it.
        let old = std::time::SystemTime::now() - std::time::Duration::from_secs(2 * 3600);
        fs::File::options().write(true).open(&fp).unwrap().set_modified(old).unwrap();
        assert!(!failed(&uri, 7000), "an old marker is not a verdict");
        std::env::remove_var("KIKI_THUMB_FAIL_TTL_S");
        std::env::remove_var("KIKI_THUMB_DIR");
        let _ = fs::remove_dir_all(&d);
    }

    use super::*;

    #[test]
    fn png_round_trip_with_keys() {
        let _guard = crate::ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let dir = std::env::temp_dir().join(format!("kiki-thumbs-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        std::env::set_var("KIKI_THUMB_DIR", &dir);
        let src = dir.join("src.png");
        fs::create_dir_all(&dir).unwrap();
        let mut img = image::RgbaImage::new(640, 480);
        for p in img.pixels_mut() {
            *p = image::Rgba([200, 40, 40, 255]);
        }
        img.save(&src).unwrap();
        let uri = Uri::from_path(&src);
        assert!(lookup(&uri, Size::Normal, 1000).is_none());
        let out = generate(&uri, Kind::Image, Size::Normal, 1000).unwrap();
        assert!(out.exists());
        assert_eq!(read_mtime_key(&out), Some(1));
        assert!(lookup(&uri, Size::Normal, 1000).is_some());
        assert!(lookup(&uri, Size::Normal, 5000).is_none()); // mtime changed: regenerate
        let img = image::open(&out).unwrap();
        assert!(img.width() <= 128 && img.height() <= 128);
        // an unreadable image records a failure and is not retried
        let bad = dir.join("bad.jpg");
        fs::write(&bad, b"not an image").unwrap();
        let buri = Uri::from_path(&bad);
        assert!(generate(&buri, Kind::Image, Size::Normal, 7000).is_none());
        assert!(failed(&buri, 7000));
        fs::remove_dir_all(&dir).unwrap();
        std::env::remove_var("KIKI_THUMB_DIR");
    }

    /// `Uri::to_path` drops the scheme and the host, so a remote picture was thumbnailed by
    /// opening its path **on this machine**: usually nothing is there and a permanent failure is
    /// recorded, but where something is, a remote row wore a local file's picture.
    #[test]
    fn a_remote_picture_is_not_thumbnailed_from_a_local_path() {
        let _guard = crate::ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let dir = std::env::temp_dir().join(format!("kiki-thumbs-remote-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        std::env::set_var("KIKI_THUMB_DIR", dir.join("cache"));
        let src = dir.join("local.png");
        let mut img = image::RgbaImage::new(64, 48);
        for p in img.pixels_mut() {
            *p = image::Rgba([10, 200, 10, 255]);
        }
        img.save(&src).unwrap();
        // Same path, another machine.
        let remote = Uri::parse(&format!("sftp://elsewhere{}", src.to_string_lossy())).unwrap();
        assert!(!remote.is_local());
        assert_eq!(generate(&remote, Kind::Image, Size::Normal, 1000), None, "a remote file is not this machine's to read");
        assert!(!failed(&remote, 1000), "and no failure is recorded against it: nothing was tried");
        fs::remove_dir_all(&dir).unwrap();
        std::env::remove_var("KIKI_THUMB_DIR");
    }
}
