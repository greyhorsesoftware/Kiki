# 08 — CI builds once, and the suite runs in less time

**Status:** built, 2026-10-02 — one run read with a stopwatch, then the changes it named; the
workflow edits are **unverified on Actions until the owner's next push** (nothing here can run
them), everything they do is verified on this machine below. Gate on this tree: `make lint`
clean; `cargo test --release --locked` under makepkg's flags 265 / 0; `make test-qml` 845 + 12
/ 0; the suite in its two halves, `local` 326 / 0 / 1 (pointer_ops) and `servers` 465 / 0 / 0
— 791 together, the whole suite's count. Owner, 2026-09-26: "in our CI we
build multiple times — why?"

## Read on 2026-10-02

The latest green CI run (`36480358949`, "release fixins", 2026-09-28) and the release run that
followed it (`36484328434`, v0.4.1), step by step, from the Actions API:

| CI step | x86_64 | aarch64 |
|---|---|---|
| Install toolchain | 25 s | 30 s |
| Format and lint (clippy, a dev build of everything) | 71 s | 52 s |
| Unit and integration tests (`cargo test`, dev) | 168 s | 126 s |
| Contract tests (`--include-ignored`, dev) | 23 s | 21 s |
| makepkg (`cargo build --release` from cold, then package) | **294 s** | **159 s** |
| namcap, desktop entry | 2 s | — |
| Dependency audit (three `cargo test`s, dev) | 32 s | — |
| the job | **11 min 01 s** | 7 min 07 s |

Then `store-package` 1 min 15 s (needs `rust`) and `qml` 1 min 50 s beside it: the run is
**12 min 23 s**, of which the x86_64 `rust` job is the critical path and the release build
inside it the largest single piece.

| Release step | |
|---|---|
| pick | 12 s |
| drive: install runtime 26 s, the suite under cage **170 s** | 3 min 42 s |
| publish | 16 s |
| checksums: digests 19 s, store package from the asset 17 s, commit 2 s | 1 min 11 s |
| the workflow | **5 min 32 s** |

Two things the plan's "today" had wrong, found by reading rather than remembering: the release
workflow **builds nothing** — it takes CI's artifacts, drives one, tags and publishes, and the
checksums job's `makepkg -f -s` repackages a downloaded asset in 17 s — so decision 3 (a warm
cache for the release build) had nothing to warm and is dropped. And there is no e2e job in
`ci.yml` at all: the suite runs once, in the release's `drive` job, against the installed
package. Decision 4 applies there.

## Decisions, as built

1. **Read one run** — above.
2. **Tests on the release profile, once** (`ci.yml`). The `rust` job's three test steps and the
   audit's three run `cargo test --release --locked`, and they run **under makepkg's own
   compiler flags** (`.github/as-makepkg.sh`: `load_makepkg_config` and an export of `CPPFLAGS
   CFLAGS CXXFLAGS LDFLAGS RUSTFLAGS`, the way makepkg itself sets them; nothing appended, the
   PKGBUILD's `!lto` and `!debug` being the two options that would). That second half is the
   finding of the day: cargo fingerprints every crate by `RUSTFLAGS`, makepkg exports
   `-C force-frame-pointers=yes` from `makepkg.conf.d/rust.conf`, and a release build made
   without it is not the release build makepkg wants — it was throwing the test build away and
   compiling everything again, which is the "builds multiple times" the owner saw, and which
   the plan's own "today" misdiagnosed as a dev/release pair. Measured here, on this
   machine: `cargo test --release --locked` with the flags, then `KIKI_LOCAL_SRC=1 makepkg -f
   --nodeps --noconfirm --nocheck` over the same `target` — **0 `Compiling` lines, `Finished`
   in 0.15 s**; without the flags, every crate compiled again (`kiki-json`, `kikid`, the SDK,
   every plugin). The makepkg step itself is unchanged; its `build()` is now the no-op it was
   meant to be, and the package is the binaries the tests ran. Expected on Actions: the 168 s
   dev test build and the 294 s release rebuild become one release compile, in the test step;
   the lint step's clippy build (dev, a check, no link) stays. Eleven minutes to about six is
   the honest guess; the first push says.
3. **Dropped** — see above. What was in its place was wrong: the checksums commit on main read
   "Release v: checksums", the version empty, because `VERSION` was an `env` of the job's first
   step alone and the commit step read nothing. It is the job's `env` now, so the next release
   commits as "Release v0.5.0: checksums".
4. **The suite in two halves** (`tests/e2e/driver.py` `PARTS`, `run.sh --part`, `release.yml`).
   `servers` is the seven flows that start a real server — `sshd`, `vsftpd`, `smbd` or a
   session bus: `remote_transfers`, `side_by_side`, `drag_between_panes`, `quick_look`,
   `dbus_activation`, `mirror_sftp`, `smb` — and `local` the other twenty-one. The driver
   asserts at import that every flow is in exactly one half, so a flow added to `FLOWS` cannot
   fall through; `--part` with any other name is an error. The release's `drive` job is a matrix
   of the two, each its own machine with its own cage, daemon and ports; `publish` waits for
   both. Verified here, one after the other (never two cages at once): counts below, their union
   the full suite's 791. The two halves are not equal in time — the servers half is the heavier
   by its fixtures — but the longest job is shorter than the whole, which was the point.

## Verification

`make lint`; `cargo test --release --locked` with `.github/as-makepkg.sh` sourced; `make
test-qml`; `tests/e2e/run.sh --part local` then `--part servers`; the makepkg no-rebuild check
(0 `Compiling` lines over the test build). Counts in the status above are from the day this
was built. One flake seen on the way and left: `kikid/tests/open.rs`
`default_opens_a_local_file…` failed once at its five-second wait for the stub application's
log under a full release-profile run and passed three reruns alone and the next full run —
pre-existing, a timing wait, not this plan's. On Actions, the next push's `rust` job and the next release's `drive`
matrix are the proof; if the release-profile test step is slower on a runner than the two
builds were, that is a finding for this file, not a reason to keep both.
