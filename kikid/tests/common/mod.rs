//! What every test that drives the stub plugin needs: a directory with the stub in it for the
//! plugin host to find, a config directory of its own, and a `secret-tool` that is not the
//! desktop's — CI has no Secret Service, and a test must not touch the developer's keyring.
//!
//! Each of these test binaries is its own process, which is what makes them independent: the
//! plugin registry and the session map are one per process and keyed by scheme, so two tests
//! sharing a binary would share one stub and one in-memory tree.

// Each test binary uses the part of this it needs; the rest is not dead, only unwanted there.
#![allow(dead_code)]

use std::path::{Path, PathBuf};

/// The fake `secret-tool`'s record of what it was asked to do, one call per line.
pub fn secret_log(dir: &Path) -> String {
    std::fs::read_to_string(dir.join("secret-tool.log")).unwrap_or_default()
}

/// Sets `KIKI_PLUGIN_DIR`, `KIKI_CONFIG_DIR` and `KIKI_SECRET_TOOL` and returns the temporary
/// directory they are under; the caller removes it.
pub fn setup(tag: &str) -> PathBuf {
    let _guard = kikid::ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let bin = PathBuf::from(env!("CARGO_BIN_EXE_kiki-plugin-stub"));
    let dir = std::env::temp_dir().join(format!("kiki-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("plugins")).unwrap();
    std::fs::copy(&bin, dir.join("plugins/kiki-plugin-stub")).unwrap();
    std::env::set_var("KIKI_PLUGIN_DIR", dir.join("plugins"));
    std::env::set_var("KIKI_CONFIG_DIR", dir.join("config"));
    // A `secret-tool` that succeeds, stores nothing, finds nothing, and writes down every call so
    // a test can say what the daemon asked the keyring for.
    let fake = dir.join("secret-tool");
    std::fs::write(&fake, "#!/bin/sh\nprintf '%s\\n' \"$*\" >> \"$0.log\"\nif [ \"$1\" = lookup ]; then exit 1; fi\ncat >/dev/null; exit 0\n").unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&fake, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
    std::env::set_var("KIKI_SECRET_TOOL", &fake);
    dir
}

/// A saved location served by the stub, pointing at the root of its tree, with a password in
/// (the fake) keyring.
pub fn save_location(name: &str) {
    use kikid::json::Value;
    let loc = Value::obj().s("name", name).s("plugin", "stub").s("remoteUri", format!("stub://{name}/")).v("config", Value::obj().s("name", name).done()).done();
    kikid::locations::save(loc, &Value::obj().s("password", "hunter2").done(), None, true).unwrap();
}
