//! Bridge to `kiki-helper-dbus` (plan 09): the helper sends `ShowItems` and `ShowChooser`
//! requests on its stdout; the daemon forwards them to the shell as events and relays the
//! shell's `ChooserResult` back as the helper's reply.

use crate::json::Value;
use crate::proto::{self, Frame, Framing, Reader};
use std::collections::HashMap;
use std::io::Write;
use std::process::{Command, Stdio};
use std::sync::mpsc::Sender;
use std::sync::{Mutex, OnceLock};

struct State {
    shell: Option<Sender<Value>>,
    helper_in: Option<std::process::ChildStdin>,
    /// chooser token -> helper request id
    pending: HashMap<String, u64>,
}

fn state() -> &'static Mutex<State> {
    static S: OnceLock<Mutex<State>> = OnceLock::new();
    S.get_or_init(|| Mutex::new(State { shell: None, helper_in: None, pending: HashMap::new() }))
}

/// The most recent shell connection receives portal and FileManager1 requests.
pub fn register_shell(tx: Sender<Value>) {
    state().lock().unwrap().shell = Some(tx);
}

pub fn start() {
    let Some(bin) = crate::helpers::find("kiki-plugin-dbus") else { return };
    let mut child = match Command::new(&bin).stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::inherit()).spawn() {
        Ok(c) => c,
        Err(e) => {
            eprintln!("kiki-helper-dbus: {e}");
            return;
        }
    };
    state().lock().unwrap().helper_in = child.stdin.take();
    let stdout = child.stdout.take().unwrap();
    std::thread::Builder::new()
        .name("dbus-bridge".into())
        .spawn(move || {
            let mut reader = Reader::new(stdout);
            while let Ok(Some(Frame::Json(v))) = reader.next() {
                let id = v.u64_field("id").unwrap_or(0);
                match v.str_field("type") {
                    Some("ShowChooser") => {
                        let token = v.str_field("token").unwrap_or("").to_string();
                        let mut st = state().lock().unwrap();
                        st.pending.insert(token, id);
                        match &st.shell {
                            Some(tx) => {
                                let mut ev = v.clone();
                                if let Value::Obj(m) = &mut ev {
                                    m.remove("id");
                                    m.remove("type");
                                    m.insert("event".into(), Value::Str("ShowChooser".into()));
                                }
                                if tx.send(ev).is_err() {
                                    st.shell = None;
                                    reply_helper(&mut st, id, Value::obj().v("uris", Value::Null).done());
                                }
                            }
                            None => reply_helper(&mut st, id, Value::obj().v("uris", Value::Null).done()),
                        }
                    }
                    Some("ShowItems") | Some("ShowFolders") => {
                        let mut st = state().lock().unwrap();
                        if let Some(tx) = &st.shell {
                            let _ = tx.send(
                                proto::event("ShowItems")
                                    .v("uris", v.get("uris").cloned().unwrap_or(Value::Arr(vec![])))
                                    .b("properties", v.get("properties").and_then(Value::as_bool).unwrap_or(false))
                                    .b("folders", v.str_field("type") == Some("ShowFolders"))
                                    .done(),
                            );
                        } else {
                            // No window: launch one.
                            let first = v.get("uris").and_then(Value::as_arr).and_then(|a| a.first()).and_then(Value::as_str).unwrap_or("").to_string();
                            let _ = Command::new("kiki").arg(first).stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null()).spawn();
                        }
                        reply_helper(&mut st, id, Value::obj().done());
                    }
                    _ => {
                        let mut st = state().lock().unwrap();
                        reply_helper(&mut st, id, Value::obj().done());
                    }
                }
            }
            let _ = child.wait();
        })
        .expect("spawn dbus bridge");
}

fn reply_helper(st: &mut State, id: u64, result: Value) {
    if let Some(w) = st.helper_in.as_mut() {
        let _ = proto::write_json(w, Framing::Binary, &proto::ok(id, result));
        let _ = w.flush();
    }
}

/// From the shell: the user's answer to a chooser.
pub fn chooser_result(token: &str, uris: Value) {
    let mut st = state().lock().unwrap();
    if let Some(id) = st.pending.remove(token) {
        reply_helper(&mut st, id, Value::obj().v("uris", uris).done());
    }
}
