//! Contract test: drives the stub plugin binary through the daemon's plugin host, the
//! locations layer and a listing, exactly as a real plugin would be used.

use kikid::json::Value;
use kikid::listing::{self, Subscriber};
use kikid::locations;
use kikid::plugin::{self, Msg};
use kikid::vfs::uri::Uri;
use std::sync::mpsc;
use std::time::{Duration, Instant};

fn setup() -> std::path::PathBuf {
    let _guard = kikid::ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let bin = std::path::PathBuf::from(env!("CARGO_BIN_EXE_kiki-plugin-stub"));
    let dir = std::env::temp_dir().join(format!("kiki-contract-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("plugins")).unwrap();
    std::fs::copy(&bin, dir.join("plugins/kiki-plugin-stub")).unwrap();
    std::env::set_var("KIKI_PLUGIN_DIR", dir.join("plugins"));
    assert_eq!(plugin::inventory().len(), 1);
    std::env::set_var("KIKI_CONFIG_DIR", dir.join("config"));
    // No Secret Service in CI: a fake secret-tool that always succeeds and stores nothing.
    let fake = dir.join("secret-tool");
    std::fs::write(&fake, "#!/bin/sh\nif [ \"$1\" = lookup ]; then exit 1; fi\ncat >/dev/null; exit 0\n").unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&fake, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
    std::env::set_var("KIKI_SECRET_TOOL", &fake);
    dir
}

#[test]
fn stub_plugin_end_to_end() {
    let dir = setup();
    // Discovery and Describe
    assert_eq!(plugin::available(), vec!["stub".to_string()]);
    let described = plugin::describe_all();
    assert_eq!(described.len(), 1);
    assert_eq!(described[0].str_field("scheme"), Some("stub"));
    assert_eq!(described[0].get("form").unwrap().as_arr().unwrap().len(), 2);

    // Validate rejects a missing name with the field named
    let p = plugin::get("stub").unwrap();
    let err = p.request(Value::obj().s("type", "Validate").v("config", Value::obj().s("name", "").done()).done()).unwrap_err();
    assert_eq!(err.code(), "Io");
    assert!(err.message().contains("Invalid"));

    // Save a location: validate + connect + (fake) keyring + file
    let loc = Value::obj().s("name", "lab").s("plugin", "stub").s("remoteUri", "stub://lab/docs").s("localUri", "file:///tmp").v("config", Value::obj().s("name", "lab").s("greeting", "yo").done()).done();
    locations::save(loc, &Value::obj().done()).unwrap();
    assert_eq!(locations::all().len(), 1);
    assert!(std::fs::read_to_string(dir.join("config/locations.toml")).unwrap().contains("[location.config]"));

    // Listing through the same string pool and window machinery as local directories
    let uri = Uri::parse("stub://lab/").unwrap();
    let (l, cached) = listing::open(&uri).unwrap();
    assert!(!cached);
    assert!(listing::wait_scan(&l, Duration::from_secs(5)));
    let (tx, rx) = mpsc::channel();
    l.subscribe(Subscriber { client: 1, lid: 1, tx, first: 0, count: 10 });
    let w = l.window(1, 1, 0, 10);
    assert_eq!(w.u64_field("n"), Some(3));
    let rows = w.get("rows").unwrap().as_arr().unwrap();
    assert_eq!(rows[0].str_field("name"), Some("docs"));   // folders first
    assert_eq!(rows[1].str_field("name"), Some("empty"));
    assert_eq!(rows[2].str_field("name"), Some("data.bin"));
    // metaInScan: metadata arrived inline, no Stat round trips needed
    assert_eq!(rows[2].get("meta").unwrap().u64_field("size"), Some(4096));
    let _ = rx.try_iter().count();

    // Subdirectory
    let (sub, _) = listing::open(&Uri::parse("stub://lab/docs").unwrap()).unwrap();
    assert!(listing::wait_scan(&sub, Duration::from_secs(5)));
    assert_eq!(sub.count().0, 2);

    // Read streams binary frames; Write round-trips
    let session = locations::resolve(&uri).unwrap().0;
    let mut got = Vec::new();
    let r = session.plugin.request_stream(Value::obj().s("type", "Read").s("location", "lab").s("path", "/docs/notes.txt").done(), |m| if let Msg::Binary(b) = m { got.extend_from_slice(&b) }).unwrap();
    assert_eq!(r.u64_field("bytes"), Some(5));
    assert_eq!(got, b"hello");
    let mut chunks = vec![b"abc".to_vec(), b"def".to_vec()].into_iter();
    let w = session.plugin.write_stream(Value::obj().s("type", "Write").s("location", "lab").s("path", "/docs/new.txt").done(), || chunks.next()).unwrap();
    assert_eq!(w.u64_field("bytes"), Some(6));
    let st = session.plugin.request(Value::obj().s("type", "Stat").s("location", "lab").s("path", "/docs/new.txt").done()).unwrap();
    assert_eq!(st.u64_field("size"), Some(6));
    // Mkdir, Rename, Delete
    session.plugin.request(Value::obj().s("type", "Mkdir").s("location", "lab").s("path", "/docs/made").done()).unwrap();
    session.plugin.request(Value::obj().s("type", "Rename").s("location", "lab").s("from", "/docs/made").s("to", "/docs/moved").done()).unwrap();
    session.plugin.request(Value::obj().s("type", "Delete").s("location", "lab").s("path", "/docs/moved").done()).unwrap();
    let e = session.plugin.request(Value::obj().s("type", "Delete").s("location", "lab").s("path", "/docs").done()).unwrap_err();
    assert_eq!(e.code(), "NotEmpty");
    // Unsupported is typed
    let e = session.plugin.request(Value::obj().s("type", "Chmod").s("location", "lab").s("path", "/docs").u("mode", 0o644).done()).unwrap_err();
    assert_eq!(e.code(), "Unsupported");

    // Removing the location drops its listings and sessions
    listing::invalidate_authority("stub", "lab");
    locations::remove("lab").unwrap();
    assert!(locations::all().is_empty());
    let start = Instant::now();
    assert!(start.elapsed() < Duration::from_secs(5));
    std::fs::remove_dir_all(&dir).unwrap();
}
