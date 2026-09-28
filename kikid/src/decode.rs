//! Turning a file into pixels, in one place: images (with a JPEG's EXIF thumbnail first),
//! a frame of a video through ffmpeg, a page of a PDF through pdftoppm. The thumbnailer and
//! Quick Look both come here, so that a tool is run one way, and a failure is judged one way —
//! the file's fault, or the moment's (`Why`) — wherever it happens (2026-09-28).

use crate::kinds::Kind;
use std::fs;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;
use std::process::{Command, Stdio};

/// A picture for `kind` of file, `px` across at most.
pub fn decode(src: &Path, kind: Kind, px: u32) -> Result<image::RgbaImage, Why> {
    match kind {
        Kind::Image => image(src, px),
        Kind::Video => video_frame(src, px),
        Kind::Pdf => pdf_page(src, px),
        _ => Err(Why::BadFile("no thumbnailer for this kind of file".into())),
    }
}

/// Why a thumbnail was not made, and whose fault it is: the file's (so a marker is written and
/// it is not retried for a while) or the moment's (so it is simply tried again next time).
#[derive(Debug, PartialEq)]
pub enum Why {
    BadFile(String),
    CouldNotRun(String),
}

/// The verdict on a tool's run — ffmpeg's or pdftoppm's. Killed by a signal, or not found at
/// all (126/127 from the shell that `nice` is), is not the file's doing; an ordinary non-zero
/// exit, or a run that produced nothing, is the tool saying the file is no good.
fn verdict(status: Option<std::process::ExitStatus>, produced: usize, tool: &str) -> Result<(), Why> {
    match status {
        None => Err(Why::CouldNotRun(format!("{tool} could not be run"))),
        Some(st) => match st.code() {
            None => Err(Why::CouldNotRun(format!("{tool} was killed"))),
            Some(126) | Some(127) => Err(Why::CouldNotRun(format!("{tool} is not installed"))),
            Some(0) if produced == 0 => Err(Why::BadFile(format!("{tool} produced no image"))),
            Some(0) => Ok(()),
            Some(code) => Err(Why::BadFile(format!("{tool} exited with {code}"))),
        },
    }
}

fn image(src: &Path, px: u32) -> Result<image::RgbaImage, Why> {
    // A file that cannot be read is not a bad file; a file that reads and does not decode is.
    if let Err(e) = fs::File::open(src) {
        return Err(Why::CouldNotRun(format!("could not open: {e}")));
    }
    decode_image(src, px).ok_or_else(|| Why::BadFile("the image could not be decoded".into()))
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
    let rd16 = |b: &[u8], o: usize| -> u32 {
        if o + 2 > b.len() {
            0
        } else if le {
            u16::from_le_bytes([b[o], b[o + 1]]) as u32
        } else {
            u16::from_be_bytes([b[o], b[o + 1]]) as u32
        }
    };
    let rd32 = |b: &[u8], o: usize| -> u32 {
        if o + 4 > b.len() {
            0
        } else if le {
            u32::from_le_bytes([b[o], b[o + 1], b[o + 2], b[o + 3]])
        } else {
            u32::from_be_bytes([b[o], b[o + 1], b[o + 2], b[o + 3]])
        }
    };
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

fn video_frame(src: &Path, px: u32) -> Result<image::RgbaImage, Why> {
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
        .output();
    let out = match out {
        Ok(o) => o,
        Err(e) => return Err(Why::CouldNotRun(format!("ffmpeg: {e}"))),
    };
    verdict(Some(out.status), out.stdout.len(), "ffmpeg")?;
    image::load_from_memory(&out.stdout).map(|i| i.thumbnail(px, px).to_rgba8()).map_err(|e| Why::BadFile(format!("ffmpeg's frame did not decode: {e}")))
}

/// How pdftoppm is to size a page: to fit in a square (a thumbnail) or to a width (Quick Look).
#[derive(Clone, Copy)]
pub enum PdfScale {
    Fit(u32),
    Width(u32),
}

/// One page of a PDF as PNG bytes, the way both the thumbnailer and Quick Look want it; the
/// verdict says whether the tool or the file is to blame when there is none.
pub fn pdf_page_png(src: &Path, page: u64, scale: PdfScale) -> Result<Vec<u8>, Why> {
    let n = page.to_string();
    let mut cmd = Command::new("nice");
    cmd.args(["-n", "15", "pdftoppm", "-f", &n, "-l", &n, "-png"]);
    match scale {
        PdfScale::Fit(px) => {
            cmd.args(["-scale-to", &px.to_string()]);
        }
        PdfScale::Width(w) => {
            cmd.args(["-scale-to-x", &w.to_string(), "-scale-to-y", "-1"]);
        }
    }
    let out = match cmd.arg(src).stderr(Stdio::null()).output() {
        Ok(o) => o,
        Err(e) => return Err(Why::CouldNotRun(format!("pdftoppm: {e}"))),
    };
    verdict(Some(out.status), out.stdout.len(), "pdftoppm")?;
    Ok(out.stdout)
}

fn pdf_page(src: &Path, px: u32) -> Result<image::RgbaImage, Why> {
    let png = pdf_page_png(src, 1, PdfScale::Fit(px))?;
    image::load_from_memory(&png).map(|i| i.to_rgba8()).map_err(|e| Why::BadFile(format!("pdftoppm's page did not decode: {e}")))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Whose fault: killed or missing tools are the moment's, an ordinary failure is the file's.
    #[test]
    fn a_verdict_tells_the_files_fault_from_the_moments() {
        use std::os::unix::process::ExitStatusExt;
        let st = |c: i32| Some(std::process::ExitStatus::from_raw(c << 8));
        assert_eq!(verdict(None, 0, "ffmpeg"), Err(Why::CouldNotRun("ffmpeg could not be run".into())));
        assert_eq!(verdict(Some(std::process::ExitStatus::from_raw(9)), 0, "ffmpeg"), Err(Why::CouldNotRun("ffmpeg was killed".into())));
        assert_eq!(verdict(st(127), 0, "pdftoppm"), Err(Why::CouldNotRun("pdftoppm is not installed".into())));
        assert_eq!(verdict(st(1), 0, "ffmpeg"), Err(Why::BadFile("ffmpeg exited with 1".into())));
        assert_eq!(verdict(st(0), 0, "ffmpeg"), Err(Why::BadFile("ffmpeg produced no image".into())));
        assert_eq!(verdict(st(0), 10, "ffmpeg"), Ok(()));
    }
}
