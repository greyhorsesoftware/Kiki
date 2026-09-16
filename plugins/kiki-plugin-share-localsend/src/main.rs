//! LocalSend (plan 18): the open nearby-transfer protocol, v2. Discovery by multicast announce;
//! sending is the protocol's own two-step HTTPS exchange (prepare-upload, then one upload per
//! file with progress), implemented here without any external tool.
mod http;

use kiki_plugin_sdk::json::{self, Value};
use kiki_plugin_sdk::{self as sdk, PluginError, Result, ShareDescribe, ShareHandler, ShareProgress, ShareResult, Target};
use std::net::UdpSocket;
use std::time::{Duration, Instant};

const PORT: u16 = 53317;
const MULTICAST: &str = "224.0.0.167";

struct LocalSend;

fn fingerprint() -> String {
    // A stable per-machine id is all the protocol needs from a sender without a certificate.
    let host = std::fs::read_to_string("/etc/hostname").unwrap_or_else(|_| "kiki".into());
    let d = ring::digest::digest(&ring::digest::SHA256, format!("kiki:{}", host.trim()).as_bytes());
    d.as_ref().iter().map(|b| format!("{b:02x}")).collect()
}

fn info_json(download: bool) -> Value {
    Value::obj()
        .s("alias", "kiki")
        .s("version", "2.1")
        .s("deviceModel", "Linux")
        .s("deviceType", "desktop")
        .s("fingerprint", fingerprint())
        .u("port", PORT as u64)
        .s("protocol", "http")
        .b("download", download)
        .done()
}

/// Announce on the multicast group and collect replies for two seconds.
fn discover() -> Vec<Target> {
    let mut out: Vec<Target> = Vec::new();
    let Ok(sock) = UdpSocket::bind(("0.0.0.0", PORT)) else { return out };
    let _ = sock.join_multicast_v4(&MULTICAST.parse().unwrap(), &"0.0.0.0".parse().unwrap());
    let _ = sock.set_read_timeout(Some(Duration::from_millis(300)));
    let mut announce = info_json(false);
    if let Value::Obj(m) = &mut announce {
        m.insert("announce".into(), Value::Bool(true));
    }
    let _ = sock.send_to(json::to_string(&announce).as_bytes(), (MULTICAST, PORT));
    let start = Instant::now();
    let mut buf = [0u8; 4096];
    let me = fingerprint();
    while start.elapsed() < Duration::from_secs(2) {
        if let Ok((n, addr)) = sock.recv_from(&mut buf) {
            if let Ok(v) = json::parse(&buf[..n]) {
                let alias = v.str_field("alias").unwrap_or("").to_string();
                if alias.is_empty() || v.str_field("fingerprint") == Some(me.as_str()) {
                    continue;
                }
                let proto = v.str_field("protocol").unwrap_or("https");
                let port = v.u64_field("port").unwrap_or(PORT as u64);
                let id = format!("{proto}://{}:{port}", addr.ip());
                if out.iter().any(|t| t.id == id) {
                    continue;
                }
                let ty = v.str_field("deviceType").unwrap_or("");
                out.push(Target { id, name: alias, detail: v.str_field("deviceModel").unwrap_or("").to_string(), online: true, icon: if ty == "mobile" { "phone".into() } else { "hdd".into() } });
            }
        }
    }
    out
}

/// "https://host:port" -> (https, host, port)
fn parse_target(t: &str) -> Result<(bool, String, u16)> {
    let (proto, rest) = t.split_once("://").unwrap_or(("https", t));
    let (host, port) = match rest.rsplit_once(':') {
        Some((h, p)) if p.chars().all(|c| c.is_ascii_digit()) => (h, p.parse().unwrap_or(PORT)),
        _ => (rest, PORT),
    };
    if host.is_empty() {
        return Err(PluginError::new("Invalid", "pick a device"));
    }
    Ok((proto != "http", host.to_string(), port))
}

fn file_type(name: &str) -> &'static str {
    match name.rsplit('.').next().unwrap_or("").to_ascii_lowercase().as_str() {
        "jpg" | "jpeg" => "image/jpeg",
        "png" => "image/png",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "mp4" => "video/mp4",
        "mov" => "video/quicktime",
        "pdf" => "application/pdf",
        "txt" | "md" => "text/plain",
        "zip" => "application/zip",
        _ => "application/octet-stream",
    }
}

fn status_error(status: u16, field_pin: bool) -> PluginError {
    match status {
        401 if field_pin => PluginError::invalid("pin", "the receiver asks for a PIN"),
        401 => PluginError::invalid("pin", "wrong PIN"),
        403 => PluginError::new("Cancelled", "the receiver declined"),
        409 => PluginError::new("Busy", "the receiver is busy with another transfer"),
        429 => PluginError::new("Busy", "too many requests; the receiver blocked us for now"),
        s => PluginError::network(format!("receiver answered {s}")),
    }
}

impl ShareHandler for LocalSend {
    fn describe(&self) -> ShareDescribe {
        ShareDescribe {
            id: "localsend",
            name: "LocalSend",
            icon: "share",
            version: "1.0",
            accepts_files: true,
            accepts_folders: false,
            accepts_multiple: true,
            max_bytes: None,
            targets: "list",
            form: vec![],
            secret_fields: vec![],
            compose: vec![sdk::field("pin", "PIN (if the receiver asks)", "text", false, None)],
        }
    }

    fn targets(&mut self, _c: &Value, _s: &Value, query: Option<&str>) -> Result<Vec<Target>> {
        // A typed "host" or "host:port" is a manual target for receivers that do not answer multicast.
        if let Some(q) = query.map(str::trim).filter(|q| !q.is_empty() && !q.contains(' ')) {
            if q.contains('.') || q.contains(':') {
                let id = if q.contains("://") { q.to_string() } else { format!("https://{q}") };
                return Ok(vec![Target { id: id.clone(), name: q.to_string(), detail: "manual".into(), online: true, icon: "hdd".into() }]);
            }
        }
        Ok(discover())
    }

    fn share(&mut self, _c: &Value, _s: &Value, files: &[String], target: Option<&str>, compose: &Value, p: &mut ShareProgress) -> Result<ShareResult> {
        let (https, host, port) = parse_target(target.ok_or_else(|| PluginError::new("Invalid", "pick a device"))?)?;
        let pin = compose.str_field("pin").unwrap_or("").trim().to_string();
        let query = if pin.is_empty() { String::new() } else { format!("?pin={}", sdk::shell_quote(&pin).trim_matches('\'')) };
        // 1. prepare-upload: describe every file; the receiver answers with a session and one token per accepted file
        let mut meta = Vec::new();
        let mut files_obj = Value::obj();
        let mut total = 0u64;
        for (i, f) in files.iter().enumerate() {
            let md = std::fs::metadata(f).map_err(PluginError::io)?;
            let name = std::path::Path::new(f).file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| "file".into());
            let id = format!("f{i}");
            files_obj = files_obj.v(&id, Value::obj().s("id", id.clone()).s("fileName", name.clone()).u("size", md.len()).s("fileType", file_type(&name)).done());
            total += md.len();
            meta.push((id, f.clone(), md.len()));
        }
        let body = json::to_string(&Value::obj().v("info", info_json(false)).v("files", files_obj.done()).done());
        p.report(0, meta.len() as u64, 0, total, "waiting for the receiver to accept");
        let r = http::post(&host, port, https, &format!("/api/localsend/v2/prepare-upload{query}"), "application/json", &mut body.as_bytes(), body.len() as u64, |_| {})?;
        if r.status == 204 {
            return Ok(ShareResult { result: "sent", detail: Some("the receiver accepted nothing".into()) });
        }
        if r.status != 200 {
            return Err(status_error(r.status, pin.is_empty()));
        }
        let accepted = json::parse(&r.body).map_err(|_| PluginError::network("bad prepare-upload reply"))?;
        let session = accepted.str_field("sessionId").unwrap_or("").to_string();
        let tokens = accepted.get("files").cloned().unwrap_or(Value::Null);
        // 2. one upload per accepted file
        let mut done_files = 0u64;
        let mut done_bytes = 0u64;
        let mut skipped = 0;
        for (id, path, size) in &meta {
            let Some(token) = tokens.str_field(id) else {
                skipped += 1;
                continue;
            };
            if sdk::cancelled() {
                let _ = http::post(&host, port, https, &format!("/api/localsend/v2/cancel?sessionId={session}"), "application/json", &mut &b""[..], 0, |_| {});
                return Err(sdk::cancel_error());
            }
            let mut f = std::fs::File::open(path).map_err(PluginError::io)?;
            let name = std::path::Path::new(path).file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
            let mut sent_here = 0u64;
            let r = http::post(&host, port, https, &format!("/api/localsend/v2/upload?sessionId={session}&fileId={id}&token={token}"), "application/octet-stream", &mut f, *size, |n| {
                sent_here += n;
                p.report(done_files, meta.len() as u64, done_bytes + sent_here, total, &format!("sending {name}"));
            })?;
            if r.status != 200 {
                return Err(status_error(r.status, false));
            }
            done_files += 1;
            done_bytes += size;
            p.report(done_files, meta.len() as u64, done_bytes, total, "sent");
        }
        Ok(ShareResult { result: "sent", detail: if skipped > 0 { Some(format!("{skipped} file(s) not accepted by the receiver")) } else { None } })
    }
}

fn main() {
    let _ = sdk::run_share(&mut LocalSend);
}
