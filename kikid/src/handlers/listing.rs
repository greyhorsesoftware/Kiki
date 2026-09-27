//! A window's listings: `Open`, `Window`, `Sort`, `Filter`, `ShowHidden`, `SeekName`, `Enrich`,
//! `Close`, `Refresh`; and the two one-shot looks at a path, `Stat` and `Preview`.
//!
//! `Window` is asked by listing id, and a listing id may belong to a mirror plan, a tree or a
//! search result as well as to a folder — the window does not care which — so `Window` looks at
//! what the id is before it answers, and `Close` forgets whichever it was.

use super::Reply;
use crate::json::Value;
use crate::listing::{self, Listing, SortRole, Subscriber};
use crate::proto;
use crate::server::{parse_uri, vfs_err, Cx};
use std::sync::Arc;

pub fn handle(cx: &mut Cx, name: &str, b: &Value) -> Option<Reply> {
    Some(match name {
        "Open" => open(cx, b),
        "Window" => {
            if let Some(lid) = b.u64_field("lid") {
                if cx.plans.contains_key(&lid) {
                    return Some(super::mirror::plan_window(cx, lid, b.u64_field("first").unwrap_or(0) as usize, b.u64_field("count").unwrap_or(60) as usize).map(Some));
                }
                if let Some(t) = cx.trees.get(&lid) {
                    let first = b.u64_field("first").unwrap_or(0) as usize;
                    let count = b.u64_field("count").unwrap_or(60).min(512) as usize;
                    let rows: Vec<Value> = (first..(first + count).min(t.visible.len())).filter_map(|r| t.row_json(r)).collect();
                    return Some(Ok(Some(Value::obj().u("first", first as u64).u("n", t.visible.len() as u64).b("done", true).v("rows", Value::Arr(rows)).done())));
                }
                if let Some(rows) = cx.searches.get(&lid) {
                    let first = b.u64_field("first").unwrap_or(0) as usize;
                    let count = b.u64_field("count").unwrap_or(60).min(512) as usize;
                    let slice: Vec<Value> = rows.iter().skip(first).take(count).cloned().collect();
                    return Some(Ok(Some(Value::obj().u("first", first as u64).u("n", rows.len() as u64).b("done", true).v("rows", Value::Arr(slice)).done())));
                }
            }
            window(cx, b)
        }
        "Sort" => sort(cx, b),
        "Filter" => filter(cx, b),
        "ShowHidden" => show_hidden(cx, b),
        "SeekName" => seek_name(cx, b),
        "Enrich" => enrich(cx, b),
        "Close" => close(cx, b),
        "Refresh" => refresh(cx, b),
        "Stat" => match parse_uri(b, "uri") {
            Ok(u) => Listing::stat_uri(&u).map(Some).map_err(vfs_err),
            Err(e) => Err(e),
        },
        "Preview" => match parse_uri(b, "uri") {
            Ok(u) => crate::preview::preview(&u).map(Some).map_err(vfs_err),
            Err(e) => Err(e),
        },
        _ => return None,
    })
}

fn open(cx: &mut Cx, b: &Value) -> Reply {
    let lid = b.u64_field("lid").ok_or(("Protocol", "missing lid".to_string()))?;
    let uri = parse_uri(b, "uri")?;
    let (l, cached) = listing::open(&uri).map_err(vfs_err)?;
    if let Some(old) = cx.listings.insert(lid, Arc::clone(&l)) {
        old.unsubscribe(cx.id, lid);
    }
    l.subscribe(Subscriber { client: cx.id, lid, tx: cx.tx.clone(), first: 0, count: 0, view_first: 0, view_count: 0 });
    let (n, done) = l.count();
    // The first Count arrives with the reply so a client can size its model immediately.
    let _ = cx.tx.send(proto::event("Count").u("lid", lid).u("n", n).b("done", done).done());
    if let Some(e) = l.error() {
        return Err(("Io", e));
    }
    Ok(Some(Value::obj().b("cached", cached).done()))
}

fn window(cx: &mut Cx, b: &Value) -> Reply {
    let (lid, l) = cx.lid(b)?;
    let first = b.u64_field("first").unwrap_or(0) as u32;
    let count = b.u64_field("count").unwrap_or(60) as u32;
    let view = match (b.u64_field("viewFirst"), b.u64_field("viewCount")) {
        (Some(f), Some(c)) => Some((f as u32, c as u32)),
        _ => None,
    };
    Ok(Some(l.window(cx.id, lid, first, count, view)))
}

/// The listing answers this one itself, from its own thread, once the sort is done: the reply
/// is `None` here and the request id goes with the waiter.
fn sort(cx: &mut Cx, b: &Value) -> Reply {
    let (_, l) = cx.lid(b)?;
    let role = SortRole::parse(b.str_field("role").unwrap_or("name")).ok_or(("Protocol", "bad role".to_string()))?;
    let asc = b.str_field("order").unwrap_or("asc") != "desc";
    l.sort(role, asc, Some((cx.tx.clone(), cx.req_id)));
    Ok(None)
}

fn filter(cx: &mut Cx, b: &Value) -> Reply {
    let (_, l) = cx.lid(b)?;
    let n = l.filter(b.str_field("text").unwrap_or(""));
    Ok(Some(Value::obj().u("n", n).done()))
}

fn seek_name(cx: &mut Cx, b: &Value) -> Reply {
    let (_, l) = cx.lid(b)?;
    let after = b.u64_field("after").map(|a| a as u32);
    Ok(Some(match l.seek(b.str_field("prefix").unwrap_or(""), after) {
        Some(i) => Value::obj().u("index", i as u64).done(),
        None => Value::obj().v("index", Value::Null).done(),
    }))
}

fn show_hidden(cx: &mut Cx, b: &Value) -> Reply {
    let (_, l) = cx.lid(b)?;
    let n = l.set_hidden(b.get("show").and_then(Value::as_bool).unwrap_or(false));
    Ok(Some(Value::obj().u("n", n).done()))
}

/// Answered by the listing when the enrichment has run, like `Sort`.
fn enrich(cx: &mut Cx, b: &Value) -> Reply {
    let (_, l) = cx.lid(b)?;
    l.enrich(Some((cx.tx.clone(), cx.req_id)));
    Ok(None)
}

fn close(cx: &mut Cx, b: &Value) -> Reply {
    let lid = b.u64_field("lid").ok_or(("Protocol", "missing lid".to_string()))?;
    if let Some(l) = cx.listings.remove(&lid) {
        l.unsubscribe(cx.id, lid);
    }
    if let Some((job, _)) = cx.plans.remove(&lid) {
        crate::mirror::unview(job);
    }
    cx.searches.remove(&lid);
    cx.trees.remove(&lid);
    Ok(Some(Value::obj().done()))
}

fn refresh(cx: &mut Cx, b: &Value) -> Reply {
    let (_, l) = cx.lid(b)?;
    l.rescan();
    Ok(Some(Value::obj().done()))
}
