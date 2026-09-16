//! The name index for Everywhere search (plan 12): every name under the roots in one string
//! pool with parent pointers and case-folded keys; substring, prefix and fuzzy queries.

use crate::json::Value;
use crate::kinds::Kind;
use crate::string_pool::sort_key;
use crate::vfs::uri::Uri;
use crate::vfs::EntryType;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock, RwLock};
use std::time::{Duration, Instant};

pub const MAX_RESULTS: usize = 10_000;
pub const REFRESH_EVERY: Duration = Duration::from_secs(600);

#[derive(Default)]
pub struct Index {
    names: Vec<u8>,
    name_off: Vec<u32>,
    name_len: Vec<u16>,
    keys: Vec<u8>,
    key_off: Vec<u32>,
    key_len: Vec<u16>,
    kinds: Vec<u8>,
    parents: Vec<u32>, // parent entry index; self for roots
    is_root: Vec<bool>,
    pub roots: Vec<PathBuf>,
    pub built_at: u64,
    dir_mtime: HashMap<u32, u64>, // entry index of a directory -> mtime seen at build
}

impl Index {
    pub fn len(&self) -> usize {
        self.name_off.len()
    }
    fn push(&mut self, name: &[u8], kind: Kind, parent: u32, root: bool) -> u32 {
        let i = self.name_off.len() as u32;
        self.name_off.push(self.names.len() as u32);
        self.name_len.push(name.len().min(u16::MAX as usize) as u16);
        self.names.extend_from_slice(&name[..name.len().min(u16::MAX as usize)]);
        self.key_off.push(self.keys.len() as u32);
        let s = self.keys.len();
        sort_key(name, &mut self.keys);
        self.key_len.push((self.keys.len() - s).min(u16::MAX as usize) as u16);
        self.kinds.push(kind as u8);
        self.parents.push(parent);
        self.is_root.push(root);
        i
    }
    pub fn name(&self, i: u32) -> &[u8] {
        let o = self.name_off[i as usize] as usize;
        &self.names[o..o + self.name_len[i as usize] as usize]
    }
    fn key(&self, i: u32) -> &[u8] {
        let o = self.key_off[i as usize] as usize;
        &self.keys[o..o + self.key_len[i as usize] as usize]
    }
    pub fn kind(&self, i: u32) -> Kind {
        Kind::from_u8(self.kinds[i as usize])
    }
    pub fn path(&self, i: u32) -> PathBuf {
        let mut parts: Vec<&[u8]> = Vec::new();
        let mut cur = i;
        loop {
            parts.push(self.name(cur));
            if self.is_root[cur as usize] {
                break;
            }
            cur = self.parents[cur as usize];
        }
        let mut p = PathBuf::new();
        for part in parts.iter().rev() {
            use std::os::unix::ffi::OsStrExt;
            p.push(std::ffi::OsStr::from_bytes(part));
        }
        p
    }
    pub fn depth(&self, i: u32) -> u32 {
        let mut d = 0;
        let mut cur = i;
        while !self.is_root[cur as usize] {
            cur = self.parents[cur as usize];
            d += 1;
        }
        d
    }
    pub fn bytes(&self) -> usize {
        self.names.len() + self.keys.len() + self.name_off.len() * 12 + self.kinds.len() * 2
    }
}

// ---------------------------------------------------------------- build

pub fn default_excludes() -> Vec<String> {
    [".cache", ".git", "node_modules", "__pycache__", ".Trash", "target"].iter().map(|s| s.to_string()).collect()
}

pub fn build(roots: &[PathBuf], excludes: &[String], cancel: &AtomicBool) -> Index {
    let mut ix = Index { roots: roots.to_vec(), built_at: crate::ops::unix_now(), ..Default::default() };
    use std::os::unix::ffi::OsStrExt;
    for root in roots {
        let ri = ix.push(root.as_os_str().as_bytes(), Kind::Folder, 0, true);
        let mut stack: Vec<(u32, PathBuf)> = vec![(ri, root.clone())];
        while let Some((parent, dir)) = stack.pop() {
            if cancel.load(Ordering::Relaxed) {
                return ix;
            }
            let Ok(rd) = std::fs::read_dir(&dir) else { continue };
            if let Ok(md) = std::fs::symlink_metadata(&dir) {
                let mt = md.modified().ok().and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok()).map(|d| d.as_millis() as u64).unwrap_or(0);
                ix.dir_mtime.insert(parent, mt);
            }
            for e in rd.flatten() {
                let name = e.file_name();
                let nb = name.as_bytes();
                if excludes.iter().any(|x| x.as_bytes() == nb) || dir.join(".kiki-noindex").exists() {
                    continue;
                }
                let ft = match e.file_type() {
                    Ok(t) => t,
                    Err(_) => continue,
                };
                let t = if ft.is_dir() { EntryType::Dir } else if ft.is_symlink() { EntryType::Link } else { EntryType::File };
                let i = ix.push(nb, Kind::guess(t, nb), parent, false);
                if ft.is_dir() {
                    stack.push((i, e.path()));
                }
            }
        }
    }
    ix
}

// ---------------------------------------------------------------- query

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Substring,
    Prefix,
    Fuzzy,
}

pub struct Hit {
    pub entry: u32,
    pub score: u32, // lower is better
}

pub fn query(ix: &Index, q: &str, mode: Mode) -> (Vec<Hit>, bool) {
    let mut key = Vec::new();
    sort_key(q.as_bytes(), &mut key);
    let lower: Vec<u8> = q.bytes().map(|b| b.to_ascii_lowercase()).collect();
    let mut hits: Vec<Hit> = Vec::new();
    let n = ix.len() as u32;
    if lower.is_empty() {
        return (hits, false);
    }
    for i in 0..n {
        if ix.is_root[i as usize] {
            continue;
        }
        let name = ix.name(i);
        let rank = match mode {
            Mode::Prefix => {
                if ix.key(i).starts_with(&key) {
                    Some(1)
                } else {
                    None
                }
            }
            Mode::Substring => substring_rank(name, &lower),
            Mode::Fuzzy => substring_rank(name, &lower).or_else(|| fuzzy_rank(name, &lower)),
        };
        if let Some(r) = rank {
            hits.push(Hit { entry: i, score: r * 64 + ix.depth(i).min(63) });
            if hits.len() >= MAX_RESULTS * 4 {
                break;
            }
        }
    }
    hits.sort_by_key(|h| h.score);
    let capped = hits.len() > MAX_RESULTS;
    hits.truncate(MAX_RESULTS);
    (hits, capped)
}

fn substring_rank(name: &[u8], lower: &[u8]) -> Option<u32> {
    if name.len() < lower.len() {
        return None;
    }
    let eq = name.iter().zip(lower).all(|(a, b)| a.to_ascii_lowercase() == *b) && name.len() == lower.len();
    if eq {
        return Some(0);
    }
    if name.iter().zip(lower).all(|(a, b)| a.to_ascii_lowercase() == *b) {
        return Some(1);
    }
    if name.windows(lower.len()).any(|w| w.iter().zip(lower).all(|(a, b)| a.to_ascii_lowercase() == *b)) {
        return Some(2);
    }
    None
}

fn fuzzy_rank(name: &[u8], lower: &[u8]) -> Option<u32> {
    let mut it = name.iter().map(|b| b.to_ascii_lowercase());
    let mut gaps = 0u32;
    for &c in lower {
        let mut found = false;
        for b in it.by_ref() {
            if b == c {
                found = true;
                break;
            }
            gaps += 1;
        }
        if !found {
            return None;
        }
    }
    Some(3 + gaps.min(20))
}

// ---------------------------------------------------------------- service

struct Service {
    index: RwLock<Arc<Index>>,
    building: AtomicBool,
    last_refresh: Mutex<Instant>,
}

fn service() -> &'static Service {
    static S: OnceLock<Service> = OnceLock::new();
    S.get_or_init(|| Service { index: RwLock::new(Arc::new(Index::default())), building: AtomicBool::new(false), last_refresh: Mutex::new(Instant::now()) })
}

pub fn roots() -> Vec<PathBuf> {
    let s = crate::config::settings();
    let from_settings: Vec<PathBuf> = s.get("index").and_then(|i| i.get("roots")).and_then(Value::as_arr).map(|a| a.iter().filter_map(|v| v.as_str()).filter_map(|u| Uri::parse(u).ok()).filter(|u| u.is_local()).map(|u| u.to_path()).collect()).unwrap_or_default();
    if from_settings.is_empty() {
        vec![crate::config::home()]
    } else {
        from_settings
    }
}

pub fn excludes() -> Vec<String> {
    let s = crate::config::settings();
    s.get("index").and_then(|i| i.get("excludes")).and_then(Value::as_arr).map(|a| a.iter().filter_map(|v| v.as_str().map(str::to_string)).collect()).filter(|v: &Vec<String>| !v.is_empty()).unwrap_or_else(default_excludes)
}

pub fn current() -> Arc<Index> {
    Arc::clone(&service().index.read().unwrap())
}

/// Builds in the background unless a build is already running.
pub fn rebuild_async() {
    let s = service();
    if s.building.swap(true, Ordering::AcqRel) {
        return;
    }
    std::thread::Builder::new()
        .name("index".into())
        .spawn(|| {
            #[cfg(target_os = "linux")]
            unsafe {
                libc::setpriority(libc::PRIO_PROCESS, 0, 10);
            }
            let cancel = AtomicBool::new(false);
            let ix = build(&roots(), &excludes(), &cancel);
            *service().index.write().unwrap() = Arc::new(ix);
            *service().last_refresh.lock().unwrap() = Instant::now();
            service().building.store(false, Ordering::Release);
        })
        .expect("spawn index");
}

pub fn maybe_refresh() {
    let s = service();
    let due = s.last_refresh.lock().unwrap().elapsed() >= REFRESH_EVERY;
    if due || s.index.read().unwrap().len() == 0 {
        rebuild_async();
    }
}

pub fn status_json() -> Value {
    let ix = current();
    Value::obj()
        .u("entries", ix.len() as u64)
        .u("bytes", ix.bytes() as u64)
        .v("roots", Value::Arr(ix.roots.iter().map(|r| Value::Str(Uri::from_path(r).to_string())).collect()))
        .u("builtAt", ix.built_at)
        .b("refreshing", service().building.load(Ordering::Relaxed))
        .done()
}

pub fn hit_row(ix: &Index, h: &Hit) -> Value {
    let p = ix.path(h.entry);
    let parent = p.parent().map(Path::to_path_buf).unwrap_or_default();
    let kind = ix.kind(h.entry);
    Value::obj()
        .s("name", String::from_utf8_lossy(ix.name(h.entry)).into_owned())
        .s("kind", kind.as_str())
        .b("isDir", kind == Kind::Folder)
        .b("isLink", kind == Kind::Link)
        .v("meta", Value::Null)
        .v("thumb", Value::Null)
        .v("git", Value::Null)
        .s("parent", Uri::from_path(&parent).to_string())
        .s("uri", Uri::from_path(&p).to_string())
        .done()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_and_queries() {
        let d = std::env::temp_dir().join(format!("kiki-index-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(d.join("Projects/omarchy/src")).unwrap();
        std::fs::create_dir_all(d.join("node_modules/x")).unwrap();
        std::fs::write(d.join("omarchy-cheatsheet.pdf"), b"").unwrap();
        std::fs::write(d.join("Projects/omarchy/src/main.rs"), b"").unwrap();
        std::fs::write(d.join("Projects/notes-omarchy.md"), b"").unwrap();
        std::fs::write(d.join("node_modules/x/omarchy.js"), b"").unwrap();
        let ix = build(&[d.clone()], &default_excludes(), &AtomicBool::new(false));
        let (hits, capped) = query(&ix, "omarchy", Mode::Substring);
        assert!(!capped);
        let names: Vec<String> = hits.iter().map(|h| String::from_utf8_lossy(ix.name(h.entry)).into_owned()).collect();
        assert_eq!(names[0], "omarchy"); // exact match first
        assert_eq!(names[1], "omarchy-cheatsheet.pdf"); // prefix, shallow
        assert!(names.contains(&"notes-omarchy.md".to_string()));
        assert!(!names.contains(&"omarchy.js".to_string())); // excluded subtree
        let row = hit_row(&ix, &hits[0]);
        assert!(row.str_field("uri").unwrap().ends_with("/Projects/omarchy"));
        assert_eq!(query(&ix, "mainrs", Mode::Fuzzy).0.len(), 1);
        assert_eq!(query(&ix, "main", Mode::Prefix).0.len(), 1);
        std::fs::remove_dir_all(&d).unwrap();
    }
}
