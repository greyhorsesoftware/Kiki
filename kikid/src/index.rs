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
use std::sync::{Mutex, OnceLock, RwLock};
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
    removed: Vec<bool>,
    pub roots: Vec<PathBuf>,
    pub built_at: u64,
    dir_mtime: HashMap<u32, u64>, // entry index of a directory -> mtime seen at build
}

/// Where the index lives between runs.
pub fn index_path() -> PathBuf {
    let base = std::env::var("KIKI_CACHE_DIR").map(PathBuf::from).unwrap_or_else(|_| std::env::var("XDG_CACHE_HOME").map(PathBuf::from).unwrap_or_else(|_| crate::config::home().join(".cache")).join("kiki"));
    base.join("index.bin")
}

const MAGIC: &[u8; 8] = b"KIKIIDX1";

fn put_u64(out: &mut Vec<u8>, v: u64) {
    out.extend_from_slice(&v.to_le_bytes());
}
fn put_bytes(out: &mut Vec<u8>, b: &[u8]) {
    put_u64(out, b.len() as u64);
    out.extend_from_slice(b);
}
struct Cur<'a>(&'a [u8], usize);
impl Cur<'_> {
    fn u64(&mut self) -> Option<u64> {
        let b = self.0.get(self.1..self.1 + 8)?;
        self.1 += 8;
        Some(u64::from_le_bytes(b.try_into().ok()?))
    }
    fn bytes(&mut self) -> Option<&[u8]> {
        let n = self.u64()? as usize;
        let b = self.0.get(self.1..self.1 + n)?;
        self.1 += n;
        Some(b)
    }
}

fn le_u32(v: &[u32]) -> Vec<u8> {
    v.iter().flat_map(|x| x.to_le_bytes()).collect()
}
fn le_u16(v: &[u16]) -> Vec<u8> {
    v.iter().flat_map(|x| x.to_le_bytes()).collect()
}
fn from_le_u32(b: &[u8]) -> Vec<u32> {
    let (chunks, _) = b.as_chunks::<4>();
    chunks.iter().map(|c| u32::from_le_bytes(*c)).collect()
}
fn from_le_u16(b: &[u8]) -> Vec<u16> {
    let (chunks, _) = b.as_chunks::<2>();
    chunks.iter().map(|c| u16::from_le_bytes(*c)).collect()
}

impl Index {
    /// Plain little-endian dump of every vector; written atomically after a build or a walk.
    pub fn save(&self, path: &Path) -> std::io::Result<()> {
        let mut out = Vec::with_capacity(self.names.len() * 3);
        out.extend_from_slice(MAGIC);
        put_u64(&mut out, self.built_at);
        put_bytes(&mut out, &self.names);
        put_bytes(&mut out, &le_u32(&self.name_off));
        put_bytes(&mut out, &le_u16(&self.name_len));
        put_bytes(&mut out, &self.keys);
        put_bytes(&mut out, &le_u32(&self.key_off));
        put_bytes(&mut out, &le_u16(&self.key_len));
        put_bytes(&mut out, &self.kinds);
        put_bytes(&mut out, &le_u32(&self.parents));
        put_bytes(&mut out, &self.is_root.iter().map(|&b| b as u8).collect::<Vec<u8>>());
        put_bytes(&mut out, &self.removed.iter().map(|&b| b as u8).collect::<Vec<u8>>());
        put_u64(&mut out, self.roots.len() as u64);
        for r in &self.roots {
            put_bytes(&mut out, r.to_string_lossy().as_bytes());
        }
        put_u64(&mut out, self.dir_mtime.len() as u64);
        for (k, v) in &self.dir_mtime {
            put_u64(&mut out, *k as u64);
            put_u64(&mut out, *v);
        }
        if let Some(d) = path.parent() {
            std::fs::create_dir_all(d)?;
        }
        let tmp = path.with_extension("bin.tmp");
        std::fs::write(&tmp, &out)?;
        std::fs::rename(&tmp, path)
    }

    pub fn load(path: &Path) -> Option<Index> {
        let data = std::fs::read(path).ok()?;
        if data.get(..8)? != MAGIC {
            return None;
        }
        let mut c = Cur(&data, 8);
        let built_at = c.u64()?;
        let names = c.bytes()?.to_vec();
        let name_off = from_le_u32(c.bytes()?);
        let name_len = from_le_u16(c.bytes()?);
        let keys = c.bytes()?.to_vec();
        let key_off = from_le_u32(c.bytes()?);
        let key_len = from_le_u16(c.bytes()?);
        let kinds = c.bytes()?.to_vec();
        let parents = from_le_u32(c.bytes()?);
        let is_root: Vec<bool> = c.bytes()?.iter().map(|&b| b != 0).collect();
        let removed: Vec<bool> = c.bytes()?.iter().map(|&b| b != 0).collect();
        let nroots = c.u64()? as usize;
        let mut roots = Vec::with_capacity(nroots);
        for _ in 0..nroots {
            roots.push(PathBuf::from(String::from_utf8_lossy(c.bytes()?).into_owned()));
        }
        let nd = c.u64()? as usize;
        let mut dir_mtime = HashMap::with_capacity(nd);
        for _ in 0..nd {
            dir_mtime.insert(c.u64()? as u32, c.u64()?);
        }
        let n = name_off.len();
        if name_len.len() != n || key_off.len() != n || key_len.len() != n || kinds.len() != n || parents.len() != n || is_root.len() != n || removed.len() != n {
            return None;
        }
        Some(Index { names, name_off, name_len, keys, key_off, key_len, kinds, parents, is_root, removed, roots, built_at, dir_mtime })
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

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
        self.removed.push(false);
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
            for (i, p) in ix.list_dir(parent, &dir, excludes) {
                stack.push((i, p));
            }
        }
    }
    ix
}

impl Index {
    /// Appends the children of `dir` under `parent`; returns the subdirectories (entry, path).
    fn list_dir(&mut self, parent: u32, dir: &Path, excludes: &[String]) -> Vec<(u32, PathBuf)> {
        use std::os::unix::ffi::OsStrExt;
        let mut subdirs = Vec::new();
        let Ok(rd) = std::fs::read_dir(dir) else { return subdirs };
        if let Ok(md) = std::fs::symlink_metadata(dir) {
            let mt = md.modified().ok().and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok()).map(|d| d.as_millis() as u64).unwrap_or(0);
            self.dir_mtime.insert(parent, mt);
        }
        if dir.join(".kiki-noindex").exists() {
            return subdirs;
        }
        for e in rd.flatten() {
            let name = e.file_name();
            let nb = name.as_bytes();
            if excludes.iter().any(|x| x.as_bytes() == nb) {
                continue;
            }
            let ft = match e.file_type() {
                Ok(t) => t,
                Err(_) => continue,
            };
            let t = if ft.is_dir() {
                EntryType::Dir
            } else if ft.is_symlink() {
                EntryType::Link
            } else {
                EntryType::File
            };
            let i = self.push(nb, Kind::guess(t, nb), parent, false);
            if ft.is_dir() {
                subdirs.push((i, e.path()));
            }
        }
        subdirs
    }

    fn find_dir(&self, path: &Path) -> Option<u32> {
        // Walk from the root that contains the path.
        let (ri, root) = self.roots.iter().enumerate().filter(|(_, r)| path.starts_with(r)).max_by_key(|(_, r)| r.as_os_str().len())?;
        let root_entry = (0..self.len() as u32).filter(|&i| self.is_root[i as usize]).nth(ri)?;
        let rel = path.strip_prefix(root).ok()?;
        let mut cur = root_entry;
        for comp in rel.components() {
            use std::os::unix::ffi::OsStrExt;
            let want = comp.as_os_str().as_bytes();
            cur = (0..self.len() as u32).find(|&i| !self.removed[i as usize] && !self.is_root[i as usize] && self.parents[i as usize] == cur && self.name(i) == want)?;
        }
        Some(cur)
    }

    fn children(&self, parent: u32) -> Vec<u32> {
        (0..self.len() as u32).filter(|&i| !self.removed[i as usize] && !self.is_root[i as usize] && self.parents[i as usize] == parent).collect()
    }

    /// Re-lists one directory: its direct children are replaced; new subdirectories are crawled.
    fn relist(&mut self, dir_entry: u32, dir: &Path, excludes: &[String]) {
        let old: Vec<u32> = self.children(dir_entry);
        let old_names: std::collections::HashSet<Vec<u8>> = old.iter().map(|&i| self.name(i).to_vec()).collect();
        // Remove children that no longer exist; keep the rest (and their subtrees).
        let present: std::collections::HashSet<Vec<u8>> = std::fs::read_dir(dir)
            .map(|rd| {
                rd.flatten()
                    .map(|e| {
                        use std::os::unix::ffi::OsStrExt;
                        e.file_name().as_bytes().to_vec()
                    })
                    .collect()
            })
            .unwrap_or_default();
        for &i in &old {
            if !present.contains(self.name(i)) {
                self.remove_subtree(i);
            }
        }
        // Add new ones.
        let Ok(rd) = std::fs::read_dir(dir) else { return };
        let mut stack = Vec::new();
        for e in rd.flatten() {
            use std::os::unix::ffi::OsStrExt;
            let nb = e.file_name().as_bytes().to_vec();
            if old_names.contains(&nb) || excludes.iter().any(|x| x.as_bytes() == nb.as_slice()) {
                continue;
            }
            let Ok(ft) = e.file_type() else { continue };
            let t = if ft.is_dir() {
                EntryType::Dir
            } else if ft.is_symlink() {
                EntryType::Link
            } else {
                EntryType::File
            };
            let i = self.push(&nb, Kind::guess(t, &nb), dir_entry, false);
            if ft.is_dir() {
                stack.push((i, e.path()));
            }
        }
        while let Some((p, d)) = stack.pop() {
            for (i, sub) in self.list_dir(p, &d, excludes) {
                stack.push((i, sub));
            }
        }
        if let Ok(md) = std::fs::symlink_metadata(dir) {
            let mt = md.modified().ok().and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok()).map(|d| d.as_millis() as u64).unwrap_or(0);
            self.dir_mtime.insert(dir_entry, mt);
        }
    }

    fn remove_subtree(&mut self, i: u32) {
        self.removed[i as usize] = true;
        for c in self.children(i) {
            self.remove_subtree(c);
        }
        self.dir_mtime.remove(&i);
    }
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
    let n = ix.len() as u32;
    if lower.is_empty() {
        return (Vec::new(), false);
    }
    // The best MAX_RESULTS of EVERY match. This used to stop at the first 40,000 matches and rank
    // those, so in a big index a common word could leave the best hit — the shallowest, the exact
    // name — unseen because it sat late in the scan. The pass over the names is the cost either
    // way; what is kept is bounded by a heap whose top is the worst hit so far.
    let mut best: std::collections::BinaryHeap<(u32, u32)> = std::collections::BinaryHeap::with_capacity(MAX_RESULTS + 1);
    let mut matches = 0usize;
    for i in 0..n {
        if ix.is_root[i as usize] || ix.removed[i as usize] {
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
            matches += 1;
            let hit = (r * 64 + ix.depth(i).min(63), i);
            if best.len() < MAX_RESULTS {
                best.push(hit);
            } else if best.peek().is_some_and(|worst| hit < *worst) {
                best.pop();
                best.push(hit);
            }
        }
    }
    // Ascending: best score first, index order within a score, as before.
    let hits = best.into_sorted_vec().into_iter().map(|(score, entry)| Hit { entry, score }).collect();
    (hits, matches > MAX_RESULTS)
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
    index: RwLock<Index>,
    building: AtomicBool,
    last_refresh: Mutex<Instant>,
}

fn service() -> &'static Service {
    static S: OnceLock<Service> = OnceLock::new();
    S.get_or_init(|| Service { index: RwLock::new(Index::default()), building: AtomicBool::new(false), last_refresh: Mutex::new(Instant::now()) })
}

/// Runs `f` against the current index under a read lock.
pub fn with_index<R>(f: impl FnOnce(&Index) -> R) -> R {
    f(&service().index.read().unwrap())
}

/// A watched directory changed (plan 12): re-list just that directory in the index.
pub fn patch_dir(dir: &Path) {
    let mut ix = service().index.write().unwrap();
    if ix.is_empty() {
        return;
    }
    let Some(entry) = ix.find_dir(dir) else { return };
    let ex = excludes();
    ix.relist(entry, dir, &ex);
}

/// The periodic walk: stat every indexed directory and re-list those whose mtime changed.
pub fn refresh_walk() {
    let s = service();
    if s.building.swap(true, Ordering::AcqRel) {
        return;
    }
    let ex = excludes();
    let dirs: Vec<(u32, PathBuf, u64)> = {
        let ix = s.index.read().unwrap();
        ix.dir_mtime.iter().map(|(&e, &mt)| (e, ix.path(e), mt)).collect()
    };
    let mut changed = Vec::new();
    for (e, p, mt) in dirs {
        if let Ok(md) = std::fs::symlink_metadata(&p) {
            let now = md.modified().ok().and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok()).map(|d| d.as_millis() as u64).unwrap_or(0);
            if now != mt {
                changed.push((e, p));
            }
        }
    }
    if !changed.is_empty() {
        let mut ix = s.index.write().unwrap();
        for (e, p) in changed {
            if (e as usize) < ix.len() && !ix.removed[e as usize] {
                ix.relist(e, &p, &ex);
            }
        }
    }
    let _ = s.index.read().unwrap().save(&index_path());
    *s.last_refresh.lock().unwrap() = Instant::now();
    s.building.store(false, Ordering::Release);
}

/// Startup (plan 12): load the saved index when its roots still match, then bring it up to date
/// with the directory-mtime walk instead of crawling everything again; otherwise rebuild.
pub fn start() {
    let s = service();
    match Index::load(&index_path()) {
        Some(ix) if ix.roots == roots() && !ix.is_empty() => {
            *s.index.write().unwrap() = ix;
            std::thread::Builder::new().name("index-walk".into()).spawn(refresh_walk).expect("spawn walk");
        }
        _ => rebuild_async(),
    }
}

pub fn roots() -> Vec<PathBuf> {
    let s = crate::config::settings();
    let from_settings: Vec<PathBuf> = s
        .get("index")
        .and_then(|i| i.get("roots"))
        .and_then(Value::as_arr)
        .map(|a| a.iter().filter_map(|v| v.as_str()).filter_map(|u| Uri::parse(u).ok()).filter(|u| u.is_local()).map(|u| u.to_path()).collect())
        .unwrap_or_default();
    if from_settings.is_empty() {
        vec![crate::config::home()]
    } else {
        from_settings
    }
}

pub fn excludes() -> Vec<String> {
    let s = crate::config::settings();
    s.get("index")
        .and_then(|i| i.get("excludes"))
        .and_then(Value::as_arr)
        .map(|a| a.iter().filter_map(|v| v.as_str().map(str::to_string)).collect())
        .filter(|v: &Vec<String>| !v.is_empty())
        .unwrap_or_else(default_excludes)
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
            let _ = ix.save(&index_path());
            *service().index.write().unwrap() = ix;
            *service().last_refresh.lock().unwrap() = Instant::now();
            service().building.store(false, Ordering::Release);
        })
        .expect("spawn index");
}

pub fn maybe_refresh() {
    let s = service();
    if s.index.read().unwrap().is_empty() {
        rebuild_async();
        return;
    }
    let due = s.last_refresh.lock().unwrap().elapsed() >= REFRESH_EVERY;
    if due {
        std::thread::Builder::new().name("index-walk".into()).spawn(refresh_walk).expect("spawn walk");
    }
}

pub fn status_json() -> Value {
    let ix = service().index.read().unwrap();
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

    /// Two bugs in one place: the scan stopped at the first 40,000 matches and ranked only those,
    /// so the best hit could be one it never reached; and exactly MAX_RESULTS matches was reported
    /// as "there are more".
    #[test]
    fn every_match_is_ranked_and_capped_means_more_than_fit() {
        let d = std::env::temp_dir().join(format!("kiki-index-many-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(d.join("aaa/bulk")).unwrap();
        std::fs::create_dir_all(d.join("zzz")).unwrap();
        // Enough to overflow what is returned. (The old cut-off was four times that; a fixture that
        // size starved the other tests of this binary of I/O, and what is asserted here — the best
        // hit wins wherever it was walked, and `capped` means what it says — does not need it.)
        for i in 0..(MAX_RESULTS + 50) {
            std::fs::write(d.join(format!("aaa/bulk/old-report-{i}.txt")), b"").unwrap();
        }
        // The one that should come first — an exact name — in the folder walked last.
        std::fs::write(d.join("zzz/report"), b"").unwrap();
        for i in 0..MAX_RESULTS {
            std::fs::write(d.join(format!("aaa/tenk{i}")), b"").unwrap();
        }
        let ix = build(std::slice::from_ref(&d), &default_excludes(), &AtomicBool::new(false));
        let (hits, capped) = query(&ix, "report", Mode::Substring);
        assert_eq!(hits.len(), MAX_RESULTS);
        assert!(capped, "there were more matches than are returned");
        assert_eq!(String::from_utf8_lossy(ix.name(hits[0].entry)), "report", "the exact name is first however late it was walked");
        let (hits, capped) = query(&ix, "tenk", Mode::Substring);
        assert_eq!(hits.len(), MAX_RESULTS);
        assert!(!capped, "exactly as many as fit is not 'more'");
        std::fs::remove_dir_all(&d).unwrap();
    }

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
        let ix = build(std::slice::from_ref(&d), &default_excludes(), &AtomicBool::new(false));
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
        // patch: a new file in a known directory appears after relist; a removed one disappears
        let mut ix = ix;
        std::fs::write(d.join("Projects/omarchy/src/lib.rs"), b"").unwrap();
        std::fs::remove_file(d.join("Projects/omarchy/src/main.rs")).unwrap();
        let e = ix.find_dir(&d.join("Projects/omarchy/src")).unwrap();
        ix.relist(e, &d.join("Projects/omarchy/src"), &default_excludes());
        assert_eq!(query(&ix, "lib.rs", Mode::Prefix).0.len(), 1);
        assert_eq!(query(&ix, "main.rs", Mode::Prefix).0.len(), 0);
        // save / load round trip keeps every query and the walk bookkeeping
        let file = d.join("index.bin");
        ix.save(&file).unwrap();
        let back = Index::load(&file).expect("loads");
        assert_eq!(back.len(), ix.len());
        assert_eq!(back.roots, ix.roots);
        assert_eq!(query(&back, "lib.rs", Mode::Prefix).0.len(), 1);
        assert_eq!(query(&back, "main.rs", Mode::Prefix).0.len(), 0);
        assert_eq!(back.dir_mtime.len(), ix.dir_mtime.len());
        assert!(Index::load(&d.join("nope.bin")).is_none());
        std::fs::remove_dir_all(&d).unwrap();
    }
}
