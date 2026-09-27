//! The daemon about itself: `Ping`, `Version`, `About`, `Icon`, `ResetSettings`. `Hello` is
//! not here — it is the handshake, and stays with the connection in `server.rs`.

use super::Reply;
use crate::json::Value;
use crate::server::Cx;

pub fn handle(_cx: &mut Cx, name: &str, b: &Value) -> Option<Reply> {
    Some(match name {
        "Icon" => {
            let name = b.str_field("name").unwrap_or("");
            let theme = b.str_field("theme").unwrap_or("hicolor");
            let size = b.u64_field("size").unwrap_or(32) as u32;
            match crate::icons::lookup(theme, name, size) {
                Some(p) => Ok(Some(Value::obj().s("path", p.to_string_lossy()).done())),
                None => Ok(Some(Value::obj().done())),
            }
        }
        "About" => Ok(Some(
            Value::obj()
                .s("version", env!("CARGO_PKG_VERSION"))
                .s("build", env!("KIKI_BUILD"))
                .s("socket", crate::config::socket_path_string())
                .s("pluginDir", crate::plugin::plugin_dirs().iter().map(|p| p.to_string_lossy().into_owned()).collect::<Vec<_>>().join(":"))
                .s("configDir", crate::config::config_dir().to_string_lossy())
                .done(),
        )),
        "ResetSettings" => crate::config::reset_all().map(|_| Some(Value::obj().done())).map_err(|e| ("Io", e.to_string())),
        "Ping" => Ok(Some(Value::obj().done())),
        "Version" => Ok(Some(Value::obj().s("version", env!("CARGO_PKG_VERSION")).done())),
        _ => return None,
    })
}
