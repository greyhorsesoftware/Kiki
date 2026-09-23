//! GIO/GVfs location plugin (plan 25): SMB, WebDAV and AFP through the GVfs daemons Omarchy
//! already runs. One binary; the scheme comes from the executable's name.
//!
//! **0.1.0 installs it as `kiki-plugin-smb` and nothing else.** WebDAV and AFP are out of 0.1.0
//! (owner, 2026-09-21), so `dav` and `afp` are kinds this binary would answer for and no build
//! ever runs it under: the arms below are what putting either back would need, together with
//! `plugin::LOCATION_KINDS`, the workspace's default members and the package's install line.
//!
//! SMB means SMB2 or later. SMB1/NT1 is not supported and will not be (`25-smb.md`); nothing here
//! lowers libsmbclient's own minimum, and a server that offers only SMB1 is refused by name.
//!
//! First build on Omarchy: the `gio`/`glib` crate APIs below follow the 0.20 series; check the
//! mount-operation signal signature and `FileEnumerator` iteration against the installed docs.

use gio::prelude::*;
use kiki_plugin_sdk::json::Value;
use kiki_plugin_sdk::{self as sdk, Describe, Entry, Features, Handler, Kind, Meta, Outgoing, PluginError, Result, WriteArgs};
use std::collections::HashMap;
use std::io::{Read, Write};
use std::sync::Mutex;

const ATTRS: &str = "standard::name,standard::type,standard::size,standard::is-hidden,time::modified,time::modified-usec,unix::mode,owner::user,owner::group";

fn scheme() -> String {
    let exe = std::env::args().next().unwrap_or_default();
    let base = exe.rsplit('/').next().unwrap_or("").to_string();
    base.strip_prefix("kiki-plugin-").filter(|s| !s.is_empty() && *s != "gio").unwrap_or("smb").to_string()
}

fn cfg<'a>(c: &'a Value, k: &str) -> &'a str {
    c.str_field(k).unwrap_or("")
}

/// Root URI of a location's mount, and the path inside it, per scheme.
fn root_uri(sch: &str, config: &Value) -> Result<String> {
    let host = cfg(config, "host").trim();
    if host.is_empty() {
        return Err(PluginError::invalid("host", "host is required"));
    }
    Ok(match sch {
        "smb" => {
            let share = cfg(config, "share").trim();
            if share.is_empty() {
                return Err(PluginError::invalid("share", "share is required"));
            }
            format!("smb://{host}/{share}/")
        }
        "dav" => {
            let secure = cfg(config, "security") != "plain";
            let port = cfg(config, "port").trim();
            let p = if port.is_empty() { String::new() } else { format!(":{port}") };
            format!("{}://{host}{p}{}", if secure { "davs" } else { "dav" }, ensure_slashes(cfg(config, "prefix")))
        }
        "afp" => {
            let volume = cfg(config, "volume").trim();
            if volume.is_empty() {
                return Err(PluginError::invalid("volume", "volume is required"));
            }
            format!("afp://{host}/{volume}/")
        }
        other => return Err(PluginError::invalid("scheme", format!("unknown scheme {other}"))),
    })
}

fn ensure_slashes(p: &str) -> String {
    let t = p.trim_matches('/');
    if t.is_empty() {
        "/".into()
    } else {
        format!("/{t}/")
    }
}

fn file_for(root: &str, path: &str) -> gio::File {
    let rel = path.trim_start_matches('/');
    if rel.is_empty() {
        gio::File::for_uri(root)
    } else {
        gio::File::for_uri(&format!("{}{}", root, percent(rel)))
    }
}

fn percent(s: &str) -> String {
    let mut o = String::new();
    for b in s.bytes() {
        if b.is_ascii_alphanumeric() || b"-._~/".contains(&b) {
            o.push(b as char);
        } else {
            o.push_str(&format!("%{b:02X}"));
        }
    }
    o
}

/// The bindings only expose the bulk read asynchronously, so batch `next_file` by hand.
fn next_batch(en: &gio::FileEnumerator, max: usize) -> std::result::Result<Vec<gio::FileInfo>, glib::Error> {
    let mut out = Vec::with_capacity(max);
    while out.len() < max {
        match en.next_file(gio::Cancellable::NONE)? {
            Some(info) => out.push(info),
            None => break,
        }
    }
    Ok(out)
}

fn gerr(e: glib::Error) -> PluginError {
    use gio::IOErrorEnum as E;
    match e.kind::<E>() {
        Some(E::NotFound) | Some(E::NotMounted) => PluginError::not_found(),
        Some(E::PermissionDenied) => PluginError::new("Denied", e.message()),
        Some(E::Exists) => PluginError::new("Exists", e.message()),
        Some(E::NotEmpty) => PluginError::new("NotEmpty", e.message()),
        Some(E::NotSupported) => PluginError::unsupported(),
        Some(E::HostNotFound) | Some(E::HostUnreachable) | Some(E::ConnectionRefused) | Some(E::TimedOut) | Some(E::NetworkUnreachable) => PluginError::network(e.message()),
        Some(E::FailedHandled) | Some(E::Cancelled) => PluginError::new("Cancelled", e.message()),
        _ => PluginError::io(e.message()),
    }
}

/// Whether a **mount** failed because the server speaks only SMB1.
///
/// kiki does not support SMB1 and will not (`25-smb.md`): it relies on libsmbclient's own
/// `client min protocol`, SMB2_02 on current Samba, and passes no option that would lower it. So
/// an NT1-only server fails in negotiation, and "Software caused connection abort" sends a user
/// looking at their network instead of at their NAS. Saying SMB1 explains it.
///
/// **Corrected by an observation (phase 8), against `smbd` 4.24 with `server max protocol = NT1`.**
/// Plan 25 said the plugin would find Samba's own sentence — "Protocol negotiation to server nas
/// (for a protocol between SMB2_02 and SMB3_11) failed: NT_STATUS_CONNECTION_RESET" — inside
/// gvfsd-smb's "Failed to mount Windows share: …". It never arrives: gvfsd-smb keeps nothing of
/// Samba's status but the errno it maps to, and what a real NT1-only server produces here is
///
/// > Failed to mount Windows share: Software caused connection abort
///
/// `ECONNABORTED`, from `NT_STATUS_CONNECTION_DISCONNECTED` — and that is all there is to go on.
/// The narrowing plan 25 wanted from the word "negotiat" comes from somewhere else instead: this
/// is asked only of a **mount**, before a byte of anything has been asked for. A server that takes
/// the connection and drops it while the protocol is being agreed will not speak what kiki speaks;
/// a host that is not there, or has nothing listening, fails with "Connection refused", "No route
/// to host" or "Connection timed out" and keeps its own words. Samba's sentence is still matched,
/// for a gvfs that passes more of it on than this one does. (`g_strerror` is the running locale's
/// rendering of the errno, so under another language this falls through to the plain network
/// error — which is the safe way round, and the only one gvfs leaves open.)
fn smb1_only(message: &str) -> bool {
    let m = message.to_ascii_lowercase();
    m.contains("software caused connection abort")
        || m.contains("connection reset by peer")
        || (m.contains("negotiat")
            && (m.contains("smb1") || m.contains("nt1") || m.contains("nt_status_connection_reset") || m.contains("nt_status_connection_disconnected") || m.contains("nt_status_invalid_network_response")))
}

const SMB1_REFUSED: &str = "This server only offers SMB1, which kiki does not support.";

/// What to say when the server turned the credentials down — it offers none of its own.
fn refused(auth: &str) -> &'static str {
    match auth {
        "guest" => "The server would not let us in as a guest.",
        "kerberos" => "The server would not accept the Kerberos ticket.",
        _ => "The server refused this username and password.",
    }
}

struct Session {
    root: String,
}

/// What makes a session **this** one: the location and the role it was opened for. A location has
/// the browser's session and one per transfer or mirror run (`06-remote-locations.md`), and they
/// come and go independently — so a map keyed by the location alone has one entry that they all
/// share and any one of them can take away. Which is what happened: the first copy to a share
/// opened a job session, found the browser's entry under the same key and reused it, and then
/// took it out of the map when the job ended. Everything after that — New Folder, Rename, Delete,
/// any folder not already listed — answered "not connected" until kiki was restarted.
///
/// The requests that carry no role of their own read the one the calling thread is serving
/// (`sdk::current_role`), exactly as the SFTP plugin does.
fn key(location: &str, role: &str) -> String {
    format!("{location}\u{0}{role}")
}

struct Gio {
    scheme: String,
    sessions: Mutex<HashMap<String, Session>>,
    ctx: glib::MainContext,
}

impl Gio {
    fn root(&self, location: &str) -> Result<String> {
        let k = key(location, &sdk::current_role());
        self.sessions.lock().unwrap().get(&k).map(|s| s.root.clone()).ok_or_else(|| PluginError::network("not connected"))
    }

    fn mount(&self, root: &str, config: &Value, secrets: &Value) -> Result<()> {
        match self.attempt(root, config, secrets) {
            Ok(()) => Ok(()),
            Err((e, refused_us)) => Err(self.mount_error(root, e, refused_us, config, secrets)),
        }
    }

    /// One go at mounting `root`, answering the backend's prompt from the location's own
    /// credentials. `true` beside the error means the server turned those credentials down.
    ///
    /// **The second prompt is the server saying no**, and answering it is what used to hang.
    /// gvfsd-smb asks again after every refusal and never gives up, so a handler that always
    /// replies with the same password loops for ever: the plugin never came back from `Connect`,
    /// the pane that opened the location waited out the daemon's two minutes for nothing, the
    /// plugin process was left spinning afterwards — and the server was being offered a bad
    /// password several times a second the whole time, which is a good way to have an account
    /// locked out. So the second ask is answered `Aborted`. GIO reports that as `FAILED_HANDLED`;
    /// what it means is `Auth`.
    fn attempt(&self, root: &str, config: &Value, secrets: &Value) -> std::result::Result<(), (glib::Error, bool)> {
        let file = gio::File::for_uri(root);
        let op = gio::MountOperation::new();
        let auth = cfg(config, "auth").to_string();
        let user = cfg(config, "username").to_string();
        let domain = cfg(config, "domain").to_string();
        let password = secrets.str_field("password").unwrap_or("").to_string();
        let asked = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let asks = asked.clone();
        op.connect_ask_password(move |op, _message, default_user, default_domain, _flags| {
            if asks.fetch_add(1, std::sync::atomic::Ordering::SeqCst) > 0 {
                op.reply(gio::MountOperationResult::Aborted);
                return;
            }
            match auth.as_str() {
                "guest" => op.set_anonymous(true),
                "kerberos" => {
                    // a ticket in the cache is used by the backend; just confirm the identity
                    op.set_username(Some(if user.is_empty() { default_user } else { &user }));
                }
                _ => {
                    op.set_anonymous(false);
                    let (u, d) = match user.split_once('\\') {
                        Some((d, u)) => (u.to_string(), d.to_string()),
                        None => (user.clone(), domain.clone()),
                    };
                    op.set_username(Some(if u.is_empty() { default_user } else { &u }));
                    op.set_domain(Some(if d.is_empty() { default_domain } else { &d }));
                    op.set_password(Some(&password));
                    op.set_password_save(gio::PasswordSave::Never);
                }
            }
            op.reply(gio::MountOperationResult::Handled);
        });
        let fut = file.mount_enclosing_volume_future(gio::MountMountFlags::NONE, Some(&op));
        match self.ctx.block_on(fut) {
            Ok(()) => Ok(()),
            Err(e) if e.kind::<gio::IOErrorEnum>() == Some(gio::IOErrorEnum::AlreadyMounted) => Ok(()),
            Err(e) => Err((e, asked.load(std::sync::atomic::Ordering::SeqCst) > 1)),
        }
    }

    /// Why a mount failed, in kiki's terms rather than in gvfs's.
    ///
    /// Plan 25 listed the `GIOErrorEnum` codes to map and **not one of them arrives**: gvfsd-smb
    /// fails every mount with plain `G_IO_ERROR_FAILED` and a sentence of its own — "Failed to
    /// mount Windows share: " and then `g_strerror` of libsmbclient's errno — so the code carries
    /// nothing and the message is one locale's rendering of an errno. Every mount failure used to
    /// come out as `Io` with that sentence: a share name with a typo in it said "No such file or
    /// directory" and named no field, and a NAS that was switched off said the same about a
    /// Windows share nobody had mentioned.
    ///
    /// So the two things plan 25 promised to tell apart are asked for instead of read off:
    /// the credentials, from whether the backend asked for them a second time (`refused_us`), and
    /// the share, by mounting the server's **own root** — `smb://host/`, its list of shares, which
    /// needs the same credentials over the same network. A server that answers that is a server
    /// that is there and has let us in, so it is the share that is not. One extra round trip, on
    /// the failure path only.
    fn mount_error(&self, root: &str, e: glib::Error, refused_us: bool, config: &Value, secrets: &Value) -> PluginError {
        if refused_us {
            return PluginError::auth(refused(cfg(config, "auth")));
        }
        if self.scheme == "smb" {
            // An SMB1-only server first: it fails as a connection error would.
            if smb1_only(e.message()) {
                return PluginError::network(SMB1_REFUSED);
            }
            let host = cfg(config, "host").trim().to_string();
            let server = format!("smb://{host}/");
            if !host.is_empty() && root != server && self.attempt(&server, config, secrets).is_ok() {
                let share = cfg(config, "share").trim();
                return PluginError::invalid("share", format!("{host} has no share called \"{share}\" — or this account is not allowed to see it"));
            }
            return PluginError::network(format!("{host}: {}", e.message()));
        }
        match e.kind::<gio::IOErrorEnum>() {
            Some(gio::IOErrorEnum::PermissionDenied) | Some(gio::IOErrorEnum::FailedHandled) => PluginError::auth(e.message()),
            Some(gio::IOErrorEnum::NotFound) => PluginError::invalid("host", e.message()),
            _ => gerr(e),
        }
    }

    fn entry(info: &gio::FileInfo) -> Entry {
        let name = info.name().to_string_lossy().into_owned();
        let kind = match info.file_type() {
            gio::FileType::Directory => Kind::Dir,
            gio::FileType::SymbolicLink => Kind::Link,
            gio::FileType::Regular => Kind::File,
            _ => Kind::Other,
        };
        let mtime_ms = info.modification_date_time().map(|d| (d.to_unix().max(0) as u64) * 1000 + (d.microsecond() as u64) / 1000).unwrap_or(0);
        let mode = if info.has_attribute("unix::mode") { Some(info.attribute_uint32("unix::mode") & 0o7777) } else { None };
        let owner = info.attribute_string("owner::user").map(|s| s.to_string());
        let group = info.attribute_string("owner::group").map(|s| s.to_string());
        // Asked for, but only answered when it is true: gvfsd-smb sets `standard::is-hidden` on
        // the files that carry the DOS Hidden attribute and leaves it off the rest, and
        // `is_hidden()` on an info that has not got it is two GLib CRITICALs — printed for every
        // ordinary file in every listing, into the daemon's log, and an abort outright under
        // `G_DEBUG=fatal-criticals`.
        let hidden = info.has_attribute("standard::is-hidden") && info.is_hidden();
        Entry { name, kind, meta: Some(Meta { hidden, size: info.size().max(0) as u64, mtime_ms, mode, owner, group }), rel: String::new() }
    }
}

impl Handler for Gio {
    fn describe(&self) -> Describe {
        let (display, form): (&'static str, Vec<Value>) = match self.scheme.as_str() {
            "dav" => (
                "WebDAV",
                vec![
                    sdk::field("name", "Name", "text", true, None),
                    sdk::field("host", "Host", "text", true, None),
                    sdk::field("port", "Port", "port", false, None),
                    sdk::select_field("security", "Security", &["https", "plain"], "https"),
                    sdk::field("prefix", "Path prefix", "text", false, Some("/")),
                    sdk::field("username", "Username", "text", false, None),
                    sdk::field("password", "Password", "password", false, None),
                    sdk::on_page("Locations", sdk::field("remotePath", "Remote path", "path", false, Some("/"))),
                    sdk::on_page("Locations", sdk::field("localPath", "Local path", "path", false, None)),
                ],
            ),
            "afp" => (
                "AFP",
                vec![
                    sdk::field("name", "Name", "text", true, None),
                    sdk::field("host", "Host", "browse", true, None),
                    sdk::field("volume", "Volume", "browse", true, None),
                    sdk::select_field("auth", "Authentication", &["password", "guest"], "password"),
                    sdk::field("username", "Username", "text", false, None),
                    sdk::field("password", "Password", "password", false, None),
                    sdk::on_page("Locations", sdk::field("remotePath", "Remote path", "path", false, Some("/"))),
                    sdk::on_page("Locations", sdk::field("localPath", "Local path", "path", false, None)),
                ],
            ),
            _ => (
                "SMB",
                vec![
                    sdk::field("name", "Name", "text", true, None),
                    sdk::field("host", "Host", "browse", true, None),
                    sdk::field("share", "Share", "browse", true, None),
                    sdk::select_field("auth", "Authentication", &["password", "kerberos", "guest"], "password"),
                    sdk::field("username", "Username", "text", false, None),
                    sdk::field("password", "Password", "password", false, None),
                    sdk::field("domain", "Domain / workgroup", "text", false, Some("WORKGROUP")),
                    sdk::on_page("Locations", sdk::field("remotePath", "Remote path", "path", false, Some("/"))),
                    sdk::on_page("Locations", sdk::field("localPath", "Local path", "path", false, None)),
                ],
            ),
        };
        let available = gio::Vfs::default().is_active() && gio::Vfs::default().supported_uri_schemes().iter().any(|s| s.as_str() == self.scheme || (self.scheme == "dav" && s.as_str() == "davs"));
        Describe {
            scheme: Box::leak(self.scheme.clone().into_boxed_str()),
            display_name: display,
            version: "0.1",
            form,
            defaults: Value::obj().done(),
            secret_fields: vec!["password"],
            detector_upload: "sizeMtime",
            detector_download: "sizeMtime",
            features: Features { set_mtime: true, mode: false, real_dirs: true, meta_in_scan: true, pipelining: false, partial_read: true },
            available: Some((available, if available { String::new() } else { format!("{display} needs GVfs and its {display} backend: install {}", if self.scheme == "dav" { "gvfs-dnssd" } else { "gvfs-smb" }) })),
        }
    }

    fn validate(&self, config: &Value) -> Result<()> {
        root_uri(&self.scheme, config).map(|_| ())
    }

    fn connect(&self, location: &str, role: &str, config: &Value, secrets: &Value) -> Result<Value> {
        let k = key(location, role);
        if self.sessions.lock().unwrap().contains_key(&k) {
            return Ok(Value::obj().v("fingerprint", Value::Null).v("banner", Value::Null).done());
        }
        let root = root_uri(&self.scheme, config)?;
        // A job's session mounts nothing new: the mount lives in gvfsd and is shared with the rest
        // of the desktop, so this is a round trip that comes back AlreadyMounted (plan 25).
        self.mount(&root, config, secrets)?;
        self.sessions.lock().unwrap().insert(k, Session { root });
        Ok(Value::obj().v("fingerprint", Value::Null).v("banner", Value::Null).done())
    }

    fn disconnect(&self, location: &str, role: &str) {
        // The mount stays for the rest of the desktop; forget this session only — and only this
        // one, or the browser loses its own when a job that was using the share ends.
        self.sessions.lock().unwrap().remove(&key(location, role));
    }

    fn capabilities(&self, _location: &str) -> Result<Value> {
        Ok(Value::obj().b("trash", false).b("setMtime", true).b("mode", false).b("realDirs", true).v("digestKind", Value::Null).s("separator", "/").s("fastScan", "gio").b("partialRead", true).done())
    }

    fn scan(&self, location: &str, path: &str, recursive: bool, sink: &mut dyn FnMut(Vec<Entry>)) -> Result<u64> {
        if recursive {
            return Err(PluginError::unsupported());
        }
        let root = self.root(location)?;
        let dir = file_for(&root, path);
        let en = dir.enumerate_children(ATTRS, gio::FileQueryInfoFlags::NOFOLLOW_SYMLINKS, gio::Cancellable::NONE).map_err(gerr)?;
        let mut n = 0u64;
        loop {
            if sdk::cancelled() {
                let _ = en.close(gio::Cancellable::NONE);
                return Err(sdk::cancel_error());
            }
            let infos = next_batch(&en, 512).map_err(gerr)?;
            if infos.is_empty() {
                break;
            }
            let batch: Vec<Entry> = infos.iter().map(Gio::entry).collect();
            n += batch.len() as u64;
            sink(batch);
        }
        let _ = en.close(gio::Cancellable::NONE);
        Ok(n)
    }

    fn stat(&self, location: &str, path: &str) -> Result<Meta> {
        let root = self.root(location)?;
        let info = file_for(&root, path).query_info(ATTRS, gio::FileQueryInfoFlags::NOFOLLOW_SYMLINKS, gio::Cancellable::NONE).map_err(gerr)?;
        Ok(Gio::entry(&info).meta.unwrap())
    }

    fn read(&self, location: &str, path: &str, offset: u64, out: &mut Outgoing) -> Result<()> {
        let root = self.root(location)?;
        let stream = file_for(&root, path).read(gio::Cancellable::NONE).map_err(gerr)?;
        if offset > 0 {
            stream.seek(offset as i64, glib::SeekType::Set, gio::Cancellable::NONE).map_err(gerr)?;
        }
        let mut buf = vec![0u8; 1024 * 1024];
        loop {
            if sdk::cancelled() {
                return Err(sdk::cancel_error());
            }
            let n = stream.read(&mut buf, gio::Cancellable::NONE).map_err(gerr)?;
            if n == 0 {
                break;
            }
            out.write_all(&buf[..n]).map_err(PluginError::io)?;
        }
        Ok(())
    }

    fn write(&self, location: &str, path: &str, mut args: WriteArgs) -> Result<u64> {
        let root = self.root(location)?;
        let f = file_for(&root, path);
        let stream = f.replace(None, false, gio::FileCreateFlags::NONE, gio::Cancellable::NONE).map_err(gerr)?;
        let mut buf = vec![0u8; 1024 * 1024];
        let mut total = 0u64;
        loop {
            if sdk::cancelled() {
                return Err(sdk::cancel_error());
            }
            let n = args.data.read(&mut buf).map_err(PluginError::io)?;
            if n == 0 {
                break;
            }
            stream.write_all(&buf[..n], gio::Cancellable::NONE).map_err(gerr)?;
            total += n as u64;
        }
        stream.close(gio::Cancellable::NONE).map_err(gerr)?;
        if let Some(t) = args.mtime_ms {
            let _ = f.set_attribute_uint64("time::modified", t / 1000, gio::FileQueryInfoFlags::NONE, gio::Cancellable::NONE);
        }
        Ok(total)
    }

    fn mkdir(&self, location: &str, path: &str) -> Result<()> {
        let root = self.root(location)?;
        file_for(&root, path).make_directory(gio::Cancellable::NONE).map_err(gerr)
    }

    fn rename(&self, location: &str, from: &str, to: &str) -> Result<()> {
        let root = self.root(location)?;
        file_for(&root, from).move_(&file_for(&root, to), gio::FileCopyFlags::NONE, gio::Cancellable::NONE, None).map_err(gerr)
    }

    fn delete(&self, location: &str, path: &str) -> Result<()> {
        let root = self.root(location)?;
        file_for(&root, path).delete(gio::Cancellable::NONE).map_err(gerr)
    }

    fn set_mtime(&self, location: &str, path: &str, mtime_ms: u64) -> Result<()> {
        let root = self.root(location)?;
        file_for(&root, path).set_attribute_uint64("time::modified", mtime_ms / 1000, gio::FileQueryInfoFlags::NONE, gio::Cancellable::NONE).map_err(gerr)
    }

    /// Host: everything GVfs found on the network; Share/Volume: the server's own list.
    fn browse(&self, field: &str, config: &Value, secrets: &Value) -> Result<Vec<(String, String)>> {
        let list = |uri: &str| -> Result<Vec<(String, String)>> {
            let en = gio::File::for_uri(uri).enumerate_children("standard::name,standard::display-name,standard::target-uri", gio::FileQueryInfoFlags::NONE, gio::Cancellable::NONE).map_err(gerr)?;
            let mut out = Vec::new();
            while let Ok(infos) = next_batch(&en, 64) {
                if infos.is_empty() {
                    break;
                }
                for i in &infos {
                    let label = i.display_name().to_string();
                    let value = i.attribute_string("standard::target-uri").map(|s| s.to_string()).unwrap_or_else(|| i.name().to_string_lossy().into_owned());
                    out.push((value, label));
                }
            }
            Ok(out)
        };
        match field {
            "host" => Ok(list("network:///")?
                .into_iter()
                .filter(|(v, _)| v.starts_with(&format!("{}://", self.scheme)) || v.starts_with("smb://"))
                .map(|(v, l)| (v.trim_start_matches(|c: char| c != ':').trim_start_matches("://").trim_end_matches('/').to_string(), l))
                .collect()),
            "share" | "volume" => {
                let host = cfg(config, "host").trim();
                if host.is_empty() {
                    return Err(PluginError::invalid("host", "fill in the host first"));
                }
                let uri = format!("{}://{host}/", self.scheme);
                // authenticate the server root once so the share list is not the guest view
                let _ = self.mount(&uri, config, secrets);
                Ok(list(&uri)?.into_iter().map(|(_, l)| (l.clone(), l)).filter(|(v, _)| !v.ends_with('$') && v != "IPC$").collect())
            }
            _ => Err(PluginError::unsupported()),
        }
    }
}

fn main() {
    let ctx = glib::MainContext::default();
    let h = Gio { scheme: scheme(), sessions: Mutex::new(HashMap::new()), ctx };
    if let Err(e) = sdk::run(&h) {
        eprintln!("kiki-plugin-gio: {e}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The first string is the one a real NT1-only `smbd` produces through gvfsd-smb — observed,
    /// not read out of Samba's source, and it is not what plan 25 predicted. The rest of the left
    /// column is the same failure as other servers and other gvfs versions render it. The right
    /// column is failures that are not about the protocol and must keep their own words: the point
    /// of asking this only of a mount is that a host which is simply not there must never send
    /// somebody looking for an SMB1 setting on a NAS that is switched off.
    #[test]
    fn an_smb1_only_server_is_recognised_and_nothing_else_is() {
        for m in [
            // `smbd` 4.24.6, `server max protocol = NT1`, through gvfsd-smb, 2026-09-21.
            "Failed to mount Windows share: Software caused connection abort",
            "Failed to mount Windows share: Connection reset by peer",
            "Failed to mount Windows share: Protocol negotiation to server nas (for a protocol between SMB2_02 and SMB3_11) failed: NT_STATUS_CONNECTION_RESET",
            "protocol negotiation failed: NT_STATUS_CONNECTION_DISCONNECTED",
            "Protocol negotiation to server NAS failed: NT_STATUS_INVALID_NETWORK_RESPONSE",
            "SMB1 negotiation is disabled",
            "negotiation refused: server offers NT1 only",
        ] {
            assert!(smb1_only(m), "{m}");
        }
        for m in [
            "Failed to mount Windows share: Connection refused",
            "Failed to mount Windows share: No route to host",
            "Failed to mount Windows share: Connection timed out",
            "Failed to mount Windows share: Permission denied",
            "Failed to mount Windows share: No such file or directory",
            "Failed to retrieve share list from server: Connection refused",
            "The specified location is not mounted",
            // The server is *called* smb1 and the name is all there is to go on: not enough.
            "Failed to mount Windows share: smb1.lan: Connection timed out",
            "",
        ] {
            assert!(!smb1_only(m), "{m}");
        }
    }

    /// The server offers no words of its own when it turns credentials down — gvfs says "Password
    /// dialog cancelled", which is about a dialog there never was — so the plugin finds its own,
    /// and they say which way in was refused.
    #[test]
    fn a_refusal_says_which_way_in_was_refused() {
        assert!(refused("password").contains("username and password"));
        assert!(refused("").contains("username and password"), "password is the default");
        assert!(refused("guest").contains("guest"));
        assert!(refused("kerberos").contains("Kerberos"));
    }

    /// One scheme in 0.1.0, whatever name the binary is given: `kiki-plugin-gio` run as itself
    /// is SMB, because that is the only name the package installs it under.
    #[test]
    fn the_scheme_is_the_name_the_binary_is_run_by() {
        assert_eq!(root_uri("smb", &Value::obj().s("host", "nas").s("share", "media").done()).unwrap(), "smb://nas/media/");
        assert!(root_uri("smb", &Value::obj().s("host", "nas").done()).is_err(), "a share is required");
        assert!(root_uri("nonsense", &Value::obj().s("host", "nas").done()).is_err());
    }
}
