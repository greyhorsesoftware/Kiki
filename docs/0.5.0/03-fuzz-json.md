# 03 — Fuzz the JSON reader for real

**Status:** planned, 2026-09-29. From the audit (`docs/audit-2026-09-18.md`, §4): "the JSON
parser that reads every frame from the socket and every plugin is not fuzzed", and CI has a
step that suggests it is — `cargo +nightly fuzz run json_reader … || echo "::warning::fuzz
target not present yet"` — over a `fuzz/` directory that has never existed. A step that always
warns is worse than no step: it reads as coverage.

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
