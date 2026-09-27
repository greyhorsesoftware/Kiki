//! The job queue: `Submit`, `Cancel`, `Jobs`, `ClearJobs`, `DismissJob`, `Undo`, `Redo`,
//! `JobEvents`, `PromptReply`; and the logs a person reads when something went wrong, `JobLog`
//! and `LocationLog`.

use super::Reply;
use crate::json::Value;
use crate::server::Cx;

pub fn handle(cx: &mut Cx, name: &str, b: &Value) -> Option<Reply> {
    Some(match name {
        "Submit" => match b.get("op") {
            Some(op) => cx.started(crate::jobs::submit(op.clone(), Some(cx.tx.clone()))),
            None => Err(("Protocol", "missing op".into())),
        },
        "Cancel" => match b.u64_field("job") {
            Some(j) if crate::jobs::cancel(j) => Ok(Some(Value::obj().done())),
            Some(_) => Err(("NotFound", "no such job".into())),
            None => Err(("Protocol", "missing job".into())),
        },
        "Jobs" => Ok(Some(Value::obj().v("jobs", crate::jobs::list()).done())),
        // Forgetting finished jobs is the daemon's to do: the list is fetched again on every
        // reconnect, which would bring back whatever a window had only hidden. Live jobs stay.
        // What the Log button opens; asked again from `next` while the window is up.
        "JobLog" => match b.u64_field("job") {
            Some(job) => crate::joblog::read_job(job, b.u64_field("from").unwrap_or(0)).map(Some).ok_or(("NotFound", "no log for that job".to_string())),
            None => Err(("Protocol", "missing job".into())),
        },
        // Everything a location's plugin has said, for a location that will not connect.
        "LocationLog" => match b.str_field("location").and_then(crate::locations::find) {
            Some(loc) => Ok(Some(crate::joblog::read_plugin(loc.str_field("plugin").unwrap_or(""), b.u64_field("from").unwrap_or(0)))),
            None => Err(("NotFound", "no such location".into())),
        },
        "ClearJobs" => Ok(Some(Value::obj().u("cleared", crate::jobs::dismiss(None)).done())),
        "DismissJob" => match b.u64_field("job") {
            Some(job) => Ok(Some(Value::obj().u("cleared", crate::jobs::dismiss(Some(job))).done())),
            None => Err(("Protocol", "missing job".into())),
        },
        "Undo" => cx.started(crate::jobs::undo(Some(cx.tx.clone()))),
        "Redo" => cx.started(crate::jobs::redo(Some(cx.tx.clone()))),
        "JobEvents" => {
            crate::jobs::subscribe(cx.tx.clone());
            Ok(Some(Value::obj().done()))
        }
        "PromptReply" => match (b.u64_field("job"), b.str_field("choice")) {
            (Some(j), Some(c)) => {
                let all = b.get("applyToAll").and_then(Value::as_bool).unwrap_or(false);
                if crate::jobs::prompt_reply(j, c, all) {
                    Ok(Some(Value::obj().done()))
                } else {
                    Err(("NotFound", "no prompt pending".into()))
                }
            }
            _ => Err(("Protocol", "missing job or choice".into())),
        },
        _ => return None,
    })
}
