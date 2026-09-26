//! A remote directory served by a plugin session, as a listing `Source`.
//!
//! The session is resolved — the location looked up, the plugin connected — the first time the
//! directory is read, which is on the listing's scan thread, not when it is opened: `Open` is
//! answered at once and the window's other pane goes on listing while a server takes its time
//! to answer (owner, 2026-09-25: "why does local file listing wait for remote to fill in? can't
//! we do it in parallel?" — requests from one window are handled in order, and the connect
//! used to sit in `Open` itself). A connect that fails is the scan's failure, said by number.

use super::local::RawEntry;
use super::{EntryType, Meta, Result, Source, VfsError};
use crate::json::Value;
use crate::locations::Session;
use crate::plugin::Msg;
use std::ffi::OsString;
use std::sync::Arc;

pub struct RemoteDir {
    pub uri: crate::vfs::uri::Uri,
    /// The session and the path on the server, once resolved; the lock is held through the
    /// connect so a second reader waits for it rather than connecting again.
    resolved: std::sync::Mutex<Option<(Arc<Session>, String)>>,
    pub cancel: Arc<std::sync::atomic::AtomicBool>,
}

impl RemoteDir {
    pub fn lazy(uri: crate::vfs::uri::Uri) -> RemoteDir {
        RemoteDir { uri, resolved: std::sync::Mutex::new(None), cancel: Arc::new(std::sync::atomic::AtomicBool::new(false)) }
    }

    /// The session, connecting the first time it is asked for.
    fn resolved(&self) -> Result<(Arc<Session>, String)> {
        let mut r = self.resolved.lock().unwrap();
        if let Some((s, p)) = r.as_ref() {
            return Ok((Arc::clone(s), p.clone()));
        }
        let got = crate::locations::resolve(&self.uri)?;
        *r = Some((Arc::clone(&got.0), got.1.clone()));
        Ok(got)
    }
}

pub fn meta_from(v: &Value) -> Meta {
    Meta {
        hidden: v.get("hidden").and_then(Value::as_bool).unwrap_or(false),
        size: v.u64_field("size").unwrap_or(0),
        mtime_ms: v.u64_field("mtime").unwrap_or(0),
        atime_ms: v.u64_field("atime").unwrap_or(0),
        mode: v.u64_field("mode").map(|m| m as u32).unwrap_or(Meta::NONE),
        // The names the backend gave, kept as interned ids (see `listing::names`).
        uid: v.str_field("owner").map(crate::listing::names::intern).unwrap_or(Meta::NONE),
        gid: v.str_field("group").map(crate::listing::names::intern).unwrap_or(Meta::NONE),
    }
}

impl Source for RemoteDir {
    fn scan(&self, sink: &mut dyn FnMut(Vec<RawEntry>)) -> Result<usize> {
        let mut total = 0usize;
        let (session, path) = self.resolved()?;
        let req = session.req("Scan").s("path", path).done();
        session.plugin.request_stream_with(req, Some(&self.cancel), |m| {
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
        let (session, path) = self.resolved()?;
        let p = format!("{}/{}", path.trim_end_matches('/'), name.to_string_lossy());
        let v = session.plugin.request(session.req("Stat").s("path", p).done())?;
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
