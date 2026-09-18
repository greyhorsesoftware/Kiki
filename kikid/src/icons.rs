//! Freedesktop icon lookup: a theme name and an icon name in, a file path out.
//!
//! The shell cannot ask Qt for this. Qt resolves icons against whatever theme it decided on at
//! start-up and hands back a provider URL (`image://icon/folder`) that never changes, so a theme
//! switch leaves every icon exactly as it was until the delegate happens to be rebuilt. Reading
//! the theme directories here means the answer is a plain file path that changes when the theme
//! does, and it costs one lookup per icon per theme.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

/// Where icon themes live, most specific first.
fn icon_dirs() -> Vec<PathBuf> {
    let home = crate::config::home();
    let mut v = Vec::new();
    if let Ok(d) = std::env::var("XDG_DATA_HOME") {
        v.push(PathBuf::from(d).join("icons"));
    } else {
        v.push(home.join(".local/share/icons"));
    }
    v.push(home.join(".icons"));
    for d in std::env::var("XDG_DATA_DIRS").unwrap_or_else(|_| "/usr/local/share:/usr/share".into()).split(':') {
        v.push(PathBuf::from(d).join("icons"));
    }
    v.push(PathBuf::from("/usr/share/pixmaps"));
    v
}

/// The themes `theme` falls back to, in order, ending at hicolor.
fn inherits(theme: &str) -> Vec<String> {
    let mut out = Vec::new();
    for d in icon_dirs() {
        let index = d.join(theme).join("index.theme");
        let Ok(text) = std::fs::read_to_string(&index) else { continue };
        for line in text.lines() {
            let line = line.trim();
            if let Some(rest) = line.strip_prefix("Inherits=") {
                for p in rest.split(',') {
                    let p = p.trim();
                    if !p.is_empty() && !out.iter().any(|x| x == p) {
                        out.push(p.to_string());
                    }
                }
            }
        }
        if !out.is_empty() {
            break;
        }
    }
    if !out.iter().any(|x| x == "hicolor") {
        out.push("hicolor".into());
    }
    out
}

/// How well a directory matches the size asked for. Scalable wins; after that the smallest icon
/// at least as large as the one asked for, since shrinking a picture looks better than blowing
/// one up. Only when nothing is big enough does the largest of the small ones do.
fn score(dir: &str, want: u32) -> i64 {
    if dir.contains("scalable") {
        return -1;
    }
    let n: Option<u32> = dir.split(['x', '/']).next().and_then(|s| s.parse().ok());
    match n {
        Some(px) if px >= want => (px - want) as i64,
        Some(px) => 1_000 + (want - px) as i64,
        None => 10_000,
    }
}

fn find_in_theme(root: &Path, name: &str, size: u32) -> Option<PathBuf> {
    let mut best: Option<(i64, PathBuf)> = None;
    let Ok(sizes) = std::fs::read_dir(root) else { return None };
    for size_dir in sizes.flatten() {
        let s = size_dir.file_name().to_string_lossy().into_owned();
        let Ok(cats) = std::fs::read_dir(size_dir.path()) else { continue };
        for cat in cats.flatten() {
            for ext in ["svg", "png", "xpm"] {
                let p = cat.path().join(format!("{name}.{ext}"));
                if !p.is_file() {
                    continue;
                }
                let sc = score(&s, size) + if ext == "svg" { 0 } else { 1 };
                if best.as_ref().map(|(b, _)| sc < *b).unwrap_or(true) {
                    best = Some((sc, p));
                }
            }
        }
    }
    best.map(|(_, p)| p)
}

type Cache = HashMap<(String, String, u32), Option<PathBuf>>;

fn cache() -> &'static Mutex<Cache> {
    static C: OnceLock<Mutex<Cache>> = OnceLock::new();
    C.get_or_init(|| Mutex::new(HashMap::new()))
}

/// The file for `name` in `theme` at about `size` pixels, following the theme's inheritance and
/// ending in hicolor. `None` when no theme has it.
pub fn lookup(theme: &str, name: &str, size: u32) -> Option<PathBuf> {
    let key = (theme.to_string(), name.to_string(), size);
    if let Some(hit) = cache().lock().unwrap().get(&key) {
        return hit.clone();
    }
    let mut chain = vec![theme.to_string()];
    chain.extend(inherits(theme));
    let mut found = None;
    'outer: for t in &chain {
        for d in icon_dirs() {
            let root = d.join(t);
            if let Some(p) = find_in_theme(&root, name, size) {
                found = Some(p);
                break 'outer;
            }
        }
    }
    // Loose files in /usr/share/pixmaps have no theme and no size.
    if found.is_none() {
        for ext in ["svg", "png", "xpm"] {
            let p = PathBuf::from("/usr/share/pixmaps").join(format!("{name}.{ext}"));
            if p.is_file() {
                found = Some(p);
                break;
            }
        }
    }
    cache().lock().unwrap().insert(key, found.clone());
    found
}

/// Dropped when the theme changes, so a renamed or replaced icon is looked up again.
pub fn forget() {
    cache().lock().unwrap().clear();
}

#[cfg(test)]
mod tests {
    use super::*;

    fn theme_tree() -> PathBuf {
        let root = std::env::temp_dir().join(format!("kiki-icons-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        // a theme with one icon at two sizes, inheriting from a base theme
        for (theme, dir, name) in [
            ("Test", "48x48/mimetypes", "text-x-generic"),
            ("Test", "16x16/mimetypes", "text-x-generic"),
            ("Test", "scalable/places", "folder"),
            ("Base", "32x32/mimetypes", "application-pdf"),
            ("hicolor", "32x32/apps", "lonely"),
        ] {
            let d = root.join(theme).join(dir);
            std::fs::create_dir_all(&d).unwrap();
            let ext = if dir.contains("scalable") { "svg" } else { "png" };
            std::fs::write(d.join(format!("{name}.{ext}")), b"x").unwrap();
        }
        std::fs::write(root.join("Test/index.theme"), "[Icon Theme]\nInherits=Base\n").unwrap();
        std::env::set_var("XDG_DATA_HOME", &root);
        std::env::set_var("XDG_DATA_DIRS", root.to_string_lossy().to_string());
        root
    }

    #[test]
    fn lookup_finds_icons_by_theme_size_and_inheritance() {
        let _guard = crate::ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let root = theme_tree();
        // XDG_DATA_HOME is <root>, and icon_dirs() appends "icons" to it, so point at that layout
        std::fs::create_dir_all(root.join("icons")).unwrap();
        for t in ["Test", "Base", "hicolor"] {
            let _ = std::fs::rename(root.join(t), root.join("icons").join(t));
        }
        forget();

        let big = lookup("Test", "text-x-generic", 48).unwrap();
        assert!(big.to_string_lossy().contains("48x48"), "{big:?}");
        let small = lookup("Test", "text-x-generic", 16).unwrap();
        assert!(small.to_string_lossy().contains("16x16"), "{small:?}");
        // asking for more than anything on offer takes the largest rather than the nearest
        forget();
        let huge = lookup("Test", "text-x-generic", 256).unwrap();
        assert!(huge.to_string_lossy().contains("48x48"), "{huge:?}");
        // and between two that fit, the smaller one that still covers it
        forget();
        let mid = lookup("Test", "text-x-generic", 24).unwrap();
        assert!(mid.to_string_lossy().contains("48x48"), "{mid:?}");

        // scalable beats a fixed size whatever was asked for
        let folder = lookup("Test", "folder", 48).unwrap();
        assert!(folder.to_string_lossy().ends_with("folder.svg"), "{folder:?}");

        // the inherited theme answers what this one does not have
        let pdf = lookup("Test", "application-pdf", 32).unwrap();
        assert!(pdf.to_string_lossy().contains("Base"), "{pdf:?}");

        // and hicolor is the end of every chain
        assert!(lookup("Test", "lonely", 32).is_some());
        assert!(lookup("Test", "nothing-has-this", 32).is_none());

        let _ = std::fs::remove_dir_all(&root);
        forget();
    }

    #[test]
    fn a_theme_with_no_index_still_ends_at_hicolor() {
        assert!(inherits("no-such-theme").contains(&"hicolor".to_string()));
    }
}
