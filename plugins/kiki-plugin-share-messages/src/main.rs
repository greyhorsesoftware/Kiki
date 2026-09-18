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
            if let Some((id, name)) = kdeconnect_line(line) {
                let online = Command::new("kdeconnect-cli").args(["-d", &id, "--ping"]).output().map(|o| o.status.success()).unwrap_or(false);
                v.push(Target { id: format!("kdeconnect:{id}"), name, detail: "KDE Connect".into(), online, icon: "phone".into() });
            }
        }
    }
    v
}

/// `kdeconnect-cli -l --id-name-only` prints "<id> <name>", and a name may have spaces in it.
fn kdeconnect_line(line: &str) -> Option<(String, String)> {
    let (id, name) = line.trim().split_once(' ')?;
    if id.is_empty() || name.trim().is_empty() {
        return None;
    }
    Some((id.to_string(), name.trim().to_string()))
}

/// `signal-cli listContacts` prints "Number: +1555…  Name: Alice …"; a contact with no name is
/// known by its number.
fn signal_line(line: &str) -> Option<(String, String)> {
    let num = line.split_whitespace().nth(1).unwrap_or("");
    if !num.starts_with('+') {
        return None;
    }
    let name = line.split("Name:").nth(1).map(|s| s.split("  ").next().unwrap_or("").trim().to_string()).unwrap_or_default();
    Some((num.to_string(), if name.is_empty() { num.to_string() } else { name }))
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
            if let Some((num, name)) = signal_line(line) {
                v.push(Target { id: format!("signal:{num}"), name, detail: "Signal".into(), online: true, icon: "message".into() });
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_kdeconnect_device_keeps_the_spaces_in_its_name() {
        assert_eq!(kdeconnect_line("abc123 Gideon's Phone"), Some(("abc123".into(), "Gideon's Phone".into())));
    }

    #[test]
    fn a_line_with_no_name_is_not_a_device() {
        assert_eq!(kdeconnect_line("abc123"), None);
        assert_eq!(kdeconnect_line(""), None);
    }

    #[test]
    fn a_signal_contact_is_read_by_number_and_name() {
        assert_eq!(signal_line("Number: +15551234  Name: Alice  Blocked: false"), Some(("+15551234".into(), "Alice".into())));
    }

    #[test]
    fn a_contact_with_no_name_is_known_by_its_number() {
        assert_eq!(signal_line("Number: +15551234  Name:   Blocked: false"), Some(("+15551234".into(), "+15551234".into())));
    }

    #[test]
    fn a_header_line_is_not_a_contact() {
        assert_eq!(signal_line("Contacts:"), None);
    }
}
