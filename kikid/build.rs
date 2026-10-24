//! A build stamp for the About panel: the short commit and the date it was built, or "dev" in a
//! tree without git. Cheap enough to run on every build of the daemon alone.
use std::process::Command;

fn main() {
    let sha = Command::new("git").args(["rev-parse", "--short", "HEAD"]).output().ok().filter(|o| o.status.success()).map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string()).unwrap_or_else(|| "dev".into());
    let dirty = Command::new("git").args(["status", "--porcelain"]).output().ok().map(|o| !o.stdout.is_empty()).unwrap_or(false);
    let date = Command::new("date").args(["-u", "+%Y%m%d"]).output().ok().map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string()).unwrap_or_default();
    println!("cargo:rustc-env=KIKI_BUILD={}{}.{}", sha, if dirty { "+" } else { "" }, date);
    // What has to change for cargo to ask for a new stamp. `../.git/HEAD` on its own was the bug:
    // on a branch it holds the text `ref: refs/heads/<name>`, and that does not change when you
    // commit — the ref file does. So a stamp baked before a commit survived it, and both the
    // About panel and `kikid bench`'s `build` field went on naming the commit before, with
    // whatever `+` it was built with. Found 2026-10-24 recording the 1.0.0 baseline: a clean
    // tree at 5399065 stamped itself `de27b55+`.
    println!("cargo:rerun-if-changed=../.git/HEAD");
    // Deliberately NOT `../.git/index`: it would turn the `+` over on a `git add` as well, but
    // it moves on every staging operation (and when `git status` refreshes a stale stat cache),
    // and each move recompiles the daemon — the one crate in the tree where that is felt. A
    // commit moves the ref, which is what the stamp is for.
    for r in watch_refs() {
        println!("cargo:rerun-if-changed=../.git/{r}");
    }
    println!("cargo:rerun-if-changed=src");
    // An edit that is not committed moves nothing cargo is watching, so `+` can lag until the
    // daemon is rebuilt for another reason. That is the trade above, and it is the right way
    // round: the stamp exists to say which commit an artefact came from.
}

/// The ref `HEAD` names, plus `packed-refs`, because a ref that has been packed has no file of
/// its own. Either may be absent — cargo is content to watch a path that is not there yet.
fn watch_refs() -> Vec<String> {
    let mut out = vec!["packed-refs".to_string()];
    if let Some(r) = std::fs::read_to_string("../.git/HEAD").ok().and_then(|h| h.strip_prefix("ref: ").map(|r| r.trim().to_string())) {
        out.push(r);
    }
    out
}
