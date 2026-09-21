//! Every message the socket carries, through the wire and back (API-DAEMON.md).
//!
//! One message of each type is built, written with **both** framings — the binary one tools and
//! tests use, the newline one the QML shell uses — read back with the daemon's own `Reader`, and
//! compared with what went in. A request is then put through `proto::parse_request`, which is the
//! first thing the daemon does with anything that arrives.
//!
//! The list of request types is **read out of `server.rs`** rather than written here: the whole
//! value of a test like this is that it cannot fall behind, and a hand-kept list of ninety-odd
//! names falls behind the first time somebody adds a request. The events are the ones the daemon
//! actually sends, found the same way.

use kikid::json::Value;
use kikid::proto::{self, Frame, Framing, Reader};

/// Round-trips one value through both framings; returns it as it came back.
fn wire(v: &Value) -> Value {
    let mut out = Value::Null;
    for framing in [Framing::Binary, Framing::Text] {
        let mut buf = Vec::new();
        proto::write_json(&mut buf, framing, v).unwrap();
        // Two of them, one after the other: a reader that loses its place between frames is a
        // reader that works on the first message of a connection and no other.
        proto::write_json(&mut buf, framing, v).unwrap();
        let mut r = Reader::new(&buf[..]);
        for _ in 0..2 {
            match r.next().unwrap().expect("a frame") {
                Frame::Json(got) => {
                    assert_eq!(&got, v, "{framing:?}");
                    out = got;
                }
                Frame::Binary(_) => panic!("a JSON frame came back binary"),
            }
        }
        assert_eq!(r.framing(), Some(framing), "the framing is decided by the first byte and kept");
        assert!(r.next().unwrap().is_none(), "and the stream ends cleanly");
    }
    out
}

/// The request types `Client::handle` answers, taken from its own match arms. Alternatives
/// (`"AddLocation" | "UpdateLocation" =>`) count as both; the lower-case arms further down the
/// file belong to a nested match over mirror reasons, and a request type is capitalised.
fn request_types() -> Vec<String> {
    let src = include_str!("../src/server.rs");
    let body = src.split("fn handle(&mut self, req: Request)").nth(1).expect("handle()");
    let mut out = Vec::new();
    for line in body.lines() {
        let Some(patterns) = line.split_once("=>").map(|(p, _)| p.trim()) else { continue };
        if !patterns.starts_with('"') {
            continue;
        }
        let mut names = Vec::new();
        let mut ok = true;
        for part in patterns.split('|') {
            match part.trim().strip_prefix('"').and_then(|p| p.strip_suffix('"')) {
                Some(name) if name.starts_with(|c: char| c.is_ascii_uppercase()) => names.push(name.to_string()),
                _ => ok = false,
            }
        }
        if ok {
            out.extend(names);
        }
        if line.starts_with("        }") {
            break; // the end of the match, before the helpers below it
        }
    }
    out.sort();
    out.dedup();
    out
}

/// A body worth sending for each type: the fields API-DAEMON.md gives it, and between them every
/// shape the protocol has — URIs, counts, flags, arrays, nested objects, a name with a space and
/// one with a character outside ASCII.
fn body(kind: &str) -> Value {
    let uris = || Value::Arr(vec![Value::Str("file:///home/david/a%20b.txt".into()), Value::Str("sftp://homelab/srv/kiki".into())]);
    let o = Value::obj();
    match kind {
        "Hello" => o.u("version", 1).s("client", "kiki"),
        "Open" | "Prefetch" | "Refresh" | "Close" | "Enrich" => o.u("lid", 7).s("uri", "file:///home/david/Projets été"),
        "Window" => o.u("lid", 7).u("first", 0).u("count", 512).u("viewFirst", 0).u("viewCount", 60),
        "Sort" => o.u("lid", 7).s("role", "atime").s("order", "desc"),
        "Filter" => o.u("lid", 7).s("text", "ét"),
        "SeekName" => o.u("lid", 7).s("prefix", "ba").u("after", 3),
        "ShowHidden" => o.u("lid", 7).b("show", true),
        "Stat" | "Preview" | "Repo" | "GitStatus" | "GitRefresh" | "OpenWith" => o.s("uri", "file:///home/david/a%20b.txt"),
        "Thumbnail" => o.s("uri", "file:///home/david/pic.jpg").u("size", 256),
        "Search" => o.u("lid", 9).s("scope", "location").s("uri", "sftp://homelab/srv").s("query", "kiki").s("mode", "fuzzy"),
        "SetIndexRoots" => o.v("roots", uris()),
        "OpenTree" | "TreeReveal" => o.u("lid", 4).s("uri", "file:///home/david/Projects"),
        "TreeExpand" => o.u("lid", 4).u("first", 12).b("expanded", true),
        "TreeFilter" => o.u("lid", 4).s("text", "src"),
        "Arrange" => o.s("layout", "project").s("root", "file:///home/david/Projects/kiki").u("leftWidth", 320).v("windows", Value::Arr(vec![Value::obj().s("role", "editor").s("class", "nvim").u("pid", 4242).done()])),
        // `tool`, not `id`: the request's own id is a number, and a tool named in an `id` field
        // beside it would take its place.
        "OpenIn" | "OpenInTest" => o.s("tool", "nvim").s("role", "editor").v("uris", uris()).u("line", 42),
        "OpenInClose" => o.s("tool", "nvim"),
        "SetOpenIn" => o.v("tools", Value::Arr(vec![Value::obj().s("id", "nvim").s("name", "Neovim").s("command", "nvim %F").b("enabled", true).done()])),
        "Submit" => o.v("op", Value::obj().s("op", "chmod").v("items", uris()).u("mode", 0o755).b("recursive", true).done()),
        "Cancel" | "DismissJob" | "MirrorReport" => o.u("job", 41),
        "JobLog" => o.u("job", 41).u("from", 120),
        "LocationLog" => o.s("location", "homelab").u("from", 0),
        "PromptReply" => o.u("job", 41).s("choice", "keepBoth").b("applyToAll", true),
        "MirrorPlan" => o.u("job", 41).u("lid", 11),
        "MirrorFilter" => o.u("lid", 11).s("reason", "delete"),
        "MirrorCheck" => o.u("lid", 11).u("first", 0).u("count", 40).b("checked", false),
        "SetMirrorFilters" => o.v("rules", Value::Arr(vec![Value::obj().s("kind", "endsWith").s("value", ".tmp").done()])).b("defaults", false),
        "SetFavorites" => o.v("items", Value::Arr(vec![Value::obj().s("name", "Projects").s("uri", "file:///home/david/Projects").done()])),
        "SetViewPref" => o.s("uri", "file:///home/david/Photos").s("view", "gallery").s("sort", "mtime").s("order", "desc").b("hidden", false),
        "SetSettings" => o.v("patch", Value::obj().v("view", Value::obj().b("showHidden", true).done()).done()),
        "Integrate" | "Unintegrate" => o.v("parts", Value::Arr(vec![Value::Str("mime".into()), Value::Str("portal".into())])),
        "Mount" | "Unmount" => o.s("device", "/dev/sda1"),
        "Eject" => o.s("uri", "mtp://phone/"),
        "RenameDevice" => o.s("uri", "mtp://phone/").s("name", "David's phone"),
        "AccessLog" => o.v("uris", uris()),
        "Launch" => o.s("app", "org.gnome.Loupe.desktop").v("uris", uris()),
        "Plugins" | "PluginStatus" => o,
        "PluginPing" => o.s("name", "sftp"),
        "PluginBrowse" => o.s("plugin", "smb").s("field", "share").v("config", Value::obj().s("host", "nas").done()).v("secrets", Value::obj().s("password", "hunter2").done()),
        "AddLocation" | "UpdateLocation" | "TestLocation" => o
            .v(
                "location",
                Value::obj()
                    .s("name", "homelab")
                    .s("plugin", "sftp")
                    .s("remoteUri", "sftp://homelab/srv")
                    .s("localUri", "file:///home/david/srv")
                    .v("config", Value::obj().s("host", "homelab").s("port", "22").done())
                    .done(),
            )
            .v("secrets", Value::obj().s("password", "hunter2").done())
            .s("trust", "SHA256:abc")
            .b("check", true),
        "SetLocationImage" => o.s("name", "homelab").s("image", "file:///home/david/lab.png"),
        "RemoveLocation" | "Disconnect" => o.s("name", "homelab"),
        "SharePlugins" => o,
        "ShareTargets" => o.s("plugin", "tailscale").s("query", "phone"),
        "Share" => o.s("plugin", "mail").v("uris", uris()).s("target", "david@example.com").v("compose", Value::obj().s("subject", "Photos").done()),
        "ShareConfigure" => o.s("plugin", "mail").v("config", Value::obj().s("host", "smtp.example.com").done()).v("secrets", Value::obj().s("password", "hunter2").done()),
        "AiConfigure" => o.s("provider", "omarchy").s("cliCommand", "claude"),
        "AiOpen" => o.s("dir", "file:///home/david/Projects").v("uris", uris()),
        "OpenTerminal" => o.s("dir", "file:///home/david/Projects"),
        "ChooserResult" => o.s("token", "t-1").v("uris", uris()),
        "Icon" => o.s("name", "folder-documents").s("theme", "Adwaita").u("size", 32),
        // Everything else takes no fields at all (`Ping`, `Jobs`, `Undo`, `Volumes`, …).
        _ => o,
    }
    .done()
}

#[test]
fn every_request_the_daemon_answers_survives_both_framings() {
    let types = request_types();
    assert!(types.len() > 90, "only {} request types were found — the scrape of server.rs has broken: {types:?}", types.len());
    for (must, present) in [("Hello", true), ("Undo", true), ("AddLocation", true), ("UpdateLocation", true), ("Window", true), ("new", false)] {
        assert_eq!(types.iter().any(|t| t == must), present, "{must}");
    }

    for (i, kind) in types.iter().enumerate() {
        // The id is the client's, unique per outstanding request; ids near the top of the range
        // are as real as small ones and are what a long session reaches.
        let id = u64::MAX - i as u64;
        let mut o = match body(kind) {
            Value::Obj(m) => m,
            _ => unreachable!(),
        };
        o.insert("id".into(), Value::Uint(id));
        o.insert("type".into(), Value::Str(kind.clone()));
        let req = Value::Obj(o);
        let back = wire(&req);
        let parsed = proto::parse_request(back).unwrap_or_else(|e| panic!("{kind}: {e}"));
        assert_eq!(&parsed.kind, kind);
        assert_eq!(parsed.id, id);
        assert_eq!(parsed.body.get("type").and_then(Value::as_str), Some(kind.as_str()), "the whole request is kept, not only what was read out of it");
    }
}

#[test]
fn every_reply_and_event_survives_both_framings() {
    let meta = || Value::obj().u("size", 4096).u("mtime", 1_789_224_840_000).u("atime", 0).u("mode", 0o644).s("owner", "david").s("group", "david").v("digest", Value::Null).done();
    let row = || {
        Value::obj()
            .s("name", "Été.txt")
            .s("kind", "text")
            .b("isDir", false)
            .b("isLink", false)
            .v("meta", meta())
            .v("thumb", Value::Null)
            .v("git", Value::obj().s("state", "modified").b("staged", false).done())
            .done()
    };
    let job = || {
        Value::obj()
            .u("id", 41)
            .s("op", "trash")
            .s("state", "done")
            .u("done", 1)
            .u("total", 1)
            .u("bytes", 0)
            .u("bytesTotal", 0)
            .s("title", "Move wallpapers.zip to Trash")
            .v("error", Value::Null)
            .b("undoable", true)
            .s("name", "wallpapers.zip")
            .u("count", 1)
            .b("isDir", false)
            .s("direction", "local")
            .b("hidden", false)
            .s("phase", "running")
            .b("cancelling", false)
            .v("current", Value::Null)
            .u("rate", 0)
            .v("result", Value::Null)
            .done()
    };

    // Replies: the `ok` and `err` envelopes `proto` builds, over the answers the daemon gives.
    let ok_bodies = [
        Value::obj().done(),
        Value::obj().u("version", 1).s("daemon", "kikid 0.1.0").v("plugins", Value::Arr(vec![Value::Str("sftp".into()), Value::Str("smb".into())])).done(),
        Value::obj().u("first", 0).u("n", 10312).b("done", false).u("gen", 3).v("rows", Value::Arr(vec![row(), row()])).done(),
        Value::obj().v("jobs", Value::Arr(vec![job()])).done(),
        meta(),
        // A float, which is the only kind of number the protocol carries that is not a count.
        Value::obj().v("counts", Value::obj().u("new", 3).u("changed", 1).u("copyBytes", 2048).done()).i("clockOffsetMs", -3_600_000).done(),
        Value::obj().v("blastRadius", Value::Float(0.5)).done(),
    ];
    for (i, b) in ok_bodies.iter().enumerate() {
        let v = wire(&proto::ok(i as u64 + 1, b.clone()));
        assert_eq!(v.get("ok"), Some(b));
    }
    for code in ["NotFound", "Denied", "Exists", "NotEmpty", "Unsupported", "Cancelled", "Busy", "Io", "Protocol", "Plugin", "Safety", "Version"] {
        let v = wire(&proto::err(7, code, format!("{code}: a message meant for a person — “quoted”, with a\ttab")));
        assert_eq!(v.get("err").unwrap().str_field("code"), Some(code));
    }

    // Events: the ones the daemon really sends, and nothing invented — the list is the
    // `event("…")` call sites across its own source.
    let sources = [
        include_str!("../src/server.rs"),
        include_str!("../src/listing/scan.rs"),
        include_str!("../src/listing/cache.rs"),
        include_str!("../src/listing/rows.rs"),
        include_str!("../src/listing/stats.rs"),
        include_str!("../src/listing/view.rs"),
        include_str!("../src/jobs.rs"),
        include_str!("../src/devices.rs"),
        include_str!("../src/locations.rs"),
        include_str!("../src/config.rs"),
        include_str!("../src/dbus.rs"),
        include_str!("../src/openin.rs"),
    ];
    let mut names: Vec<String> = Vec::new();
    for src in sources {
        for at in src.match_indices("event(\"") {
            let rest = &src[at.0 + "event(\"".len()..];
            if let Some(end) = rest.find('"') {
                let name = &rest[..end];
                if name.starts_with(|c: char| c.is_ascii_uppercase()) && !names.iter().any(|n| n == name) {
                    names.push(name.to_string());
                }
            }
        }
    }
    names.sort();
    // `ShowChooser` is put together field by field in `dbus.rs` rather than through `event()`;
    // it is the one the helper sends and the shell answers with `ChooserResult`.
    names.push("ShowChooser".into());
    assert!(names.len() >= 19, "the scrape found only {names:?}");
    for must in ["Rows", "Reset", "Splice", "Count", "Gone", "JobEvent", "Toast", "Prompt", "JobsCleared", "RepoChanged", "ShowItems", "ShowChooser"] {
        assert!(names.iter().any(|n| n == must), "{must} is not among {names:?}");
    }

    for name in &names {
        let o = proto::event(name);
        let e = match name.as_str() {
            "Rows" => o.u("lid", 7).u("first", 0).v("rows", Value::Arr(vec![row()])),
            "Count" => o.u("lid", 7).u("n", 10312).b("done", true),
            "Reset" => o.u("lid", 7).u("n", 10312).u("gen", 4),
            "Splice" => o.u("lid", 7).u("n", 10).u("gen", 5).v("ops", Value::Arr(vec![Value::obj().s("op", "remove").u("pos", 1).done(), Value::obj().s("op", "insert").u("pos", 2).v("row", row()).done()])),
            "Progress" => o.u("lid", 7).u("done", 40).u("total", 100),
            "Gone" => o.u("lid", 7),
            "JobEvent" => o.v("job", job()),
            "JobsCleared" => o.v("jobs", Value::Arr(vec![Value::Uint(41), Value::Uint(42)])),
            "Toast" => o.u("job", 41).s("text", "Moved wallpapers.zip to Trash").b("undoable", true),
            "Prompt" => o
                .u("job", 41)
                .s("kind", "collision")
                .s("uri", "file:///home/david/dst/a%20b.txt")
                .v("existing", meta())
                .v("incoming", meta())
                .v("choices", Value::Arr(vec![Value::Str("replace".into()), Value::Str("keepBoth".into()), Value::Str("skip".into())])),
            "RepoChanged" => o.u("lid", 7).s("root", "file:///home/david/Projects/kiki"),
            "ViewPrefsChanged" => o.s("uri", "file:///home/david/Photos"),
            "DeviceAdded" => o.v("device", Value::obj().s("uri", "mtp://phone/").s("kind", "mtp").s("name", "Phone").b("connected", true).v("busy", Value::Null).done()),
            "DeviceRemoved" => o.s("uri", "mtp://phone/"),
            "ShowItems" => o.v("uris", Value::Arr(vec![Value::Str("file:///home/david/a%20b.txt".into())])).b("properties", true),
            "ShowChooser" => o
                .s("token", "t-1")
                .s("mode", "save")
                .s("title", "Save as…")
                .b("multiple", false)
                .b("directory", false)
                .v("filters", Value::Arr(vec![Value::obj().s("name", "Images").v("patterns", Value::Arr(vec![Value::Str("*.png".into())])).done()]))
                .v("currentFolder", Value::Null)
                .opt_s("currentName", Some("untitled.png"))
                .v("parentWindow", Value::Null),
            // LocationsChanged, VolumesChanged, FavoritesChanged, OpenInChanged: bare.
            _ => o,
        }
        .done();
        let back = wire(&e);
        assert_eq!(back.str_field("event"), Some(name.as_str()));
        assert!(back.u64_field("id").is_none(), "an event carries no id: that is what tells it from a reply");
    }
}

/// The framing itself, at its edges: a binary frame beside JSON ones, a frame that claims to be
/// bigger than the protocol allows, and a truncated one. None of them may be taken for a message.
#[test]
fn the_framing_refuses_what_it_cannot_carry() {
    let mut buf = Vec::new();
    proto::write_json(&mut buf, Framing::Binary, &Value::obj().u("id", 1).s("type", "Read").done()).unwrap();
    proto::write_binary(&mut buf, &[7u8; 4096]).unwrap();
    proto::write_binary(&mut buf, &[]).unwrap();
    let mut r = Reader::new(&buf[..]);
    assert!(matches!(r.next().unwrap(), Some(Frame::Json(_))));
    assert!(matches!(r.next().unwrap(), Some(Frame::Binary(b)) if b.len() == 4096));
    assert!(matches!(r.next().unwrap(), Some(Frame::Binary(b)) if b.is_empty()), "the empty frame that ends a stream");
    assert!(r.next().unwrap().is_none());

    // A length past the cap is refused without allocating it.
    let mut huge = (proto::MAX_JSON_FRAME as u32 + 1).to_le_bytes().to_vec();
    huge.push(0);
    assert!(Reader::new(&huge[..]).next().is_err());
    let mut huge_bin = (proto::MAX_BINARY_FRAME as u32 + 1).to_le_bytes().to_vec();
    huge_bin.push(1);
    assert!(Reader::new(&huge_bin[..]).next().is_err());

    // A frame that ends early is an error, not a message with a missing tail.
    let mut short = 40u32.to_le_bytes().to_vec();
    short.push(0);
    short.extend_from_slice(b"{\"id\":1}");
    assert!(Reader::new(&short[..]).next().is_err());

    // Text framing: blank lines are skipped, a line that is not JSON is refused.
    let mut r = Reader::new(&b"{\"id\":1,\"type\":\"Ping\"}\n\n   \n{\"id\":2,\"type\":\"Version\"}\n"[..]);
    assert!(matches!(r.next().unwrap(), Some(Frame::Json(v)) if v.u64_field("id") == Some(1)));
    assert!(matches!(r.next().unwrap(), Some(Frame::Json(v)) if v.u64_field("id") == Some(2)));
    assert!(r.next().unwrap().is_none());
    assert!(Reader::new(&b"{not json}\n"[..]).next().is_err());

    // And a request without the two fields every one of them has.
    assert!(proto::parse_request(Value::obj().s("type", "Ping").done()).is_err());
    assert!(proto::parse_request(Value::obj().u("id", 1).done()).is_err());
    assert!(proto::parse_request(Value::Arr(vec![])).is_err());
}
