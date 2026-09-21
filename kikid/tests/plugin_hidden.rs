//! A plugin's own `Meta.hidden`: an entry a backend calls hidden although its name says nothing
//! (SMB's DOS attribute, gio's `standard::is-hidden`). The daemon is where that is decided — the
//! shell is only ever sent the rows the view holds — so this drives the stub plugin through a
//! real listing and reads what comes out.

mod common;

use kikid::listing::{self, Subscriber};
use kikid::vfs::uri::Uri;
use std::sync::mpsc;
use std::time::Duration;

#[test]
fn a_plugin_can_hide_an_entry_whose_name_is_not_hidden() {
    let dir = common::setup("hidden");
    common::save_location("lab");

    let uri = Uri::parse("stub://lab/").unwrap();
    let (l, _) = listing::open(&uri).unwrap();
    assert!(listing::wait_scan(&l, Duration::from_secs(5)));
    let (tx, rx) = mpsc::channel();
    l.subscribe(Subscriber { client: 1, lid: 1, tx, first: 0, count: 20, view_first: 0, view_count: 20 });
    let names = |l: &std::sync::Arc<listing::Listing>| -> Vec<String> {
        let w = l.window(1, 1, 0, 20, None);
        w.get("rows").unwrap().as_arr().unwrap().iter().map(|r| r.str_field("name").unwrap_or("").to_string()).collect()
    };

    // `secret.bin` is an ordinary name — nothing but the plugin's flag can keep it out.
    assert_eq!(names(&l), ["docs", "empty", "data.bin", "slow.bin"], "the plugin's hidden entry is not in the view");
    assert_eq!(l.count().0, 4);

    // Show hidden files, and it is there with the rest.
    assert_eq!(l.set_hidden(true), 5, "shown when hidden files are asked for");
    assert_eq!(names(&l), ["docs", "empty", "data.bin", "secret.bin", "slow.bin"]);
    // It is a row like any other while it is shown: the flag hides it, it does not hollow it out.
    let w = l.window(1, 1, 0, 20, None);
    let row = w.get("rows").unwrap().as_arr().unwrap().iter().find(|r| r.str_field("name") == Some("secret.bin")).unwrap().clone();
    assert_eq!(row.get("meta").unwrap().u64_field("size"), Some(3));

    assert_eq!(l.set_hidden(false), 4, "and hidden again on the way back");
    assert!(!names(&l).contains(&"secret.bin".to_string()));
    let _ = rx.try_iter().count();

    listing::invalidate_authority("stub", "lab");
    kikid::locations::remove("lab").unwrap();
    std::fs::remove_dir_all(&dir).unwrap();
}
