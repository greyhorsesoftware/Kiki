//! LocalSend (plan 18): the open nearby-transfer protocol, v2. Discovery by multicast announce;
//! sending is the protocol's own two-step HTTPS exchange (prepare-upload, then one upload per
//! file with progress), implemented here without any external tool.
mod http;
mod identity;

use kiki_plugin_sdk::json::{self, Value};
use kiki_plugin_sdk::{self as sdk, PluginError, Result, ShareDescribe, ShareHandler, ShareProgress, ShareResult, Target};
use std::net::UdpSocket;
use std::time::{Duration, Instant};

const PORT: u16 = 53317;
const MULTICAST: &str = "224.0.0.167";

struct LocalSend;

/// What this machine announces itself as: the SHA-256 of the certificate it will present when
/// it sends (`identity`). A receiver compares the two. If no certificate can be made the
/// announce still needs a stable id, and the send will say what went wrong.
fn fingerprint() -> String {
    match identity::get() {
        Ok(me) => me.fingerprint(),
        Err(_) => {
            let host = std::fs::read_to_string("/etc/hostname").unwrap_or_else(|_| "kiki".into());
            ring::digest::digest(&ring::digest::SHA256, format!("kiki:{}", host.trim()).as_bytes()).as_ref().iter().map(|b| format!("{b:02x}")).collect()
        }
    }
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

/// The protocol's port, shared. Omarchy ships the LocalSend app, and while it runs it holds this
/// port: a plain bind fails, and discovery found nothing at all, silently. Replies to a multicast
/// announce are multicast too, so every socket bound here with SO_REUSEADDR gets them.
fn discovery_socket() -> Option<UdpSocket> {
    use socket2::{Domain, Protocol, Socket, Type};
    let s = Socket::new(Domain::IPV4, Type::DGRAM, Some(Protocol::UDP)).ok()?;
    s.set_reuse_address(true).ok()?;
    let _ = s.set_reuse_port(true);
    let addr: std::net::SocketAddr = ([0, 0, 0, 0], PORT).into();
    s.bind(&addr.into()).ok()?;
    Some(s.into())
}

/// Two ways at once, for two seconds: announce on the multicast group and listen for answers,
/// and ask every address on the subnet directly (`scan_subnet`, which says why both).
fn discover() -> Vec<Target> {
    let mut out: Vec<Target> = Vec::new();
    let me = fingerprint();
    let scanned = scan_subnet(&me);
    let start = Instant::now();
    let window = Duration::from_secs(2);
    // Without the socket there is still the scan.
    if let Some(sock) = discovery_socket() {
        let _ = sock.join_multicast_v4(&MULTICAST.parse().unwrap(), &"0.0.0.0".parse().unwrap());
        let _ = sock.set_read_timeout(Some(Duration::from_millis(300)));
        let mut announce = info_json(false);
        if let Value::Obj(m) = &mut announce {
            m.insert("announce".into(), Value::Bool(true));
        }
        let _ = sock.send_to(json::to_string(&announce).as_bytes(), (MULTICAST, PORT));
        let mut buf = [0u8; 4096];
        while start.elapsed() < window {
            let Ok((n, addr)) = sock.recv_from(&mut buf) else { continue };
            let Ok(v) = json::parse(&buf[..n]) else { continue };
            if v.str_field("fingerprint") == Some(me.as_str()) {
                continue;
            }
            if let Some(t) = target_from(&v, addr.ip()) {
                if !out.iter().any(|o| o.id == t.id) {
                    out.push(t);
                }
            }
        }
    }
    std::thread::sleep(window.saturating_sub(start.elapsed()));
    // What the scan found that multicast did not. Same device, same id: listed once.
    for t in scanned.try_iter() {
        if !out.iter().any(|o| o.id == t.id) {
            out.push(t);
        }
    }
    out
}

/// A device's own description of itself, from wherever it came, as a target. The fingerprint it
/// gave rides in the id, so the send can hold whoever answers at this address to it (see
/// `http::Pinned`).
fn target_from(v: &Value, ip: std::net::IpAddr) -> Option<Target> {
    let alias = v.str_field("alias").filter(|a| !a.is_empty())?.to_string();
    let proto = v.str_field("protocol").unwrap_or("https");
    let port = v.u64_field("port").unwrap_or(PORT as u64);
    let address = match ip {
        std::net::IpAddr::V6(ip) => format!("{proto}://[{ip}]:{port}"),
        std::net::IpAddr::V4(ip) => format!("{proto}://{ip}:{port}"),
    };
    let id = match v.str_field("fingerprint").filter(|f| proto != "http" && http::parse_fingerprint(f).is_some()) {
        Some(f) => format!("{address}#{f}"),
        None => address,
    };
    let ty = v.str_field("deviceType").unwrap_or("");
    Some(Target { id, name: alias, detail: v.str_field("deviceModel").unwrap_or("").to_string(), online: true, icon: if ty == "mobile" { "phone".into() } else { "hdd".into() } })
}

/// Is a LocalSend device at this address? HTTPS first — what the app speaks unless its user
/// turned encryption off — then plain HTTP. A refusal is instant; silence costs `within`.
fn probe(ip: std::net::IpAddr, port: u16, me: &str, body: &str) -> Option<Target> {
    let within = Duration::from_millis(900);
    for https in [true, false] {
        let Ok(r) = http::ask(&ip.to_string(), port, https, "/api/localsend/v2/register", body, within) else { continue };
        if r.status != 200 {
            continue;
        }
        let Ok(mut v) = json::parse(&r.body) else { continue };
        if v.str_field("fingerprint").map(|f| f.eq_ignore_ascii_case(me)).unwrap_or(false) {
            return None;
        }
        // The answer does not say how it was reached; we know, we just reached it.
        if let Value::Obj(m) = &mut v {
            m.insert("protocol".into(), Value::Str(if https { "https" } else { "http" }.into()));
            m.insert("port".into(), Value::Uint(port as u64));
        }
        return target_from(&v, ip);
    }
    None
}

/// This machine's address on the network it would reach the devices by. No packet is sent:
/// connecting a UDP socket only picks the route.
fn local_ipv4() -> Option<std::net::Ipv4Addr> {
    let s = UdpSocket::bind(("0.0.0.0", 0)).ok()?;
    s.connect((MULTICAST, PORT)).ok()?;
    match s.local_addr().ok()?.ip() {
        std::net::IpAddr::V4(ip) if !ip.is_loopback() && !ip.is_unspecified() => Some(ip),
        _ => None,
    }
}

/// Ask every address beside ours whether a LocalSend device is there — the protocol's own
/// fallback (`POST /register`), and what the app itself does when multicast finds nothing. It
/// is needed more often than it sounds:
/// - many Wi-Fi access points do not pass multicast between clients at all;
/// - a device answers an announce by POSTing to the announcer's port 53317, and only falls back
///   to a multicast reply when that fails. On a machine where the LocalSend app is running — it
///   ships with Omarchy — the app takes that POST, it succeeds, and kiki hears nothing.
///
/// Outbound connections need nothing from the firewall. The same /24 the app assumes.
fn scan_subnet(me: &str) -> std::sync::mpsc::Receiver<Target> {
    let (tx, rx) = std::sync::mpsc::channel();
    let Some(mine) = local_ipv4() else { return rx };
    let body = json::to_string(&info_json(false));
    let [a, b, c, _] = mine.octets();
    for last in 1..=254u8 {
        let ip = std::net::Ipv4Addr::new(a, b, c, last);
        if ip == mine {
            continue;
        }
        let (tx, body, me) = (tx.clone(), body.clone(), me.to_string());
        std::thread::spawn(move || {
            if let Some(t) = probe(ip.into(), PORT, &me, &body) {
                let _ = tx.send(t);
            }
        });
    }
    rx
}

/// "address#fingerprint" -> (address, fingerprint). A discovered device's id carries the
/// fingerprint it announced; an address typed by hand has none, unless the user adds one. One
/// that is there but is not a fingerprint is an error, not "none": silently dropping it would
/// turn a pinned send into an unpinned one.
fn split_fingerprint(t: &str) -> Result<(&str, Option<http::Fingerprint>)> {
    match t.split_once('#') {
        None => Ok((t, None)),
        Some((address, f)) => match http::parse_fingerprint(f) {
            Some(p) => Ok((address, Some(p))),
            None => Err(PluginError::new("Invalid", "the fingerprint after # is not 64 hex digits")),
        },
    }
}

/// "https://host:port" -> (https, host, port)
fn parse_target(t: &str) -> Result<(bool, String, u16)> {
    let (proto, rest) = t.split_once("://").unwrap_or(("https", t));
    // `[fe80::1]:53317` is the only unambiguous way to write an address with colons in it; a bare
    // one is all host, or the last group would be read as a port.
    let (host, port) = if let Some(rest) = rest.strip_prefix('[') {
        match rest.split_once(']') {
            Some((h, after)) => (h, after.strip_prefix(':').and_then(|p| p.parse().ok()).unwrap_or(PORT)),
            None => (rest, PORT),
        }
    } else if rest.matches(':').count() > 1 {
        (rest, PORT)
    } else {
        match rest.rsplit_once(':') {
            Some((h, p)) if !p.is_empty() && p.chars().all(|c| c.is_ascii_digit()) => (h, p.parse().unwrap_or(PORT)),
            _ => (rest, PORT),
        }
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
            requires: vec![],
            // Off until it has sent to a real phone (29, section D): discovery and the handshake are
            // checked against the desktop app only.
            off_by_default: true,
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
        let (address, device) = split_fingerprint(target.ok_or_else(|| PluginError::new("Invalid", "pick a device"))?)?;
        let (https, host, port) = parse_target(address)?;
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
        let r = http::post(&host, port, https, device.as_ref(), &format!("/api/localsend/v2/prepare-upload{query}"), "application/json", &mut body.as_bytes(), body.len() as u64, |_| {})?;
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
                let _ = http::post(&host, port, https, device.as_ref(), &format!("/api/localsend/v2/cancel?sessionId={session}"), "application/json", &mut &b""[..], 0, |_| {});
                return Err(sdk::cancel_error());
            }
            let mut f = std::fs::File::open(path).map_err(PluginError::io)?;
            let name = std::path::Path::new(path).file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
            let mut sent_here = 0u64;
            let r = http::post(&host, port, https, device.as_ref(), &format!("/api/localsend/v2/upload?sessionId={session}&fileId={id}&token={token}"), "application/octet-stream", &mut f, *size, |n| {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_target_may_name_its_protocol_host_and_port() {
        assert_eq!(parse_target("http://192.168.1.5:53317").unwrap(), (false, "192.168.1.5".into(), 53317));
        assert_eq!(parse_target("https://phone.local:1234").unwrap(), (true, "phone.local".into(), 1234));
    }

    #[test]
    fn a_bare_host_is_https_on_the_default_port() {
        assert_eq!(parse_target("phone.local").unwrap(), (true, "phone.local".into(), PORT));
    }

    #[test]
    fn an_address_with_colons_in_it_is_all_host() {
        let (secure, host, port) = parse_target("fe80::1").unwrap();
        assert!(secure);
        assert_eq!(host, "fe80::1");
        assert_eq!(port, PORT);
    }

    #[test]
    fn brackets_are_how_such_an_address_names_a_port() {
        assert_eq!(parse_target("http://[fe80::1]:53317").unwrap(), (false, "fe80::1".into(), 53317));
        assert_eq!(parse_target("[fe80::1]").unwrap(), (true, "fe80::1".into(), PORT));
    }

    #[test]
    fn a_fingerprint_rides_after_a_hash_and_a_bad_one_is_refused() {
        let f = "ab".repeat(32);
        let id = format!("https://[fe80::1]:53317#{f}");
        let (address, pin) = split_fingerprint(&id).unwrap();
        assert_eq!(address, "https://[fe80::1]:53317");
        assert_eq!(pin, Some([0xab; 32]));
        assert_eq!(parse_target(address).unwrap(), (true, "fe80::1".into(), 53317));
        assert_eq!(split_fingerprint("phone.local").unwrap(), ("phone.local", None));
        assert!(split_fingerprint("https://phone.local#abc").is_err(), "not dropped: refused");
        assert!(http::parse_fingerprint(&"zz".repeat(32)).is_none());
    }

    /// One plain-HTTP answer to `POST /register`, the way the app answers it: no protocol, no port.
    fn registrar(reply: &'static str) -> u16 {
        use std::io::{Read, Write};
        // The HTTPS attempt makes this machine's certificate: not in the developer's own cache.
        std::env::set_var("XDG_CACHE_HOME", std::env::temp_dir().join(format!("kiki-localsend-unit-{}", std::process::id())));
        let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = l.local_addr().unwrap().port();
        std::thread::spawn(move || {
            // The HTTPS attempt comes first and is not HTTP: drop it, answer the second.
            for mut c in l.incoming().flatten() {
                // Head and body arrive as two writes: read until the sender goes quiet, or an
                // answer sent between them is a reset to a client that is still writing.
                let _ = c.set_read_timeout(Some(Duration::from_millis(150)));
                let (mut buf, mut n) = ([0u8; 4096], 0);
                while let Ok(k) = c.read(&mut buf[n..]) {
                    if k == 0 {
                        break;
                    }
                    n += k;
                }
                if buf[..n].starts_with(b"POST /api/localsend/v2/register") {
                    let _ = write!(c, "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{reply}", reply.len());
                }
            }
        });
        port
    }

    #[test]
    fn an_address_that_answers_register_is_a_target() {
        let port = registrar(r#"{"alias":"Good Apple","version":"2.2","deviceModel":"Pixel","deviceType":"mobile","fingerprint":"1D77693C90628E7C59AB2FA4227DBD5F89509777B19E99CB5B1F761AD5900FEE","download":false}"#);
        let t = probe("127.0.0.1".parse().unwrap(), port, &"00".repeat(32), "{}").expect("found");
        assert_eq!((t.name.as_str(), t.detail.as_str(), t.icon.as_str()), ("Good Apple", "Pixel", "phone"));
        // Reached over plain HTTP: there is no certificate to hold it to, so no fingerprint rides along.
        assert_eq!(t.id, format!("http://127.0.0.1:{port}"));
    }

    #[test]
    fn we_are_not_our_own_target_and_a_closed_port_is_nobody() {
        let port = registrar(r#"{"alias":"kiki","fingerprint":"ABABABABABABABABABABABABABABABABABABABABABABABABABABABABABABABAB"}"#);
        assert!(probe("127.0.0.1".parse().unwrap(), port, &"ab".repeat(32), "{}").is_none(), "upper case or lower, it is us");
        let closed = std::net::TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port();
        assert!(probe("127.0.0.1".parse().unwrap(), closed, "x", "{}").is_none());
    }

    #[test]
    fn a_device_found_over_https_carries_the_fingerprint_it_gave() {
        let v = json::parse(r#"{"alias":"Mac","protocol":"https","port":53317,"fingerprint":"1D77693C90628E7C59AB2FA4227DBD5F89509777B19E99CB5B1F761AD5900FEE"}"#.as_bytes()).unwrap();
        let t = target_from(&v, "192.168.1.20".parse().unwrap()).unwrap();
        assert_eq!(t.id, "https://192.168.1.20:53317#1D77693C90628E7C59AB2FA4227DBD5F89509777B19E99CB5B1F761AD5900FEE");
        let (address, pin) = split_fingerprint(&t.id).unwrap();
        assert_eq!(parse_target(address).unwrap(), (true, "192.168.1.20".into(), 53317));
        assert!(pin.is_some());
        assert!(target_from(&json::parse(br#"{"alias":""}"#).unwrap(), "10.0.0.1".parse().unwrap()).is_none());
    }

    #[test]
    fn discovery_shares_the_port_with_whoever_already_holds_it() {
        // The LocalSend app, or a second kiki window asking at the same moment.
        let first = discovery_socket().expect("the first socket");
        let second = discovery_socket().expect("and a second, on the same port");
        assert_eq!(first.local_addr().unwrap().port(), PORT);
        assert_eq!(second.local_addr().unwrap().port(), PORT);
    }

    #[test]
    fn an_empty_target_is_refused() {
        assert!(parse_target("https://").is_err());
    }

    #[test]
    fn the_type_comes_from_the_extension_and_falls_back_to_bytes() {
        assert_eq!(file_type("holiday.JPG"), "image/jpeg");
        assert_eq!(file_type("notes.md"), "text/plain");
        assert_eq!(file_type("archive.zip"), "application/zip");
        assert_eq!(file_type("no-extension"), "application/octet-stream");
    }

    #[test]
    fn the_receivers_refusals_are_told_apart() {
        assert_eq!(status_error(401, true).code, "Invalid");      // asks for a PIN
        assert_eq!(status_error(401, true).field.as_deref(), Some("pin"));
        assert_eq!(status_error(403, false).code, "Cancelled");   // declined
        assert_eq!(status_error(409, false).code, "Busy");
        assert_eq!(status_error(500, false).code, "Network");
    }
}
