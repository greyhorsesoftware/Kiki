//! Saved locations and the plugins that serve them: `Locations`, `TestLocation`,
//! `AddLocation`/`UpdateLocation`, `SetLocationImage`, `RemoveLocation`, `Disconnect`;
//! `Plugins`, `PluginStatus`, `PluginPing`, `PluginBrowse`.

use super::Reply;
use crate::json::Value;
use crate::proto;
use crate::server::{vfs_err, Cx};

pub fn handle(_cx: &mut Cx, name: &str, b: &Value) -> Option<Reply> {
    Some(match name {
        "PluginStatus" => Ok(Some(Value::obj().v("plugins", crate::plugin::status_json()).done())),
        "PluginPing" => plugin_ping(b),
        "Plugins" => Ok(Some(Value::obj().v("plugins", Value::Arr(crate::plugin::describe_all())).done())),
        "PluginBrowse" => plugin_browse(b),
        "Locations" => Ok(Some(Value::obj().v("locations", crate::locations::json_list()).done())),
        "TestLocation" => match b.get("location") {
            Some(loc) => crate::locations::test(loc, b.get("secrets").unwrap_or(&Value::Null)).map(|(fp, known)| Some(Value::obj().opt_s("fingerprint", fp.as_deref()).b("knownHost", known).done())).map_err(vfs_err),
            None => Err(("Protocol", "missing location".into())),
        },
        "AddLocation" | "UpdateLocation" => match b.get("location") {
            Some(loc) if !crate::plugin::ships(loc.str_field("plugin").unwrap_or("")) => Err(("Unsupported", crate::plugin::no_such_kind(loc.str_field("plugin").unwrap_or("")))),
            // `verify` in the reply means the server offered a key nobody has accepted yet:
            // the shell shows it and asks again with `trust` set to what it displayed.
            Some(loc) => crate::locations::save(loc.clone(), b.get("secrets").unwrap_or(&Value::Null), b.str_field("trust"), b.get("check").and_then(Value::as_bool).unwrap_or(true))
                .map(|verify| match verify {
                    Some(fp) => Some(Value::obj().s("verify", fp).s("host", loc.get("config").and_then(|c| c.str_field("host")).unwrap_or("")).done()),
                    None => {
                        crate::jobs::broadcast(proto::event("LocationsChanged").done());
                        Some(Value::obj().done())
                    }
                })
                .map_err(vfs_err),
            None => Err(("Protocol", "missing location".into())),
        },
        "SetLocationImage" => match b.str_field("name") {
            Some(n) => crate::locations::set_image(n, b.str_field("image"))
                .map(|_| {
                    crate::jobs::broadcast(proto::event("LocationsChanged").done());
                    Some(Value::obj().done())
                })
                .map_err(|e| ("Io", e.to_string())),
            None => Err(("Protocol", "missing name".into())),
        },
        "RemoveLocation" => match b.str_field("name") {
            Some(n) => {
                if let Some(l) = crate::locations::find(n) {
                    if let Some(p) = l.str_field("plugin") {
                        crate::listing::invalidate_authority(p, n);
                    }
                }
                crate::locations::remove(n)
                    .map(|_| {
                        crate::jobs::broadcast(proto::event("LocationsChanged").done());
                        Some(Value::obj().done())
                    })
                    .map_err(|e| ("Io", e.to_string()))
            }
            None => Err(("Protocol", "missing name".into())),
        },
        "Disconnect" => match b.str_field("name") {
            Some(n) => {
                if let Some(l) = crate::locations::find(n) {
                    if let Some(p) = l.str_field("plugin") {
                        crate::listing::invalidate_authority(p, n);
                    }
                }
                crate::locations::disconnect(n);
                Ok(Some(Value::obj().done()))
            }
            None => Err(("Protocol", "missing name".into())),
        },
        _ => return None,
    })
}

fn plugin_browse(b: &Value) -> Reply {
    let scheme = b.str_field("plugin").unwrap_or("");
    let p = crate::plugin::get(scheme).map_err(|e| (e.code(), e.message()))?;
    let req = Value::obj()
        .s("type", "Browse")
        .s("field", b.str_field("field").unwrap_or(""))
        .v("config", b.get("config").cloned().unwrap_or(Value::Null))
        .v("secrets", b.get("secrets").cloned().unwrap_or(Value::Null))
        .done();
    p.request(req).map(Some).map_err(|e| (e.code(), e.message()))
}

fn plugin_ping(b: &Value) -> Reply {
    let n = b.str_field("name").ok_or(("Protocol", "missing name".to_string()))?;
    let bin = crate::plugin::inventory().into_iter().find(|(x, _)| x == n).map(|(_, p)| p).ok_or(("NotFound", "no such plugin".to_string()))?;
    let start = std::time::Instant::now();
    let p = crate::plugin::Plugin::spawn_path(&bin, n).map_err(vfs_err)?;
    let d = p.request(Value::obj().s("type", "Describe").done()).map_err(vfs_err)?;
    p.request(Value::obj().s("type", "Ping").done()).map_err(vfs_err)?;
    p.shutdown();
    Ok(Some(Value::obj().u("ms", start.elapsed().as_millis() as u64).v("describe", d).done()))
}
