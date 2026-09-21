//! MD5 (RFC 1321): freedesktop thumbnail cache keys, and the mirror's content comparison. Not
//! for security.

use std::io::Read;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::OnceLock;

const S: [u32; 64] = [
    7, 12, 17, 22, 7, 12, 17, 22, 7, 12, 17, 22, 7, 12, 17, 22, 5, 9, 14, 20, 5, 9, 14, 20, 5, 9, 14, 20, 5, 9, 14, 20, 4, 11, 16, 23, 4, 11, 16, 23, 4, 11, 16, 23, 4, 11, 16, 23, 6, 10, 15, 21, 6, 10, 15, 21, 6, 10,
    15, 21, 6, 10, 15, 21,
];

fn k() -> &'static [u32; 64] {
    static K: OnceLock<[u32; 64]> = OnceLock::new();
    K.get_or_init(|| std::array::from_fn(|i| ((i as f64 + 1.0).sin().abs() * 4294967296.0) as u32))
}

/// How much is read at a time, and so how often a cancelled hash notices (see `hex_reader`).
pub const CHUNK: usize = 256 * 1024;

/// MD5 a block at a time, so a whole file never has to be in memory at once.
pub struct Md5 {
    state: [u32; 4],
    block: [u8; 64],
    held: usize,
    len: u64,
}

impl Default for Md5 {
    fn default() -> Md5 {
        Md5::new()
    }
}

impl Md5 {
    pub fn new() -> Md5 {
        Md5 { state: [0x67452301, 0xefcdab89, 0x98badcfe, 0x10325476], block: [0; 64], held: 0, len: 0 }
    }

    pub fn update(&mut self, data: &[u8]) {
        self.len = self.len.wrapping_add(data.len() as u64);
        let mut rest = data;
        if self.held > 0 {
            let take = (64 - self.held).min(rest.len());
            self.block[self.held..self.held + take].copy_from_slice(&rest[..take]);
            self.held += take;
            rest = &rest[take..];
            if self.held < 64 {
                return; // not a whole block yet, and there is nothing left to add to it
            }
            let block = self.block;
            compress(&mut self.state, &block);
            self.held = 0;
        }
        let (blocks, tail) = rest.as_chunks::<64>();
        for b in blocks {
            compress(&mut self.state, b);
        }
        self.block[..tail.len()].copy_from_slice(tail);
        self.held = tail.len();
    }

    pub fn finish(mut self) -> [u8; 16] {
        let bits = self.len.wrapping_mul(8);
        // 0x80, zeroes to 56 bytes into the last block, then the length in bits.
        let pad = if self.held < 56 { 56 - self.held } else { 120 - self.held };
        let mut tail = [0u8; 72];
        tail[0] = 0x80;
        tail[pad..pad + 8].copy_from_slice(&bits.to_le_bytes());
        self.update(&tail[..pad + 8]);
        let mut out = [0u8; 16];
        for (i, w) in self.state.iter().enumerate() {
            out[i * 4..i * 4 + 4].copy_from_slice(&w.to_le_bytes());
        }
        out
    }
}

fn compress(state: &mut [u32; 4], chunk: &[u8; 64]) {
    let k = k();
    let mut m = [0u32; 16];
    for (i, w) in m.iter_mut().enumerate() {
        *w = u32::from_le_bytes([chunk[i * 4], chunk[i * 4 + 1], chunk[i * 4 + 2], chunk[i * 4 + 3]]);
    }
    let (mut a, mut b, mut c, mut d) = (state[0], state[1], state[2], state[3]);
    for i in 0..64 {
        let (f, g) = match i / 16 {
            0 => ((b & c) | (!b & d), i),
            1 => ((d & b) | (!d & c), (5 * i + 1) % 16),
            2 => (b ^ c ^ d, (3 * i + 5) % 16),
            _ => (c ^ (b | !d), (7 * i) % 16),
        };
        let f2 = f.wrapping_add(a).wrapping_add(k[i]).wrapping_add(m[g]);
        a = d;
        d = c;
        c = b;
        b = b.wrapping_add(f2.rotate_left(S[i]));
    }
    state[0] = state[0].wrapping_add(a);
    state[1] = state[1].wrapping_add(b);
    state[2] = state[2].wrapping_add(c);
    state[3] = state[3].wrapping_add(d);
}

pub fn hex(data: &[u8]) -> String {
    hex_of(&digest(data))
}

fn hex_of(d: &[u8; 16]) -> String {
    let mut s = String::with_capacity(32);
    for b in d {
        s.push_str(&format!("{b:02x}"));
    }
    s
}

pub fn digest(data: &[u8]) -> [u8; 16] {
    let mut m = Md5::new();
    m.update(data);
    m.finish()
}

/// The MD5 of everything a reader has, read in `CHUNK` bites and given up the moment `cancel` is
/// set — `None` when that happened. A mirror hashes whole files to compare them, and a cancel that
/// was only looked at BETWEEN files waits out the big one somebody is cancelling because of.
pub fn hex_reader(r: &mut impl Read, cancel: &AtomicBool) -> std::io::Result<Option<String>> {
    let mut m = Md5::new();
    let mut buf = vec![0u8; CHUNK];
    loop {
        if cancel.load(Ordering::Relaxed) {
            return Ok(None);
        }
        let n = r.read(&mut buf)?;
        if n == 0 {
            return Ok(Some(hex_of(&m.finish())));
        }
        m.update(&buf[..n]);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_vectors() {
        assert_eq!(hex(b""), "d41d8cd98f00b204e9800998ecf8427e");
        assert_eq!(hex(b"abc"), "900150983cd24fb0d6963f7d28e17f72");
        assert_eq!(hex(b"The quick brown fox jumps over the lazy dog"), "9e107d9d372bb6826bd81d3542a419d6");
    }

    /// Fed in awkward bites — across the 64-byte block, and past the point where the length is
    /// written — it is the same digest as one call.
    #[test]
    fn a_stream_hashes_the_same_as_one_call() {
        let data: Vec<u8> = (0..1000u32).map(|i| (i % 251) as u8).collect();
        for step in [1, 7, 60, 63, 64, 65, 200, 999] {
            let mut m = Md5::new();
            for part in data.chunks(step) {
                m.update(part);
            }
            assert_eq!(hex_of(&m.finish()), hex(&data), "in bites of {step}");
        }
    }

    /// A file with no end: what a cancel has to be able to stop. The reader flips the flag the
    /// first time it is asked for bytes, so this returns only if the hash looks INSIDE the file.
    #[test]
    fn a_cancelled_hash_stops_inside_the_file() {
        struct Endless<'a> {
            reads: usize,
            cancel: &'a AtomicBool,
        }
        impl Read for Endless<'_> {
            fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
                self.reads += 1;
                self.cancel.store(true, Ordering::Relaxed);
                buf.fill(b'x');
                Ok(buf.len())
            }
        }
        let cancel = AtomicBool::new(false);
        let mut endless = Endless { reads: 0, cancel: &cancel };
        assert_eq!(hex_reader(&mut endless, &cancel).unwrap(), None, "a cancelled hash has no answer");
        assert_eq!(endless.reads, 1, "and it stopped at the first chunk, not at the end of the file");
        // And the flag not being set is the file being hashed to the end, as before.
        let quiet = AtomicBool::new(false);
        assert_eq!(hex_reader(&mut &b"abc"[..], &quiet).unwrap().as_deref(), Some("900150983cd24fb0d6963f7d28e17f72"));
    }
}
