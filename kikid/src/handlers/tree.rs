//! The column view's trees: `OpenTree`, `TreeExpand`, `TreeFilter`, `TreeReveal` — and
//! `Arrange`, which lays a set of tree windows out and is `crate::tree`'s, so it lives here
//! rather than with Quick Look. A tree's rows are served by `Window` (in `listing`), by the
//! listing id the tree was opened under.

use super::Reply;
use crate::json::Value;
use crate::proto;
use crate::server::{parse_uri, vfs_err, Cx};

pub fn handle(cx: &mut Cx, name: &str, b: &Value) -> Option<Reply> {
    Some(match name {
        "OpenTree" => match (b.u64_field("lid"), parse_uri(b, "uri")) {
            (Some(lid), Ok(u)) => match crate::tree::Tree::open(&u) {
                Ok(t) => {
                    let n = t.visible.len() as u64;
                    cx.trees.insert(lid, t);
                    let _ = cx.tx.send(proto::event("Count").u("lid", lid).u("n", n).b("done", true).done());
                    Ok(Some(Value::obj().u("n", n).done()))
                }
                Err(e) => Err(vfs_err(e)),
            },
            (None, _) => Err(("Protocol", "missing lid".into())),
            (_, Err(e)) => Err(e),
        },
        "TreeExpand" => tree_expand(cx, b),
        "TreeFilter" => tree_filter(cx, b),
        "TreeReveal" => tree_reveal(cx, b),
        // Arranging walks every window's tree; it is work for a thread of its own.
        "Arrange" => {
            let windows: Vec<Value> = b.get("windows").and_then(Value::as_arr).map(|a| a.to_vec()).unwrap_or_default();
            let width = b.u64_field("leftWidth").unwrap_or(320) as u32;
            cx.later("arrange", move || Ok(crate::tree::arrange(&windows, width)))
        }
        _ => return None,
    })
}

fn tree_expand(cx: &mut Cx, b: &Value) -> Reply {
    let lid = b.u64_field("lid").ok_or(("Protocol", "missing lid".to_string()))?;
    let t = cx.trees.get_mut(&lid).ok_or(("NotFound", "no tree".to_string()))?;
    t.expand(b.u64_field("first").unwrap_or(0) as usize, b.get("expanded").and_then(Value::as_bool).unwrap_or(true)).map_err(vfs_err)?;
    let n = t.visible.len() as u64;
    let _ = cx.tx.send(proto::event("Reset").u("lid", lid).u("n", n).done());
    Ok(Some(Value::obj().u("n", n).done()))
}

fn tree_filter(cx: &mut Cx, b: &Value) -> Reply {
    let lid = b.u64_field("lid").ok_or(("Protocol", "missing lid".to_string()))?;
    let t = cx.trees.get_mut(&lid).ok_or(("NotFound", "no tree".to_string()))?;
    t.set_filter(b.str_field("text").unwrap_or(""));
    let n = t.visible.len() as u64;
    let _ = cx.tx.send(proto::event("Reset").u("lid", lid).u("n", n).done());
    Ok(Some(Value::obj().u("n", n).done()))
}

fn tree_reveal(cx: &mut Cx, b: &Value) -> Reply {
    let lid = b.u64_field("lid").ok_or(("Protocol", "missing lid".to_string()))?;
    let u = parse_uri(b, "uri")?;
    let t = cx.trees.get_mut(&lid).ok_or(("NotFound", "no tree".to_string()))?;
    let row = t.reveal(&u).map_err(vfs_err)?;
    let n = t.visible.len() as u64;
    let _ = cx.tx.send(proto::event("Reset").u("lid", lid).u("n", n).done());
    Ok(Some(Value::obj().u("row", row as u64).done()))
}
