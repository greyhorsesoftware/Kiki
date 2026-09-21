//! The share plugins (plan 18): the two binaries this build ships, run for their `Describe`,
//! and `share.rs` driven end to end against a stub that writes down what it was handed.
//!
//! The `Describe` half is the same kind of contract as `plugin_contract.rs` is for location
//! plugins: the real binaries are spawned and asked, and what each one says about itself — what
//! it takes, whether it needs a program that is not there — is pinned here, because every one of
//! those answers is something the Share menu draws.
//!
//! The `share.rs` half is what the daemon does **before** a plugin is reached and is the part
//! nothing tested: fetching a file off a server, zipping a folder a plugin will not take, handing
//! over local paths, carrying the plugin's progress into the job, and clearing the scratch
//! directory however the share ends.

mod common;

use kikid::json::Value;
use kikid::{jobs, share};
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::time::{Duration, Instant};

const PATIENCE: Duration = Duration::from_secs(30);

/// Where cargo put this build's binaries: this test runs from `<profile>/deps`.
fn built_binaries() -> PathBuf {
    let exe = std::env::current_exe().expect("current exe");
    exe.parent().and_then(Path::parent).expect("…/<profile>/deps/<test>").to_path_buf()
}

/// A share plugin is another package's binary, so `cargo test` does not necessarily rebuild it —
/// it builds a package's binaries only when that package has tests of its own, and tailscale has
/// none. What this test then spawns is whatever was left in `target/` by the last `cargo build`,
/// which can be a plugin built against an older SDK: the contract below passes or fails on a
/// binary nobody meant to run. Caught the hard way while `defaultEnabled` was being taken out.
///
/// So: if a plugin is older than the SDK every plugin is built on, say what to do about it rather
/// than let the assertions argue about a stale answer.
fn not_stale(bin: &Path) {
    let sdk = Path::new(env!("CARGO_MANIFEST_DIR")).parent().expect("the workspace root").join("crates/kiki-plugin-sdk/src/lib.rs");
    let when = |p: &Path| std::fs::metadata(p).and_then(|m| m.modified()).ok();
    if let (Some(plugin), Some(sdk_changed)) = (when(bin), when(&sdk)) {
        assert!(plugin >= sdk_changed, "{} was built before the last change to the plugin SDK: run `cargo build` before `cargo test`", bin.display());
    }
}

fn job_json(id: u64) -> Value {
    jobs::list().as_arr().unwrap().iter().find(|j| j.u64_field("id") == Some(id)).cloned().expect("the job is in the list")
}

fn finished(id: u64) -> Value {
    let start = Instant::now();
    loop {
        let j = job_json(id);
        if !matches!(j.str_field("state"), Some("running") | Some("queued")) {
            return j;
        }
        assert!(start.elapsed() < PATIENCE, "the share never ended: {}", kikid::json::to_string(&j));
        std::thread::sleep(Duration::from_millis(5));
    }
}

/// What one of the stubs wrote down, in order, for the call it was asked to make.
fn stub_log(dir: &Path, plugin: &str, call: &str) -> Vec<Value> {
    std::fs::read_to_string(dir.join("share-stub.log"))
        .unwrap_or_default()
        .lines()
        .filter_map(|l| kikid::json::parse(l.as_bytes()).ok())
        .filter(|v: &Value| v.str_field("plugin") == Some(plugin) && v.str_field("call") == Some(call))
        .collect()
}

fn keys(v: Option<&Value>) -> Vec<String> {
    v.and_then(Value::as_arr).map(|a| a.iter().filter_map(|f| f.str_field("key")).map(str::to_string).collect()).unwrap_or_default()
}

fn strings(v: Option<&Value>) -> Vec<String> {
    v.and_then(Value::as_arr).map(|a| a.iter().filter_map(Value::as_str).map(str::to_string).collect()).unwrap_or_default()
}

fn on_path(bin: &str) -> bool {
    std::env::var_os("PATH").map(|p| std::env::split_paths(&p).any(|d| d.join(bin).is_file())).unwrap_or(false)
}

/// One row of the table below: what a share plugin says about itself.
struct Contract {
    id: &'static str,
    name: &'static str,
    icon: &'static str,
    /// files, folders, multiple
    accepts: (bool, bool, bool),
    max_bytes: Option<u64>,
    targets: &'static str,
    secrets: &'static [&'static str],
    form: &'static [&'static str],
    compose: &'static [&'static str],
    requires: &'static [&'static str],
}

#[test]
fn the_share_plugins_describe_themselves_and_share_rs_hands_them_local_files() {
    let dir = common::setup("share");
    std::env::set_var("KIKI_STATE_DIR", dir.join("state"));
    std::env::set_var("KIKI_DATA_DIR", dir.join("data"));
    std::env::set_var("KIKI_TRASH_DIR", dir.join("trash"));
    let plugins = dir.join("plugins");
    let log = dir.join("share-stub.log");
    std::env::set_var("KIKI_SHARE_STUB_LOG", &log);

    // The stub, three times: a plugin takes its id from the name it is run by, so the cases that
    // differ only in what a `Describe` says are three copies rather than three sources.
    for name in ["stub", "folders", "needy"] {
        std::fs::copy(env!("CARGO_BIN_EXE_kiki-plugin-share-stub"), plugins.join(format!("kiki-plugin-share-{name}"))).unwrap();
    }
    let built = built_binaries();
    let mut real: Vec<&str> = Vec::new();
    for id in ["mail", "tailscale"] {
        let bin = built.join(format!("kiki-plugin-share-{id}"));
        if !bin.is_file() {
            eprintln!("kiki-plugin-share-{id} is not built here; skipped");
            continue;
        }
        not_stale(&bin);
        #[cfg(unix)]
        std::os::unix::fs::symlink(&bin, plugins.join(format!("kiki-plugin-share-{id}"))).unwrap();
        real.push(id);
    }
    assert_eq!(real.len(), 2, "run `cargo test` over the workspace: the share plugins are default members");

    // ---------------------------------------------------------------- what they say about themselves
    #[rustfmt::skip]
    let table = [
        // Mail attaches files, never a folder, and 20 MB is what a mailbox will take. It has no
        // target list: the address is typed in the compose form.
        Contract { id: "mail", name: "Mail", icon: "mail", accepts: (true, false, true), max_bytes: Some(20 * 1024 * 1024), targets: "none",
                   secrets: &["password"], form: &["mode", "host", "port", "security", "username", "password", "from", "trustFingerprint"], compose: &["to", "subject", "body"], requires: &[] },
        // Tailscale sends to a peer, so it lists them — and it cannot work without the program.
        Contract { id: "tailscale", name: "Tailscale", icon: "cloud", accepts: (true, false, true), max_bytes: None, targets: "list",
                   secrets: &[], form: &[], compose: &[], requires: &["tailscale"] },
        // LocalSend was the third; it was removed from 0.1.0 on 2026-09-21 (`18-share.md`).
    ];

    let listed = share::list_json();
    let listed = listed.as_arr().unwrap();
    let mine = |id: Option<&str>| id.is_some_and(|id| table.iter().any(|c| c.id == id) || ["stub", "folders", "needy"].contains(&id));
    assert_eq!(listed.iter().filter(|p| mine(p.str_field("id"))).count(), 5, "the two that ship and the three stubs: {:?}", listed.iter().filter_map(|p| p.str_field("id")).collect::<Vec<_>>());
    // Each of them is the binary in this test's own plugin directory, not one installed on the
    // machine the test happens to be running on.
    for (id, path) in share::available() {
        if mine(Some(&id)) {
            assert_eq!(path.parent(), Some(plugins.as_path()), "{id} was found somewhere else");
        }
    }

    for c in &table {
        let d = listed.iter().find(|p| p.str_field("id") == Some(c.id)).unwrap_or_else(|| panic!("{} is not listed", c.id));
        assert_eq!(d.str_field("kind"), Some("share"), "{}: a share plugin says which kind it is", c.id);
        assert_eq!(d.str_field("name"), Some(c.name), "{}", c.id);
        assert_eq!(d.str_field("icon"), Some(c.icon), "{}", c.id);
        assert!(d.str_field("version").is_some_and(|v| !v.is_empty()), "{}: a version", c.id);
        let a = d.get("accepts").unwrap_or_else(|| panic!("{}: accepts", c.id));
        assert_eq!((a.get("files").and_then(Value::as_bool), a.get("folders").and_then(Value::as_bool), a.get("multiple").and_then(Value::as_bool)), (Some(c.accepts.0), Some(c.accepts.1), Some(c.accepts.2)), "{}", c.id);
        assert_eq!(a.u64_field("maxBytes"), c.max_bytes, "{}", c.id);
        assert_eq!(d.str_field("targets"), Some(c.targets), "{}", c.id);
        // A plugin that is installed is on: nothing ships switched off any more, and no plugin
        // can ask to (`defaultEnabled` went with LocalSend, which was its only user).
        assert_eq!(d.get("defaultEnabled"), None, "{}: a plugin no longer says how it ships", c.id);
        assert_eq!(d.get("enabled"), Some(&Value::Bool(true)), "{}: nobody has said otherwise, so it is on", c.id);
        assert_eq!(strings(d.get("secretFields")), c.secrets, "{}", c.id);
        assert_eq!(keys(d.get("form")), c.form, "{}", c.id);
        assert_eq!(keys(d.get("compose")), c.compose, "{}", c.id);
        assert_eq!(strings(d.get("requires")), c.requires, "{}", c.id);
        // A program it cannot work without is looked for now, not when it described itself, so
        // installing one brings the entry alive without a restart.
        let missing: Vec<&&str> = c.requires.iter().filter(|b| !on_path(b)).collect();
        match missing.as_slice() {
            [] => assert_eq!(d.get("unavailable"), None, "{}: everything it needs is here", c.id),
            _ => assert_eq!(d.str_field("unavailable"), Some(format!("{} is not installed", c.requires.join(", ")).as_str()), "{}", c.id),
        }
    }

    // A plugin that needs a program nobody has is dimmed and says which. Asserted through a stub
    // as well as through tailscale above, so it holds whether or not tailscale is installed here.
    let needy = listed.iter().find(|p| p.str_field("id") == Some("needy")).unwrap();
    assert_eq!(needy.str_field("unavailable"), Some("definitely-not-a-program is not installed"));
    assert_eq!(listed.iter().find(|p| p.str_field("id") == Some("stub")).unwrap().get("unavailable"), None);

    // ---------------------------------------------------------------- configure, and the secret
    share::configure("stub", &Value::obj().b("enabled", true).s("note", "for the lab").done(), &Value::obj().s("token", "hunter2").done()).unwrap();
    let calls = stub_log(&dir, "stub", "Configure");
    let configured = calls.first().expect("the plugin was told");
    assert_eq!(configured.get("config").unwrap().str_field("note"), Some("for the lab"));
    assert_eq!(configured.get("secrets").unwrap().str_field("token"), Some("hunter2"));
    // The secret went to the keyring under the plugin's own name, and not into the config file.
    assert!(common::secret_log(&dir).lines().any(|l| l.contains("store") && l.contains("share:stub")), "{}", common::secret_log(&dir));
    let written = std::fs::read_to_string(dir.join("config/share.toml")).unwrap();
    assert!(written.contains("for the lab") && !written.contains("hunter2"), "{written}");
    let (cfg, secrets) = share::config_for("stub");
    assert_eq!(cfg.str_field("note"), Some("for the lab"));
    assert_eq!(cfg.get("enabled"), Some(&Value::Bool(true)));
    // The test's `secret-tool` finds nothing (there is no Secret Service in CI), which is the
    // case the daemon has to survive: no secret, not a failure.
    assert_eq!(secrets, Value::obj().done());

    // ---------------------------------------------------------------- targets
    let all = share::targets("stub", None).unwrap();
    assert_eq!(all.as_arr().map(<[Value]>::len), Some(2));
    assert_eq!(all.as_arr().unwrap()[0].str_field("name"), Some("Near phone"));
    assert_eq!(share::targets("stub", Some("far")).unwrap().as_arr().map(<[Value]>::len), Some(1));
    assert!(share::targets("nosuch", None).is_err(), "a plugin that is not there is an error, not an empty list");

    // ---------------------------------------------------------------- a share, end to end
    common::save_location("lab");
    let here = dir.join("here");
    std::fs::create_dir_all(here.join("folder/inner")).unwrap();
    std::fs::write(here.join("one.txt"), b"one").unwrap();
    std::fs::write(here.join("folder/inner/deep.txt"), b"deep").unwrap();
    let (tx, rx) = mpsc::channel();
    jobs::subscribe(tx.clone());

    let share_op = |uris: Vec<&str>, target: &str| {
        Value::obj()
            .s("op", "share")
            .s("plugin", "stub")
            .v("uris", Value::Arr(uris.into_iter().map(|u| Value::Str(u.to_string())).collect()))
            .s("target", target)
            .v("compose", Value::obj().s("subject", "have these").done())
            .done()
    };
    let local = kikid::vfs::uri::Uri::from_path(&here.join("one.txt")).to_string();
    let folder = kikid::vfs::uri::Uri::from_path(&here.join("folder")).to_string();
    let id = jobs::submit(share_op(vec![&local, &folder, "stub://lab/docs/notes.txt"], "near"), Some(tx.clone())).unwrap();
    let j = finished(id);
    assert_eq!(j.str_field("state"), Some("done"), "{}", j.str_field("error").unwrap_or(""));

    let shared = stub_log(&dir, "stub", "Share").into_iter().next().expect("the plugin was asked to share");
    assert_eq!(shared.str_field("target"), Some("near"));
    assert_eq!(shared.get("compose").unwrap().str_field("subject"), Some("have these"));
    let handed: Vec<String> = shared.get("uris").and_then(Value::as_arr).map(|a| a.iter().filter_map(Value::as_str).map(str::to_string).collect()).unwrap_or_default();
    assert_eq!(handed.len(), 3);
    assert_eq!(handed[0], local, "a local file is handed over as it stands");
    let scratch = std::env::temp_dir().join(format!("kiki-share-{id}"));
    // A plugin that does not take folders is given a zip of one, made in the job's own scratch.
    assert!(handed[1].ends_with("/folder.zip"), "{}", handed[1]);
    assert!(handed[1].contains(&format!("kiki-share-{id}")), "{}", handed[1]);
    // And a file on a server is fetched first: what the plugin gets is a path on this machine.
    assert!(handed[2].ends_with("/notes.txt") && handed[2].starts_with("file:///"), "{}", handed[2]);
    assert!(handed[2].contains(&format!("kiki-share-{id}")), "{}", handed[2]);
    assert!(!scratch.exists(), "the fetched files and the zip are cleared away when the share ends");

    // The plugin's own progress is the job's.
    assert_eq!((j.u64_field("done"), j.u64_field("total")), (Some(3), Some(3)));
    let events: Vec<Value> = rx.try_iter().collect();
    let toast = events.iter().filter(|e| e.str_field("event") == Some("Toast")).find(|e| e.u64_field("job") == Some(id)).expect("a Toast");
    // What went and to whom, not the bare word: "sent" alone is nothing to go on when a file
    // that was handed over does not turn up.
    let said = toast.str_field("text").unwrap_or("");
    assert!(said.starts_with("Shared via stub: sent — 3 to "), "{said}");
    assert_eq!(toast.get("undoable"), Some(&Value::Bool(false)), "a share cannot be taken back");
    assert!(jobs::undo(None).is_err(), "and it journals nothing");

    // ---------------------------------------------------------------- a share that fails
    let id = jobs::submit(share_op(vec![&local], "fail"), Some(tx.clone())).unwrap();
    let j = finished(id);
    assert_eq!(j.str_field("state"), Some("failed"));
    assert_eq!(j.str_field("error"), Some("Network: the stub was told to fail"), "the plugin's own words reach the job, under the code it gave them");
    assert!(!std::env::temp_dir().join(format!("kiki-share-{id}")).exists(), "and the scratch goes even when it fails");

    // ---------------------------------------------------------------- a plugin that takes folders
    // One that says it accepts them is handed the folder as a folder, and nothing is zipped.
    let mut op = match share_op(vec![&folder], "near") {
        Value::Obj(m) => m,
        _ => unreachable!(),
    };
    op.insert("plugin".into(), Value::Str("folders".into()));
    let id = jobs::submit(Value::Obj(op), Some(tx)).unwrap();
    assert_eq!(finished(id).str_field("state"), Some("done"));
    let handed = strings(stub_log(&dir, "folders", "Share").last().unwrap().get("uris"));
    assert_eq!(handed, vec![folder.clone()], "no zip: the plugin said it takes folders");

    kikid::locations::remove("lab").unwrap();
    std::env::remove_var("KIKI_SHARE_STUB_LOG");
    std::env::remove_var("KIKI_TRASH_DIR");
    std::env::remove_var("KIKI_DATA_DIR");
    std::env::remove_var("KIKI_STATE_DIR");
    std::fs::remove_dir_all(&dir).unwrap();
}
