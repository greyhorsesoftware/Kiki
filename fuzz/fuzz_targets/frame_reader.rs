#![no_main]
//! The framed reader on the socket and on every plugin's pipe, fed arbitrary bytes as if they
//! were a stream: binary frames with a length prefix a peer chose, text lines, both. It must
//! refuse a length over its bound before it allocates for it (a four-gigabyte prefix is five
//! bytes to send), end cleanly on a truncated stream, and never panic.
use libfuzzer_sys::fuzz_target;
use std::io::Cursor;

fuzz_target!(|data: &[u8]| {
    let mut r = kikid::proto::Reader::new(Cursor::new(data));
    // Bounded by the input: every frame consumes bytes, and an error ends the stream.
    for _ in 0..1024 {
        match r.next() {
            Ok(Some(_)) => continue,
            Ok(None) | Err(_) => break,
        }
    }
});
