//! Sharing through a plugin (mail, Tailscale): `SharePlugins`, `ShareTargets`,
//! `ShareConfigure`, and `Share` itself, which is a job.

use super::Reply;
use crate::json::Value;
use crate::server::{vfs_err, Cx};

pub fn handle(cx: &mut Cx, name: &str, b: &Value) -> Option<Reply> {
    Some(match name {
        "SharePlugins" => Ok(Some(Value::obj().v("plugins", crate::share::list_json()).done())),
        "ShareTargets" => match b.str_field("plugin") {
            Some(p) => crate::share::targets(p, b.str_field("query")).map(|t| Some(Value::obj().v("targets", t).done())).map_err(vfs_err),
            None => Err(("Protocol", "missing plugin".into())),
        },
        "ShareConfigure" => match b.str_field("plugin") {
            Some(p) => crate::share::configure(p, b.get("config").unwrap_or(&Value::Null), b.get("secrets").unwrap_or(&Value::Null)).map(|_| Some(Value::obj().done())).map_err(vfs_err),
            None => Err(("Protocol", "missing plugin".into())),
        },
        "Share" => {
            for u in b.get("uris").and_then(Value::as_arr).into_iter().flatten().filter_map(Value::as_str) {
                crate::access::record(u);
            }
            let op = Value::obj()
                .s("op", "share")
                .s("plugin", b.str_field("plugin").unwrap_or(""))
                .v("uris", b.get("uris").cloned().unwrap_or(Value::Arr(vec![])))
                .opt_s("target", b.str_field("target"))
                .v("compose", b.get("compose").cloned().unwrap_or(Value::Null))
                .done();
            cx.started(crate::jobs::submit(op, Some(cx.tx.clone())))
        }
        _ => return None,
    })
}
