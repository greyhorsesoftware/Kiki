//! Mail: opens the desktop mail composer with attachments through `xdg-email`.
use kiki_plugin_sdk::json::Value;
use kiki_plugin_sdk::{self as sdk, PluginError, Result, ShareDescribe, ShareHandler, ShareProgress, ShareResult};
use std::process::Command;

struct Mail;

impl ShareHandler for Mail {
    fn describe(&self) -> ShareDescribe {
        ShareDescribe { id: "mail", name: "Mail", icon: "mail", version: "1.0", accepts_files: true, accepts_folders: false, accepts_multiple: true, max_bytes: Some(20 * 1024 * 1024), targets: "none", form: vec![], secret_fields: vec![],
            compose: vec![sdk::field("to", "To", "text", false, None), sdk::field("subject", "Subject", "text", false, None), sdk::field("body", "Message", "text", false, None)] }
    }
    fn share(&mut self, _c: &Value, _s: &Value, files: &[String], _t: Option<&str>, compose: &Value, p: &mut ShareProgress) -> Result<ShareResult> {
        if !sdk::detected("xdg-email") {
            return Err(PluginError::new("Unsupported", "xdg-email is not installed"));
        }
        p.report(0, 1, 0, 0, "opening composer");
        let mut cmd = Command::new("xdg-email");
        if let Some(s) = compose.str_field("subject") { if !s.is_empty() { cmd.arg("--subject").arg(s); } }
        if let Some(b) = compose.str_field("body") { if !b.is_empty() { cmd.arg("--body").arg(b); } }
        for f in files { cmd.arg("--attach").arg(f); }
        if let Some(to) = compose.str_field("to") { if !to.is_empty() { cmd.arg(format!("mailto:{to}")); } }
        let st = cmd.status().map_err(PluginError::io)?;
        if !st.success() { return Err(PluginError::io("mail composer could not be opened")); }
        p.report(1, 1, 0, 0, "opened");
        Ok(ShareResult { result: "opened", detail: Some("composer opened; attachments need a desktop mail client".into()) })
    }
}

fn main() { let _ = sdk::run_share(&mut Mail); }
