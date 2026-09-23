//! Removing a location, and letting go of its connections — the two daemon defects behind plan
//! 31's "after `remote_transfers` had run, the shell listed an FTPS folder that plainly had a
//! file in it as empty":
//!
//! * `remove` asked which secrets a location had AFTER writing the file without it, so `find`
//!   had nothing to answer with and the password stayed in the keyring for ever;
//! * `disconnect` held the session map's lock across the plugin round trip, and a plugin that
//!   logs while it answers takes that same lock to ask which jobs hold sessions on it — a
//!   deadlock, after which the daemon answers nothing, ever. The sidebar's own "Disconnect" runs
//!   this, so a live location was one menu click from a frozen daemon.
//!
//! The stub speaks before it answers a `Disconnect`, as every real plugin does, so the second is
//! what this hangs on if the lock ever comes back — hence the watchdog rather than a plain call:
//! a deadlock has to be a failing test, not a suite that never finishes.
//!
//! One test function, not two: the plugin registry, the session map and the environment are one
//! per process, so the two halves take turns rather than racing.

mod common;

use kikid::json::Value;
use kikid::{joblog, locations};
use std::sync::mpsc;
use std::time::Duration;

/// Runs `f` on a thread of its own and fails, rather than hanging, if it has not finished in time.
fn within<T: Send + 'static>(what: &str, patience: Duration, f: impl FnOnce() -> T + Send + 'static) -> T {
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let _ = tx.send(f());
    });
    rx.recv_timeout(patience).unwrap_or_else(|_| panic!("{what} never answered in {patience:?} — the session map's lock is held across the plugin round trip again"))
}

/// Everything the stub has said, as one string.
fn plugin_log() -> String {
    joblog::read_plugin("stub", 0).get("lines").and_then(Value::as_arr).map(|a| a.iter().map(|l| l.str_field("text").unwrap_or("").to_string()).collect::<Vec<_>>().join("\n")).unwrap_or_default()
}

#[test]
fn removing_a_location_clears_its_secrets_and_lets_every_session_go() {
    let dir = common::setup("locrm");

    // ------------------------------------------------ remove: the keyring and every session
    common::save_location("lab");
    assert!(common::secret_log(&dir).contains("location lab field password"), "the password was offered to the keyring: {}", common::secret_log(&dir));

    // Two sessions on the one location: the browser's and a job's. `remove` has to tell the
    // plugin about both, or the plugin keeps the connection and hands it to whatever takes this
    // name next — which is then browsing the server that was removed, with nothing to say so.
    let loc = locations::find("lab").unwrap();
    locations::connect(&loc, "browse", None).unwrap();
    locations::connect(&loc, "job-9", None).unwrap();
    assert!(locations::connected_names().iter().any(|n| n == "lab"));

    within("remove", Duration::from_secs(10), || locations::remove("lab").unwrap());

    assert!(locations::find("lab").is_none());
    assert!(locations::connected_names().iter().all(|n| n != "lab"), "no session is left behind");
    // Asked for BEFORE the location was written away: afterwards there is nothing left to ask.
    assert!(common::secret_log(&dir).contains("clear app kiki location lab field password"), "the password was cleared from the keyring: {}", common::secret_log(&dir));
    // The plugin was told, once per session it held — not merely forgotten about here.
    let said = plugin_log();
    assert_eq!(said.matches("letting lab go").count(), 2, "both roles were disconnected: {said}");

    // ------------------------------------------------ disconnect: the sidebar's own menu item
    // Nothing is removed, and the location is there to connect to again.
    common::save_location("workshop");
    locations::connect(&locations::find("workshop").unwrap(), "browse", None).unwrap();

    within("disconnect", Duration::from_secs(10), || locations::disconnect("workshop"));
    assert!(locations::connected_names().iter().all(|n| n != "workshop"));
    assert!(locations::find("workshop").is_some(), "disconnecting is not removing");
    assert!(plugin_log().contains("letting workshop go"));

    locations::connect(&locations::find("workshop").unwrap(), "browse", None).unwrap();
    assert!(locations::connected_names().iter().any(|n| n == "workshop"), "and it connects again: what was let go of was the session, not the plugin");

    locations::remove("workshop").unwrap();

    // ------------------------------------------------ a location this build no longer speaks
    // WebDAV was removed from 0.1.0 (owner, 2026-09-21), and a `locations.toml` written by a
    // build that had it keeps its `dav` entries: upgrading kiki must not be a crash, an empty
    // sidebar, or a plugin name shown to somebody who never typed one.
    let dav = Value::obj().s("name", "shelf").s("plugin", "dav").s("remoteUri", "dav://shelf/").v("config", Value::obj().s("host", "shelf.lan").done()).done();
    locations::upsert(dav.clone()).unwrap();
    assert!(locations::all().iter().any(|l| l.str_field("name") == Some("shelf")), "it is still listed: it is the user's, and removing it is their decision");
    let refused = within("connect to a dav location", Duration::from_secs(10), move || locations::connect(&dav, "browse", None).map(|_| ()));
    assert_eq!(refused.unwrap_err().message(), "WebDAV is not in this version", "in words, not in scheme names");
    assert_eq!(kikid::plugin::no_such_kind("afp"), "afp locations are not part of this build", "a kind nobody has a word for still says something");
    assert!(!kikid::plugin::ships("dav"));
    locations::remove("shelf").unwrap();

    std::fs::remove_dir_all(&dir).unwrap();
}
