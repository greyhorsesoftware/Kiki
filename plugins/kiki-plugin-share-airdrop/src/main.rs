//! AirDrop (experimental): wraps OpenDrop over OWL. Hidden unless both are present.
use kiki_plugin_sdk::json::Value;
use kiki_plugin_sdk::{self as sdk, PluginError, Result, ShareDescribe, ShareHandler, ShareProgress, ShareResult, Target};
use std::process::Command;

struct AirDrop;

fn ready() -> Result<()> {
    if !sdk::detected("opendrop") {
        return Err(PluginError::new("Unsupported", "opendrop is not installed"));
    }
    let owl = Command::new("pgrep").arg("-x").arg("owl").output().map(|o| o.status.success()).unwrap_or(false);
    if !owl {
        return Err(PluginError::new("Unsupported", "the owl daemon is not running (needs a Wi-Fi adapter with active monitor mode)"));
    }
    Ok(())
}

impl ShareHandler for AirDrop {
    fn describe(&self) -> ShareDescribe {
        ShareDescribe {
            id: "airdrop",
            name: "AirDrop (experimental)",
            icon: "share",
            version: "0.1",
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
    fn targets(&mut self, _c: &Value, _s: &Value, _q: Option<&str>) -> Result<Vec<Target>> {
        ready()?;
        let out = Command::new("opendrop").args(["find", "--timeout", "3"]).output().map_err(PluginError::io)?;
        let mut t = Vec::new();
        for line in String::from_utf8_lossy(&out.stdout).lines() {
            // "Found index 0  ID abc123  name Someone's iPhone"
            if let Some(rest) = line.strip_prefix("Found index ") {
                let mut it = rest.split_whitespace();
                let idx = it.next().unwrap_or("0").to_string();
                let name = line.split("name ").nth(1).unwrap_or("Apple device").to_string();
                t.push(Target { id: idx, name, detail: "AirDrop".into(), online: true, icon: "phone".into() });
            }
        }
        Ok(t)
    }
    fn share(&mut self, _c: &Value, _s: &Value, files: &[String], target: Option<&str>, _compose: &Value, p: &mut ShareProgress) -> Result<ShareResult> {
        ready()?;
        let idx = target.ok_or_else(|| PluginError::new("Invalid", "pick a device"))?;
        let total = files.len() as u64;
        for (i, f) in files.iter().enumerate() {
            p.report(i as u64, total, 0, 0, &format!("sending {f}"));
            let st = Command::new("opendrop").args(["send", "-r", idx, "-f"]).arg(f).status().map_err(PluginError::io)?;
            if !st.success() {
                return Err(PluginError::new("Cancelled", "the receiver declined or the transfer failed"));
            }
        }
        p.report(total, total, 0, 0, "sent");
        Ok(ShareResult { result: "sent", detail: None })
    }
}

fn main() {
    let _ = sdk::run_share(&mut AirDrop);
}
