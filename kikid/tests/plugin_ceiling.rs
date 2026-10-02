//! A request that is never idle and never ends. The patience between frames (`REQUEST_TIMEOUT`)
//! resets on every frame, so a plugin saying something every so often was never given up on;
//! `REQUEST_CEILING` is the one clock that is not reset (docs/0.5.0/02-daemon-bounds.md).

mod common;

use kikid::json::Value;
use kikid::plugin::{self, Msg};
use std::time::{Duration, Instant};

#[test]
fn a_request_still_moving_past_the_ceiling_is_ended_with_1273() {
    let dir = common::setup("plugin-ceiling");
    std::env::set_var("KIKI_STUB_DRIP_MS", "100");
    std::env::set_var("KIKI_REQUEST_CEILING_MS", "600");
    let p = plugin::get("stub").expect("the stub starts");
    let mut frames = 0;
    let t = Instant::now();
    let r = p.request_stream_with(Value::obj().s("type", "Scan").s("path", "/").done(), None, |m| {
        if matches!(m, Msg::Json(_)) {
            frames += 1;
        }
    });
    let took = t.elapsed();
    assert_eq!(r.unwrap_err().said_json().map(|(n, _)| n), Some(1273), "ended by the ceiling, by number");
    assert!(frames >= 3, "it was moving the whole time: {frames} frames");
    assert!(took >= Duration::from_millis(500) && took < Duration::from_secs(3), "at the ceiling, not before or long after: {took:?}");
    std::env::remove_var("KIKI_REQUEST_CEILING_MS");
    p.kill_group();
    let _ = std::fs::remove_dir_all(dir);
}
