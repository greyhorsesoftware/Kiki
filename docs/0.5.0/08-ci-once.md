# 08 — CI builds once, and the suite runs in less time

**Status:** planned, 2026-09-29. Owner, 2026-09-26: "in our CI we build multiple times — why?"
The honest answer starts with a stopwatch: this plan is an hour of reading one run's timings
before anything is changed, then the two or three changes the timings name.

## Today

`.github/workflows/ci.yml`: a `rust` job per architecture (x86_64 and aarch64 in a container)
that runs `cargo test`, then `cargo test --test '*' -- --include-ignored` for the contract
tests, then `makepkg` with `KIKI_LOCAL_SRC=1 --nocheck` into the same cached `target/` (so the
package build reuses the test build's artefacts where the profile matches — tests are `dev`,
the package is `release`, so it does not), then the dependency audit's three `cargo test`
runs; a `store-package` job that needs `rust`; a `qml` job; the e2e job against the installed
package. `release.yml` builds the release again per architecture from cold, then the
checksums job builds the store package once more.

So one push builds the daemon in `dev` once (tests), in `release` once per architecture
(package), and a tag builds `release` again. The `dev`/`release` pair is inherent unless the
tests run against a release build (slower to compile, faster to run — and `PKGBUILD`'s own
`check()` already does exactly that, which is why CI passes `--nocheck`). The release-from-cold
is the cache not carrying across workflows.

## Decisions

1. **Read one run.** The timings of each step on both architectures into this file, before
   anything moves. If the `dev` build dominates, decision 2; if the release rebuild does,
   decision 3; if the e2e job does, `09`.
2. **Tests on the release profile, once.** `cargo test --release --locked` (what `check()` runs)
   and the package built from the same `target/release`; the `dev` build goes. The test run is
   longer to compile and shorter to run; on a cached runner the compile is mostly cached.
3. **The release workflow restores the CI cache** (`actions/cache` keyed on `Cargo.lock` and the
   architecture, shared between workflows by key) so a tag's build starts warm — the store
   package step then repackages the asset it already has, which it does.
4. **The e2e suite in two `cage`s.** The flows that need no real server (most) and those that
   do (`mirror_sftp`, `real_smb`, the ftps ones) are already partitioned by `NEEDS`; two
   runners each taking one half, with the daemon per `cage` (the harness starts its own),
   halves the wall time of the longest job. The harness's "never two suites at once" rule is
   about one machine; two runners are two machines.

## Size

The reading an hour; 2 and 3 half a day together; 4 half a day. A CI run that is twenty
minutes shorter pays this back in a week.
