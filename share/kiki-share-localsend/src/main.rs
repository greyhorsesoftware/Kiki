//! LocalSend: the open nearby-transfer protocol. Discovery by multicast is implemented; the
//! HTTPS upload needs a TLS client, so this plugin hands off to the `localsend` CLI when
//! present and reports `Unsupported` otherwise (the in-plugin sender is a follow-up).
use kiki_plugin_sdk::json::{self, Value};
use kiki_plugin_sdk::{self as sdk, PluginError, Result, ShareDescribe, ShareHandler, ShareProgress, ShareResult, Target};
use std::net::UdpSocket;
use std::process::Command;
use std::time::{Duration, Instant};

struct LocalSend;

fn discover() -> Vec<Target> {
    let mut out = Vec::new();
    let Ok(sock) = UdpSocket::bind("0.0.0.0:53317") else { return out };
    let _ = sock.join_multicast_v4(&"224.0.0.167".parse().unwrap(), &"0.0.0.0".parse().unwrap());
    let _ = sock.set_read_timeout(Some(Duration::from_millis(300)));
    let announce = r#"{"alias":"kiki","version":"2.0","deviceModel":"Linux","deviceType":"desktop","fingerprint":"kiki","port":53317,"protocol":"https","download":false,"announce":true}"#;
    let _ = sock.send_to(announce.as_bytes(), "224.0.0.167:53317");
    let start = Instant::now();
    let mut buf = [0u8; 4096];
    while start.elapsed() < Duration::from_secs(2) {
        if let Ok((n, addr)) = sock.recv_from(&mut buf) {
            if let Ok(v) = json::parse(&buf[..n]) {
                let alias = v.str_field("alias").unwrap_or("").to_string();
                if alias.is_empty() || alias == "kiki" { continue; }
                let model = v.str_field("deviceModel").unwrap_or("").to_string();
                out.push(Target { id: format!("{}:{}", addr.ip(), v.u64_field("port").unwrap_or(53317)), name: alias, detail: model, online: true, icon: "phone".into() });
            }
        }
    }
    out.dedup_by(|a, b| a.id == b.id);
    out
}

impl ShareHandler for LocalSend {
    fn describe(&self) -> ShareDescribe {
        ShareDescribe { id: "localsend", name: "LocalSend", icon: "share", version: "0.5", accepts_files: true, accepts_folders: false, accepts_multiple: true, max_bytes: None, targets: "list", form: vec![], secret_fields: vec![], compose: vec![sdk::field("pin", "PIN (if the receiver asks)", "text", false, None)] }
    }
    fn targets(&mut self, _c: &Value, _s: &Value, _q: Option<&str>) -> Result<Vec<Target>> {
        Ok(discover())
    }
    fn share(&mut self, _c: &Value, _s: &Value, files: &[String], target: Option<&str>, compose: &Value, p: &mut ShareProgress) -> Result<ShareResult> {
        let t = target.ok_or_else(|| PluginError::new("Invalid", "pick a device"))?;
        if !sdk::detected("localsend") {
            return Err(PluginError::new("Unsupported", "the in-plugin LocalSend sender is not built yet; install the localsend CLI"));
        }
        let host = t.split(':').next().unwrap_or(t);
        let mut cmd = Command::new("localsend");
        cmd.args(["send", "--to", host]);
        if let Some(pin) = compose.str_field("pin") { if !pin.is_empty() { cmd.args(["--pin", pin]); } }
        for f in files { cmd.arg(f); }
        p.report(0, files.len() as u64, 0, 0, "sending");
        let st = cmd.status().map_err(PluginError::io)?;
        if !st.success() { return Err(PluginError::network("localsend failed")); }
        p.report(files.len() as u64, files.len() as u64, 0, 0, "sent");
        Ok(ShareResult { result: "sent", detail: None })
    }
}

fn main() { let _ = sdk::run_share(&mut LocalSend); }
