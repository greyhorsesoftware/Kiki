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
    assert_eq!(locations::save(loc, &Value::obj().done(), None, true).unwrap(), None);
    assert_eq!(locations::all().len(), 1);
    assert!(std::fs::read_to_string(dir.join("config/locations.toml")).unwrap().contains("[location.config]"));

    // A server that offers a key is not saved until the user has accepted that key.
    let keyed = Value::obj()
        .s("name", "keyed")
        .s("plugin", "stub")
        .s("remoteUri", "stub://keyed/")
        .v("config", Value::obj().s("name", "keyed").s("fingerprint", "SHA256:stub-key").done())
        .done();
    let asked = locations::save(keyed.clone(), &Value::obj().done(), None, true).unwrap();
    assert_eq!(asked.as_deref(), Some("SHA256:stub-key"), "the key should have come back for the user to check");
    assert_eq!(locations::all().len(), 1, "nothing is written until the key is accepted");

    // Accepting it saves the location with the key pinned, and a second save needs no question.
    assert_eq!(locations::save(keyed.clone(), &Value::obj().done(), Some("SHA256:stub-key"), true).unwrap(), None);
    let saved = locations::find("keyed").unwrap();
    assert_eq!(saved.get("config").unwrap().str_field("trustedFingerprint"), Some("SHA256:stub-key"));
    assert_eq!(locations::save(saved, &Value::obj().done(), None, true).unwrap(), None, "a pinned location saves straight through");
    locations::remove("keyed").unwrap();
    assert_eq!(locations::all().len(), 1);

    // "Add", unchecked: what was typed is saved as it stands, nothing is connected to, and so
    // no key is pinned…
    assert_eq!(locations::save(keyed.clone(), &Value::obj().done(), None, false).unwrap(), None, "no question asked: nothing was looked at");
    let unchecked = locations::find("keyed").expect("saved without being checked");
    assert!(unchecked.get("config").unwrap().str_field("trustedFingerprint").is_none());
    // …which must not turn into trusting whoever answers first: the first real connect to a
    // server whose key nobody has seen is refused, and says which key it was.
    let refused = locations::connect(&unchecked, "browse", None).map(|_| ()).unwrap_err();
    assert!(refused.message().contains(locations::UNVERIFIED_PREFIX) && refused.message().contains("SHA256:stub-key"), "{}", refused.message());
    assert!(locations::connected_names().iter().all(|n| n != "keyed"), "and no session is left behind");
    // A field that is simply wrong is still caught, without any network: that is validation.
    let nameless = Value::obj().s("name", "bad").s("plugin", "stub").s("remoteUri", "stub://bad/").v("config", Value::obj().s("name", "").done()).done();
    assert!(locations::save(nameless, &Value::obj().done(), None, false).is_err());
    assert!(locations::find("bad").is_none());
    locations::remove("keyed").unwrap();
    assert_eq!(locations::all().len(), 1);

    // Listing through the same string pool and window machinery as local directories
    let uri = Uri::parse("stub://lab/").unwrap();
    let (l, cached) = listing::open(&uri).unwrap();
    assert!(!cached);
    assert!(listing::wait_scan(&l, Duration::from_secs(5)));
    let (tx, rx) = mpsc::channel();
    l.subscribe(Subscriber { client: 1, lid: 1, tx, first: 0, count: 10, view_first: 0, view_count: 10 });
    let w = l.window(1, 1, 0, 10, None);
    assert_eq!(w.u64_field("n"), Some(4));
    let rows = w.get("rows").unwrap().as_arr().unwrap();
    assert_eq!(rows[0].str_field("name"), Some("docs")); // folders first
    assert_eq!(rows[1].str_field("name"), Some("empty"));
    assert_eq!(rows[2].str_field("name"), Some("data.bin"));
    assert_eq!(rows[3].str_field("name"), Some("slow.bin"));
    // metaInScan: metadata arrived inline, no Stat round trips needed
    assert_eq!(rows[2].get("meta").unwrap().u64_field("size"), Some(4096));
    let _ = rx.try_iter().count();

    // A plugin that cannot walk a tree in one request says so, and the daemon walks it directory
    // by directory. Answering the top level and calling it recursive would make every mirror
    // re-copy the files it never saw.
    let p = plugin::get("stub").unwrap();
    let e = p.request(Value::obj().s("type", "Scan").s("location", "lab").s("path", "/").b("recursive", true).done()).unwrap_err();
    assert!(e.message().contains("Unsupported"), "{}", e.message());

    // Subdirectory
    let (sub, _) = listing::open(&Uri::parse("stub://lab/docs").unwrap()).unwrap();
    assert!(listing::wait_scan(&sub, Duration::from_secs(5)));
    assert_eq!(sub.count().0, 2);

    // Read streams binary frames; Write round-trips
    let session = locations::resolve(&uri).unwrap().0;
    let mut got = Vec::new();
    let r = session
        .plugin
        .read_stream(Value::obj().s("type", "Read").s("location", "lab").s("path", "/docs/notes.txt").done(), |m| {
            if let Msg::Binary(b) = m {
                got.extend_from_slice(&b)
            }
        })
        .unwrap();
    assert_eq!(r.u64_field("bytes"), Some(5));
    assert_eq!(got, b"hello");
    // Two transfers at once on one plugin. Binary frames carry no id, so the only way the daemon
    // can tell whose bytes are whose is to serialise them; before it did, the second Read took the
    // first one's frames — in a mirror, one file's bytes written to another file's path.
    // `slow.bin` streams over about 180 ms, so the second request certainly starts mid-stream.
    let read = |path: &str| {
        let mut got = Vec::new();
        let n = session
            .plugin
            .read_stream(Value::obj().s("type", "Read").s("location", "lab").s("path", path).done(), |m| {
                if let Msg::Binary(b) = m {
                    got.extend_from_slice(&b)
                }
            })
            .unwrap()
            .u64_field("bytes")
            .unwrap_or(0);
        (got, n)
    };
    let (slow, quick) = std::thread::scope(|sc| {
        let a = sc.spawn(|| read("/slow.bin"));
        std::thread::sleep(Duration::from_millis(40));
        let b = sc.spawn(|| read("/docs/notes.txt"));
        (a.join().unwrap(), b.join().unwrap())
    });
    assert_eq!(slow.0.len(), 3072, "the slow read kept its own frames");
    assert!(slow.0.iter().all(|&b| b == 7), "and they were its own bytes");
    assert_eq!(slow.1, 3072);
    assert_eq!(quick.0, b"hello", "the read that began mid-stream got what it asked for");
    assert_eq!(quick.1, 5);

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

    // The same plugin, driven by the mirror rather than by a listing.
    mirror_uploads_to_a_location_and_settles(&dir);
    let start = Instant::now();
    assert!(start.elapsed() < Duration::from_secs(5));
    std::fs::remove_dir_all(&dir).unwrap();
}

/// A mirror that runs against a plugin, not just the local filesystem: the scan walks the remote
/// side through `Scan`, every file is uploaded through `Write`, and a second run has nothing left
/// to do. This is the path the binary-stream gate protects, and the one the local-only mirror
/// tests never reach.
///
/// Called from the test above rather than being one of its own: the plugin registry is global and
/// keyed by scheme, so every test in this binary would share one stub process and one in-memory
/// tree, and they would see each other's files.
fn mirror_uploads_to_a_location_and_settles(dir: &std::path::Path) {
    use kikid::mirror::{self, ActionKind, Detector, Direction, ExecCtx, Spec};
    use std::sync::atomic::AtomicBool;
    use std::sync::{Arc, Mutex};

    let loc = Value::obj()
        .s("name", "lab")
        .s("plugin", "stub")
        .s("remoteUri", "stub://lab/")
        .v("config", Value::obj().s("name", "lab").done())
        .done();
    locations::save(loc, &Value::obj().done(), None, true).unwrap();

    // A little tree to send up.
    let master = dir.join("master");
    std::fs::create_dir_all(master.join("notes")).unwrap();
    std::fs::write(master.join("top.txt"), b"top").unwrap();
    std::fs::write(master.join("notes/a.txt"), b"aaaa").unwrap();
    std::fs::write(master.join("notes/b.bin"), vec![b'b'; 5000]).unwrap();

    let mut spec = Spec {
        master: Uri::from_path(&master),
        replica: Uri::parse("stub://lab/upload").unwrap(),
        direction: Direction::Upload,
        delete_extras: false,
        blast_radius: 0.5,
        confirmed_large_delete: false,
        clock_offset_ms: 0,
        clock_offset_auto: false,
        detector: Detector::SizeOnly, // the stub keeps no mtimes
        modified_within_ms: None,
        apply_filters: false,
    };
    let cancel = AtomicBool::new(false);

    // The replica root is a folder the user picked, so it exists before a mirror is configured.
    let session = locations::resolve(&Uri::parse("stub://lab/").unwrap()).unwrap().0;
    session.plugin.request(Value::obj().s("type", "Mkdir").s("location", "lab").s("path", "/upload").done()).unwrap();

    // Nothing is in it yet, so everything is new.
    let plan = mirror::scan(&mut spec, &cancel).unwrap();
    let copies = plan.actions.iter().filter(|a| a.kind == ActionKind::Copy).count();
    let mkdirs = plan.actions.iter().filter(|a| a.kind == ActionKind::Mkdir).count();
    assert_eq!((copies, mkdirs), (3, 1), "three files and the notes/ folder");

    let ctx = ExecCtx { cancel: &cancel, workers: 3, on_change: &|_| {}, on_bytes: &|_| {}, exact_times: false };
    let out = mirror::execute(&Arc::new(Mutex::new(plan)), &spec, &ctx).unwrap();
    assert_eq!((out.copies, out.deletes), (3, 0));

    // The bytes really arrived: ask the plugin itself.
    let stat = session.plugin.request(Value::obj().s("type", "Stat").s("location", "lab").s("path", "/upload/notes/b.bin").done()).unwrap();
    assert_eq!(stat.u64_field("size"), Some(5000));
    let mut got = Vec::new();
    session
        .plugin
        .read_stream(Value::obj().s("type", "Read").s("location", "lab").s("path", "/upload/notes/a.txt").done(), |m| {
            if let Msg::Binary(b) = m {
                got.extend_from_slice(&b)
            }
        })
        .unwrap();
    assert_eq!(got, b"aaaa", "the file that landed is the file that was sent");

    // A second scan sees a replica that already matches.
    let again = mirror::scan(&mut spec, &cancel).unwrap();
    assert!(again.actions.iter().all(|a| a.kind == ActionKind::Skip), "nothing left to do: {:?}", again.actions.iter().map(|a| (a.rel.to_string(), a.kind)).collect::<Vec<_>>());
    assert_eq!(again.replica_entry_count, 4);

    // An extra file on the replica is a delete once deletes are on, and the guards allow it.
    let mut chunks = vec![b"old".to_vec()].into_iter();
    session.plugin.write_stream(Value::obj().s("type", "Write").s("location", "lab").s("path", "/upload/stale.txt").done(), || chunks.next()).unwrap();
    spec.delete_extras = true;
    spec.confirmed_large_delete = true;
    let plan = mirror::scan(&mut spec, &cancel).unwrap();
    assert_eq!(plan.delete_count(), 1);
    let out = mirror::execute(&Arc::new(Mutex::new(plan)), &spec, &ctx).unwrap();
    assert_eq!(out.deletes, 1);
    let gone = session.plugin.request(Value::obj().s("type", "Stat").s("location", "lab").s("path", "/upload/stale.txt").done());
    assert!(gone.is_err(), "the extra file is gone from the replica");

    // A file that changed replaces the one on the replica — uploaded under a part name and moved
    // into place over what was there — and no part file is left beside it.
    std::fs::write(master.join("notes/a.txt"), b"a longer second draft").unwrap();
    let plan = mirror::scan(&mut spec, &cancel).unwrap();
    assert_eq!(plan.actions.iter().filter(|a| a.kind == ActionKind::Copy).count(), 1, "only the changed file");
    mirror::execute(&Arc::new(Mutex::new(plan)), &spec, &ctx).unwrap();
    let read = |path: &str| {
        let mut got = Vec::new();
        session
            .plugin
            .read_stream(Value::obj().s("type", "Read").s("location", "lab").s("path", path).done(), |m| {
                if let Msg::Binary(b) = m {
                    got.extend_from_slice(&b)
                }
            })
            .map(|_| got)
    };
    assert_eq!(read("/upload/notes/a.txt").unwrap(), b"a longer second draft");
    let part = mirror::part_name("/upload/notes/a.txt");
    assert!(session.plugin.request(Value::obj().s("type", "Stat").s("location", "lab").s("path", part.clone()).done()).is_err(), "no part file is left behind");

    // Cancelled before the first byte, an upload is an error and the file already on the replica
    // is exactly as it was: not truncated, not replaced by half of the new one. (A plugin cannot
    // tell "that was all" from "we stopped", which is why the bytes go to a part name first.)
    std::fs::write(master.join("notes/a.txt"), b"a third draft that must not arrive at all").unwrap();
    let plan = mirror::scan(&mut spec, &cancel).unwrap();
    let stop = AtomicBool::new(true);
    let stopped = ExecCtx { cancel: &stop, workers: 1, on_change: &|_| {}, on_bytes: &|_| {}, exact_times: false };
    let _ = mirror::execute(&Arc::new(Mutex::new(plan)), &spec, &stopped);
    assert_eq!(read("/upload/notes/a.txt").unwrap(), b"a longer second draft", "the replica's file survives a cancelled upload");
    assert!(session.plugin.request(Value::obj().s("type", "Stat").s("location", "lab").s("path", part).done()).is_err(), "and the part file is removed");

    locations::remove("lab").unwrap();
}
