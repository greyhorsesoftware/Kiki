//! Tailscale: Taildrop to online peers via `tailscale file cp`.
use kiki_plugin_sdk::json::{self, Value};
use kiki_plugin_sdk::{self as sdk, PluginError, Result, ShareDescribe, ShareHandler, ShareProgress, ShareResult, Target};
use std::process::Command;

struct Tailscale;

fn peers() -> Result<Vec<Target>> {
    if !sdk::detected("tailscale") {
        return Err(PluginError::new("Unsupported", "tailscale is not installed"));
    }
    let out = Command::new("tailscale").args(["status", "--json"]).output().map_err(PluginError::io)?;
    if !out.status.success() {
        return Err(PluginError::network("tailscale is not running or not logged in (try: tailscale up)"));
    }
    let v = json::parse(&out.stdout).map_err(|e| PluginError::io(e.to_string()))?;
    let mut targets = Vec::new();
    if let Some(Value::Obj(peers)) = v.get("Peer") {
        for p in peers.values() {
            let name = p.str_field("HostName").unwrap_or("").to_string();
            let dns = p.str_field("DNSName").unwrap_or("").trim_end_matches('.').to_string();
            let online = p.get("Online").and_then(Value::as_bool).unwrap_or(false);
            let os = p.str_field("OS").unwrap_or("").to_string();
            targets.push(Target { id: dns.split('.').next().unwrap_or(&name).to_string(), name, detail: os, online, icon: "server".into() });
        }
    }
    targets.sort_by(|a, b| b.online.cmp(&a.online).then(a.name.cmp(&b.name)));
    Ok(targets)
}

impl ShareHandler for Tailscale {
    fn describe(&self) -> ShareDescribe {
        ShareDescribe {
            id: "tailscale",
            name: "Tailscale",
            icon: "cloud",
            version: "1.0",
            accepts_files: true,
            accepts_folders: false,
            accepts_multiple: true,
            max_bytes: None,
            targets: "list",
            form: vec![],
            secret_fields: vec![],
            compose: vec![],
        }
    }
    fn targets(&mut self, _c: &Value, _s: &Value, query: Option<&str>) -> Result<Vec<Target>> {
        let mut t = peers()?;
        if let Some(q) = query {
            let q = q.to_lowercase();
            t.retain(|x| x.name.to_lowercase().contains(&q));
        }
        Ok(t)
    }
    fn share(&mut self, _c: &Value, _s: &Value, files: &[String], target: Option<&str>, _compose: &Value, p: &mut ShareProgress) -> Result<ShareResult> {
        let peer = target.ok_or_else(|| PluginError::new("Invalid", "pick a peer"))?;
        let total = files.len() as u64;
        let bytes_total: u64 = files.iter().filter_map(|f| std::fs::metadata(f).ok()).map(|m| m.len()).sum();
        let mut bytes = 0u64;
        for (i, f) in files.iter().enumerate() {
            p.report(i as u64, total, bytes, bytes_total, &format!("sending {f}"));
            let st = Command::new("tailscale").args(["file", "cp"]).arg(f).arg(format!("{peer}:")).status().map_err(PluginError::io)?;
            if !st.success() {
                return Err(PluginError::network(format!("tailscale file cp failed for {f}")));
            }
            bytes += std::fs::metadata(f).map(|m| m.len()).unwrap_or(0);
        }
        p.report(total, total, bytes, bytes_total, "sent");
        Ok(ShareResult { result: "sent", detail: Some(format!("{total} file(s) to {peer}")) })
    }
}

fn main() {
    let _ = sdk::run_share(&mut Tailscale);
}
