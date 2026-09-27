//! What git says about a folder: `Repo`, `GitStatus`, `GitRefresh`. A path on a server has no
//! repository to ask, and answers null.

use super::Reply;
use crate::json::Value;
use crate::server::{parse_uri, Cx};

pub fn handle(_cx: &mut Cx, name: &str, b: &Value) -> Option<Reply> {
    Some(match name {
        "Repo" => match parse_uri(b, "uri") {
            Ok(u) if u.is_local() => Ok(Some(crate::git::repo_json(&u.to_path()))),
            Ok(_) => Ok(Some(Value::Null)),
            Err(e) => Err(e),
        },
        "GitStatus" => match parse_uri(b, "uri") {
            Ok(u) if u.is_local() => Ok(Some(crate::git::file_json(&u.to_path()))),
            Ok(_) => Ok(Some(Value::Null)),
            Err(e) => Err(e),
        },
        "GitRefresh" => match parse_uri(b, "uri") {
            Ok(u) if u.is_local() => {
                crate::git::invalidate(&u.to_path());
                if let Some(l) = crate::listing::find(&u.to_path()) {
                    l.rescan();
                }
                Ok(Some(Value::obj().done()))
            }
            Ok(_) => Ok(Some(Value::obj().done())),
            Err(e) => Err(e),
        },
        _ => return None,
    })
}
