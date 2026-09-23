//! Drives the tailscale plugin binary: what it says about itself over the wire, and that it
//! shuts down when told. Small on purpose — the behaviour is unit-tested in `main.rs` — but this
//! file is what makes `cargo test` build the binary at all: cargo builds a package's binaries
//! for its integration tests and for nothing else, and without it the daemon's share contract
//! test (`kikid/tests/share_contract.rs`) found only the mail plugin in `target/`.

use kiki_plugin_sdk::json::{self, Value};
use kiki_plugin_sdk::{read_frame, write_json};
use std::io::BufReader;
use std::process::{Command, Stdio};

#[test]
fn it_describes_itself_and_shuts_down_when_told() {
    let mut child = Command::new(env!("CARGO_BIN_EXE_kiki-plugin-share-tailscale")).stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::inherit()).spawn().unwrap();
    let mut stdin = child.stdin.take().unwrap();
    let mut stdout = BufReader::new(child.stdout.take().unwrap());

    write_json(&mut stdin, &Value::obj().u("id", 1).s("type", "Describe").done()).unwrap();
    let mut reply = Value::Null;
    while let Some((_, payload)) = read_frame(&mut stdout).unwrap() {
        let v = json::parse(&payload).unwrap();
        if v.get("ok").is_some() || v.get("err").is_some() {
            reply = v;
            break;
        }
    }
    let ok = reply.get("ok").unwrap_or_else(|| panic!("{}", json::to_string(&reply)));
    assert_eq!(ok.str_field("id"), Some("tailscale"));
    assert_eq!(ok.str_field("name"), Some("Tailscale"));
    assert_eq!(ok.str_field("targets"), Some("list"));
    // It needs the tailscale CLI, and says so rather than failing later on a peer list.
    let requires: Vec<&str> = ok.get("requires").and_then(Value::as_arr).unwrap().iter().filter_map(Value::as_str).collect();
    assert_eq!(requires, ["tailscale"]);

    write_json(&mut stdin, &Value::obj().u("id", 2).s("type", "Shutdown").done()).unwrap();
    let status = child.wait().unwrap();
    assert!(status.success(), "{status}");
}
