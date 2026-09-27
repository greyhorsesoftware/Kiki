//! Finding files: `Search` — the index for local scope, a walk of the location for a remote
//! one — and the index itself: `IndexStatus`, `IndexRebuild`, `IndexRoots`, `SetIndexRoots`.
//! A search's rows are served by `Window` (in `listing`), by the listing id it was asked under.

use super::Reply;
use crate::json::Value;
use crate::proto;
use crate::server::{parse_uri, vfs_err, Cx};
use crate::vfs::uri::Uri;
use crate::vfs::VfsError;

pub fn handle(cx: &mut Cx, name: &str, b: &Value) -> Option<Reply> {
    Some(match name {
        "Search" => search(cx, b),
        "IndexStatus" => Ok(Some(crate::index::status_json())),
        "IndexRebuild" => {
            crate::index::rebuild_async();
            Ok(Some(Value::obj().done()))
        }
        "IndexRoots" => Ok(Some(Value::obj().v("roots", Value::Arr(crate::index::roots().iter().map(|r| Value::Str(Uri::from_path(r).to_string())).collect())).done())),
        "SetIndexRoots" => match b.get("roots") {
            Some(r) => crate::config::set_settings(&Value::obj().v("index", Value::obj().v("roots", r.clone()).done()).done())
                .map(|_| {
                    crate::index::rebuild_async();
                    Some(Value::obj().done())
                })
                .map_err(|e| ("Io", e.to_string())),
            None => Err(("Protocol", "missing roots".into())),
        },
        _ => return None,
    })
}

fn search(cx: &mut Cx, b: &Value) -> Reply {
    let lid = b.u64_field("lid").ok_or(("Protocol", "missing lid".to_string()))?;
    let q = b.str_field("query").unwrap_or("").to_string();
    let mode = match b.str_field("mode") {
        Some("prefix") => crate::index::Mode::Prefix,
        Some("fuzzy") => crate::index::Mode::Fuzzy,
        _ => crate::index::Mode::Substring,
    };
    let scope = b.str_field("scope").unwrap_or("everywhere");
    let (rows, capped): (Vec<Value>, bool) = if scope == "location" {
        // Remote walk, filtered by substring: one recursive Scan where the plugin has one,
        // folder by folder where it answers Unsupported (FTPS; SFTP without a shell) — which
        // used to be the end of it, so searching those locations found nothing and said so.
        let uri = parse_uri(b, "uri")?;
        let (session, path) = crate::locations::resolve(&uri).map_err(vfs_err)?;
        let lower = q.to_ascii_lowercase();
        let out = std::cell::RefCell::new(Vec::new());
        let full = std::cell::Cell::new(false);
        // `rel` is the entry's path under the folder being searched.
        let take = |rel: &str, e: &Value| {
            let name = rel.rsplit('/').next().unwrap_or("");
            if !lower.is_empty() && !name.to_ascii_lowercase().contains(&lower) {
                return;
            }
            if out.borrow().len() >= crate::index::MAX_RESULTS {
                full.set(true);
                return;
            }
            let kind = e.str_field("kind").unwrap_or("file");
            let at = uri.join(rel);
            out.borrow_mut().push(
                Value::obj()
                    .s("name", name)
                    .s("kind", if kind == "dir" { "folder" } else { "file" })
                    .b("isDir", kind == "dir")
                    .b("isLink", kind == "link")
                    .v("meta", e.get("meta").cloned().unwrap_or(Value::Null))
                    .v("thumb", Value::Null)
                    .v("git", Value::Null)
                    .s("parent", at.parent().map(|p| p.to_string()).unwrap_or_default())
                    .s("uri", at.to_string())
                    .done(),
            );
        };
        let entries_of = |m: crate::plugin::Msg, each: &mut dyn FnMut(&Value)| {
            if let crate::plugin::Msg::Json(v) = m {
                for e in v.get("entries").and_then(Value::as_arr).unwrap_or(&[]) {
                    each(e);
                }
            }
        };
        let whole = session.plugin.request_stream(session.req("Scan").s("path", path.clone()).b("recursive", true).done(), |m| {
            entries_of(m, &mut |e| take(e.str_field("rel").or(e.str_field("name")).unwrap_or(""), e));
        });
        match whole {
            Ok(_) => {}
            Err(VfsError::Unsupported) => {
                out.borrow_mut().clear();
                let mut folders = std::collections::VecDeque::from([String::new()]);
                while let Some(prefix) = folders.pop_front() {
                    if full.get() {
                        break;
                    }
                    let at = if prefix.is_empty() { path.clone() } else { format!("{}/{}", path.trim_end_matches('/'), prefix) };
                    let mut below = Vec::new();
                    // A folder that cannot be read is skipped, as a recursive scan skips it.
                    let _ = session.plugin.request_stream(session.req("Scan").s("path", at).done(), |m| {
                        entries_of(m, &mut |e| {
                            let name = e.str_field("name").unwrap_or("");
                            if name.is_empty() {
                                return;
                            }
                            let rel = if prefix.is_empty() { name.to_string() } else { format!("{prefix}/{name}") };
                            take(&rel, e);
                            if e.str_field("kind") == Some("dir") {
                                below.push(rel);
                            }
                        });
                    });
                    folders.extend(below);
                }
            }
            Err(e) => return Err(vfs_err(e)),
        }
        (out.into_inner(), full.get())
    } else {
        crate::index::maybe_refresh();
        crate::index::with_index(|ix| {
            let (hits, capped) = crate::index::query(ix, &q, mode);
            (hits.iter().map(|h| crate::index::hit_row(ix, h)).collect(), capped)
        })
    };
    let n = rows.len() as u64;
    cx.searches.insert(lid, rows);
    let _ = cx.tx.send(proto::event("Count").u("lid", lid).u("n", n).b("done", true).done());
    let _ = cx.tx.send(proto::event("Reset").u("lid", lid).u("n", n).done());
    // `indexAge` only once there is an index: never built, its age is not a number to show.
    let built = crate::index::with_index(|ix| ix.built_at);
    let mut reply = Value::obj().u("n", n).b("capped", capped);
    if built > 0 {
        reply = reply.u("indexAge", crate::ops::unix_now().saturating_sub(built));
    }
    Ok(Some(reply.done()))
}
