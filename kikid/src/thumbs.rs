//! Thumbnails per the freedesktop thumbnail spec: `~/.cache/thumbnails/{normal,large}/<md5 of uri>.png`
//! with `Thumb::URI` and `Thumb::MTime` keys. JPEGs try the embedded EXIF thumbnail first;
//! video frames come from `ffmpeg`; PDF pages from `pdftoppm`. Failures are recorded under
//! `fail/kiki/` so they are not retried on every scroll.

use crate::kinds::Kind;
use crate::vfs::uri::Uri;
use std::fs;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::mpsc::{self, Sender};
use std::sync::{Arc, Mutex, OnceLock};

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

fn failed(uri: &Uri, mtime_ms: u64) -> bool {
    let p = fail_path(uri);
    match read_mtime_key(&p) {
        Some(t) => t == mtime_ms / 1000,
        None => false,
    }
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
    if failed(uri, mtime_ms) {
        return None;
    }
    let src = uri.to_path();
    let img = match kind {
        Kind::Image => decode_image(&src, size.px()),
        Kind::Video => video_frame(&src, size.px()),
        Kind::Pdf => pdf_page(&src, size.px()),
        _ => None,
    };
    match img {
        Some(img) => {
            let out = cached_path(uri, size);
            if write_png(&out, &img, uri, mtime_ms).is_ok() {
                Some(out)
            } else {
                None
            }
        }
        None => {
            let fp = fail_path(uri);
            let _ = fs::create_dir_all(fp.parent().unwrap());
            let _ = write_png(&fp, &image::RgbaImage::new(1, 1), uri, mtime_ms);
            None
        }
    }
}

fn write_png(out: &Path, img: &image::RgbaImage, uri: &Uri, mtime_ms: u64) -> std::io::Result<()> {
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
    std::io::Error::new(std::io::ErrorKind::Other, e.to_string())
}

// ---------------------------------------------------------------- images

fn decode_image(src: &Path, px: u32) -> Option<image::RgbaImage> {
    let is_jpeg = src.extension().map(|e| e.eq_ignore_ascii_case("jpg") || e.eq_ignore_ascii_case("jpeg")).unwrap_or(false);
    let (bytes, orientation) = if is_jpeg {
        let (thumb, orient) = exif_thumbnail(src);
        (thumb, orient)
    } else {
        (None, 1)
    };
    let img = match bytes {
        Some(b) if b.len() > 512 => image::load_from_memory(&b).ok(),
        _ => None,
    }
    .or_else(|| {
        let reader = image::ImageReader::open(src).ok()?.with_guessed_format().ok()?;
        reader.decode().ok()
    })?;
    let img = match orientation {
        3 => img.rotate180(),
        6 => img.rotate90(),
        8 => img.rotate270(),
        _ => img,
    };
    Some(img.thumbnail(px, px).to_rgba8())
}

/// Reads the EXIF APP1 segment of a JPEG and returns (embedded thumbnail bytes, orientation).
fn exif_thumbnail(src: &Path) -> (Option<Vec<u8>>, u32) {
    let mut f = match fs::File::open(src) {
        Ok(f) => f,
        Err(_) => return (None, 1),
    };
    let mut head = vec![0u8; 128 * 1024];
    let n = f.read(&mut head).unwrap_or(0);
    head.truncate(n);
    if head.len() < 4 || head[0] != 0xFF || head[1] != 0xD8 {
        return (None, 1);
    }
    let mut i = 2;
    while i + 4 <= head.len() {
        if head[i] != 0xFF {
            return (None, 1);
        }
        let marker = head[i + 1];
        let len = u16::from_be_bytes([head[i + 2], head[i + 3]]) as usize;
        if marker == 0xE1 && i + 4 + 6 <= head.len() && &head[i + 4..i + 10] == b"Exif\0\0" {
            let seg_end = (i + 2 + len).min(head.len());
            let tiff = &head[i + 10..seg_end];
            return parse_tiff(tiff, &mut f, i + 10);
        }
        if marker == 0xDA {
            break;
        }
        i += 2 + len;
    }
    (None, 1)
}

fn parse_tiff(t: &[u8], f: &mut fs::File, tiff_file_offset: usize) -> (Option<Vec<u8>>, u32) {
    if t.len() < 8 {
        return (None, 1);
    }
    let le = &t[..2] == b"II";
    let rd16 = |b: &[u8], o: usize| -> u32 { if o + 2 > b.len() { 0 } else if le { u16::from_le_bytes([b[o], b[o + 1]]) as u32 } else { u16::from_be_bytes([b[o], b[o + 1]]) as u32 } };
    let rd32 = |b: &[u8], o: usize| -> u32 { if o + 4 > b.len() { 0 } else if le { u32::from_le_bytes([b[o], b[o + 1], b[o + 2], b[o + 3]]) } else { u32::from_be_bytes([b[o], b[o + 1], b[o + 2], b[o + 3]]) } };
    let ifd0 = rd32(t, 4) as usize;
    let mut orientation = 1;
    let count = rd16(t, ifd0) as usize;
    for k in 0..count {
        let e = ifd0 + 2 + k * 12;
        if rd16(t, e) == 0x0112 {
            orientation = rd16(t, e + 8);
        }
    }
    let ifd1 = rd32(t, ifd0 + 2 + count * 12) as usize;
    if ifd1 == 0 || ifd1 + 2 > t.len() {
        return (None, orientation);
    }
    let (mut off, mut len) = (0usize, 0usize);
    let c1 = rd16(t, ifd1) as usize;
    for k in 0..c1 {
        let e = ifd1 + 2 + k * 12;
        match rd16(t, e) {
            0x0201 => off = rd32(t, e + 8) as usize,
            0x0202 => len = rd32(t, e + 8) as usize,
            _ => {}
        }
    }
    if off == 0 || len == 0 || len > 512 * 1024 {
        return (None, orientation);
    }
    let mut buf = vec![0u8; len];
    if f.seek(SeekFrom::Start((tiff_file_offset + off) as u64)).is_err() || f.read_exact(&mut buf).is_err() {
        return (None, orientation);
    }
    (Some(buf), orientation)
}

// ---------------------------------------------------------------- video and pdf via tools

fn video_frame(src: &Path, px: u32) -> Option<image::RgbaImage> {
    let duration = Command::new("ffprobe")
        .args(["-v", "error", "-show_entries", "format=duration", "-of", "csv=p=0"])
        .arg(src)
        .stderr(Stdio::null())
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .and_then(|s| s.trim().parse::<f64>().ok())
        .unwrap_or(0.0);
    let at = format!("{:.2}", (duration * 0.10).max(0.0));
    let out = Command::new("nice")
        .args(["-n", "15", "ffmpeg", "-v", "error", "-ss", &at, "-i"])
        .arg(src)
        .args(["-frames:v", "1", "-vf", &format!("scale={px}:-2"), "-f", "image2", "-c:v", "png", "-"])
        .stderr(Stdio::null())
        .output()
        .ok()?;
    if !out.status.success() || out.stdout.is_empty() {
        return None;
    }
    Some(image::load_from_memory(&out.stdout).ok()?.thumbnail(px, px).to_rgba8())
}

fn pdf_page(src: &Path, px: u32) -> Option<image::RgbaImage> {
    let out = Command::new("nice")
        .args(["-n", "15", "pdftoppm", "-f", "1", "-l", "1", "-png", "-scale-to", &px.to_string()])
        .arg(src)
        .stderr(Stdio::null())
        .output()
        .ok()?;
    if !out.status.success() || out.stdout.is_empty() {
        return None;
    }
    Some(image::load_from_memory(&out.stdout).ok()?.to_rgba8())
}

pub fn thumbable(kind: Kind) -> bool {
    matches!(kind, Kind::Image | Kind::Video | Kind::Pdf)
}

// ---------------------------------------------------------------- worker pool (low priority)

pub struct ThumbJob {
    pub uri: Uri,
    pub kind: Kind,
    pub mtime_ms: u64,
    pub size: Size,
    pub done: Box<dyn FnOnce(Option<PathBuf>) + Send>,
}

fn pool() -> &'static Sender<ThumbJob> {
    static P: OnceLock<Sender<ThumbJob>> = OnceLock::new();
    P.get_or_init(|| {
        let (tx, rx) = mpsc::channel::<ThumbJob>();
        let rx = Arc::new(Mutex::new(rx));
        let n = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(2).clamp(1, 4);
        for i in 0..n {
            let rx = Arc::clone(&rx);
            std::thread::Builder::new()
                .name(format!("thumb-{i}"))
                .spawn(move || {
                    #[cfg(target_os = "linux")]
                    unsafe {
                        libc::setpriority(libc::PRIO_PROCESS, 0, 10);
                    }
                    loop {
                        let job = { rx.lock().unwrap().recv() };
                        match job {
                            Ok(j) => {
                                let r = generate(&j.uri, j.kind, j.size, j.mtime_ms);
                                (j.done)(r);
                            }
                            Err(_) => return,
                        }
                    }
                })
                .expect("spawn thumb worker");
        }
        tx
    })
}

pub fn submit(job: ThumbJob) {
    let _ = pool().send(job);
}

#[cfg(test)]
mod tests {
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
}
