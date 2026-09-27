//! Mirror plans served as windowed views: `MirrorPlan`, `MirrorFilter`, `MirrorCheck`,
//! `MirrorReport`, and the rules — `MirrorFilters`, `SetMirrorFilters`. A plan's rows are
//! served by `Window` (in `listing`) through `plan_window` here, by the listing id the plan was
//! opened under.

use super::Reply;
use crate::json::Value;
use crate::proto;
use crate::server::Cx;
use crate::vfs::uri::Uri;
use std::sync::Arc;

pub fn handle(cx: &mut Cx, name: &str, b: &Value) -> Option<Reply> {
    Some(match name {
        "MirrorPlan" => mirror_plan(cx, b),
        "MirrorFilter" => mirror_filter(cx, b),
        "MirrorCheck" => mirror_check(cx, b),
        // With `saveTo`, the report is written to that file (a local URI) as well as answered.
        "MirrorReport" => match b.u64_field("job").and_then(crate::mirror::stored) {
            Some(s) => {
                let text = crate::mirror::report(&s.spec, &s.plan.lock().unwrap());
                let saved: Result<(), (&'static str, String)> = match b.str_field("saveTo") {
                    Some(to) => Uri::parse(to)
                        .map_err(|e| ("Protocol", e.0.to_string()))
                        .and_then(|uri| crate::ops::local_path(&uri).map_err(|e| (e.code(), e.message())))
                        .and_then(|path| std::fs::write(&path, &text).map_err(|e| ("Io", format!("{}: {e}", path.display())))),
                    None => Ok(()),
                };
                saved.map(|_| Some(Value::obj().s("text", text).done()))
            }
            None => Err(("NotFound", "no such plan".into())),
        },
        "MirrorFilters" => Ok(Some(crate::mirror::filters_json())),
        // `defaults: true` is "Restore defaults": the file goes, and the built-in rules are
        // what the next scan reads. Otherwise every rule is checked before any of it is
        // written, so a refusal leaves the file exactly as it was. Either way the answer is
        // the rules as they now stand, so the dialog never has to ask again.
        "SetMirrorFilters" => {
            if b.get("defaults").and_then(Value::as_bool) == Some(true) {
                crate::mirror::restore_default_filters().map(|_| Some(crate::mirror::filters_json())).map_err(|e| ("Io", e.to_string()))
            } else {
                match b.get("rules").and_then(Value::as_arr) {
                    Some(rules) => crate::mirror::check_rules(rules)
                        .map_err(|m| ("Invalid", m))
                        .and_then(|checked| crate::mirror::set_filters(&checked).map_err(|e| ("Io", e.to_string())))
                        .map(|_| Some(crate::mirror::filters_json())),
                    None => Err(("Protocol", "missing rules".into())),
                }
            }
        }
        _ => return None,
    })
}

#[allow(clippy::type_complexity)]
fn plan_rows(cx: &Cx, lid: u64) -> Result<(Arc<crate::mirror::Stored>, Vec<usize>), (&'static str, String)> {
    let (job, reason) = cx.plans.get(&lid).cloned().ok_or(("NotFound", "no plan view".to_string()))?;
    let stored = crate::mirror::stored(job).ok_or(("NotFound", "no such plan".to_string()))?;
    let idx: Vec<usize> = {
        let p = stored.plan.lock().unwrap();
        p.actions
            .iter()
            .enumerate()
            .filter(|(_, a)| match reason.as_str() {
                "new" => a.reason == crate::mirror::Reason::New,
                "changed" => a.reason == crate::mirror::Reason::Changed,
                "equal" => a.reason == crate::mirror::Reason::Equal,
                "delete" => matches!(a.kind, crate::mirror::ActionKind::Delete | crate::mirror::ActionKind::Rmdir),
                _ => true,
            })
            .map(|(i, _)| i)
            .collect()
    };
    Ok((stored, idx))
}

/// A window of a plan's rows, for `Window` when the listing id is a plan's.
pub(super) fn plan_window(cx: &Cx, lid: u64, first: usize, count: usize) -> Result<Value, (&'static str, String)> {
    let (stored, idx) = plan_rows(cx, lid)?;
    let p = stored.plan.lock().unwrap();
    let rows: Vec<Value> = idx.iter().skip(first).take(count.min(512)).map(|&i| crate::mirror::action_json(&p.actions[i])).collect();
    Ok(Value::obj().u("first", first as u64).u("n", idx.len() as u64).b("done", true).v("rows", Value::Arr(rows)).done())
}

fn mirror_plan(cx: &mut Cx, b: &Value) -> Reply {
    let job = b.u64_field("job").ok_or(("Protocol", "missing job".to_string()))?;
    let lid = b.u64_field("lid").ok_or(("Protocol", "missing lid".to_string()))?;
    let stored = crate::mirror::view(job).ok_or(("NotFound", "no such plan".to_string()))?;
    if let Some((old, _)) = cx.plans.insert(lid, (job, "all".into())) {
        crate::mirror::unview(old);
    }
    let (counts, offset, n) = {
        let p = stored.plan.lock().unwrap();
        (p.counts_json(), p.clock_offset_ms, p.actions.len() as u64)
    };
    let _ = cx.tx.send(proto::event("Count").u("lid", lid).u("n", n).b("done", true).done());
    Ok(Some(Value::obj().v("counts", counts).i("clockOffsetMs", offset).u("n", n).done()))
}

fn mirror_filter(cx: &mut Cx, b: &Value) -> Reply {
    let lid = b.u64_field("lid").ok_or(("Protocol", "missing lid".to_string()))?;
    let reason = b.str_field("reason").unwrap_or("all").to_string();
    let entry = cx.plans.get_mut(&lid).ok_or(("NotFound", "no plan view".to_string()))?;
    entry.1 = reason;
    let (_, idx) = plan_rows(cx, lid)?;
    let _ = cx.tx.send(proto::event("Reset").u("lid", lid).u("n", idx.len() as u64).done());
    Ok(Some(Value::obj().u("n", idx.len() as u64).done()))
}

fn mirror_check(cx: &mut Cx, b: &Value) -> Reply {
    let lid = b.u64_field("lid").ok_or(("Protocol", "missing lid".to_string()))?;
    let first = b.u64_field("first").unwrap_or(0) as usize;
    let count = b.u64_field("count").unwrap_or(1) as usize;
    let checked = b.get("checked").and_then(Value::as_bool).unwrap_or(true);
    let (stored, idx) = plan_rows(cx, lid)?;
    let mut p = stored.plan.lock().unwrap();
    for &i in idx.iter().skip(first).take(count) {
        if p.actions[i].kind != crate::mirror::ActionKind::Skip {
            p.actions[i].checked = checked;
        }
    }
    let counts = p.counts_json();
    Ok(Some(Value::obj().v("counts", counts).done()))
}
