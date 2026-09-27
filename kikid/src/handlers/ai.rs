//! The AI assistant beside a folder: `AiStatus` and `AiConfigure`. Opening it is `Open` with
//! `ai` (`handlers/open.rs`), since 0.3.0.

use super::Reply;
use crate::json::Value;
use crate::server::{vfs_err, Cx};

pub fn handle(_cx: &mut Cx, name: &str, b: &Value) -> Option<Reply> {
    Some(match name {
        "AiStatus" => Ok(Some(crate::ai::status())),
        "AiConfigure" => crate::ai::configure(b.str_field("provider"), b.str_field("cliCommand")).map(|_| Some(crate::ai::status())).map_err(vfs_err),
        _ => return None,
    })
}
