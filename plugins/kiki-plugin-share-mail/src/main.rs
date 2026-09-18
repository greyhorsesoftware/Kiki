//! Mail (plan 18): the desktop composer through `xdg-email`, or SMTP mode where the plugin
//! sends the message itself with attachments (for web-mail handlers that cannot take them).
mod smtp;

use kiki_plugin_sdk::json::Value;
use kiki_plugin_sdk::{self as sdk, PluginError, Result, ShareDescribe, ShareHandler, ShareProgress, ShareResult};
use std::process::Command;

struct Mail;

fn cfg<'a>(c: &'a Value, k: &str) -> &'a str {
    c.str_field(k).unwrap_or("")
}

fn account(config: &Value, secrets: &Value) -> Result<smtp::Account> {
    let host = cfg(config, "host").trim().to_string();
    if host.is_empty() {
        return Err(PluginError::invalid("host", "SMTP server is required"));
    }
    let port: u16 = cfg(config, "port").trim().parse().unwrap_or(0);
    let security = match cfg(config, "security") {
        "tls" => smtp::Security::Tls,
        "none" => smtp::Security::None,
        _ => smtp::Security::StartTls,
    };
    let port = if port == 0 {
        if security == smtp::Security::Tls {
            465
        } else {
            587
        }
    } else {
        port
    };
    let from = cfg(config, "from").trim().to_string();
    if from.is_empty() {
        return Err(PluginError::invalid("from", "from address is required"));
    }
    Ok(smtp::Account {
        host,
        port,
        security,
        username: cfg(config, "username").to_string(),
        password: secrets.str_field("password").unwrap_or("").to_string(),
        from,
        trust_fingerprint: config.str_field("trustFingerprint").map(str::to_string),
    })
}

/// What `xdg-email` is asked to do: the parts of the message that were filled in, then the
/// attachments, then the recipient — which has to come last, as the address is the operand.
fn composer_args(compose: &Value, files: &[String]) -> Vec<String> {
    let mut args = Vec::new();
    if let Some(s) = compose.str_field("subject").filter(|s| !s.is_empty()) {
        args.push("--subject".into());
        args.push(s.to_string());
    }
    if let Some(b) = compose.str_field("body").filter(|b| !b.is_empty()) {
        args.push("--body".into());
        args.push(b.to_string());
    }
    for f in files {
        args.push("--attach".into());
        args.push(f.clone());
    }
    if let Some(to) = compose.str_field("to").filter(|t| !t.is_empty()) {
        args.push(format!("mailto:{to}"));
    }
    args
}

impl ShareHandler for Mail {
    fn describe(&self) -> ShareDescribe {
        ShareDescribe {
            id: "mail",
            name: "Mail",
            icon: "mail",
            version: "1.1",
            accepts_files: true,
            accepts_folders: false,
            accepts_multiple: true,
            max_bytes: Some(20 * 1024 * 1024),
            targets: "none",
            form: vec![
                sdk::select_field("mode", "Send with", &["composer", "smtp"], "composer"),
                sdk::field("host", "SMTP server", "text", false, None),
                sdk::field("port", "Port", "port", false, Some("587")),
                sdk::select_field("security", "Security", &["starttls", "tls", "none"], "starttls"),
                sdk::field("username", "Username", "text", false, None),
                sdk::field("password", "Password", "password", false, None),
                sdk::field("from", "From address", "text", false, None),
                sdk::field("trustFingerprint", "Trusted certificate (sha256, self-signed servers)", "text", false, None),
            ],
            secret_fields: vec!["password"],
            compose: vec![sdk::field("to", "To", "text", false, None), sdk::field("subject", "Subject", "text", false, None), sdk::field("body", "Message", "text", false, None)],
        }
    }

    fn configure(&mut self, config: &Value, secrets: &Value) -> Result<()> {
        if cfg(config, "mode") == "smtp" {
            account(config, secrets)?;
        }
        Ok(())
    }

    fn share(&mut self, config: &Value, secrets: &Value, files: &[String], _t: Option<&str>, compose: &Value, p: &mut ShareProgress) -> Result<ShareResult> {
        if cfg(config, "mode") == "smtp" {
            let acc = account(config, secrets)?;
            let to: Vec<String> = compose.str_field("to").unwrap_or("").split([',', ';']).map(str::trim).filter(|s| !s.is_empty()).map(str::to_string).collect();
            if to.is_empty() {
                return Err(PluginError::invalid("to", "recipient is required"));
            }
            let mut attachments = Vec::new();
            let mut total = 0u64;
            for f in files {
                let bytes = std::fs::read(f).map_err(PluginError::io)?;
                total += bytes.len() as u64;
                attachments.push((std::path::Path::new(f).file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| "file".into()), bytes));
            }
            let n = attachments.len() as u64;
            let msg = smtp::Message { to, subject: compose.str_field("subject").unwrap_or(""), body: compose.str_field("body").unwrap_or(""), attachments };
            smtp::send(&acc, &msg, |status| p.report(0, n, 0, total, status))?;
            p.report(n, n, total, total, "sent");
            return Ok(ShareResult { result: "sent", detail: Some(format!("sent through {}", acc.host)) });
        }
        if !sdk::detected("xdg-email") {
            return Err(PluginError::new("Unsupported", "xdg-email is not installed"));
        }
        p.report(0, 1, 0, 0, "opening composer");
        let mut cmd = Command::new("xdg-email");
        cmd.args(composer_args(compose, files));
        let st = cmd.status().map_err(PluginError::io)?;
        if !st.success() {
            return Err(PluginError::io("mail composer could not be opened"));
        }
        p.report(1, 1, 0, 0, "opened");
        Ok(ShareResult { result: "opened", detail: Some("composer opened; attachments need a desktop mail client".into()) })
    }
}

fn main() {
    let _ = sdk::run_share(&mut Mail);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_bare_share_attaches_the_files_and_asks_nothing_else() {
        let args = composer_args(&Value::obj().done(), &["/tmp/a.png".to_string(), "/tmp/b.png".to_string()]);
        assert_eq!(args, vec!["--attach", "/tmp/a.png", "--attach", "/tmp/b.png"]);
    }

    #[test]
    fn what_was_filled_in_is_passed_on_and_the_address_comes_last() {
        let compose = Value::obj().s("subject", "Holiday").s("body", "Here they are").s("to", "a@example.com").done();
        let args = composer_args(&compose, &["/tmp/a.png".to_string()]);
        assert_eq!(args, vec!["--subject", "Holiday", "--body", "Here they are", "--attach", "/tmp/a.png", "mailto:a@example.com"]);
    }

    #[test]
    fn empty_fields_are_left_out_rather_than_passed_empty() {
        let compose = Value::obj().s("subject", "").s("body", "").s("to", "").done();
        assert!(composer_args(&compose, &[]).is_empty());
    }
}
