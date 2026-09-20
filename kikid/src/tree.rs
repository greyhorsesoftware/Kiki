//! Project mode (plan 16): a flattened, virtualised tree over listings, and Hyprland arrangement.

use crate::json::Value;
use crate::kinds::Kind;
use crate::listing;
use crate::vfs::uri::Uri;
use crate::vfs::VfsError;
use std::process::{Command, Stdio};
use std::time::Duration;

#[derive(Clone)]
pub struct Node {
    pub uri: Uri,
    pub name: String,
    pub kind: Kind,
    pub is_dir: bool,
    pub depth: u32,
    pub expanded: bool,
    pub git: Option<crate::git::Entry>,
}

pub struct Tree {
    pub root: Uri,
    pub nodes: Vec<Node>, // flattened, visible order
    pub filter: String,
    pub visible: Vec<usize>,
}

fn children_of(uri: &Uri) -> Result<Vec<Node>, VfsError> {
    let (l, _) = listing::open(uri)?;
    if !listing::wait_scan(&l, Duration::from_secs(10)) {
        return Err(VfsError::Io("listing timed out".into()));
    }
    let w = l.window(0, 0, 0, 100_000);
    let rows: Vec<Value> = w.get("rows").and_then(Value::as_arr).map(|a| a.to_vec()).unwrap_or_default();
    let dir = if uri.is_local() { Some(uri.to_path()) } else { None };
    Ok(rows
        .iter()
        .filter(|r| r.str_field("name") != Some(".git"))
        .map(|r| {
            let name = r.str_field("name").unwrap_or("").to_string();
            let is_dir = r.get("isDir").and_then(Value::as_bool).unwrap_or(false);
            let git = r.get("git").filter(|g| !matches!(g, Value::Null)).map(|g| crate::git::Entry {
                state: match g.str_field("state") {
                    Some("modified") => crate::git::State::Modified,
                    Some("added") => crate::git::State::Added,
                    Some("deleted") => crate::git::State::Deleted,
                    Some("renamed") => crate::git::State::Renamed,
                    Some("conflicted") => crate::git::State::Conflicted,
                    Some("untracked") => crate::git::State::Untracked,
                    Some("ignored") => crate::git::State::Ignored,
                    _ => crate::git::State::Clean,
                },
                staged: g.get("staged").and_then(Value::as_bool).unwrap_or(false),
            });
            let git = git.or_else(|| dir.as_ref().and_then(|d| crate::git::state_for(d, &name, is_dir)));
            Node {
                uri: uri.join(&name),
                name,
                kind: Kind::from_u8(match r.str_field("kind") {
                    Some("folder") => 0,
                    Some("image") => 3,
                    Some("video") => 4,
                    Some("audio") => 5,
                    Some("document") => 6,
                    Some("pdf") => 7,
                    Some("text") => 8,
                    Some("code") => 9,
                    Some("archive") => 10,
                    Some("link") => 2,
                    _ => 1,
                }),
                is_dir,
                depth: 0,
                expanded: false,
                git,
            }
        })
        .collect())
}

impl Tree {
    pub fn open(root: &Uri) -> Result<Tree, VfsError> {
        let mut nodes = children_of(root)?;
        for n in &mut nodes {
            n.depth = 0;
        }
        let mut t = Tree { root: root.clone(), nodes, filter: String::new(), visible: Vec::new() };
        t.refilter();
        Ok(t)
    }

    pub fn expand(&mut self, row: usize, expanded: bool) -> Result<(), VfsError> {
        let idx = *self.visible.get(row).ok_or(VfsError::NotFound)?;
        let node = self.nodes[idx].clone();
        if !node.is_dir || node.expanded == expanded {
            return Ok(());
        }
        if expanded {
            let mut kids = children_of(&node.uri)?;
            for k in &mut kids {
                k.depth = node.depth + 1;
            }
            self.nodes[idx].expanded = true;
            let n = kids.len();
            self.nodes.splice(idx + 1..idx + 1, kids);
            let _ = n;
        } else {
            let end = self.nodes[idx + 1..].iter().position(|n| n.depth <= node.depth).map(|p| idx + 1 + p).unwrap_or(self.nodes.len());
            self.nodes.drain(idx + 1..end);
            self.nodes[idx].expanded = false;
        }
        self.refilter();
        Ok(())
    }

    pub fn set_filter(&mut self, text: &str) {
        self.filter = text.to_ascii_lowercase();
        self.refilter();
    }

    fn refilter(&mut self) {
        if self.filter.is_empty() {
            self.visible = (0..self.nodes.len()).collect();
            return;
        }
        // Matching rows plus their ancestors.
        let mut keep = vec![false; self.nodes.len()];
        for i in 0..self.nodes.len() {
            if self.nodes[i].name.to_ascii_lowercase().contains(&self.filter) {
                keep[i] = true;
                let mut d = self.nodes[i].depth;
                let mut j = i;
                while d > 0 && j > 0 {
                    j -= 1;
                    if self.nodes[j].depth < d {
                        keep[j] = true;
                        d = self.nodes[j].depth;
                    }
                }
            }
        }
        self.visible = keep.iter().enumerate().filter(|(_, k)| **k).map(|(i, _)| i).collect();
    }

    /// Expands ancestors so `uri` is visible; returns its visible row.
    pub fn reveal(&mut self, uri: &Uri) -> Result<usize, VfsError> {
        let rel = uri.path.strip_prefix(&self.root.path).ok_or(VfsError::NotFound)?.trim_start_matches('/').to_string();
        let parts: Vec<&str> = rel.split('/').filter(|s| !s.is_empty()).collect();
        let mut cur_depth = 0;
        let mut search_from = 0;
        let mut found = 0;
        for (i, part) in parts.iter().enumerate() {
            let idx = self.nodes.iter().enumerate().skip(search_from).find(|(_, n)| n.depth == cur_depth && n.name == *part).map(|(i, _)| i).ok_or(VfsError::NotFound)?;
            found = idx;
            if i + 1 < parts.len() {
                if !self.nodes[idx].expanded {
                    let row = self.visible.iter().position(|&v| v == idx).ok_or(VfsError::NotFound)?;
                    self.expand(row, true)?;
                }
                cur_depth += 1;
                search_from = idx + 1;
            }
        }
        self.visible.iter().position(|&v| v == found).ok_or(VfsError::NotFound)
    }

    pub fn row_json(&self, row: usize) -> Option<Value> {
        let n = self.nodes.get(*self.visible.get(row)?)?;
        let rel = n.uri.path.strip_prefix(&self.root.path).unwrap_or("").trim_start_matches('/');
        Some(
            Value::obj()
                .s("name", n.name.clone())
                .s("kind", n.kind.as_str())
                .b("isDir", n.is_dir)
                .b("isLink", n.kind == Kind::Link)
                .v("meta", Value::Null)
                .v("thumb", Value::Null)
                .v("git", n.git.as_ref().map(crate::git::entry_json).unwrap_or(Value::Null))
                .u("depth", n.depth as u64)
                .v("expanded", if n.is_dir { Value::Bool(n.expanded) } else { Value::Null })
                .s("rel", rel)
                .s("uri", n.uri.to_string())
                .done(),
        )
    }
}

// ---------------------------------------------------------------- Hyprland arrangement (best effort)

fn hyprctl(args: &[&str]) -> Option<String> {
    let out = Command::new("hyprctl").args(args).stdin(Stdio::null()).stderr(Stdio::null()).output().ok()?;
    if out.status.success() {
        Some(String::from_utf8_lossy(&out.stdout).into_owned())
    } else {
        None
    }
}

/// Finds a window's address from `hyprctl clients -j`: by pid when one is known — that is THE
/// window — and by class otherwise. Never one in `taken`: two roles must not resolve to the same
/// window, which is what happened when the editor and the agent shared a class.
fn find_window(class: &str, pid: Option<u64>, taken: &[String]) -> Option<String> {
    let text = hyprctl(&["clients", "-j"])?;
    let v = crate::json::parse(text.as_bytes()).ok()?;
    pick_window(v.as_arr()?, class, pid, taken)
}

fn pick_window(clients: &[Value], class: &str, pid: Option<u64>, taken: &[String]) -> Option<String> {
    let free = |c: &&Value| c.str_field("address").is_some_and(|a| !taken.iter().any(|t| t == a));
    let by_pid = clients.iter().filter(free).find(|c| pid.is_some_and(|p| p > 0) && c.u64_field("pid") == pid);
    let by_class = || clients.iter().filter(free).find(|c| !class.is_empty() && c.str_field("class") == Some(class));
    by_pid.or_else(by_class).and_then(|c| c.str_field("address").map(str::to_string))
}

/// `windows`: [{ role, class, pid }] in left-to-right order; the first gets `left_width` px.
pub fn arrange(windows: &[Value], left_width: u32) -> Value {
    let mut arranged = Vec::new();
    let mut missing = Vec::new();
    if hyprctl(&["version"]).is_none() {
        return Value::obj()
            .v("arranged", Value::Arr(vec![]))
            .v("missing", Value::Arr(windows.iter().map(|w| Value::Str(w.str_field("role").unwrap_or("").into())).collect()))
            .s("reason", "hyprctl not available")
            .done();
    }
    let mut addrs: Vec<(String, String)> = Vec::new();
    for w in windows {
        let role = w.str_field("role").unwrap_or("").to_string();
        let mut found = None;
        for _ in 0..25 {
            let taken: Vec<String> = addrs.iter().map(|(_, a)| a.clone()).collect();
            found = find_window(w.str_field("class").unwrap_or(""), w.u64_field("pid"), &taken);
            if found.is_some() {
                break;
            }
            std::thread::sleep(Duration::from_millis(200));
        }
        match found {
            Some(a) => {
                addrs.push((role.clone(), a));
                arranged.push(Value::Str(role));
            }
            None => missing.push(Value::Str(role)),
        }
    }
    // Focus each in order and push it right of the previous; then fix the first one's width.
    for (i, (_, a)) in addrs.iter().enumerate() {
        let _ = hyprctl(&["dispatch", "focuswindow", &format!("address:{a}")]);
        let _ = hyprctl(&["dispatch", "settiled", &format!("address:{a}")]);
        if i > 0 {
            let _ = hyprctl(&["dispatch", "movewindow", "r"]);
        } else {
            let _ = hyprctl(&["dispatch", "movewindow", "l"]);
        }
    }
    if let Some((_, a)) = addrs.first() {
        let _ = hyprctl(&["dispatch", "focuswindow", &format!("address:{a}")]);
        let _ = hyprctl(&["dispatch", "resizeactive", "exact", &left_width.to_string(), "0"]);
    }
    if let Some((_, a)) = addrs.get(1) {
        let _ = hyprctl(&["dispatch", "focuswindow", &format!("address:{a}")]);
    }
    Value::obj().v("arranged", Value::Arr(arranged)).v("missing", Value::Arr(missing)).done()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tree_expand_filter_reveal() {
        let d = std::env::temp_dir().join(format!("kiki-tree-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(d.join("src/vfs")).unwrap();
        std::fs::create_dir_all(d.join("docs")).unwrap();
        std::fs::write(d.join("src/main.rs"), b"").unwrap();
        std::fs::write(d.join("src/vfs/local.rs"), b"").unwrap();
        std::fs::write(d.join("README.md"), b"").unwrap();
        let mut t = Tree::open(&Uri::from_path(&d)).unwrap();
        assert_eq!(t.visible.len(), 3); // docs, src, README.md (folders first)
        assert_eq!(t.row_json(1).unwrap().str_field("name"), Some("src"));
        t.expand(1, true).unwrap();
        assert_eq!(t.visible.len(), 5);
        assert_eq!(t.row_json(2).unwrap().u64_field("depth"), Some(1));
        let row = t.reveal(&Uri::from_path(&d.join("src/vfs/local.rs"))).unwrap();
        assert_eq!(t.row_json(row).unwrap().str_field("name"), Some("local.rs"));
        assert_eq!(t.row_json(row).unwrap().str_field("rel"), Some("src/vfs/local.rs"));
        t.set_filter("local");
        assert_eq!(t.visible.len(), 3); // src, vfs, local.rs
        t.set_filter("");
        t.expand(1, false).unwrap();
        assert_eq!(t.visible.len(), 3);
        std::fs::remove_dir_all(&d).unwrap();
    }

    /// Project mode puts three windows side by side, and has to know which is which.
    #[test]
    fn a_window_is_found_by_pid_first_and_never_twice() {
        let client = |addr: &str, class: &str, pid: u64| Value::obj().s("address", addr).s("class", class).u("pid", pid).done();
        let clients = vec![client("0xa", "org.quickshell", 100), client("0xb", "kiki-tool-neovim", 200), client("0xc", "kiki-tool-claude", 300), client("0xd", "kiki-tool-claude", 301)];
        // kiki's own window has Quickshell's class: only the pid finds it.
        assert_eq!(pick_window(&clients, "", Some(100), &[]), Some("0xa".into()));
        assert_eq!(pick_window(&clients, "kiki", Some(0), &[]), None, "no pid and a class nobody has: not found, rather than the first window");
        // The pid is THE window, even when another shares its class.
        assert_eq!(pick_window(&clients, "kiki-tool-claude", Some(301), &[]), Some("0xd".into()));
        // A terminal that forks (its pid is not the window's) is found by its class instead…
        assert_eq!(pick_window(&clients, "kiki-tool-neovim", Some(999), &[]), Some("0xb".into()));
        // …and a window already given to one role is not given to another.
        assert_eq!(pick_window(&clients, "kiki-tool-claude", Some(999), &["0xc".to_string()]), Some("0xd".into()));
        assert_eq!(pick_window(&clients, "kiki-tool-claude", None, &["0xc".to_string(), "0xd".to_string()]), None);
    }
}
