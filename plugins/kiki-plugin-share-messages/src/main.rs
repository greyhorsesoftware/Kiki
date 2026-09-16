//! Messages: KDE Connect devices (share to a phone) and Signal contacts (signal-cli).
use kiki_plugin_sdk::json::Value;
use kiki_plugin_sdk::{self as sdk, PluginError, Result, ShareDescribe, ShareHandler, ShareProgress, ShareResult, Target};
use std::process::Command;

struct Messages;

fn kdeconnect_devices() -> Vec<Target> {
    if !sdk::detected("kdeconnect-cli") {
        return Vec::new();
    }
    let out = Command::new("kdeconnect-cli").args(["-l", "--id-name-only"]).output().ok();
    let mut v = Vec::new();
    if let Some(o) = out {
        for line in String::from_utf8_lossy(&o.stdout).lines() {
            let mut it = line.splitn(2, ' ');
            if let (Some(id), Some(name)) = (it.next(), it.next()) {
                let online = Command::new("kdeconnect-cli").args(["-d", id, "--ping"]).output().map(|o| o.status.success()).unwrap_or(false);
                v.push(Target { id: format!("kdeconnect:{id}"), name: name.to_string(), detail: "KDE Connect".into(), online, icon: "phone".into() });
            }
        }
    }
    v
}

fn signal_contacts(config: &Value) -> Vec<Target> {
    if !sdk::detected("signal-cli") {
        return Vec::new();
    }
    let account = config.str_field("signalAccount").unwrap_or("");
    if account.is_empty() {
        return Vec::new();
    }
    let out = Command::new("signal-cli").args(["-a", account, "listContacts"]).output().ok();
    let mut v = Vec::new();
    if let Some(o) = out {
        for line in String::from_utf8_lossy(&o.stdout).lines() {
            // "Number: +1555…  Name: Alice …"
            let num = line.split_whitespace().nth(1).unwrap_or("");
            let name = line.split("Name:").nth(1).map(|s| s.trim().split("  ").next().unwrap_or("").to_string()).unwrap_or_default();
            if num.starts_with('+') {
                v.push(Target { id: format!("signal:{num}"), name: if name.is_empty() { num.to_string() } else { name }, detail: "Signal".into(), online: true, icon: "message".into() });
            }
        }
    }
    v
}

impl ShareHandler for Messages {
    fn describe(&self) -> ShareDescribe {
        ShareDescribe {
            id: "messages",
            name: "Messages",
            icon: "message",
            version: "1.0",
            accepts_files: true,
            accepts_folders: false,
            accepts_multiple: true,
            max_bytes: None,
            targets: "list",
            form: vec![sdk::field("signalAccount", "Signal account (phone number linked to signal-cli)", "text", false, None)],
            secret_fields: vec![],
            compose: vec![sdk::field("text", "Message", "text", false, None)],
        }
    }
    fn targets(&mut self, config: &Value, _s: &Value, query: Option<&str>) -> Result<Vec<Target>> {
        let mut all = kdeconnect_devices();
        all.extend(signal_contacts(config));
        if let Some(q) = query {
            let q = q.to_lowercase();
            all.retain(|t| t.name.to_lowercase().contains(&q));
        }
        Ok(all)
    }
    fn share(&mut self, config: &Value, _s: &Value, files: &[String], target: Option<&str>, compose: &Value, p: &mut ShareProgress) -> Result<ShareResult> {
        let t = target.ok_or_else(|| PluginError::new("Invalid", "pick a recipient"))?;
        let total = files.len() as u64;
        if let Some(dev) = t.strip_prefix("kdeconnect:") {
            for (i, f) in files.iter().enumerate() {
                p.report(i as u64, total, 0, 0, &format!("sending {f}"));
                let st = Command::new("kdeconnect-cli").args(["-d", dev, "--share"]).arg(f).status().map_err(PluginError::io)?;
                if !st.success() {
                    return Err(PluginError::io(format!("kdeconnect-cli failed for {f}")));
                }
            }
            if let Some(txt) = compose.str_field("text") {
                if !txt.is_empty() {
                    let _ = Command::new("kdeconnect-cli").args(["-d", dev, "--share-text", txt]).status();
                }
            }
            p.report(total, total, 0, 0, "sent");
            return Ok(ShareResult { result: "sent", detail: None });
        }
        if let Some(num) = t.strip_prefix("signal:") {
            let account = config.str_field("signalAccount").unwrap_or("");
            let mut cmd = Command::new("signal-cli");
            cmd.args(["-a", account, "send", "-m", compose.str_field("text").unwrap_or("")]);
            for f in files {
                cmd.arg("-a").arg(f);
            }
            cmd.arg(num);
            p.report(0, 1, 0, 0, "sending");
            let st = cmd.status().map_err(PluginError::io)?;
            if !st.success() {
                return Err(PluginError::io("signal-cli failed"));
            }
            p.report(1, 1, 0, 0, "sent");
            return Ok(ShareResult { result: "sent", detail: None });
        }
        Err(PluginError::new("Invalid", "unknown target"))
    }
}

fn main() {
    let _ = sdk::run_share(&mut Messages);
}
