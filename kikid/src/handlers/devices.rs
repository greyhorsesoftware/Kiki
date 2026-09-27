//! Drives and volumes: `Volumes`, `Mount`, `Unmount`, `Eject`, `Devices`, `RenameDevice`.

use super::Reply;
use crate::json::Value;
use crate::proto;
use crate::server::Cx;
use crate::vfs::uri::Uri;

pub fn handle(_cx: &mut Cx, name: &str, b: &Value) -> Option<Reply> {
    Some(match name {
        "Volumes" => Ok(Some(Value::obj().v("items", crate::config::volumes()).done())),
        "Mount" => mount(b),
        "Unmount" => unmount(b),
        "Eject" => eject(b),
        "Devices" => Ok(Some(crate::devices::list_json())),
        "RenameDevice" => rename_device(b),
        _ => return None,
    })
}

fn mount(b: &Value) -> Reply {
    let dev = b.str_field("device").unwrap_or("").to_string();
    let mnt = crate::config::mount(&dev).map_err(|m| ("Io", m))?;
    crate::jobs::broadcast(proto::event("VolumesChanged").done());
    Ok(Some(Value::obj().s("uri", Uri::local(&mnt).map(|u| u.to_string()).unwrap_or_default()).s("mountPoint", mnt).done()))
}

fn unmount(b: &Value) -> Reply {
    crate::config::unmount(b.str_field("device").unwrap_or("")).map_err(|m| ("Io", m))?;
    crate::jobs::broadcast(proto::event("VolumesChanged").done());
    Ok(Some(Value::obj().done()))
}

fn eject(b: &Value) -> Reply {
    if let Some(uri) = b.str_field("uri") {
        crate::devices::eject(uri).map_err(|e| (e.code(), e.message()))?;
        return Ok(Some(Value::obj().done()));
    }
    crate::config::eject(b.str_field("device").unwrap_or("")).map_err(|m| ("Io", m))?;
    crate::jobs::broadcast(proto::event("VolumesChanged").done());
    Ok(Some(Value::obj().done()))
}

fn rename_device(b: &Value) -> Reply {
    let uri = Uri::parse(b.str_field("uri").unwrap_or("")).map_err(|e| ("Protocol", e.0.to_string()))?;
    crate::devices::rename(&uri.authority, b.str_field("name").unwrap_or("")).map_err(|e| ("Io", e.to_string()))?;
    crate::jobs::broadcast(proto::event("DeviceAdded").v("device", Value::Null).done());
    Ok(Some(Value::obj().done()))
}
