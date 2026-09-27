//! What kiki does without each of the packages that are NOT in `depends`
//! (docs/0.3.0/02-omarchy-store.md, decision 6): a written consequence, and a test of it.
//!
//! These run only where the package really is absent, which is CI's dependency audit
//! (`.github/workflows/ci.yml`): it takes the package away, sets the variable, runs the test,
//! puts the package back. On a developer's machine, where all three are present, each says so
//! and passes — an assertion made against a machine that has the thing would prove nothing.

use std::path::Path;
use std::time::Duration;

fn on_path(program: &str) -> bool {
    std::env::var_os("PATH").map(|p| std::env::split_paths(&p).any(|d| d.join(program).is_file())).unwrap_or(false)
}

fn audit(var: &str) -> bool {
    if std::env::var_os(var).is_none() {
        eprintln!("{var} is not set: not an audit run, nothing to prove here");
        return false;
    }
    true
}

/// Without git, a repository is a plain folder: no overlay, and never an error.
#[test]
#[ignore = "run by CI's dependency audit with git removed (KIKI_AUDIT_NO_GIT=1)"]
fn without_git_a_repository_is_a_plain_folder() {
    if !audit("KIKI_AUDIT_NO_GIT") {
        return;
    }
    assert!(!on_path("git"), "this audit is for a machine without git");
    let dir = std::env::temp_dir().join(format!("kiki-audit-git-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join(".git")).unwrap();
    std::fs::write(dir.join(".git/HEAD"), "ref: refs/heads/main\n").unwrap();
    std::fs::write(dir.join("README"), "x").unwrap();
    // Detection is a file read and still works; status needs the program and quietly does not.
    assert!(kikid::git::status(&dir).is_none(), "status without git is 'not a repository', not an error");
    // And the folder still lists.
    let names: Vec<String> = std::fs::read_dir(&dir).unwrap().map(|e| e.unwrap().file_name().to_string_lossy().into_owned()).collect();
    assert!(names.iter().any(|n| n == "README"));
    let _ = std::fs::remove_dir_all(&dir);
}

/// Without a Secret Service, saving a password says one numbered sentence and nothing else
/// changes: the lookup answers nothing, so the location asks for the password each time.
#[test]
#[ignore = "run by CI's dependency audit with libsecret but no Secret Service (KIKI_AUDIT_NO_SECRET_SERVICE=1)"]
fn without_a_secret_service_saving_a_password_says_1261() {
    if !audit("KIKI_AUDIT_NO_SECRET_SERVICE") {
        return;
    }
    assert!(on_path("secret-tool"), "libsecret stays a dependency: secret-tool is the API kiki calls");
    std::env::remove_var("KIKI_SECRET_TOOL");
    let err = kikid::locations::keyring::store("audit", "password", "x").expect_err("no Secret Service: the store cannot succeed");
    assert_eq!(err.said_json().map(|(n, _)| n), Some(1261), "{err:?}");
    assert!(kikid::locations::keyring::lookup("audit", "password").is_none());
}

/// Without gvfs, an SMB location answers "not supported" — bounded, numbered, and nothing else
/// kiki does is touched.
#[test]
#[ignore = "run by CI's dependency audit with gvfs removed (KIKI_AUDIT_NO_GVFS=1)"]
fn without_gvfs_an_smb_location_is_not_supported() {
    if !audit("KIKI_AUDIT_NO_GVFS") {
        return;
    }
    assert!(!Path::new("/usr/lib/gvfsd").exists() && !Path::new("/usr/libexec/gvfsd").exists(), "this audit is for a machine without gvfs");
    let bin = Path::new(env!("CARGO_BIN_EXE_kikid")).with_file_name("kiki-plugin-gio");
    assert!(bin.exists(), "the SMB plugin is built beside the daemon: {}", bin.display());
    let p = kikid::plugin::Plugin::spawn_path(&bin, "smb").expect("the plugin starts: it links gio, which is glib2's, not gvfs");
    let req = kikid::json::Value::obj()
        .s("type", "Connect")
        .s("location", "audit")
        .s("role", "browse")
        .v("config", kikid::json::Value::obj().s("host", "127.0.0.1").s("share", "none").s("auth", "guest").done())
        .v("secrets", kikid::json::Value::obj().done())
        .done();
    let err = p.request_within(req, Duration::from_secs(30)).expect_err("no gvfsd: the share cannot be reached");
    assert!(err.code() == "Unsupported" || err.message().to_lowercase().contains("not supported"), "expected 'not supported', got {} / {}", err.code(), err.message());
}
