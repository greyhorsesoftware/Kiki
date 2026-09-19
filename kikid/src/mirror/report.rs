//! A plan as text for the user to save, and as JSON for the window.

use super::*;

pub fn report(spec: &Spec, plan: &Plan) -> String {
    let mut s = String::new();
    s.push_str("kiki mirror report\n==================\n\n");
    s.push_str(&format!("Direction:         {}\n", if spec.direction == Direction::Upload { "upload (local → remote)" } else { "download (remote → local)" }));
    s.push_str(&format!("Master (source):   {}\nReplica (dest):    {}\n", spec.master, spec.replica));
    s.push_str(&format!(
        "Detector:          {}\n",
        match pick_detector(spec) {
            Detector::SizeOnly => "size only",
            Detector::Digest => "digest",
            _ => "size+mtime",
        }
    ));
    s.push_str(&format!("Clock offset:      {} ms ({})\n", plan.clock_offset_ms, if spec.clock_offset_auto { "auto, subtracted from master mtime" } else { "manual" }));
    s.push_str(&format!(
        "Delete extras:     {}\nModified within:   {}\nFilters:           {} ({} filtered)\n\n",
        spec.delete_extras,
        spec.modified_within_ms.map(|w| format!("{} h", w / 3_600_000)).unwrap_or_else(|| "all files".into()),
        if spec.apply_filters { "on" } else { "off" },
        plan.filtered_count
    ));
    let copies = plan.actions.iter().filter(|a| a.kind == ActionKind::Copy).count();
    s.push_str(&format!("Summary: {} to copy, {} to delete, {} unchanged  (replica had {} items)\n\n", copies, plan.delete_count(), plan.count(Reason::Equal), plan.replica_entry_count));
    s.push_str("action/reason | path | master | replica | bytes\n");
    s.push_str(&"-".repeat(80));
    s.push('\n');
    let fmt = |e: &Option<Entry>| match e {
        Some(e) => format!("{}b @ {}", e.size, crate::listing::iso(e.mtime_ms)),
        None => "—".into(),
    };
    for a in plan.actions.iter().filter(|a| a.kind != ActionKind::Skip) {
        s.push_str(&format!("{:?}/{:?} | {} | master: {} | replica: {} | {}b\n", a.kind, a.reason, a.rel, fmt(&a.master), fmt(&a.replica), a.bytes));
    }
    s
}

pub fn action_json(a: &Action) -> Value {
    let e = |x: &Option<Entry>| match x {
        Some(e) => Value::obj().u("size", e.size).u("mtime", e.mtime_ms).v("mode", Value::Null).v("owner", Value::Null).v("group", Value::Null).v("digest", Value::Null).done(),
        None => Value::Null,
    };
    Value::obj()
        .s("rel", a.rel.to_string())
        .s(
            "action",
            match a.kind {
                ActionKind::Copy => "copy",
                ActionKind::Mkdir => "mkdir",
                ActionKind::Delete => "delete",
                ActionKind::Rmdir => "rmdir",
                ActionKind::Skip => "skip",
            },
        )
        .s(
            "reason",
            match a.reason {
                Reason::New => "new",
                Reason::Changed => "changed",
                Reason::Extra => "extra",
                Reason::Equal => "equal",
            },
        )
        .u("bytes", a.bytes)
        .b("checked", a.checked)
        .v("master", e(&a.master))
        .v("replica", e(&a.replica))
        .s(
            "state",
            match a.state {
                State::Pending => "pending",
                State::Running => "running",
                State::Done => "done",
                State::Skipped => "skipped",
            },
        )
        .u("progress", a.progress as u64)
        .opt_s("error", a.error.as_deref())
        .done()
}
