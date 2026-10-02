#![no_main]
//! Every frame from a window and every reply from a plugin goes through `kiki_json::parse`,
//! and the daemon is built with `panic = "abort"`: a panic here is the end of the daemon and
//! every window on it. So: any bytes at all, and the parser must answer — a value or an
//! error, never a panic and never a runaway. What it answers must also survive a trip through
//! `to_string`: what the daemon writes, a window must be able to read back as the same thing.
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    if let Ok(v) = kiki_json::parse(data) {
        let out = kiki_json::to_string(&v);
        let again = kiki_json::parse(out.as_bytes()).expect("what the writer wrote, the parser reads");
        assert_eq!(v, again, "a value changes on its way through the writer");
    }
});
