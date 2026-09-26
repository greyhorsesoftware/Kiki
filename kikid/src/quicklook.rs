//! What the Quick Look window asks the daemon for (docs/0.2.0/05-quicklook.md): a file's whole
//! text, a PDF's page count and its pages one by one as pictures, and a remote file brought to
//! the cache without being watched — a look writes nothing, so nothing is sent back.
//!
//! Every failure the window shows is a number (`VfsError::Said`, the 1300 block): the words are
//! the window's, in its language (`02-localization.md`, L5). A page is rendered by `pdftoppm`
//! as the thumbnails are, under `nice`, into the thumbnail cache's `pdf/` folder — keyed by the
//! file, its mtime, the page and the width, so a page asked for twice is rendered once and a
//! file saved since is rendered afresh.

use crate::json::Value;
use crate::vfs::uri::Uri;
use crate::vfs::{Result, VfsError};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

/// As much of a text file as the window shows: enough for any document a person reads, small
/// enough that a log by mistake does not take the window down with it.
pub const TEXT_CAP: usize = 4 * 1024 * 1024;

/// `ReadText { uri }`: `{ text, bytes, truncated }` — the file's text (UTF-8, what is not is
/// replaced), how many of the file's bytes it holds, and whether that is short of the file. A
/// cut document says "cut at {bytes}", so the number is the text's, not the disk's (the row
/// already has the size).
pub fn read_text(uri: &Uri) -> Result<Value> {
    if !uri.is_local() {
        return Err(VfsError::said(1301, &[("name", &uri.name())], format!("{} is on a server; fetch it first", uri.name())));
    }
    let path = uri.to_path();
    let unreadable = |e: &dyn std::fmt::Display| VfsError::said(1302, &[("name", &uri.name()), ("error", e)], format!("{} could not be read: {e}", uri.name()));
    let mut f = std::fs::File::open(&path).map_err(|e| unreadable(&e))?;
    let size = f.metadata().map(|m| m.len()).map_err(|e| unreadable(&e))?;
    let mut buf = vec![0u8; TEXT_CAP];
    let mut read = 0;
    while read < TEXT_CAP {
        let n = f.read(&mut buf[read..]).map_err(|e| unreadable(&e))?;
        if n == 0 {
            break;
        }
        read += n;
    }
    buf.truncate(read);
    // A NUL early on is what a binary looks like; a text file has none.
    if buf.iter().take(8192).any(|&b| b == 0) {
        return Err(VfsError::said(1300, &[("name", &uri.name())], format!("{} is not a text file", uri.name())));
    }
    // The size on disk says whether the cap cut anything; a file that grew while it was read
    // counts as cut too, since the text stops short of what is there now.
    let truncated = size > read as u64;
    if truncated {
        trim_split_char(&mut buf);
    }
    let text = String::from_utf8_lossy(&buf).into_owned();
    let mut o = Value::obj().s("text", text.as_str()).u("bytes", buf.len() as u64).b("truncated", truncated);
    if let Some((language, runs)) = highlight(uri.name(), &text) {
        o = o.s("language", language).v("runs", runs);
    }
    Ok(o.done())
}

// ---------------------------------------------------------------- syntax colours

/// As much as is coloured: rich text past this is slow to lay out, and a file that long is
/// being skimmed, not read.
pub const HIGHLIGHT_CAP: usize = 512 * 1024;

/// The kinds of token the window has a colour for. The daemon says which a token is; what it
/// looks like is the window's (`Theme.code`), as numbers are the daemon's and words the window's.
const KINDS: &[(&str, &str)] = &[
    ("comment", "comment"),
    ("string", "string"),
    ("constant.numeric", "number"),
    ("constant.language", "keyword"),
    ("keyword", "keyword"),
    ("storage", "keyword"),
    ("entity.name.function", "function"),
    ("support.function", "function"),
    ("entity.name.type", "type"),
    ("entity.name.class", "type"),
    ("entity.name.tag", "type"),
    ("support.type", "type"),
    ("support.class", "type"),
    ("entity.other.attribute-name", "attribute"),
    ("variable.parameter", "attribute"),
    ("punctuation", "punctuation"),
];

fn syntaxes() -> &'static syntect::parsing::SyntaxSet {
    static S: std::sync::OnceLock<syntect::parsing::SyntaxSet> = std::sync::OnceLock::new();
    S.get_or_init(syntect::parsing::SyntaxSet::load_defaults_newlines)
}

/// The kind of a token from the scopes it is in. A comment or a string anywhere in the stack
/// is the whole of it — its own marks (`//`, the quotes) and a keyword inside it are the
/// comment's or the string's colour; otherwise the innermost named scope wins.
fn kind_of(stack: &syntect::parsing::ScopeStack) -> Option<&'static str> {
    let names: Vec<String> = stack.as_slice().iter().map(|s| s.build_string()).collect();
    // `keyword` is `keyword` and `keyword.control.rust`, never `keywords`.
    let is = |name: &str, prefix: &str| name.strip_prefix(prefix).is_some_and(|rest| rest.is_empty() || rest.starts_with('.'));
    for name in &names {
        if is(name, "comment") {
            return Some("comment");
        }
        if is(name, "string") {
            return Some("string");
        }
    }
    for name in names.iter().rev() {
        for (prefix, kind) in KINDS {
            if is(name, prefix) {
                return Some(kind);
            }
        }
    }
    None
}

/// The language by the file's name, and its tokens as `[offset, length, kind]` runs — offsets
/// and lengths in UTF-16 units, since that is how the window's strings count — for a text
/// short enough to colour; nothing for a name no grammar claims or a text past the cap.
pub fn highlight(name: &str, text: &str) -> Option<(String, Value)> {
    if text.len() > HIGHLIGHT_CAP {
        return None;
    }
    let ss = syntaxes();
    let ext = name.rsplit('.').next().filter(|e| e.len() < name.len())?;
    let syntax = ss.find_syntax_by_extension(ext).or_else(|| ss.find_syntax_by_extension(&ext.to_ascii_lowercase()))?;
    if syntax.name == "Plain Text" {
        return None;
    }
    let mut state = syntect::parsing::ParseState::new(syntax);
    let mut stack = syntect::parsing::ScopeStack::new();
    let mut runs: Vec<Value> = Vec::new();
    let mut at: u64 = 0; // UTF-16 units from the start of the text
    let mut open: Option<(&'static str, u64)> = None;
    let close = |open: &mut Option<(&'static str, u64)>, upto: u64, runs: &mut Vec<Value>| {
        if let Some((kind, from)) = open.take() {
            if upto > from {
                runs.push(Value::Arr(vec![Value::Uint(from), Value::Uint(upto - from), Value::Str(kind.to_string())]));
            }
        }
    };
    for line in text.split_inclusive('\n') {
        let ops = state.parse_line(line, ss).ok()?;
        let mut byte = 0usize;
        let mut units = at;
        for (pos, op) in ops {
            units += line[byte..pos].encode_utf16().count() as u64;
            byte = pos;
            stack.apply(&op).ok()?;
            let kind = kind_of(&stack);
            match (open, kind) {
                (Some((k, _)), Some(n)) if k == n => {}
                _ => {
                    close(&mut open, units, &mut runs);
                    open = kind.map(|k| (k, units));
                }
            }
        }
        at += line.encode_utf16().count() as u64;
    }
    close(&mut open, at, &mut runs);
    if runs.is_empty() {
        return None;
    }
    Some((syntax.name.clone(), Value::Arr(runs)))
}

/// The cap falls where it falls, often inside a character: the lead bytes of one that did not
/// fit whole would show as a replacement mark that is nobody's.
fn trim_split_char(buf: &mut Vec<u8>) {
    let n = buf.len();
    for back in 1..=n.min(4) {
        let b = buf[n - back];
        if b & 0b1100_0000 == 0b1000_0000 {
            continue; // a continuation byte: its lead is further back
        }
        let need = match b {
            0xC0..=0xDF => 2,
            0xE0..=0xEF => 3,
            0xF0..=0xF7 => 4,
            _ => 1,
        };
        if need > back {
            buf.truncate(n - back);
        }
        return;
    }
}

// ---------------------------------------------------------------- pdf

/// What `pdfinfo` says of a document: its pages and the first page's size in points.
pub struct PdfInfo {
    pub pages: u64,
    pub width: f64,
    pub height: f64,
}

fn local_pdf(uri: &Uri) -> Result<PathBuf> {
    if !uri.is_local() {
        return Err(VfsError::said(1301, &[("name", &uri.name())], format!("{} is on a server; fetch it first", uri.name())));
    }
    Ok(uri.to_path())
}

fn no_poppler() -> VfsError {
    VfsError::said(1310, &[], "PDF pages need poppler (pdftoppm) installed")
}

fn not_a_pdf(uri: &Uri) -> VfsError {
    VfsError::said(1311, &[("name", &uri.name())], format!("{} could not be read as a PDF", uri.name()))
}

/// `PdfInfo { uri }`: `{ pages, width, height }`.
pub fn pdf_info(uri: &Uri) -> Result<Value> {
    let info = pdf_info_of(&local_pdf(uri)?, uri)?;
    Ok(Value::obj().u("pages", info.pages).v("width", Value::Float(info.width)).v("height", Value::Float(info.height)).done())
}

pub fn pdf_info_of(path: &Path, uri: &Uri) -> Result<PdfInfo> {
    let out = Command::new("pdfinfo").arg(path).stderr(Stdio::null()).output().map_err(|e| if e.kind() == std::io::ErrorKind::NotFound { no_poppler() } else { not_a_pdf(uri) })?;
    if !out.status.success() {
        return Err(not_a_pdf(uri));
    }
    let text = String::from_utf8_lossy(&out.stdout);
    let mut info = PdfInfo { pages: 0, width: 0.0, height: 0.0 };
    for line in text.lines() {
        let Some((key, value)) = line.split_once(':') else { continue };
        let value = value.trim();
        match key.trim() {
            "Pages" => info.pages = value.parse().unwrap_or(0),
            // `Page size:      612 x 792 pts (letter)`
            "Page size" => {
                let mut nums = value.split_whitespace().filter_map(|w| w.parse::<f64>().ok());
                info.width = nums.next().unwrap_or(0.0);
                info.height = nums.next().unwrap_or(0.0);
            }
            _ => {}
        }
    }
    if info.pages == 0 {
        return Err(not_a_pdf(uri));
    }
    Ok(info)
}

/// Where a rendered page lives: under the thumbnail cache, named for everything the picture
/// depends on. The mtime is in the name rather than in the PNG (as the thumbnails keep it) so
/// a hit costs a `stat` and nothing is decoded.
pub fn page_path(path: &Path, mtime_ms: u64, page: u64, width: u32) -> PathBuf {
    let key = crate::md5::hex(format!("{}\n{mtime_ms}\n{page}\n{width}", path.display()).as_bytes());
    crate::thumbs::cache_root().join("pdf").join(format!("{key}.png"))
}

/// `PdfPage { uri, page, width }`: `{ path, width, height }` — page `page` (from 1) as a PNG
/// `width` pixels wide, from the cache when it is there. Blocking; the server gives it a thread.
pub fn pdf_page(uri: &Uri, page: u64, width: u32) -> Result<Value> {
    let path = local_pdf(uri)?;
    let width = width.clamp(16, 8192);
    let md = std::fs::metadata(&path).map_err(|_| not_a_pdf(uri))?;
    let mtime_ms = md.modified().ok().and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok()).map(|d| d.as_millis() as u64).unwrap_or(0);
    let out = page_path(&path, mtime_ms, page, width);
    if std::fs::metadata(&out).map(|m| m.len() > 0).unwrap_or(false) {
        return answer(&out);
    }
    // The count first: a page beyond the end is a number the window can say, where pdftoppm
    // would only exit quietly with nothing.
    let info = pdf_info_of(&path, uri)?;
    if page < 1 || page > info.pages {
        return Err(VfsError::said(1312, &[("name", &uri.name()), ("pages", &info.pages), ("page", &page)], format!("{} has {} pages, not page {page}", uri.name(), info.pages)));
    }
    let png = render(&path, page, width).ok_or_else(|| not_a_pdf(uri))?;
    // Written beside and renamed over, as the thumbnails are: another request for the same
    // page never reads half a file.
    let write = || -> std::io::Result<()> {
        std::fs::create_dir_all(out.parent().expect("under the cache"))?;
        let tmp = out.with_extension(format!("{}.tmp", std::process::id()));
        std::fs::write(&tmp, &png)?;
        std::fs::rename(tmp, &out)
    };
    // A cache that cannot be written is a render that failed as far as the window is concerned;
    // the cause is for the log.
    write().map_err(|e| {
        eprintln!("quicklook: {}: {e}", out.display());
        not_a_pdf(uri)
    })?;
    answer(&out)
}

fn answer(out: &Path) -> Result<Value> {
    let (w, h) = image::image_dimensions(out).unwrap_or((0, 0));
    Ok(Value::obj().s("path", out.to_string_lossy()).u("width", w as u64).u("height", h as u64).done())
}

/// One page, `width` wide, the height by the page's own proportions; `None` when pdftoppm
/// would not — whether the tool is missing is told apart by `pdf_info_of` before this runs.
fn render(src: &Path, page: u64, width: u32) -> Option<Vec<u8>> {
    let n = page.to_string();
    let w = width.to_string();
    let out = Command::new("nice").args(["-n", "15", "pdftoppm", "-f", &n, "-l", &n, "-png", "-scale-to-x", &w, "-scale-to-y", "-1"]).arg(src).stderr(Stdio::null()).output().ok()?;
    if !out.status.success() || out.stdout.is_empty() {
        return None;
    }
    Some(out.stdout)
}

// ---------------------------------------------------------------- remote

/// `QuickLookFetch { uri }`: `{ path }` for a local file, as it is; `{ job, path }` for a remote
/// one — the copy job to follow, and where the copy will be when it is done. The folder is not
/// watched: a look is not an edit, and a save (there is none) would have nothing to send back.
pub fn fetch(uri: &Uri) -> Result<Value> {
    if uri.is_local() {
        return Ok(Value::obj().s("path", uri.to_path().to_string_lossy()).done());
    }
    let (job, locals) = crate::fetched::bring(std::slice::from_ref(uri)).map_err(|e| VfsError::said(1320, &[("name", &uri.name()), ("error", &e)], format!("{} could not be fetched: {e}", uri.name())))?;
    let path = locals.into_iter().next().expect("one copy per uri");
    Ok(Value::obj().u("job", job).s("path", path.to_string_lossy()).done())
}

/// `QuickLookDrop { path }`: the fetched copy at `path` and the folder of the moment it came in
/// — gone, once the look is over (a look leaves nothing; owner, 2026-09-25: "quicklook cleans
/// up after itself I assume?"). Only ever under the cache's `open/`: anything else is refused
/// by number, whatever the window asked.
pub fn drop(path: &Path) -> Result<Value> {
    let cache = crate::fetched::cache_dir().map_err(|e| VfsError::said(1321, &[("path", &path.display())], e))?;
    let cache = cache.canonicalize().unwrap_or(cache);
    let full = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
    let Some(folder) = full.parent() else { return Err(not_fetched(path)) };
    if folder.parent() != Some(cache.as_path()) {
        return Err(not_fetched(path));
    }
    if folder.exists() {
        std::fs::remove_dir_all(folder).map_err(|e| VfsError::said(1321, &[("path", &path.display())], format!("{} could not be dropped: {e}", path.display())))?;
    }
    Ok(Value::obj().done())
}

fn not_fetched(path: &Path) -> VfsError {
    VfsError::said(1321, &[("path", &path.display())], format!("{} is not a fetched copy", path.display()))
}
