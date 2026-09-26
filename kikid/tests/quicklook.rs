//! Quick Look's verbs (0.2.0, plan 05): a file's whole text with its cap, a PDF's pages rendered
//! once and served from the cache after, and a remote file fetched without being watched.

mod common;

use kikid::json::Value;
use kikid::quicklook;
use kikid::vfs::uri::Uri;
use kikid::vfs::VfsError;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

fn said(e: VfsError) -> (u16, Value) {
    e.said_json().unwrap_or_else(|| panic!("a numbered error, not {e:?}"))
}

fn scratch(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("kiki-quicklook-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn code_comes_with_its_colours_and_prose_does_not() {
    let dir = common::setup("quicklook-colours");
    let rs = dir.join("main.rs");
    // A character of two UTF-16 units before the keyword: the offsets count what the window
    // counts, so the run lands on `fn` and not one short of it.
    std::fs::write(&rs, "// 𝄞 clef\nfn main() { let s = \"hi\"; }\n").unwrap();
    let v = quicklook::read_text(&Uri::from_path(&rs)).unwrap();
    assert_eq!(v.str_field("language"), Some("Rust"));
    let runs: Vec<(u64, u64, String)> = v
        .get("runs")
        .and_then(Value::as_arr)
        .unwrap()
        .iter()
        .map(|r| {
            let a = r.as_arr().unwrap();
            (a[0].as_u64().unwrap(), a[1].as_u64().unwrap(), a[2].as_str().unwrap().to_string())
        })
        .collect();
    let text: Vec<u16> = "// 𝄞 clef\nfn main() { let s = \"hi\"; }\n".encode_utf16().collect();
    let word = |from: u64, len: u64| String::from_utf16(&text[from as usize..(from + len) as usize]).unwrap();
    assert!(runs.iter().any(|(f, l, k)| k == "comment" && word(*f, *l).starts_with("// 𝄞")), "{runs:?}");
    assert!(runs.iter().any(|(f, l, k)| k == "keyword" && word(*f, *l) == "fn"), "{runs:?}");
    assert!(runs.iter().any(|(f, l, k)| k == "function" && word(*f, *l) == "main"), "{runs:?}");
    assert!(runs.iter().any(|(f, l, k)| k == "string" && word(*f, *l).contains("hi")), "{runs:?}");

    let txt = dir.join("notes.txt");
    std::fs::write(&txt, "fn is just a word here\n").unwrap();
    let v = quicklook::read_text(&Uri::from_path(&txt)).unwrap();
    assert!(v.get("runs").is_none() && v.get("language").is_none(), "prose is not coloured");

    // Past the colouring cap the text still comes, plain.
    let big = dir.join("big.js");
    std::fs::write(&big, "var x = 1;\n".repeat(quicklook::HIGHLIGHT_CAP / 10 + 10)).unwrap();
    let v = quicklook::read_text(&Uri::from_path(&big)).unwrap();
    assert!(v.get("runs").is_none(), "too long to colour");
    assert!(v.str_field("text").unwrap().starts_with("var x"));
}

#[test]
fn text_is_read_whole_binaries_refused_and_the_cap_said() {
    let dir = scratch("text");
    let note = dir.join("note.md");
    std::fs::write(&note, "# Title\n\nsome words — and an accent: café\n").unwrap();
    let v = quicklook::read_text(&Uri::from_path(&note)).unwrap();
    assert_eq!(v.str_field("text"), Some("# Title\n\nsome words — and an accent: café\n"));
    assert_eq!(v.u64_field("bytes"), Some(std::fs::metadata(&note).unwrap().len()), "whole: the text's bytes are the file's");
    assert_eq!(v.get("truncated").and_then(Value::as_bool), Some(false));

    // A NUL early on is a binary, whatever its name.
    let bin = dir.join("picture.md");
    std::fs::write(&bin, b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR").unwrap();
    let (n, params) = said(quicklook::read_text(&Uri::from_path(&bin)).unwrap_err());
    assert_eq!(n, 1300);
    assert_eq!(params.str_field("name"), Some("picture.md"));

    // A remote file is not read from here; the window fetches it first.
    let (n, params) = said(quicklook::read_text(&Uri::parse("sftp://lab/docs/notes.txt").unwrap()).unwrap_err());
    assert_eq!(n, 1301);
    assert_eq!(params.str_field("name"), Some("notes.txt"));

    // Gone: the reason travels with the number.
    let (n, params) = said(quicklook::read_text(&Uri::from_path(&dir.join("missing.txt"))).unwrap_err());
    assert_eq!(n, 1302);
    assert!(params.str_field("error").is_some_and(|e| !e.is_empty()));

    // Over the cap: cut, said so, and the size on disk given whole. The cut lands inside a
    // three-byte character, which is dropped rather than shown as a broken mark.
    let big = dir.join("big.txt");
    let line = "€".repeat(100) + "\n"; // 301 bytes a line, so the cap falls mid-character
    let lines = quicklook::TEXT_CAP / line.len() + 2;
    std::fs::write(&big, line.repeat(lines)).unwrap();
    let v = quicklook::read_text(&Uri::from_path(&big)).unwrap();
    assert_eq!(v.get("truncated").and_then(Value::as_bool), Some(true));
    let held = v.u64_field("bytes").unwrap();
    assert!(held <= kikid::quicklook::TEXT_CAP as u64 && held >= kikid::quicklook::TEXT_CAP as u64 - 4, "cut: the text's bytes are the cap, less a split character ({held})");
    let text = v.str_field("text").unwrap();
    assert!(text.len() <= quicklook::TEXT_CAP && text.len() > quicklook::TEXT_CAP - 4, "cut at the cap: {}", text.len());
    assert!(!text.contains('\u{FFFD}'), "no broken character at the cut");

    std::fs::remove_dir_all(&dir).unwrap();
}

/// A two-page PDF, letter then landscape A5, written by hand with a correct xref so poppler
/// has nothing to repair.
fn write_pdf(path: &Path) {
    let objects = [
        "<< /Type /Catalog /Pages 2 0 R >>".to_string(),
        "<< /Type /Pages /Kids [3 0 R 4 0 R] /Count 2 >>".to_string(),
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] >>".to_string(),
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 595 420] >>".to_string(),
    ];
    let mut out = String::from("%PDF-1.4\n");
    let mut offsets = Vec::new();
    for (i, o) in objects.iter().enumerate() {
        offsets.push(out.len());
        out.push_str(&format!("{} 0 obj\n{o}\nendobj\n", i + 1));
    }
    let xref = out.len();
    out.push_str(&format!("xref\n0 {}\n0000000000 65535 f \n", objects.len() + 1));
    for off in offsets {
        out.push_str(&format!("{off:010} 00000 n \n"));
    }
    out.push_str(&format!("trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n", objects.len() + 1));
    std::fs::write(path, out).unwrap();
}

fn mtime(p: &Path) -> std::time::SystemTime {
    std::fs::metadata(p).unwrap().modified().unwrap()
}

#[test]
fn pdf_pages_are_counted_rendered_once_and_refused_beyond_the_end() {
    let dir = scratch("pdf");
    let pdf = dir.join("paper.pdf");
    write_pdf(&pdf);
    let uri = Uri::from_path(&pdf);
    {
        let _guard = kikid::ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        std::env::set_var("KIKI_THUMB_DIR", dir.join("thumbs"));
    }

    let have_poppler = std::process::Command::new("pdftoppm").arg("-v").output().is_ok() && std::process::Command::new("pdfinfo").arg("-v").output().is_ok();
    if !have_poppler {
        println!("poppler (pdftoppm, pdfinfo) is not installed: the render checks are skipped; only the missing-tool number is checked");
        let (n, _) = said(quicklook::pdf_info(&uri).unwrap_err());
        assert_eq!(n, 1310);
        let (n, _) = said(quicklook::pdf_page(&uri, 1, 400).unwrap_err());
        assert_eq!(n, 1310);
        std::fs::remove_dir_all(&dir).unwrap();
        return;
    }

    let info = quicklook::pdf_info(&uri).unwrap();
    assert_eq!(info.u64_field("pages"), Some(2));
    assert!(matches!(info.get("width"), Some(Value::Float(w)) if (*w - 612.0).abs() < 0.5), "{info:?}");
    assert!(matches!(info.get("height"), Some(Value::Float(h)) if (*h - 792.0).abs() < 0.5), "{info:?}");

    // Page 2 at 400 wide: landscape, so lower than it is wide, and under the cache's pdf/.
    let page = quicklook::pdf_page(&uri, 2, 400).unwrap();
    let png = PathBuf::from(page.str_field("path").unwrap());
    assert!(png.starts_with(dir.join("thumbs/pdf")), "{}", png.display());
    assert_eq!(page.u64_field("width"), Some(400));
    let h = page.u64_field("height").unwrap();
    assert!((280..=285).contains(&h), "420/595 of 400 is 282, got {h}");
    assert_eq!(image::image_dimensions(&png).unwrap(), (400, h as u32));

    // Asked again: the same file, untouched — nothing was rendered.
    let first = mtime(&png);
    std::thread::sleep(Duration::from_millis(30));
    let again = quicklook::pdf_page(&uri, 2, 400).unwrap();
    assert_eq!(again.str_field("path"), page.str_field("path"));
    assert_eq!(mtime(&png), first, "served from the cache, not rendered again");

    // Another width or page is another picture; the file saved since is rendered afresh.
    let wider = quicklook::pdf_page(&uri, 2, 800).unwrap();
    assert_ne!(wider.str_field("path"), page.str_field("path"));
    assert_eq!(wider.u64_field("width"), Some(800));
    let one = quicklook::pdf_page(&uri, 1, 400).unwrap();
    assert_ne!(one.str_field("path"), page.str_field("path"));
    assert!(one.u64_field("height").unwrap() > 400, "letter is taller than wide");
    std::thread::sleep(Duration::from_millis(1100));
    write_pdf(&pdf);
    let after = quicklook::pdf_page(&uri, 2, 400).unwrap();
    assert_ne!(after.str_field("path"), page.str_field("path"), "a changed file has a new key");

    // A page the document does not have: a number the window can say, with both counts.
    let (n, params) = said(quicklook::pdf_page(&uri, 3, 400).unwrap_err());
    assert_eq!(n, 1312);
    assert_eq!(params.str_field("pages"), Some("2"));
    assert_eq!(params.str_field("page"), Some("3"));
    assert_eq!(params.str_field("name"), Some("paper.pdf"));
    let (n, _) = said(quicklook::pdf_page(&uri, 0, 400).unwrap_err());
    assert_eq!(n, 1312);

    // Not a PDF at all.
    let junk = dir.join("junk.pdf");
    std::fs::write(&junk, "not a pdf").unwrap();
    let (n, params) = said(quicklook::pdf_info(&Uri::from_path(&junk)).unwrap_err());
    assert_eq!(n, 1311);
    assert_eq!(params.str_field("name"), Some("junk.pdf"));

    // Remote: fetched first, as the text is.
    let (n, _) = said(quicklook::pdf_page(&Uri::parse("sftp://lab/paper.pdf").unwrap(), 1, 400).unwrap_err());
    assert_eq!(n, 1301);

    {
        let _guard = kikid::ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        std::env::remove_var("KIKI_THUMB_DIR");
    }
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn a_local_file_is_its_path_and_a_remote_one_is_fetched_unwatched_and_dropped() {
    let dir = common::setup("quicklook-fetch");
    common::save_location("lab");
    std::env::set_var("XDG_CACHE_HOME", dir.join("cache"));

    let local = dir.join("note.txt");
    std::fs::write(&local, "here").unwrap();
    let v = quicklook::fetch(&Uri::from_path(&local)).unwrap();
    assert_eq!(v.str_field("path"), Some(local.to_string_lossy().as_ref()));
    assert!(v.get("job").is_none(), "a local file needs no job");

    // Remote: a copy job into the cache, its id and the copy's place answered before it is done.
    let v = quicklook::fetch(&Uri::parse("stub://lab/docs/notes.txt").unwrap()).unwrap();
    let job = v.u64_field("job").expect("the copy job");
    let copy = PathBuf::from(v.str_field("path").unwrap());
    assert!(copy.starts_with(dir.join("cache/kiki/open")) && copy.file_name().unwrap() == "notes.txt", "{}", copy.display());
    assert!(matches!(kikid::jobs::wait(job, Duration::from_secs(20)), Some(kikid::jobs::State::Done)));
    assert_eq!(std::fs::read(&copy).unwrap(), b"hello", "the stub's bytes, whole");

    // Written to (nothing in Quick Look does, but an editor pointed at the copy might): no job
    // of the daemon's own sends it back, because the folder was never watched.
    std::fs::write(&copy, b"changed!").unwrap();
    let start = Instant::now();
    while start.elapsed() < Duration::from_secs(2) {
        let jobs = kikid::jobs::list();
        assert!(!jobs.as_arr().into_iter().flatten().any(|j| j.str_field("title").is_some_and(|t| t.starts_with("Save "))), "an unwatched copy was sent back: {}", kikid::json::to_string(&jobs));
        std::thread::sleep(Duration::from_millis(100));
    }

    // The fetch is nobody's toast: a hidden job, since the window shows its progress itself.
    // (In this test rather than one of its own: the tests of a binary run in parallel and each
    // `common::setup` moves KIKI_CONFIG_DIR for the whole process — a second test saving the
    // location "lab" had its copy job resolve it in the other test's folder, "no location".)
    let listed = kikid::jobs::list();
    let j = listed.as_arr().into_iter().flatten().find(|j| j.u64_field("id") == Some(job)).cloned().expect("the job is listed");
    assert_eq!(j.get("hidden").and_then(Value::as_bool), Some(true), "a fetch for a look is nobody's toast: {}", kikid::json::to_string(&j));

    // Dropped when the look is over: the copy and its folder go; anything that is not a fetched
    // copy stays, said by number; dropping again is nothing to do.
    let folder = copy.parent().unwrap().to_path_buf();
    quicklook::drop(&copy).unwrap();
    assert!(!folder.exists(), "the copy and its folder are gone");
    let mine = dir.join("mine.txt");
    std::fs::write(&mine, "keep").unwrap();
    match quicklook::drop(&mine).unwrap_err() {
        VfsError::Said { n, .. } => assert_eq!(n, 1321),
        other => panic!("{other:?}"),
    }
    assert!(mine.exists());
    quicklook::drop(&copy).unwrap();

    std::env::remove_var("XDG_CACHE_HOME");
    let _ = std::fs::remove_dir_all(&dir);
}
