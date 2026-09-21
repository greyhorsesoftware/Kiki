//! Decorations: what is known about a row that the scan did not say — its thumbnail, its git
//! state — and the one rule for keeping them.
//!
//! These used to be two maps keyed by pool index, filled by two hand-wired workers that each had
//! their own queue flag and their own line in `rescan`. A pool index means something only inside
//! one scan, so every one of them was the same bug: a worker that finished after the folder was
//! read again wrote its answer onto whatever row had taken its index — a picture wearing another
//! file's thumbnail, a badge on the wrong row, and once an index past the end of a folder that
//! had shrunk, which panicked with the listing's lock held.
//!
//! So a decoration is keyed by **name**, which is what the user sees and what survives a rescan.
//! The map is carried across `rescan` untouched; nothing has to remember to copy it. What it costs
//! is a hash of the name where there used to be an array index, paid once per row of a window.

use crate::git;
use std::collections::HashMap;

/// What is known about one name. Absent fields are "not asked yet" — a thumbnail that was tried
/// and could not be made is `Thumb::None`, which is an answer and stops it being asked again.
#[derive(Clone, Default)]
pub struct Deco {
    pub thumb: Option<Thumb>,
    pub git: Option<git::Entry>,
    /// Set only on a row that is itself a repository (plan 15). Kept apart from `git` because a
    /// repository inside a repository — a submodule — has both, and the one to show is the
    /// inner one's own: the folder above can only say that something under it changed.
    pub repo: Option<git::RepoMark>,
}

#[derive(Clone)]
pub enum Thumb {
    /// The cache file, and the modification time it was made for.
    At { path: String, mtime_ms: u64 },
    /// Tried, and there is none to be had (not a picture after all, a broken file, a decoder that
    /// died on it). Kept so it is not tried again until the file itself changes.
    None { mtime_ms: u64 },
    /// Asked for, and the answer has not come back.
    Asked,
}

impl Thumb {
    /// The time this answer was about; `Asked` is about whatever is there now.
    fn mtime(&self) -> Option<u64> {
        match self {
            Thumb::At { mtime_ms, .. } | Thumb::None { mtime_ms } => Some(*mtime_ms),
            Thumb::Asked => None,
        }
    }
}

/// Every row's decorations, by name.
#[derive(Default)]
pub struct Decorations {
    by_name: HashMap<Vec<u8>, Deco>,
}

impl Decorations {
    pub fn get(&self, name: &[u8]) -> Option<&Deco> {
        self.by_name.get(name)
    }

    /// The thumbnail to show for a row, or `None` — which is also "ask for one". A thumbnail made
    /// for another version of the file is not shown and not counted as an answer: the file changed
    /// under it, so it is asked for again.
    pub fn thumb_path(&self, name: &[u8], mtime_ms: u64) -> Option<&str> {
        match self.by_name.get(name).and_then(|d| d.thumb.as_ref()) {
            Some(Thumb::At { path, mtime_ms: at }) if *at == mtime_ms => Some(path.as_str()),
            _ => None,
        }
    }

    /// Whether a thumbnail for this row is worth asking for: nothing is known about it, or what is
    /// known is about a version of the file that is gone.
    pub fn wants_thumb(&self, name: &[u8], mtime_ms: u64) -> bool {
        match self.by_name.get(name).and_then(|d| d.thumb.as_ref()) {
            None => true,
            Some(t) => t.mtime().is_some_and(|at| at != mtime_ms),
        }
    }

    pub fn set_thumb(&mut self, name: &[u8], t: Thumb) {
        self.by_name.entry(name.to_vec()).or_default().thumb = Some(t);
    }

    /// Forget an answer without recording another: a job that never ran (the decoder died, the
    /// folder was closed), which must be askable again rather than remembered as "no thumbnail".
    pub fn unask_thumb(&mut self, name: &[u8]) {
        if let Some(d) = self.by_name.get_mut(name) {
            if matches!(d.thumb, Some(Thumb::Asked)) {
                d.thumb = None;
            }
        }
    }

    pub fn git(&self, name: &[u8]) -> Option<&git::Entry> {
        self.by_name.get(name).and_then(|d| d.git.as_ref())
    }

    /// A state worth drawing is kept; a clean one is not, so an unchanged repository costs nothing.
    pub fn set_git(&mut self, name: &[u8], e: Option<git::Entry>) {
        match e {
            Some(e) => self.by_name.entry(name.to_vec()).or_default().git = Some(e),
            None => {
                if let Some(d) = self.by_name.get_mut(name) {
                    d.git = None;
                }
            }
        }
    }

    pub fn repo(&self, name: &[u8]) -> Option<&git::RepoMark> {
        self.by_name.get(name).and_then(|d| d.repo.as_ref())
    }

    pub fn set_repo(&mut self, name: &[u8], r: Option<git::RepoMark>) {
        match r {
            Some(r) => self.by_name.entry(name.to_vec()).or_default().repo = Some(r),
            None => {
                if let Some(d) = self.by_name.get_mut(name) {
                    d.repo = None;
                }
            }
        }
    }

    /// The names known to be repositories of their own — what the watcher's poll stats, having
    /// no watch to spare for them.
    pub fn repo_names(&self) -> Vec<Vec<u8>> {
        self.by_name.iter().filter(|(_, d)| d.repo.is_some()).map(|(k, _)| k.clone()).collect()
    }

    /// Drop everything known about names that the folder no longer has. Called after a rescan,
    /// which is the only time a name can leave; before then the map grows only with the folder.
    pub fn retain_names(&mut self, present: &std::collections::HashSet<Vec<u8>>) {
        self.by_name.retain(|k, d| present.contains(k) && (d.thumb.is_some() || d.git.is_some() || d.repo.is_some()));
    }

    pub fn clear_git(&mut self) {
        for d in self.by_name.values_mut() {
            d.git = None;
            d.repo = None;
        }
    }

    /// How many names know anything at all — what a test asserts when it wants "and nobody else".
    #[cfg(test)]
    pub fn known(&self) -> usize {
        self.by_name.len()
    }
}
