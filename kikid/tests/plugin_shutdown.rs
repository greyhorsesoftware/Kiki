//! A plugin that will not leave when asked. `Plugin::shutdown` used to send `Shutdown` and wait
//! for the process for ever; the reaper calls it for every idle plugin in turn, so one such
//! plugin stopped all reaping for as long as it lived (docs/0.5.0/02-daemon-bounds.md). Now the
//! word is given two seconds and then the kill.

mod common;

use kikid::plugin;
use std::time::{Duration, Instant};

#[test]
fn a_plugin_that_ignores_shutdown_is_killed_in_time_and_the_next_one_comes() {
    let dir = common::setup("plugin-shutdown");
    std::env::set_var("KIKI_STUB_IGNORE_SHUTDOWN", "1");
    let p = plugin::get("stub").expect("the stub starts");
    assert!(p.alive());
    let t = Instant::now();
    p.shutdown();
    assert!(!p.alive(), "gone, by the kill if not by the word");
    assert!(t.elapsed() < Duration::from_secs(3), "within the grace and a little: {:?}", t.elapsed());
    // What the reaper does next: asks for the plugin again and gets a live one.
    let again = plugin::get("stub").expect("a fresh stub");
    assert!(again.alive());
    again.kill_group();
    let _ = std::fs::remove_dir_all(dir);
}
