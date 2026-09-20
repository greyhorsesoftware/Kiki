//! A remote directory served by a plugin session, as a listing `Source`.

use super::local::RawEntry;
use super::{EntryType, Meta, Result, Source, VfsError};
use crate::json::Value;
use crate::locations::Session;
use crate::plugin::Msg;
use std::ffi::OsString;
use std::sync::Arc;

pub struct RemoteDir {
    pub session: Arc<Session>,
    pub path: String,
    pub cancel: Arc<std::sync::atomic::AtomicBool>,
}

pub fn meta_from(v: &Value) -> Meta {
    Meta {
        hidden: v.get("hidden").and_then(Value::as_bool).unwrap_or(false),
        size: v.u64_field("size").unwrap_or(0),
        mtime_ms: v.u64_field("mtime").unwrap_or(0),
        atime_ms: v.u64_field("atime").unwrap_or(0),
        mode: v.u64_field("mode").map(|m| m as u32).unwrap_or(Meta::NONE),
        uid: Meta::NONE,
        gid: Meta::NONE,
    }
}

impl Source for RemoteDir {
    fn scan(&self, sink: &mut dyn FnMut(Vec<RawEntry>)) -> Result<usize> {
        let mut total = 0usize;
        let req = self.session.req("Scan").s("path", self.path.clone()).done();
        self.session.plugin.request_stream_with(req, Some(&self.cancel), |m| {
            if let Msg::Json(v) = m {
                if let Some(entries) = v.get("entries").and_then(Value::as_arr) {
                    let chunk: Vec<RawEntry> = entries
                        .iter()
                        .map(|e| RawEntry {
                            name: OsString::from(e.str_field("name").unwrap_or("")),
                            kind: match e.str_field("kind") {
                                Some("dir") => EntryType::Dir,
                                Some("link") => EntryType::Link,
                                Some("file") => EntryType::File,
                                _ => EntryType::Other,
                            },
                            meta: e.get("meta").filter(|m| !matches!(m, Value::Null)).map(meta_from),
                        })
                        .collect();
                    total += chunk.len();
                    sink(chunk);
                }
            }
        })?;
        Ok(total)
    }

    fn stat_child(&self, name: &std::ffi::OsStr) -> Result<(Meta, EntryType)> {
        let p = format!("{}/{}", self.path.trim_end_matches('/'), name.to_string_lossy());
        let v = self.session.plugin.request(self.session.req("Stat").s("path", p).done())?;
        if v == Value::Null {
            return Err(VfsError::NotFound);
        }
        Ok((meta_from(&v), EntryType::Unknown))
    }

    fn watchable(&self) -> bool {
        false
    }

    fn cancel(&self) {
        self.cancel.store(true, std::sync::atomic::Ordering::Relaxed);
    }
}
