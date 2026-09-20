//! The Unix socket server: one thread per client for reading, one for writing.

use crate::json::Value;
use crate::listing::{self, Listing, SortRole, Subscriber};
use crate::proto::{self, Frame, Framing, Reader, Request};
use crate::vfs::uri::Uri;
use crate::vfs::VfsError;
use std::collections::HashMap;
use std::io::Write;
use std::os::unix::net::{UnixListener, UnixStream};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{self, Sender};
use std::sync::Arc;
use std::thread;

static NEXT_CLIENT: AtomicU64 = AtomicU64::new(1);

pub fn serve(listener: UnixListener) -> std::io::Result<()> {
    for stream in listener.incoming() {
        match stream {
            Ok(s) => {
                let id = NEXT_CLIENT.fetch_add(1, Ordering::Relaxed);
                thread::Builder::new().name(format!("client-{id}")).spawn(move || Client::run(id, s)).expect("spawn client");
            }
            Err(e) => eprintln!("accept: {e}"),
        }
    }
    Ok(())
}

struct Client {
    id: u64,
    tx: Sender<Value>,
    listings: HashMap<u64, Arc<Listing>>,
    /// Mirror plans served as windowed views: lid -> (job, reason filter)
    plans: HashMap<u64, (u64, String)>,
    /// Search results served as windowed views: lid -> rows
    searches: HashMap<u64, Vec<Value>>,
    trees: HashMap<u64, crate::tree::Tree>,
}

impl Client {
    fn run(id: u64, stream: UnixStream) {
        let reader_stream = match stream.try_clone() {
            Ok(s) => s,
            Err(e) => {
                eprintln!("client {id}: clone: {e}");
                return;
            }
        };
        let mut reader = Reader::new(reader_stream);
        // The writer needs the framing, which is known after the first frame.
        let first = match reader.next() {
            Ok(Some(f)) => f,
            Ok(None) => return,
            Err(e) => {
                eprintln!("client {id}: {e}");
                return;
            }
        };
        let framing = reader.framing().unwrap_or(Framing::Binary);
        let (tx, rx) = mpsc::channel::<Value>();
        let mut writer = stream;
        thread::Builder::new()
            .name(format!("writer-{id}"))
            .spawn(move || {
                while let Ok(v) = rx.recv() {
                    if proto::write_json(&mut writer, framing, &v).is_err() {
                        break;
                    }
                    if writer.flush().is_err() {
                        break;
                    }
                }
            })
            .expect("spawn writer");
        let mut client = Client { id, tx, listings: HashMap::new(), plans: HashMap::new(), searches: HashMap::new(), trees: HashMap::new() };
        client.handle_frame(first);
        loop {
            match reader.next() {
                Ok(Some(f)) => client.handle_frame(f),
                Ok(None) => break,
                Err(e) => {
                    let _ = client.tx.send(proto::err(0, "Protocol", e.to_string()));
                    break;
                }
            }
        }
        for (lid, l) in client.listings.drain() {
            l.unsubscribe(id, lid);
        }
        // A window that went away was showing these: nobody will close them now.
        for (_, (job, _)) in client.plans.drain() {
            crate::mirror::unview(job);
        }
    }

    fn handle_frame(&mut self, f: Frame) {
        match f {
            Frame::Binary(_) => {
                let _ = self.tx.send(proto::err(0, "Protocol", "unexpected binary frame"));
            }
            Frame::Json(v) => match proto::parse_request(v) {
                Ok(req) => self.handle(req),
                Err(m) => {
                    let _ = self.tx.send(proto::err(0, "Protocol", m));
                }
            },
        }
    }

    fn reply(&self, id: u64, r: Result<Value, (&str, String)>) {
        let v = match r {
            Ok(v) => proto::ok(id, v),
            Err((code, msg)) => proto::err(id, code, msg),
        };
        let _ = self.tx.send(v);
    }

    fn handle(&mut self, req: Request) {
        let id = req.id;
        let b = &req.body;
        let result: Result<Option<Value>, (&str, String)> = match req.kind.as_str() {
            "Hello" => {
                // A client built for another protocol is told so, in words it can show, instead of
                // being answered as if all were well and failing later on a request that has
                // changed shape. One that names no version is taken at its word (scripts, tests).
                if let Some(theirs) = b.u64_field("version") {
                    if theirs != proto::PROTOCOL_VERSION {
                        return self.reply(id, Err(("Version", format!("this kikid speaks protocol {}, the client {theirs}: restart kiki after an upgrade (systemctl --user restart kikid.service)", proto::PROTOCOL_VERSION))));
                    }
                }
                if b.str_field("client") == Some("kiki") {
                    crate::dbus::register_shell(self.tx.clone());
                }
                Ok(Some(
                    Value::obj()
                        .u("version", proto::PROTOCOL_VERSION)
                        .s("daemon", format!("kikid {}", env!("CARGO_PKG_VERSION")))
                        .v("plugins", Value::Arr(crate::plugin::available().into_iter().map(Value::Str).collect()))
                        .done(),
                ))
            }
            "ChooserResult" => {
                crate::dbus::chooser_result(b.str_field("token").unwrap_or(""), b.get("uris").cloned().unwrap_or(Value::Null));
                Ok(Some(Value::obj().done()))
            }
            "Icon" => {
                let name = b.str_field("name").unwrap_or("");
                let theme = b.str_field("theme").unwrap_or("hicolor");
                let size = b.u64_field("size").unwrap_or(32) as u32;
                match crate::icons::lookup(theme, name, size) {
                    Some(p) => Ok(Some(Value::obj().s("path", p.to_string_lossy()).done())),
                    None => Ok(Some(Value::obj().done())),
                }
            }
            "Keymap" => Ok(Some(Value::obj().v("keys", crate::config::keymap()).done())),
            "About" => Ok(Some(
                Value::obj()
                    .s("version", env!("CARGO_PKG_VERSION"))
                    .s("build", env!("KIKI_BUILD"))
                    .s("socket", crate::config::socket_path_string())
                    .s("pluginDir", crate::plugin::plugin_dirs().iter().map(|p| p.to_string_lossy().into_owned()).collect::<Vec<_>>().join(":"))
                    .s("configDir", crate::config::config_dir().to_string_lossy())
                    .done(),
            )),
            "ResetSettings" => crate::config::reset_all().map(|_| Some(Value::obj().done())).map_err(|e| ("Io", e.to_string())),
            "Ping" => Ok(Some(Value::obj().done())),
            "Version" => Ok(Some(Value::obj().s("version", env!("CARGO_PKG_VERSION")).done())),
            "Open" => self.open(b),
            "Window" => {
                if let Some(lid) = b.u64_field("lid") {
                    if self.plans.contains_key(&lid) {
                        return self.reply(id, self.plan_window(lid, b.u64_field("first").unwrap_or(0) as usize, b.u64_field("count").unwrap_or(60) as usize));
                    }
                    if let Some(t) = self.trees.get(&lid) {
                        let first = b.u64_field("first").unwrap_or(0) as usize;
                        let count = b.u64_field("count").unwrap_or(60).min(512) as usize;
                        let rows: Vec<Value> = (first..(first + count).min(t.visible.len())).filter_map(|r| t.row_json(r)).collect();
                        return self.reply(id, Ok(Value::obj().u("first", first as u64).u("n", t.visible.len() as u64).b("done", true).v("rows", Value::Arr(rows)).done()));
                    }
                    if let Some(rows) = self.searches.get(&lid) {
                        let first = b.u64_field("first").unwrap_or(0) as usize;
                        let count = b.u64_field("count").unwrap_or(60).min(512) as usize;
                        let slice: Vec<Value> = rows.iter().skip(first).take(count).cloned().collect();
                        return self.reply(id, Ok(Value::obj().u("first", first as u64).u("n", rows.len() as u64).b("done", true).v("rows", Value::Arr(slice)).done()));
                    }
                }
                self.window(b)
            }
            "OpenTree" => match (b.u64_field("lid"), parse_uri(b, "uri")) {
                (Some(lid), Ok(u)) => match crate::tree::Tree::open(&u) {
                    Ok(t) => {
                        let n = t.visible.len() as u64;
                        self.trees.insert(lid, t);
                        let _ = self.tx.send(proto::event("Count").u("lid", lid).u("n", n).b("done", true).done());
                        Ok(Some(Value::obj().u("n", n).done()))
                    }
                    Err(e) => Err(vfs_err(e)),
                },
                (None, _) => Err(("Protocol", "missing lid".into())),
                (_, Err(e)) => Err(e),
            },
            "TreeExpand" => self.tree_expand(b),
            "TreeFilter" => self.tree_filter(b),
            "TreeReveal" => self.tree_reveal(b),
            "Arrange" => {
                let windows: Vec<Value> = b.get("windows").and_then(Value::as_arr).map(|a| a.to_vec()).unwrap_or_default();
                let width = b.u64_field("leftWidth").unwrap_or(320) as u32;
                let tx = self.tx.clone();
                std::thread::spawn(move || {
                    let r = crate::tree::arrange(&windows, width);
                    let _ = tx.send(proto::ok(id, r));
                });
                Ok(None)
            }
            "SharePlugins" => Ok(Some(Value::obj().v("plugins", crate::share::list_json()).done())),
            "ShareTargets" => match b.str_field("plugin") {
                Some(p) => crate::share::targets(p, b.str_field("query")).map(|t| Some(Value::obj().v("targets", t).done())).map_err(vfs_err),
                None => Err(("Protocol", "missing plugin".into())),
            },
            "ShareConfigure" => match b.str_field("plugin") {
                Some(p) => crate::share::configure(p, b.get("config").unwrap_or(&Value::Null), b.get("secrets").unwrap_or(&Value::Null)).map(|_| Some(Value::obj().done())).map_err(vfs_err),
                None => Err(("Protocol", "missing plugin".into())),
            },
            "Share" => {
                for u in b.get("uris").and_then(Value::as_arr).into_iter().flatten().filter_map(Value::as_str) {
                    crate::access::record(u);
                }
                let op = Value::obj()
                    .s("op", "share")
                    .s("plugin", b.str_field("plugin").unwrap_or(""))
                    .v("uris", b.get("uris").cloned().unwrap_or(Value::Arr(vec![])))
                    .opt_s("target", b.str_field("target"))
                    .v("compose", b.get("compose").cloned().unwrap_or(Value::Null))
                    .done();
                crate::jobs::submit(op, Some(self.tx.clone())).map(|j| Some(Value::obj().u("job", j).done()))
            }
            "AiStatus" => Ok(Some(crate::ai::status())),
            "AiConfigure" => crate::ai::configure(b.str_field("provider"), b.str_field("cliCommand")).map(|_| Some(crate::ai::status())).map_err(vfs_err),
            "AiOpen" => {
                let uris: Vec<Uri> = b.get("uris").and_then(Value::as_arr).map(|a| a.iter().filter_map(Value::as_str).filter_map(|s| Uri::parse(s).ok()).collect()).unwrap_or_default();
                match parse_uri(b, "dir") {
                    Ok(dir) => crate::ai::open_external(&dir, &uris).map(|tool| Some(Value::obj().s("tool", tool).done())).map_err(vfs_err),
                    Err(e) => Err(e),
                }
            }
            "OpenTerminal" => match parse_uri(b, "dir") {
                Ok(dir) => crate::ai::open_terminal(&dir).map(|_| Some(Value::obj().done())).map_err(vfs_err),
                Err(e) => Err(e),
            },
            "Search" => self.search(b),
            "IndexStatus" => Ok(Some(crate::index::status_json())),
            "IndexRebuild" => {
                crate::index::rebuild_async();
                Ok(Some(Value::obj().done()))
            }
            "IndexRoots" => Ok(Some(Value::obj().v("roots", Value::Arr(crate::index::roots().iter().map(|r| Value::Str(Uri::from_path(r).to_string())).collect())).done())),
            "SetIndexRoots" => match b.get("roots") {
                Some(r) => crate::config::set_settings(&Value::obj().v("index", Value::obj().v("roots", r.clone()).done()).done())
                    .map(|_| {
                        crate::index::rebuild_async();
                        Some(Value::obj().done())
                    })
                    .map_err(|e| ("Io", e.to_string())),
                None => Err(("Protocol", "missing roots".into())),
            },
            "OpenInList" => Ok(Some(Value::obj().v("tools", crate::openin::list_json()).done())),
            "OpenIn" => {
                for u in b.get("uris").and_then(Value::as_arr).into_iter().flatten().filter_map(Value::as_str) {
                    crate::access::record(u);
                }
                let key = b.str_field("id").or(b.str_field("role")).unwrap_or("").to_string();
                let uris: Vec<Uri> = b.get("uris").and_then(Value::as_arr).map(|a| a.iter().filter_map(|v| v.as_str()).filter_map(|s| Uri::parse(s).ok()).collect()).unwrap_or_default();
                let class = crate::openin::find(&key).and_then(|t| t.str_field("id").map(crate::openin::window_class)).unwrap_or_default();
                crate::openin::open(&key, &uris, b.u64_field("line")).map(|(pid, reused)| Some(Value::obj().u("pid", pid as u64).b("reused", reused).s("class", class).done())).map_err(|e| ("Invalid", e))
            }
            "OpenInTest" => {
                let key = b.str_field("id").or(b.str_field("role")).unwrap_or("").to_string();
                let uris: Vec<Uri> = b.get("uris").and_then(Value::as_arr).map(|a| a.iter().filter_map(|v| v.as_str()).filter_map(|s| Uri::parse(s).ok()).collect()).unwrap_or_default();
                match crate::openin::find(&key) {
                    Some(t) => crate::openin::prepare(&t, &uris, b.u64_field("line"), t.str_field("command").unwrap_or(""))
                        .map(|p| Some(Value::obj().s("command", p.command).s("cwd", p.cwd.to_string_lossy()).done()))
                        .map_err(|e| ("Invalid", e)),
                    None => Err(("NotFound", "no such tool".into())),
                }
            }
            "OpenInSessions" => Ok(Some(Value::obj().v("sessions", crate::openin::sessions_json()).done())),
            "OpenInClose" => match b.str_field("id") {
                Some(i) if crate::openin::close(i) => Ok(Some(Value::obj().done())),
                Some(_) => Err(("NotFound", "no session".into())),
                None => Err(("Protocol", "missing id".into())),
            },
            "SetOpenIn" => match b.get("tools").and_then(Value::as_arr) {
                Some(t) => crate::openin::write_tools(t)
                    .map(|_| {
                        let _ = self.tx.send(proto::event("OpenInChanged").done());
                        Some(Value::obj().done())
                    })
                    .map_err(|e| ("Io", e.to_string())),
                None => Err(("Protocol", "missing tools".into())),
            },
            "Repo" => match parse_uri(b, "uri") {
                Ok(u) if u.is_local() => Ok(Some(crate::git::repo_json(&u.to_path()))),
                Ok(_) => Ok(Some(Value::Null)),
                Err(e) => Err(e),
            },
            "GitStatus" => match parse_uri(b, "uri") {
                Ok(u) if u.is_local() => Ok(Some(crate::git::file_json(&u.to_path()))),
                Ok(_) => Ok(Some(Value::Null)),
                Err(e) => Err(e),
            },
            "GitRefresh" => match parse_uri(b, "uri") {
                Ok(u) if u.is_local() => {
                    crate::git::invalidate(&u.to_path());
                    if let Some(l) = crate::listing::find(&u.to_path()) {
                        l.rescan();
                    }
                    Ok(Some(Value::obj().done()))
                }
                Ok(_) => Ok(Some(Value::obj().done())),
                Err(e) => Err(e),
            },
            "MirrorPlan" => self.mirror_plan(b),
            "MirrorFilter" => self.mirror_filter(b),
            "MirrorCheck" => self.mirror_check(b),
            "MirrorReport" => match b.u64_field("job").and_then(crate::mirror::stored) {
                Some(s) => Ok(Some(Value::obj().s("text", crate::mirror::report(&s.spec, &s.plan.lock().unwrap())).done())),
                None => Err(("NotFound", "no such plan".into())),
            },
            "Filters" => Ok(Some(
                Value::obj()
                    .v(
                        "rules",
                        Value::Arr(
                            crate::mirror::load_filters()
                                .iter()
                                .map(|r| match r {
                                    crate::mirror::Rule::Contains(v) => Value::obj().s("kind", "contains").s("value", v.clone()).done(),
                                    crate::mirror::Rule::StartsWith(v) => Value::obj().s("kind", "startsWith").s("value", v.clone()).done(),
                                    crate::mirror::Rule::EndsWith(v) => Value::obj().s("kind", "endsWith").s("value", v.clone()).done(),
                                    crate::mirror::Rule::Matches(v) => Value::obj().s("kind", "matches").s("value", v.clone()).done(),
                                })
                                .collect(),
                        ),
                    )
                    .done(),
            )),
            "SetFilters" => match b.get("rules").and_then(Value::as_arr) {
                Some(rules) => {
                    let mut m = std::collections::BTreeMap::new();
                    m.insert("rule".to_string(), Value::Arr(rules.to_vec()));
                    crate::config::write_named("filters.toml", &Value::Obj(m)).map(|_| Some(Value::obj().done())).map_err(|e| ("Io", e.to_string()))
                }
                None => Err(("Protocol", "missing rules".into())),
            },
            "Sort" => self.sort(b, id),
            "Filter" => self.filter(b),
            "ShowHidden" => self.show_hidden(b),
            "SeekName" => self.seek_name(b),
            "Enrich" => self.enrich(b, id),
            "Close" => self.close(b),
            "Refresh" => self.refresh(b),
            "Prefetch" => match parse_uri(b, "uri") {
                Ok(u) => listing::open(&u).map(|_| Some(Value::obj().done())).map_err(vfs_err),
                Err(e) => Err(e),
            },
            "Favorites" => Ok(Some(Value::obj().v("items", crate::config::favorites()).done())),
            "SetFavorites" => match b.get("items").and_then(Value::as_arr) {
                // Every window, not just the one that asked: a second window's sidebar listens
                // for this, and until now nobody ever said it.
                Some(items) => crate::config::set_favorites(items)
                    .map(|_| {
                        crate::jobs::broadcast(proto::event("FavoritesChanged").done());
                        Some(Value::obj().done())
                    })
                    .map_err(|e| ("Io", e.to_string())),
                None => Err(("Protocol", "missing items".into())),
            },
            "Volumes" => Ok(Some(Value::obj().v("items", crate::config::volumes()).done())),
            "Mount" | "Unmount" | "Eject" | "OpenWith" | "Launch" | "Devices" | "RenameDevice" | "AccessLog" | "ClearAccessLog" | "PluginBrowse" => self.misc_op(&req.kind, b),
            "TrashInfo" => Ok(Some(Value::obj().v("items", Value::Arr(crate::ops::trash_infos().into_iter().map(|(n, p, d)| Value::obj().s("name", n).s("path", p).s("deleted", d).done()).collect())).done())),
            "Settings" => Ok(Some(crate::config::settings())),
            "ViewPrefs" => Ok(Some(Value::obj().v("folders", crate::config::view_prefs()).done())),
            "Integration" => Ok(Some(crate::integrate::status_json())),
            "Integrate" => Ok(Some(crate::integrate::apply(b.get("parts")))),
            "Unintegrate" => Ok(Some(crate::integrate::remove(b.get("parts")))),
            "SetViewPref" => {
                let uri = b.str_field("uri").unwrap_or("");
                if uri.is_empty() {
                    Err(("Protocol", "missing uri".into()))
                } else {
                    crate::config::set_view_pref(uri, b.str_field("view").unwrap_or("list"), b.str_field("sort").unwrap_or("name"), b.str_field("order").unwrap_or("asc"), b.get("hidden").and_then(Value::as_bool))
                        .map(|_| {
                            crate::jobs::broadcast(proto::event("ViewPrefsChanged").s("uri", uri).done());
                            Some(Value::obj().done())
                        })
                        .map_err(|e| ("Io", e.to_string()))
                }
            }
            "ClearViewPrefs" => crate::config::clear_view_prefs()
                .map(|_| {
                    crate::jobs::broadcast(proto::event("ViewPrefsChanged").done());
                    Some(Value::obj().done())
                })
                .map_err(|e| ("Io", e.to_string())),
            "SetSettings" => match b.get("patch") {
                Some(p) => crate::config::set_settings(p).map(|_| Some(Value::obj().done())).map_err(|e| ("Io", e.to_string())),
                None => Err(("Protocol", "missing patch".into())),
            },
            "PluginStatus" => Ok(Some(Value::obj().v("plugins", crate::plugin::status_json()).done())),
            "PluginPing" => self.plugin_ping(b),
            "Plugins" => Ok(Some(Value::obj().v("plugins", Value::Arr(crate::plugin::describe_all())).done())),
            "Locations" => Ok(Some(Value::obj().v("locations", crate::locations::json_list()).done())),
            "TestLocation" => match b.get("location") {
                Some(loc) => crate::locations::test(loc, b.get("secrets").unwrap_or(&Value::Null)).map(|(fp, known)| Some(Value::obj().opt_s("fingerprint", fp.as_deref()).b("knownHost", known).done())).map_err(vfs_err),
                None => Err(("Protocol", "missing location".into())),
            },
            "AddLocation" | "UpdateLocation" => match b.get("location") {
                Some(loc) if !crate::plugin::ships(loc.str_field("plugin").unwrap_or("")) => Err(("Unsupported", format!("{} locations are not part of this build", loc.str_field("plugin").unwrap_or("?")))),
                // `verify` in the reply means the server offered a key nobody has accepted yet:
                // the shell shows it and asks again with `trust` set to what it displayed.
                Some(loc) => crate::locations::save(loc.clone(), b.get("secrets").unwrap_or(&Value::Null), b.str_field("trust"), b.get("check").and_then(Value::as_bool).unwrap_or(true))
                    .map(|verify| match verify {
                        Some(fp) => Some(Value::obj().s("verify", fp).s("host", loc.get("config").and_then(|c| c.str_field("host")).unwrap_or("")).done()),
                        None => {
                            crate::jobs::broadcast(proto::event("LocationsChanged").done());
                            Some(Value::obj().done())
                        }
                    })
                    .map_err(vfs_err),
                None => Err(("Protocol", "missing location".into())),
            },
            "SetLocationImage" => match b.str_field("name") {
                Some(n) => crate::locations::set_image(n, b.str_field("image"))
                    .map(|_| {
                        crate::jobs::broadcast(proto::event("LocationsChanged").done());
                        Some(Value::obj().done())
                    })
                    .map_err(|e| ("Io", e.to_string())),
                None => Err(("Protocol", "missing name".into())),
            },
            "RemoveLocation" => match b.str_field("name") {
                Some(n) => {
                    if let Some(l) = crate::locations::find(n) {
                        if let Some(p) = l.str_field("plugin") {
                            crate::listing::invalidate_authority(p, n);
                        }
                    }
                    crate::locations::remove(n)
                        .map(|_| {
                            crate::jobs::broadcast(proto::event("LocationsChanged").done());
                            Some(Value::obj().done())
                        })
                        .map_err(|e| ("Io", e.to_string()))
                }
                None => Err(("Protocol", "missing name".into())),
            },
            "Disconnect" => match b.str_field("name") {
                Some(n) => {
                    if let Some(l) = crate::locations::find(n) {
                        if let Some(p) = l.str_field("plugin") {
                            crate::listing::invalidate_authority(p, n);
                        }
                    }
                    crate::locations::disconnect(n);
                    Ok(Some(Value::obj().done()))
                }
                None => Err(("Protocol", "missing name".into())),
            },
            "Preview" => match parse_uri(b, "uri") {
                Ok(u) => crate::preview::preview(&u).map(Some).map_err(vfs_err),
                Err(e) => Err(e),
            },
            "Thumbnail" => match parse_uri(b, "uri") {
                Ok(u) => {
                    let size = if b.u64_field("size") == Some(256) { crate::thumbs::Size::Large } else { crate::thumbs::Size::Normal };
                    let tx = self.tx.clone();
                    let kind = crate::kinds::Kind::guess(crate::vfs::EntryType::File, u.name().as_bytes());
                    let mtime = std::fs::metadata(u.to_path()).ok().and_then(|m| m.modified().ok()).and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok()).map(|d| d.as_millis() as u64).unwrap_or(0);
                    crate::thumber::submit(crate::thumber::Job {
                        uri: u,
                        kind,
                        mtime_ms: mtime,
                        size,
                        // Someone asked for this one by name: it is wanted whatever happens.
                        wanted: None,
                        done: Box::new(move |a| {
                            let _ = tx.send(match a {
                                crate::thumber::Answer::Made(p) => proto::ok(id, Value::obj().s("path", p.to_string_lossy()).done()),
                                _ => proto::err(id, "Unsupported", "no thumbnail"),
                            });
                        }),
                    });
                    Ok(None)
                }
                Err(e) => Err(e),
            },
            "Submit" => match b.get("op") {
                Some(op) => crate::jobs::submit(op.clone(), Some(self.tx.clone())).map(|j| Some(Value::obj().u("job", j).done())),
                None => Err(("Protocol", "missing op".into())),
            },
            "Cancel" => match b.u64_field("job") {
                Some(j) if crate::jobs::cancel(j) => Ok(Some(Value::obj().done())),
                Some(_) => Err(("NotFound", "no such job".into())),
                None => Err(("Protocol", "missing job".into())),
            },
            "Jobs" => Ok(Some(Value::obj().v("jobs", crate::jobs::list()).done())),
            // Forgetting finished jobs is the daemon's to do: the list is fetched again on every
            // reconnect, which would bring back whatever a window had only hidden. Live jobs stay.
            // What the Log button opens; asked again from `next` while the window is up.
            "JobLog" => match b.u64_field("job") {
                Some(job) => crate::joblog::read_job(job, b.u64_field("from").unwrap_or(0)).map(Some).ok_or(("NotFound", "no log for that job".to_string())),
                None => Err(("Protocol", "missing job".into())),
            },
            // Everything a location's plugin has said, for a location that will not connect.
            "LocationLog" => match b.str_field("location").and_then(crate::locations::find) {
                Some(loc) => Ok(Some(crate::joblog::read_plugin(loc.str_field("plugin").unwrap_or(""), b.u64_field("from").unwrap_or(0)))),
                None => Err(("NotFound", "no such location".into())),
            },
            "ClearJobs" => Ok(Some(Value::obj().u("cleared", crate::jobs::dismiss(None)).done())),
            "DismissJob" => match b.u64_field("job") {
                Some(job) => Ok(Some(Value::obj().u("cleared", crate::jobs::dismiss(Some(job))).done())),
                None => Err(("Protocol", "missing job".into())),
            },
            "Undo" => crate::jobs::undo(Some(self.tx.clone())).map(|j| Some(Value::obj().u("job", j).done())),
            "Redo" => crate::jobs::redo(Some(self.tx.clone())).map(|j| Some(Value::obj().u("job", j).done())),
            "JobEvents" => {
                crate::jobs::subscribe(self.tx.clone());
                Ok(Some(Value::obj().done()))
            }
            "PromptReply" => match (b.u64_field("job"), b.str_field("choice")) {
                (Some(j), Some(c)) => {
                    let all = b.get("applyToAll").and_then(Value::as_bool).unwrap_or(false);
                    if crate::jobs::prompt_reply(j, c, all) {
                        Ok(Some(Value::obj().done()))
                    } else {
                        Err(("NotFound", "no prompt pending".into()))
                    }
                }
                _ => Err(("Protocol", "missing job or choice".into())),
            },
            "Stat" => match parse_uri(b, "uri") {
                Ok(u) => Listing::stat_uri(&u).map(Some).map_err(vfs_err),
                Err(e) => Err(e),
            },
            other => Err(("Unsupported", format!("unknown request type {other}"))),
        };
        match result {
            Ok(Some(v)) => self.reply(id, Ok(v)),
            Ok(None) => {} // the reply is sent later by whoever holds the waiter
            Err(e) => self.reply(id, Err(e)),
        }
    }

    fn lid(&self, b: &Value) -> Result<(u64, Arc<Listing>), (&'static str, String)> {
        let lid = b.u64_field("lid").ok_or(("Protocol", "missing lid".to_string()))?;
        let l = self.listings.get(&lid).cloned().ok_or(("NotFound", format!("no listing {lid}")))?;
        Ok((lid, l))
    }

    fn open(&mut self, b: &Value) -> Result<Option<Value>, (&'static str, String)> {
        let lid = b.u64_field("lid").ok_or(("Protocol", "missing lid".to_string()))?;
        let uri = parse_uri(b, "uri")?;
        let (l, cached) = listing::open(&uri).map_err(vfs_err)?;
        if let Some(old) = self.listings.insert(lid, Arc::clone(&l)) {
            old.unsubscribe(self.id, lid);
        }
        l.subscribe(Subscriber { client: self.id, lid, tx: self.tx.clone(), first: 0, count: 0, view_first: 0, view_count: 0 });
        let (n, done) = l.count();
        // The first Count arrives with the reply so a client can size its model immediately.
        let _ = self.tx.send(proto::event("Count").u("lid", lid).u("n", n).b("done", done).done());
        if let Some(e) = l.error() {
            return Err(("Io", e));
        }
        Ok(Some(Value::obj().b("cached", cached).done()))
    }

    fn window(&mut self, b: &Value) -> Result<Option<Value>, (&'static str, String)> {
        let (lid, l) = self.lid(b)?;
        let first = b.u64_field("first").unwrap_or(0) as u32;
        let count = b.u64_field("count").unwrap_or(60) as u32;
        let view = match (b.u64_field("viewFirst"), b.u64_field("viewCount")) {
            (Some(f), Some(c)) => Some((f as u32, c as u32)),
            _ => None,
        };
        Ok(Some(l.window(self.id, lid, first, count, view)))
    }

    fn sort(&mut self, b: &Value, id: u64) -> Result<Option<Value>, (&'static str, String)> {
        let (_, l) = self.lid(b)?;
        let role = SortRole::parse(b.str_field("role").unwrap_or("name")).ok_or(("Protocol", "bad role".to_string()))?;
        let asc = b.str_field("order").unwrap_or("asc") != "desc";
        l.sort(role, asc, Some((self.tx.clone(), id)));
        Ok(None)
    }

    fn filter(&mut self, b: &Value) -> Result<Option<Value>, (&'static str, String)> {
        let (_, l) = self.lid(b)?;
        let n = l.filter(b.str_field("text").unwrap_or(""));
        Ok(Some(Value::obj().u("n", n).done()))
    }

    fn seek_name(&mut self, b: &Value) -> Result<Option<Value>, (&'static str, String)> {
        let (_, l) = self.lid(b)?;
        let after = b.u64_field("after").map(|a| a as u32);
        Ok(Some(match l.seek(b.str_field("prefix").unwrap_or(""), after) {
            Some(i) => Value::obj().u("index", i as u64).done(),
            None => Value::obj().v("index", Value::Null).done(),
        }))
    }

    fn show_hidden(&mut self, b: &Value) -> Result<Option<Value>, (&'static str, String)> {
        let (_, l) = self.lid(b)?;
        let n = l.set_hidden(b.get("show").and_then(Value::as_bool).unwrap_or(false));
        Ok(Some(Value::obj().u("n", n).done()))
    }

    fn enrich(&mut self, b: &Value, id: u64) -> Result<Option<Value>, (&'static str, String)> {
        let (_, l) = self.lid(b)?;
        l.enrich(Some((self.tx.clone(), id)));
        Ok(None)
    }

    fn close(&mut self, b: &Value) -> Result<Option<Value>, (&'static str, String)> {
        let lid = b.u64_field("lid").ok_or(("Protocol", "missing lid".to_string()))?;
        if let Some(l) = self.listings.remove(&lid) {
            l.unsubscribe(self.id, lid);
        }
        if let Some((job, _)) = self.plans.remove(&lid) {
            crate::mirror::unview(job);
        }
        self.searches.remove(&lid);
        self.trees.remove(&lid);
        Ok(Some(Value::obj().done()))
    }

    fn refresh(&mut self, b: &Value) -> Result<Option<Value>, (&'static str, String)> {
        let (_, l) = self.lid(b)?;
        l.rescan();
        Ok(Some(Value::obj().done()))
    }
}

impl Client {
    #[allow(clippy::type_complexity)]
    fn plan_rows(&self, lid: u64) -> Result<(Arc<crate::mirror::Stored>, Vec<usize>), (&'static str, String)> {
        let (job, reason) = self.plans.get(&lid).cloned().ok_or(("NotFound", "no plan view".to_string()))?;
        let stored = crate::mirror::stored(job).ok_or(("NotFound", "no such plan".to_string()))?;
        let idx: Vec<usize> = {
            let p = stored.plan.lock().unwrap();
            p.actions
                .iter()
                .enumerate()
                .filter(|(_, a)| match reason.as_str() {
                    "new" => a.reason == crate::mirror::Reason::New,
                    "changed" => a.reason == crate::mirror::Reason::Changed,
                    "equal" => a.reason == crate::mirror::Reason::Equal,
                    "delete" => matches!(a.kind, crate::mirror::ActionKind::Delete | crate::mirror::ActionKind::Rmdir),
                    _ => true,
                })
                .map(|(i, _)| i)
                .collect()
        };
        Ok((stored, idx))
    }

    fn plan_window(&self, lid: u64, first: usize, count: usize) -> Result<Value, (&'static str, String)> {
        let (stored, idx) = self.plan_rows(lid)?;
        let p = stored.plan.lock().unwrap();
        let rows: Vec<Value> = idx.iter().skip(first).take(count.min(512)).map(|&i| crate::mirror::action_json(&p.actions[i])).collect();
        Ok(Value::obj().u("first", first as u64).u("n", idx.len() as u64).b("done", true).v("rows", Value::Arr(rows)).done())
    }

    fn search(&mut self, b: &Value) -> Result<Option<Value>, (&'static str, String)> {
        let lid = b.u64_field("lid").ok_or(("Protocol", "missing lid".to_string()))?;
        let q = b.str_field("query").unwrap_or("").to_string();
        let mode = match b.str_field("mode") {
            Some("prefix") => crate::index::Mode::Prefix,
            Some("fuzzy") => crate::index::Mode::Fuzzy,
            _ => crate::index::Mode::Substring,
        };
        let scope = b.str_field("scope").unwrap_or("everywhere");
        let (rows, capped): (Vec<Value>, bool) = if scope == "location" {
            // Remote walk, filtered by substring: one recursive Scan where the plugin has one,
            // folder by folder where it answers Unsupported (FTPS; SFTP without a shell) — which
            // used to be the end of it, so searching those locations found nothing and said so.
            let uri = parse_uri(b, "uri")?;
            let (session, path) = crate::locations::resolve(&uri).map_err(vfs_err)?;
            let lower = q.to_ascii_lowercase();
            let out = std::cell::RefCell::new(Vec::new());
            let full = std::cell::Cell::new(false);
            // `rel` is the entry's path under the folder being searched.
            let take = |rel: &str, e: &Value| {
                let name = rel.rsplit('/').next().unwrap_or("");
                if !lower.is_empty() && !name.to_ascii_lowercase().contains(&lower) {
                    return;
                }
                if out.borrow().len() >= crate::index::MAX_RESULTS {
                    full.set(true);
                    return;
                }
                let kind = e.str_field("kind").unwrap_or("file");
                let at = uri.join(rel);
                out.borrow_mut().push(
                    Value::obj()
                        .s("name", name)
                        .s("kind", if kind == "dir" { "folder" } else { "file" })
                        .b("isDir", kind == "dir")
                        .b("isLink", kind == "link")
                        .v("meta", e.get("meta").cloned().unwrap_or(Value::Null))
                        .v("thumb", Value::Null)
                        .v("git", Value::Null)
                        .s("parent", at.parent().map(|p| p.to_string()).unwrap_or_default())
                        .s("uri", at.to_string())
                        .done(),
                );
            };
            let entries_of = |m: crate::plugin::Msg, each: &mut dyn FnMut(&Value)| {
                if let crate::plugin::Msg::Json(v) = m {
                    for e in v.get("entries").and_then(Value::as_arr).unwrap_or(&[]) {
                        each(e);
                    }
                }
            };
            let whole = session.plugin.request_stream(session.req("Scan").s("path", path.clone()).b("recursive", true).done(), |m| {
                entries_of(m, &mut |e| take(e.str_field("rel").or(e.str_field("name")).unwrap_or(""), e));
            });
            match whole {
                Ok(_) => {}
                Err(VfsError::Unsupported) => {
                    out.borrow_mut().clear();
                    let mut folders = std::collections::VecDeque::from([String::new()]);
                    while let Some(prefix) = folders.pop_front() {
                        if full.get() {
                            break;
                        }
                        let at = if prefix.is_empty() { path.clone() } else { format!("{}/{}", path.trim_end_matches('/'), prefix) };
                        let mut below = Vec::new();
                        // A folder that cannot be read is skipped, as a recursive scan skips it.
                        let _ = session.plugin.request_stream(session.req("Scan").s("path", at).done(), |m| {
                            entries_of(m, &mut |e| {
                                let name = e.str_field("name").unwrap_or("");
                                if name.is_empty() {
                                    return;
                                }
                                let rel = if prefix.is_empty() { name.to_string() } else { format!("{prefix}/{name}") };
                                take(&rel, e);
                                if e.str_field("kind") == Some("dir") {
                                    below.push(rel);
                                }
                            });
                        });
                        folders.extend(below);
                    }
                }
                Err(e) => return Err(vfs_err(e)),
            }
            (out.into_inner(), full.get())
        } else {
            crate::index::maybe_refresh();
            crate::index::with_index(|ix| {
                let (hits, capped) = crate::index::query(ix, &q, mode);
                (hits.iter().map(|h| crate::index::hit_row(ix, h)).collect(), capped)
            })
        };
        let n = rows.len() as u64;
        self.searches.insert(lid, rows);
        let _ = self.tx.send(proto::event("Count").u("lid", lid).u("n", n).b("done", true).done());
        let _ = self.tx.send(proto::event("Reset").u("lid", lid).u("n", n).done());
        Ok(Some(Value::obj().u("n", n).b("capped", capped).u("indexAge", crate::ops::unix_now().saturating_sub(crate::index::with_index(|ix| ix.built_at))).done()))
    }

    fn plugin_ping(&mut self, b: &Value) -> Result<Option<Value>, (&'static str, String)> {
        let n = b.str_field("name").ok_or(("Protocol", "missing name".to_string()))?;
        let bin = crate::plugin::inventory().into_iter().find(|(x, _)| x == n).map(|(_, p)| p).ok_or(("NotFound", "no such plugin".to_string()))?;
        let start = std::time::Instant::now();
        let p = crate::plugin::Plugin::spawn_path(&bin, n).map_err(vfs_err)?;
        let d = p.request(Value::obj().s("type", "Describe").done()).map_err(vfs_err)?;
        p.request(Value::obj().s("type", "Ping").done()).map_err(vfs_err)?;
        p.shutdown();
        Ok(Some(Value::obj().u("ms", start.elapsed().as_millis() as u64).v("describe", d).done()))
    }

    fn tree_expand(&mut self, b: &Value) -> Result<Option<Value>, (&'static str, String)> {
        let lid = b.u64_field("lid").ok_or(("Protocol", "missing lid".to_string()))?;
        let t = self.trees.get_mut(&lid).ok_or(("NotFound", "no tree".to_string()))?;
        t.expand(b.u64_field("first").unwrap_or(0) as usize, b.get("expanded").and_then(Value::as_bool).unwrap_or(true)).map_err(vfs_err)?;
        let n = t.visible.len() as u64;
        let _ = self.tx.send(proto::event("Reset").u("lid", lid).u("n", n).done());
        Ok(Some(Value::obj().u("n", n).done()))
    }

    fn tree_filter(&mut self, b: &Value) -> Result<Option<Value>, (&'static str, String)> {
        let lid = b.u64_field("lid").ok_or(("Protocol", "missing lid".to_string()))?;
        let t = self.trees.get_mut(&lid).ok_or(("NotFound", "no tree".to_string()))?;
        t.set_filter(b.str_field("text").unwrap_or(""));
        let n = t.visible.len() as u64;
        let _ = self.tx.send(proto::event("Reset").u("lid", lid).u("n", n).done());
        Ok(Some(Value::obj().u("n", n).done()))
    }

    fn tree_reveal(&mut self, b: &Value) -> Result<Option<Value>, (&'static str, String)> {
        let lid = b.u64_field("lid").ok_or(("Protocol", "missing lid".to_string()))?;
        let u = parse_uri(b, "uri")?;
        let t = self.trees.get_mut(&lid).ok_or(("NotFound", "no tree".to_string()))?;
        let row = t.reveal(&u).map_err(vfs_err)?;
        let n = t.visible.len() as u64;
        let _ = self.tx.send(proto::event("Reset").u("lid", lid).u("n", n).done());
        Ok(Some(Value::obj().u("row", row as u64).done()))
    }

    fn misc_op(&mut self, t: &str, b: &Value) -> Result<Option<Value>, (&'static str, String)> {
        match t {
            "Mount" => {
                let dev = b.str_field("device").unwrap_or("").to_string();
                let mnt = crate::config::mount(&dev).map_err(|m| ("Io", m))?;
                crate::jobs::broadcast(proto::event("VolumesChanged").done());
                Ok(Some(Value::obj().s("uri", crate::vfs::uri::Uri::local(&mnt).map(|u| u.to_string()).unwrap_or_default()).s("mountPoint", mnt).done()))
            }
            "Unmount" => {
                crate::config::unmount(b.str_field("device").unwrap_or("")).map_err(|m| ("Io", m))?;
                crate::jobs::broadcast(proto::event("VolumesChanged").done());
                Ok(Some(Value::obj().done()))
            }
            "Eject" => {
                if let Some(uri) = b.str_field("uri") {
                    crate::devices::eject(uri).map_err(|e| (e.code(), e.message()))?;
                    return Ok(Some(Value::obj().done()));
                }
                crate::config::eject(b.str_field("device").unwrap_or("")).map_err(|m| ("Io", m))?;
                crate::jobs::broadcast(proto::event("VolumesChanged").done());
                Ok(Some(Value::obj().done()))
            }
            "Devices" => Ok(Some(crate::devices::list_json())),
            "RenameDevice" => {
                let uri = Uri::parse(b.str_field("uri").unwrap_or("")).map_err(|e| ("Protocol", e.0.to_string()))?;
                crate::devices::rename(&uri.authority, b.str_field("name").unwrap_or("")).map_err(|e| ("Io", e.to_string()))?;
                crate::jobs::broadcast(proto::event("DeviceAdded").v("device", Value::Null).done());
                Ok(Some(Value::obj().done()))
            }
            "OpenWith" => {
                let uri = Uri::parse(b.str_field("uri").unwrap_or("")).map_err(|e| ("Protocol", e.0.to_string()))?;
                let path = crate::ops::local_path(&uri).map_err(|e| (e.code(), e.message()))?;
                Ok(Some(crate::desktop::apps_json(&path)))
            }
            "PluginBrowse" => {
                let scheme = b.str_field("plugin").unwrap_or("");
                let p = crate::plugin::get(scheme).map_err(|e| (e.code(), e.message()))?;
                let req = Value::obj()
                    .s("type", "Browse")
                    .s("field", b.str_field("field").unwrap_or(""))
                    .v("config", b.get("config").cloned().unwrap_or(Value::Null))
                    .v("secrets", b.get("secrets").cloned().unwrap_or(Value::Null))
                    .done();
                p.request(req).map(Some).map_err(|e| (e.code(), e.message()))
            }
            "AccessLog" => {
                let uris: Vec<String> = b.get("uris").and_then(Value::as_arr).map(|a| a.iter().filter_map(Value::as_str).map(str::to_string).collect()).unwrap_or_default();
                let mut m = Value::obj();
                for u in &uris {
                    if let Some(ms) = crate::access::opened(u) {
                        m = m.u(u, ms);
                    }
                }
                Ok(Some(Value::obj().v("opened", m.done()).u("entries", crate::access::count() as u64).s("path", crate::access::path().to_string_lossy().into_owned()).done()))
            }
            "ClearAccessLog" => crate::access::clear().map(|_| Some(Value::obj().done())).map_err(|e| ("Io", e.to_string())),
            "Launch" => {
                let uris: Vec<String> = b.get("uris").and_then(Value::as_arr).map(|a| a.iter().filter_map(Value::as_str).map(str::to_string).collect()).unwrap_or_default();
                let uris: Vec<String> = uris.iter().map(|u| Uri::parse(u).ok().and_then(|x| crate::ops::local_path(&x).ok()).map(|p| Uri::from_path(&p).to_string()).unwrap_or_else(|| u.clone())).collect();
                crate::desktop::launch(b.str_field("app").unwrap_or(""), &uris).map_err(|m| ("Io", m))?;
                for u in &uris {
                    crate::access::record(u);
                }
                Ok(Some(Value::obj().done()))
            }
            _ => Err(("Protocol", format!("unknown {t}"))),
        }
    }

    fn mirror_plan(&mut self, b: &Value) -> Result<Option<Value>, (&'static str, String)> {
        let job = b.u64_field("job").ok_or(("Protocol", "missing job".to_string()))?;
        let lid = b.u64_field("lid").ok_or(("Protocol", "missing lid".to_string()))?;
        let stored = crate::mirror::view(job).ok_or(("NotFound", "no such plan".to_string()))?;
        if let Some((old, _)) = self.plans.insert(lid, (job, "all".into())) {
            crate::mirror::unview(old);
        }
        let (counts, offset, n) = {
            let p = stored.plan.lock().unwrap();
            (p.counts_json(), p.clock_offset_ms, p.actions.len() as u64)
        };
        let _ = self.tx.send(proto::event("Count").u("lid", lid).u("n", n).b("done", true).done());
        Ok(Some(Value::obj().v("counts", counts).i("clockOffsetMs", offset).u("n", n).done()))
    }

    fn mirror_filter(&mut self, b: &Value) -> Result<Option<Value>, (&'static str, String)> {
        let lid = b.u64_field("lid").ok_or(("Protocol", "missing lid".to_string()))?;
        let reason = b.str_field("reason").unwrap_or("all").to_string();
        let entry = self.plans.get_mut(&lid).ok_or(("NotFound", "no plan view".to_string()))?;
        entry.1 = reason;
        let (_, idx) = self.plan_rows(lid)?;
        let _ = self.tx.send(proto::event("Reset").u("lid", lid).u("n", idx.len() as u64).done());
        Ok(Some(Value::obj().u("n", idx.len() as u64).done()))
    }

    fn mirror_check(&mut self, b: &Value) -> Result<Option<Value>, (&'static str, String)> {
        let lid = b.u64_field("lid").ok_or(("Protocol", "missing lid".to_string()))?;
        let first = b.u64_field("first").unwrap_or(0) as usize;
        let count = b.u64_field("count").unwrap_or(1) as usize;
        let checked = b.get("checked").and_then(Value::as_bool).unwrap_or(true);
        let (stored, idx) = self.plan_rows(lid)?;
        let mut p = stored.plan.lock().unwrap();
        for &i in idx.iter().skip(first).take(count) {
            if p.actions[i].kind != crate::mirror::ActionKind::Skip {
                p.actions[i].checked = checked;
            }
        }
        let counts = p.counts_json();
        Ok(Some(Value::obj().v("counts", counts).done()))
    }
}

fn parse_uri(b: &Value, key: &str) -> Result<Uri, (&'static str, String)> {
    let s = b.str_field(key).ok_or(("Protocol", format!("missing {key}")))?;
    Uri::parse(s).map_err(|e| ("Protocol", format!("bad uri: {}", e.0)))
}

fn vfs_err(e: VfsError) -> (&'static str, String) {
    (e.code(), e.message())
}
