//! Quick Look (0.2.0, plan 05) — the window's own verbs, each saying its failures by number:
//! `ReadText`, `PdfInfo`, `PdfPage`, `QuickLookFetch`, `QuickLookDrop`; and `Thumbnail`, the
//! picture a list row shows, which the thumber makes in its own time.

use super::Reply;
use crate::json::Value;
use crate::server::{parse_uri, vfs_err, Cx};

pub fn handle(cx: &mut Cx, name: &str, b: &Value) -> Option<Reply> {
    Some(match name {
        "ReadText" => match parse_uri(b, "uri") {
            Ok(u) => crate::quicklook::read_text(&u).map(Some).map_err(vfs_err),
            Err(e) => Err(e),
        },
        "PdfInfo" => match parse_uri(b, "uri") {
            Ok(u) => crate::quicklook::pdf_info(&u).map(Some).map_err(vfs_err),
            Err(e) => Err(e),
        },
        "PdfPage" => match parse_uri(b, "uri") {
            Ok(u) => {
                let page = b.u64_field("page").unwrap_or(1);
                let width = b.u64_field("width").unwrap_or(1024).min(u32::MAX as u64) as u32;
                // A render takes as long as pdftoppm takes; the window's other requests
                // (the next page, a thumbnail) should not wait behind it.
                cx.later("pdf-page", move || crate::quicklook::pdf_page(&u, page, width).map_err(vfs_err))
            }
            Err(e) => Err(e),
        },
        "QuickLookFetch" => match parse_uri(b, "uri") {
            Ok(u) => crate::quicklook::fetch(&u).map(Some).map_err(vfs_err),
            Err(e) => Err(e),
        },
        "QuickLookDrop" => match b.str_field("path") {
            Some(p) => crate::quicklook::drop(std::path::Path::new(p)).map(Some).map_err(vfs_err),
            None => Err(("Invalid", "missing path".into())),
        },
        // Not `later`: the thumber has a queue of its own, with an order and a notion of which
        // thumbnails are still wanted, and a thread parked per request would only stand in its
        // way. It calls back when it has an answer, and the reply is built the one way replies
        // are built (`Cx::answer_later`).
        "Thumbnail" => match parse_uri(b, "uri") {
            Ok(u) => {
                let size = if b.u64_field("size") == Some(256) { crate::thumbs::Size::Large } else { crate::thumbs::Size::Normal };
                let answer = cx.answer_later();
                let kind = crate::kinds::Kind::guess(crate::vfs::EntryType::File, u.name().as_bytes());
                let mtime = std::fs::metadata(u.to_path()).ok().and_then(|m| m.modified().ok()).and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok()).map(|d| d.as_millis() as u64).unwrap_or(0);
                crate::thumber::submit(crate::thumber::Job {
                    uri: u,
                    kind,
                    mtime_ms: mtime,
                    size,
                    // Someone asked for this one by name: it is wanted whatever happens.
                    wanted: None,
                    done: Box::new(move |a| {
                        answer(match a {
                            crate::thumber::Answer::Made(p) => Ok(Value::obj().s("path", p.to_string_lossy()).done()),
                            _ => Err(("Unsupported", "no thumbnail".into())),
                        });
                    }),
                });
                Ok(None)
            }
            Err(e) => Err(e),
        },
        _ => return None,
    })
}
