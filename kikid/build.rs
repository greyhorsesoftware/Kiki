//! A build stamp for the About panel: the short commit and the date it was built, or "dev" in a
//! tree without git. Cheap enough to run on every build of the daemon alone.
use std::process::Command;

fn main() {
    let sha = Command::new("git").args(["rev-parse", "--short", "HEAD"]).output().ok().filter(|o| o.status.success()).map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string()).unwrap_or_else(|| "dev".into());
    let dirty = Command::new("git").args(["status", "--porcelain"]).output().ok().map(|o| !o.stdout.is_empty()).unwrap_or(false);
    let date = Command::new("date").args(["-u", "+%Y%m%d"]).output().ok().map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string()).unwrap_or_default();
    println!("cargo:rustc-env=KIKI_BUILD={}{}.{}", sha, if dirty { "+" } else { "" }, date);
    println!("cargo:rerun-if-changed=../.git/HEAD");
    println!("cargo:rerun-if-changed=src");
}
