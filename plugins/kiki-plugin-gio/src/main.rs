//! GIO/GVfs location plugin (plan 25): SMB, WebDAV and AFP through the GVfs daemons Omarchy
//! already runs. One binary, installed as `kiki-plugin-smb`, `kiki-plugin-dav` and
//! `kiki-plugin-afp`; the scheme comes from the executable's name.
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

struct Session {
    root: String,
}

struct Gio {
    scheme: String,
    sessions: Mutex<HashMap<String, Session>>,
    ctx: glib::MainContext,
}

impl Gio {
    fn root(&self, location: &str) -> Result<String> {
        self.sessions.lock().unwrap().get(location).map(|s| s.root.clone()).ok_or_else(|| PluginError::network("not connected"))
    }

    fn mount(&self, root: &str, config: &Value, secrets: &Value) -> Result<()> {
        let file = gio::File::for_uri(root);
        let op = gio::MountOperation::new();
        let auth = cfg(config, "auth").to_string();
        let user = cfg(config, "username").to_string();
        let domain = cfg(config, "domain").to_string();
        let password = secrets.str_field("password").unwrap_or("").to_string();
        op.connect_ask_password(move |op, _message, default_user, default_domain, _flags| {
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
            Err(e) => Err(match e.kind::<gio::IOErrorEnum>() {
                Some(gio::IOErrorEnum::PermissionDenied) | Some(gio::IOErrorEnum::FailedHandled) => PluginError::auth(e.message()),
                Some(gio::IOErrorEnum::NotFound) => PluginError::invalid(if self.scheme == "smb" { "share" } else { "host" }, e.message()),
                _ => gerr(e),
            }),
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
        Entry { name, kind, meta: Some(Meta { hidden: info.is_hidden(), size: info.size().max(0) as u64, mtime_ms, mode, owner, group }), rel: String::new() }
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

    fn connect(&self, location: &str, _role: &str, config: &Value, secrets: &Value) -> Result<Value> {
        if self.sessions.lock().unwrap().contains_key(location) {
            return Ok(Value::obj().v("fingerprint", Value::Null).v("banner", Value::Null).done());
        }
        let root = root_uri(&self.scheme, config)?;
        self.mount(&root, config, secrets)?;
        self.sessions.lock().unwrap().insert(location.to_string(), Session { root });
        Ok(Value::obj().v("fingerprint", Value::Null).v("banner", Value::Null).done())
    }

    fn disconnect(&self, location: &str, _role: &str) {
        // The mount stays for the rest of the desktop; forget our session only.
        self.sessions.lock().unwrap().remove(location);
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
