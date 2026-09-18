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

/// The devices in `opendrop find` output: "Found index 0  ID abc123  name Someone's iPhone".
fn found(out: &str) -> Vec<Target> {
    let mut t = Vec::new();
    for line in out.lines() {
        let Some(rest) = line.strip_prefix("Found index ") else { continue };
        let idx = rest.split_whitespace().next().unwrap_or("0").to_string();
        let name = line.split("name ").nth(1).map(str::trim).filter(|n| !n.is_empty()).unwrap_or("Apple device").to_string();
        t.push(Target { id: idx, name, detail: "AirDrop".into(), online: true, icon: "phone".into() });
    }
    t
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
        Ok(found(&String::from_utf8_lossy(&out.stdout)))
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_found_line_is_a_device() {
        let t = found("Found index 0  ID abc  name Gideon's iPhone\nFound index 1  ID def  name iPad\n");
        assert_eq!(t.len(), 2);
        assert_eq!(t[0].id, "0");
        assert_eq!(t[0].name, "Gideon's iPhone");
        assert_eq!(t[1].id, "1");
    }

    #[test]
    fn a_device_that_gives_no_name_still_appears() {
        let t = found("Found index 2  ID ghi\n");
        assert_eq!(t.len(), 1);
        assert_eq!(t[0].name, "Apple device");
    }

    #[test]
    fn chatter_around_the_list_is_ignored() {
        assert!(found("Looking for devices...\nnothing here\n").is_empty());
    }
}
