//! Opening things elsewhere (docs/0.3.0/01-daemon-on-demand.md, decision 5): one verb, `Launch`,
//! and one rule table for it, in `open_request`. `Launch`, not `Open`: `Open` is a listing's —
//! a folder shown in the window (`handlers/listing.rs`) — and this is the other thing, a file
//! handed to something outside it. `with` says where — `default` (the type's
//! application: a double-click), `app:<desktop id>` (the Open with… menu), `tool:<id or role>`
//! (an `open-in.toml` tool: the editor, the agent, Alt+Enter), `terminal` or `ai`. Until 0.3.0
//! these were six verbs — `OpenDefault`, `Launch`, `OpenIn`, `OpenTerminal`, `AiOpen` — each
//! with its own copy of the rules, and the one about a server's file had to be remembered in
//! every one of them. `desktop.rs` still knows MIME types and desktop entries, `openin.rs` still
//! knows the tools; this is the one door in front of them.
//!
//! Around it: `OpenWith` is the *question* the menu asks (which applications open these?) and
//! opens nothing; the tool management verbs — `OpenInList`, `OpenInTest`, `OpenInSessions`,
//! `OpenInClose`, `SetOpenIn`; and the access log, which `Open` writes, so `AccessLog` and
//! `ClearAccessLog` are here too.

use super::Reply;
use crate::json::Value;
use crate::proto;
use crate::server::{vfs_err, Cx};
use crate::vfs::uri::Uri;

pub fn handle(cx: &mut Cx, name: &str, b: &Value) -> Option<Reply> {
    Some(match name {
        "Launch" => open(b),
        "OpenInList" => Ok(Some(Value::obj().v("tools", crate::openin::list_json()).done())),
        "OpenInTest" => {
            let key = b.str_field("tool").or(b.str_field("role")).unwrap_or("").to_string();
            let uris: Vec<Uri> = b.get("uris").and_then(Value::as_arr).map(|a| a.iter().filter_map(|v| v.as_str()).filter_map(|s| Uri::parse(s).ok()).collect()).unwrap_or_default();
            match crate::openin::find(&key) {
                Some(t) => crate::openin::prepare(&t, &uris, b.u64_field("line"), t.str_field("command").unwrap_or(""))
                    .map(|p| Some(Value::obj().s("command", p.command).s("cwd", p.cwd.to_string_lossy()).done()))
                    .map_err(|e| ("Invalid", e)),
                None => Err(("NotFound", "no such tool".into())),
            }
        }
        "OpenInSessions" => Ok(Some(Value::obj().v("sessions", crate::openin::sessions_json()).done())),
        "OpenInClose" => match b.str_field("tool") {
            Some(i) if crate::openin::close(i) => Ok(Some(Value::obj().done())),
            Some(_) => Err(("NotFound", "no session".into())),
            None => Err(("Protocol", "missing tool".into())),
        },
        "SetOpenIn" => match b.get("tools").and_then(Value::as_arr) {
            Some(t) => crate::openin::write_tools(t)
                .map(|_| {
                    let _ = cx.tx.send(proto::event("OpenInChanged").done());
                    Some(Value::obj().done())
                })
                .map_err(|e| ("Io", e.to_string())),
            None => Err(("Protocol", "missing tools".into())),
        },
        // The question the Open with… menu asks — which applications open these? — and nothing
        // is opened by it.
        "OpenWith" => open_with(b),
        "AccessLog" => {
            let uris: Vec<String> = b.get("uris").and_then(Value::as_arr).map(|a| a.iter().filter_map(Value::as_str).map(str::to_string).collect()).unwrap_or_default();
            let mut m = Value::obj();
            for u in &uris {
                if let Some(ms) = crate::access::opened(u) {
                    m = m.u(u, ms);
                }
            }
            Ok(Some(Value::obj().v("opened", m.done()).u("entries", crate::access::count() as u64).s("path", crate::access::path().to_string_lossy().into_owned()).done()))
        }
        "ClearAccessLog" => crate::access::clear().map(|_| Some(Value::obj().done())).map_err(|e| ("Io", e.to_string())),
        _ => return None,
    })
}

fn open_with(b: &Value) -> Reply {
    // `uris` for a selection, `uri` for one file (and for clients older than the list).
    let asked: Vec<&str> = match b.get("uris").and_then(Value::as_arr) {
        Some(a) => a.iter().filter_map(Value::as_str).collect(),
        None => b.str_field("uri").into_iter().collect(),
    };
    if asked.is_empty() {
        return Err(("Protocol", "missing uris".into()));
    }
    let mut paths = Vec::new();
    for u in asked {
        let uri = Uri::parse(u).map_err(|e| ("Protocol", e.0.to_string()))?;
        paths.push(crate::ops::local_path(&uri).map_err(|e| (e.code(), e.message()))?);
    }
    Ok(Some(crate::desktop::apps_json_for(&paths)))
}

/// `Launch { uris, with, line?, dir? }` off the wire.
fn open(b: &Value) -> Reply {
    let uris: Vec<String> = b.get("uris").and_then(Value::as_arr).map(|a| a.iter().filter_map(Value::as_str).map(str::to_string).collect()).unwrap_or_default();
    let with = b.str_field("with").ok_or(("Protocol", "missing with".to_string()))?;
    open_request(&uris, with, b.u64_field("line"), b.str_field("dir")).map(Some)
}

/// The rule table, applied once, in this order:
///
/// 1. The access log is written for everything asked for — before any refusal, as it always
///    was: a file the person tried to open was still opened, as far as they are concerned.
/// 2. A file on a server is not opened in an application, a tool or by its type: Quick Look is
///    the way, and the daemon says so with 1330 — the number the window already localises. The
///    window sends a double-click on such a file to Quick Look and never asks; this is for
///    everything else that might. (`terminal` and `ai` act on a *folder* and keep their own
///    numbers, 1251 and 1252, which say a folder rather than a file.)
/// 3. A folder with `default` is the window's to enter, not the daemon's to launch: the window
///    never sends one, and the daemon does not check — the contract is the window's, as it was.
/// 4. Then `with` says where, and the reply is whatever that path has always answered:
///    `{}` for `default`, `app:` and `terminal`; `{ pid, reused, class }` for a tool; `{ tool }`
///    for the AI.
///
/// `dir`, for `terminal` and `ai`, is where they start; without it, the first uri is the place.
pub fn open_request(uris: &[String], with: &str, line: Option<u64>, dir: Option<&str>) -> Result<Value, (&'static str, String)> {
    let parsed: Vec<Uri> = uris.iter().filter_map(|u| Uri::parse(u).ok()).collect();
    // A path under a mount is still the machine's own (`local_path` unwraps it), so a file that
    // arrived as a mount uri is launched as its path, as `Launch` did.
    let parsed: Vec<Uri> = parsed.into_iter().map(|x| crate::ops::local_path(&x).ok().map(|p| Uri::from_path(&p)).unwrap_or(x)).collect();
    let folder = || -> Result<Uri, (&'static str, String)> {
        match dir {
            Some(d) => Uri::parse(d).map_err(|e| ("Protocol", e.0.to_string())),
            None => parsed.first().cloned().ok_or(("Protocol", "missing dir".to_string())),
        }
    };
    let opens_files = matches!(with, "default") || with.starts_with("app:") || with.starts_with("tool:");
    if opens_files {
        if parsed.is_empty() {
            return Err(("Protocol", "missing uris".into()));
        }
        for u in &parsed {
            crate::access::record(&u.to_string());
        }
        // A file on a server: Quick Look is the way, said by number.
        if let Some(r) = parsed.iter().find(|u| !u.is_local()) {
            return Err(vfs_err(crate::desktop::on_a_server(r)));
        }
    }
    match with {
        // A double-click on a file (0.2.0): the default application; 1331 when it will not start.
        "default" => {
            for u in &parsed {
                crate::desktop::open_default(u).map_err(vfs_err)?;
            }
            Ok(Value::obj().done())
        }
        // The Open with… menu: an application the person chose. A launch that fails is logged
        // and not an error — the menu offered what the desktop said opens this, and the desktop
        // is where a broken entry is fixed.
        _ if with.starts_with("app:") => {
            let app = &with["app:".len()..];
            let paths: Vec<String> = parsed.iter().map(|u| u.to_string()).collect();
            if let Err(e) = crate::desktop::launch(app, &paths) {
                eprintln!("launch {app}: {e}");
            }
            Ok(Value::obj().done())
        }
        // A tool from open-in.toml, by id or by role (`editor`, `agent`): its window class comes
        // back so the window can arrange it, and `reused` says an already-running one took the
        // files rather than a second one starting.
        _ if with.starts_with("tool:") => {
            let key = &with["tool:".len()..];
            let class = crate::openin::find(key).and_then(|t| t.str_field("id").map(crate::openin::window_class)).unwrap_or_default();
            crate::openin::open(key, &parsed, line).map(|(pid, reused)| Value::obj().u("pid", pid as u64).b("reused", reused).s("class", class).done()).map_err(|e| ("Invalid", e))
        }
        "terminal" => crate::ai::open_terminal(&folder()?).map(|_| Value::obj().done()).map_err(vfs_err),
        "ai" => {
            let d = folder()?;
            crate::ai::open_external(&d, &parsed).map(|tool| Value::obj().s("tool", tool).done()).map_err(vfs_err)
        }
        other => Err(("Protocol", format!("unknown with {other}"))),
    }
}
