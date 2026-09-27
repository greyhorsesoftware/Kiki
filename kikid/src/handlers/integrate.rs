//! The desktop's integration with kiki — folder handler, file chooser, "Show in folder", the
//! Hyprland keys: `Integration` (the status), `Integrate` and `Unintegrate` (by part).

use super::Reply;
use crate::json::Value;
use crate::server::Cx;

pub fn handle(_cx: &mut Cx, name: &str, b: &Value) -> Option<Reply> {
    Some(match name {
        "Integration" => Ok(Some(crate::integrate::status_json())),
        "Integrate" => Ok(Some(crate::integrate::apply(b.get("parts")))),
        "Unintegrate" => Ok(Some(crate::integrate::remove(b.get("parts")))),
        _ => return None,
    })
}
