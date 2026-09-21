//! A property target for the reader and the writer, run over documents a seeded generator makes
//! rather than over examples somebody thought of.
//!
//! Two properties, and they are the two the protocols rest on:
//!
//! 1. **`parse(to_string(v)) == v`** for every value the generator can build — which includes the
//!    strings that break naive writers: quotes, backslashes, control characters, astral-plane
//!    characters that have to go out as a surrogate pair.
//! 2. **The reader never panics**, whatever it is handed. Every document is mangled a hundred ways
//!    (a byte flipped, a byte gone, a byte inserted, the tail cut off) and parsed again; so are
//!    runs of pure noise. A parse must come back `Ok` or `Err`, and an `Ok` must **settle**: send
//!    it through the writer and the reader again and again, and it stops changing. Anything else
//!    is two ends of a socket that can disagree for ever about the same bytes.
//!
//! No generator crate: sixteen lines of xorshift, seeded, so a failure is reproducible from the
//! seed printed with it.

use kiki_json::{parse, to_string, Value};
use std::collections::BTreeMap;

/// xorshift64*, seeded: the same seed makes the same documents on every machine and every run.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.0 = x;
        x.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }
    fn below(&mut self, n: u64) -> u64 {
        self.next() % n
    }
    fn pick<'a, T>(&mut self, from: &'a [T]) -> &'a T {
        &from[self.below(from.len() as u64) as usize]
    }
}

/// The pieces strings are made of: the ones a writer has to escape, the ones a reader has to
/// decode, and a few that are merely ordinary.
const PIECES: &[&str] = &[
    "",
    "a",
    "name.txt",
    "\"",
    "\\",
    "\\\"",
    "/",
    "\n",
    "\r\n",
    "\t",
    "\u{0}",
    "\u{1f}",
    "\u{7f}",
    "é",
    "日本語",
    "😀",         // astral: a surrogate pair on the wire
    "𝄞",          // and another
    "~/Projects", // the shapes the protocol actually carries
    "file:///tmp/a b.txt",
    "sftp://homelab/srv/kiki",
    "   ",
    "null",
    "{\"not\":\"json\"}",
];

fn string(r: &mut Rng) -> String {
    let n = r.below(5);
    (0..n).map(|_| *r.pick(PIECES)).collect()
}

/// One value. Containers thin out with depth so a document ends rather than growing for ever.
fn value(r: &mut Rng, depth: usize) -> Value {
    let leaf_only = depth >= 5;
    match r.below(if leaf_only { 6 } else { 8 }) {
        0 => Value::Null,
        1 => Value::Bool(r.below(2) == 1),
        // Only negative: a non-negative `Int` is written as digits and read back as `Uint`, which
        // is pinned below as the deliberate thing it is rather than smuggled in here.
        2 => Value::Int(-((r.next() % (i64::MAX as u64)) as i64)),
        3 => Value::Uint(r.next()),
        // Never integral, never NaN or infinite — both are written as something else, and both
        // are pinned below.
        4 => Value::Float(fraction(r)),
        5 => Value::Str(string(r)),
        6 => Value::Arr((0..r.below(6)).map(|_| value(r, depth + 1)).collect()),
        _ => {
            let mut m = BTreeMap::new();
            for _ in 0..r.below(6) {
                m.insert(string(r), value(r, depth + 1));
            }
            Value::Obj(m)
        }
    }
}

/// A finite float with a fractional part, at a range of magnitudes.
fn fraction(r: &mut Rng) -> f64 {
    let mantissa = (r.below(1 << 52) as f64) / ((1u64 << 52) as f64); // 0..1
    let scale = [1e-6, 1e-3, 1.0, 1e3, 1e6, 1e12][r.below(6) as usize];
    let v = (mantissa + 0.5) * scale;
    if r.below(2) == 1 {
        -v
    } else {
        v
    }
}

#[test]
fn printing_and_reading_a_value_gives_the_value_back() {
    for seed in 1..=40u64 {
        let mut r = Rng(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1);
        for round in 0..50 {
            let v = value(&mut r, 0);
            let text = to_string(&v);
            let back = parse(text.as_bytes()).unwrap_or_else(|e| panic!("seed {seed} round {round}: {e} in {text}"));
            assert_eq!(back, v, "seed {seed} round {round}: {text}");
            // And the text is a fixed point: printing what came back writes the same bytes.
            assert_eq!(to_string(&back), text, "seed {seed} round {round}");
        }
    }
}

/// Writer, reader, writer, reader… until the bytes stop changing. Two round trips are always
/// enough, and the third is there to say so: `-0.0` is the longest chain there is — it is written
/// `-0`, read back as an `Int`, written `0`, and read back as a `Uint`, which is where it stays.
fn settles(v: &Value, from: &str) {
    let mut text = to_string(v);
    for _ in 0..3 {
        let again = to_string(&parse(text.as_bytes()).unwrap_or_else(|e| panic!("{e}: the writer wrote something the reader will not take, from {from:?}")));
        if again == text {
            return;
        }
        text = again;
    }
    panic!("{from:?} never settles: {text}");
}

/// The three values the writer deliberately does not keep the type of. They are here so that
/// changing any of them is a decision somebody makes, not a round trip that quietly stops holding.
#[test]
fn what_the_writer_does_not_promise_to_keep() {
    // A non-negative `Int` is digits on the wire, and digits read back as `Uint`.
    assert_eq!(parse(to_string(&Value::Int(5)).as_bytes()).unwrap(), Value::Uint(5));
    assert_eq!(parse(to_string(&Value::Int(-5)).as_bytes()).unwrap(), Value::Int(-5), "a negative one keeps its type");
    // A float that happens to be whole is written without a point, and reads back as an integer.
    assert_eq!(parse(to_string(&Value::Float(3.0)).as_bytes()).unwrap(), Value::Uint(3));
    assert_eq!(parse(to_string(&Value::Float(3.5)).as_bytes()).unwrap(), Value::Float(3.5));
    // Negative zero is the one that takes two trips to settle; every reader in kiki takes a
    // number however it is spelled (`bench::as_f64`, `mirror::Spec::from_json`).
    assert_eq!(parse(to_string(&Value::Float(-0.0)).as_bytes()).unwrap(), Value::Int(0));
    settles(&Value::Float(-0.0), "-0.0");
    // JSON has no NaN and no infinity: they go out as null rather than as something no reader
    // on the other end could take.
    for x in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        assert_eq!(to_string(&Value::Float(x)), "null");
    }
}

/// The same string written the long way round: every character as a `\uXXXX` escape, which for
/// anything above the basic plane is a **surrogate pair**. The writer never spells a string like
/// this — it puts the character in as itself — so nothing else reaches the reader's escape path,
/// and it is the path every other JSON writer in the world sends kiki's plugins through.
#[test]
fn a_string_spelled_in_escapes_reads_as_the_string() {
    let mut r = Rng(0xE5CA_9E00_0000_0001);
    for _ in 0..500 {
        let s = string(&mut r);
        let mut text = String::from("\"");
        for unit in s.encode_utf16() {
            text.push_str(&format!("\\u{unit:04x}"));
        }
        text.push('"');
        assert_eq!(parse(text.as_bytes()).unwrap_or_else(|e| panic!("{e}: {text}")), Value::Str(s.clone()), "{text}");
        // And in a document, where the escape is followed by more to read.
        let doc = format!("{{\"k\":{text},\"after\":1}}");
        let v = parse(doc.as_bytes()).unwrap_or_else(|e| panic!("{e}: {doc}"));
        assert_eq!(v.str_field("k"), Some(s.as_str()));
        assert_eq!(v.u64_field("after"), Some(1), "the reader knows where the string ended");
    }
    // The pieces of a surrogate pair, spelled out, so a failure names the case rather than a seed.
    let escaped = |hex: &str| parse(format!("\"{hex}\"").as_bytes()).unwrap_or_else(|e| panic!("{e}: {hex}"));
    assert_eq!(escaped(r"\ud83d\ude00"), Value::Str("\u{1f600}".into()), "a grinning face, as a surrogate pair");
    assert_eq!(escaped(r"\ud834\udd1e"), Value::Str("\u{1d11e}".into()), "and a treble clef");
    assert_eq!(escaped(r"\u00e9\u0041"), Value::Str("\u{e9}A".into()));
    assert_eq!(escaped(r"\/\b\f\n\r\t"), Value::Str("/\u{8}\u{c}\n\r\t".into()), "and the short escapes");
    // The same characters as themselves, which is how the writer spells them.
    assert_eq!(parse("\"\u{1f600}\u{1d11e}\u{e9}A\"".as_bytes()).unwrap(), Value::Str("\u{1f600}\u{1d11e}\u{e9}A".into()));
}

/// Mangled input: flip a byte, drop one, add one, cut the tail off. None of it may panic, and
/// whatever is accepted must survive a second trip through the writer unchanged.
#[test]
fn mangled_input_is_refused_or_read_but_never_panics() {
    let mut r = Rng(0x5EED_1234_5678_9ABC);
    let mut accepted = 0usize;
    for _ in 0..200 {
        let text = to_string(&value(&mut r, 0));
        let base = text.as_bytes();
        for _ in 0..25 {
            let mut bytes = base.to_vec();
            if bytes.is_empty() {
                continue;
            }
            match r.below(5) {
                0 => {
                    let at = r.below(bytes.len() as u64) as usize;
                    bytes[at] ^= 1 << r.below(8);
                }
                1 => {
                    let at = r.below(bytes.len() as u64) as usize;
                    bytes.remove(at);
                }
                2 => {
                    let at = r.below(bytes.len() as u64 + 1) as usize;
                    bytes.insert(at, r.below(256) as u8);
                }
                3 => bytes.truncate(r.below(bytes.len() as u64) as usize),
                _ => {
                    let at = r.below(bytes.len() as u64) as usize;
                    let b = bytes[at];
                    bytes.insert(at, b);
                }
            }
            if let Ok(v) = parse(&bytes) {
                accepted += 1;
                settles(&v, &String::from_utf8_lossy(&bytes));
            }
        }
    }
    assert!(accepted > 0, "every mangled document was refused: the mutations are not reaching the reader");
}

/// Pure noise, including bytes that are not UTF-8 at all, and the shapes that tempt a reader into
/// running off the end of its buffer: a lone escape, a short `\u`, an unterminated string, nesting
/// past the depth cap.
#[test]
fn noise_is_refused_without_panicking() {
    let mut r = Rng(0xD15E_A5E0_0000_0001);
    for _ in 0..4000 {
        let n = r.below(48) as usize;
        let bytes: Vec<u8> = (0..n).map(|_| r.below(256) as u8).collect();
        let _ = parse(&bytes);
        // And the same noise inside a string and inside an object, where a reader has more state
        // to lose track of.
        let mut wrapped = b"{\"k\":\"".to_vec();
        wrapped.extend_from_slice(&bytes);
        wrapped.extend_from_slice(b"\"}");
        let _ = parse(&wrapped);
    }
    for bad in [
        &b"\""[..],
        b"\"\\",
        b"\"\\u",
        b"\"\\u00",
        b"\"\\ud83d",
        b"\"\\ud83d\\u0041\"",
        b"\"\\udc00\"",
        b"{",
        b"{\"a\"",
        b"{\"a\":",
        b"[",
        b"[,]",
        b"-",
        b"-.",
        b"1.2.3.4",
        b"\xff\xfe",
        b"\"\xff\"",
        b"tru",
        b"{}{}",
        b"[] []",
    ] {
        assert!(parse(bad).is_err(), "{:?} should be refused", String::from_utf8_lossy(bad));
    }
    let deep = format!("{}{}", "[".repeat(2000), "]".repeat(2000));
    assert!(parse(deep.as_bytes()).is_err(), "nesting past the cap is refused, not a stack overflow");
    let deep_obj = format!("{}{}", "{\"a\":".repeat(2000), "1".to_string() + &"}".repeat(2000));
    assert!(parse(deep_obj.as_bytes()).is_err());
}
