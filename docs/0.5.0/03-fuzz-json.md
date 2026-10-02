# 03 — Fuzz the JSON reader for real

**Status:** built, 2026-10-02. `fuzz/` (outside the workspace; `make fuzz`, and CI's `fuzz` job on
x86_64, a minute per target, failing on a crash), two targets with a checked-in corpus, run here
for thirty minutes each. **What the first run found, in its first minutes:** no crash — the
round-trip invariant failed three ways, all in numbers, all fixed in `kiki-json` with a unit test
naming each: a whole-valued float (`1e3`) was written `1000` and read back as an integer (the
writer now keeps the point); `-0` parsed as `Int(0)`, was written `0` and read back as `Uint(0)`
(it is nought, a `Uint`, from the start); and a number too large for a double (`1e400`) parsed as
infinity, which the writer put down as `null` — a daemon writing `null` where it read a number
(refused now, like any number the reader cannot hold). `crates/kiki-json/tests/property.rs` had
pinned two of those as "what the writer does not promise to keep", a decision made before there
was a fuzzer to argue with; it pins the stronger promise now, with the reason. The depth bound
was also one more than its name said (`depth > 64` from 0) and is exact now, with a test either
side of it; a frame one byte over either size bound is refused on its prefix with nothing
allocated, with a test. Fuzzing after the fixes: `json_reader` ran thirty minutes, 20.3 million inputs, nothing found. The frame reader
(`frame_reader`): thirty minutes, 14.6 million inputs, nothing found — a length over the
bound is refused on the prefix before anything is allocated, as it was. Nothing in the two readers panicked at any point.

## Today

`crates/kiki-json` is kiki's own parser — 449 lines, `MAX_DEPTH 64`, no serde — and every
frame from a window and every reply from a plugin goes through it. It has three unit tests and
the trust of everything above it. A window is the user's own process and a plugin is kiki's
own binary, so the *attacker* is a malformed frame from a buggy peer rather than a hostile
one; but `panic = "abort"` makes any panic in the parser the end of the daemon and every
window on it.

## Decisions

1. **`cargo-fuzz` in the repository**, `fuzz/fuzz_targets/json_reader.rs`: bytes in, `parse`
   called, and the invariant that whatever it returns re-serialises and re-parses to itself
   (`to_string` then `parse` equal). A second target for the frame reader in `proto.rs`, which
   takes a length prefix and a body and must reject a length over the bound before allocating.
2. **A corpus that is checked in**: the fixtures from the unit tests, one frame of every verb
   in `API-DAEMON.md` (generated once from the e2e harness's `driver.log`), and whatever the
   first hour of fuzzing finds interesting. Crashes found are minimised and added as unit tests
   in `kiki-json` before they are fixed, so the fix has a name.
3. **CI runs it for one minute on x86_64 only**, without `continue-on-error`, and fails the job
   on a crash. The warning-that-is-not-coverage goes. Nightly is needed for `cargo fuzz`; the
   toolchain line is pinned in the workflow and nowhere else.
4. **Depth and size bounds are asserted, not just present.** A frame nested 65 deep and a
   frame one byte over the limit each have a unit test that expects the numbered rejection.

## Size

A day: the targets and corpus an hour, the CI step an hour, and the rest for whatever the
first run finds — which is the point. If the first run finds nothing after an hour, that is
recorded in this plan's status line, and the step stays.
