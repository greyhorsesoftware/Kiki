| name | code_audit |
| --- | --- |
| description | Performs a five-dimension audit of the kiki codebase — a Rust daemon, plugin processes over pipes, a Quickshell/QML shell, and Arch packaging — covering security, code quality, performance, maintainability and operational readiness, producing evidence-cited findings, a severity scorecard, and a prioritized remediation list. Use when asked to audit, review, or assess the health, risk, or release-readiness of this repository. |

# Five-Dimension Codebase Audit (kiki)

## Purpose

Audit this repository across five dimensions and one adaptive deep-dive, and report
findings that can each be traced to a file. The audit is a reading-and-judgment
exercise, not a scan: its value comes from citing literal values, naming what is
absent that should be present, and stating plain verdicts the team can act on.
Rate each dimension by its worst unmitigated finding.

## What this codebase is

Read `docs/0.1.0/CORE.md` first; it fixes the architecture. In one paragraph:

- **`kikid/`**: a Rust daemon (std, `rustix`, `libc`, own JSON in `crates/kiki-json`,
  no async runtime) serving a Unix socket at `$XDG_RUNTIME_DIR/kiki.sock` with
  newline-delimited JSON for the shell and length-prefixed frames for plugins. Threads:
  reader and writer per client, scanner per listing, stat and thumbnail pools, an
  inotify watcher, job threads, index and device threads. Listings live in a string
  pool with lazy viewport windows.
- **`crates/kiki-plugin-sdk`** and **`plugins/`**: location plugins (`sftp`, `ftps`,
  `gio` — installed as `kiki-plugin-smb`; its `dav` and `afp` arms are in the source and
  no build runs it under those names — plus `mtp`, `ptp`, `afc`, which are in the tree
  and do not ship), the `dbus` service plugin, and share plugins (`share-mail`,
  `share-tailscale`). Each is a separate process spawned on use, spoken to over
  stdin/stdout frames, running concurrently inside the plugin with cancellation. The
  device plugins hand-write `extern "C"` FFI; `dbus` uses zbus; `sftp`/`ftps` use tokio,
  russh and suppaftp; the share plugins spawn CLIs or speak SMTP over rustls.
  **0.1.0 ships three location kinds — `ftps`, `sftp`, `smb` (`plugin::LOCATION_KINDS`)
  — and two share plugins.** WebDAV, AFP, LocalSend, MTP, PTP and AFC are not in it; a
  plan that still describes one describes what was cut. `kiki-thumber` is a fourth kind
  of child: it speaks the same framing and makes every thumbnail, so the daemon decodes
  no file itself.
- **`qml/`**: the Quickshell shell. Singletons `Daemon` (socket), `Settings`, `Theme`,
  `Jobs`, `Format`; `Pane`, `WindowCache`, `Selection`; views and dialogs under
  `ui/` and `views/`. Leaf components import only QtQuick.
- **Integration**: `integrate.rs` edits the user's `mimeapps.list`, D-Bus activation
  files, Hyprland bindings and portal config; `dbus.rs` bridges FileManager1 and the
  portal FileChooser; `desktop.rs` launches desktop entries; `config.rs` runs
  `udisksctl`/`lsblk`; `locations.rs` stores secrets through `secret-tool`.
- **Delivery**: `packaging/PKGBUILD` (x86_64 and aarch64), AUR `kiki-bin`, systemd
  user socket and service, `.github/workflows/ci.yml` and `release.yml`,
  `bench/` baselines from `kikid bench`.

## Desired Outcomes

1. Every finding cites a file, and quotes the literal line or value where one exists
2. Absent controls (frame size limits, path guards, timeouts, cancellation, fail-fast
   config) are reported as findings, not omitted
3. Packaging, units, workflows and the integration writers are read as primary sources
   alongside `kikid/src`
4. Inferences are marked as inferences; undetermined items appear under "Not Verified"
5. Each dimension carries a rating from the scale below, set by its worst finding
6. The adaptive section (§6) is the daemon-and-plugin-host deep-dive unless a different
   question is clearly higher-stakes
7. Output ends with a single prioritized, cross-dimension remediation list of 8–12 items

## Data Sources

- **The repository itself** is the only required input. Read it directly.
- **The plans**: `docs/0.1.0/*.md`, `API-DAEMON.md` and `API-PLUGIN.md` state what
  the code is supposed to do. A gap between a plan's claim and the code is a
  maintainability finding; a plan describing an unsafe design is a security one.
- **The benchmarks**: `bench/baseline-*.json` and `kikid/src/bench.rs` give measured
  numbers; quote them rather than guessing about performance.
- **Tests**: `cargo test` output (natively: exclude the four Linux-only plugin crates),
  the mock-server suites under `plugins/*/tests`, `tests/qml`, `tests/e2e`.
- **This skill**: ground rules, rating scale, output structure and checklists below.

## Ground rules — these determine whether the audit is useful

1. **Every finding cites evidence.** Name the file, and quote the literal value or
   line where one exists (`MAX_CONCURRENT: usize = 8`, `frame too large` at
   16 MiB, `REQUEST_TIMEOUT`, `set_read_timeout(Some(Duration::from_secs(60)))`).
   A finding you cannot trace to a file is worthless.
2. **Absence is a finding.** The most valuable rows are things that should exist
   and don't — no bound on a buffer fed by another process, no timeout on a plugin
   reply, no guard on a delete path, no test for a parser that reads untrusted
   bytes. Search for each item in the Absence Checklist and report what is missing.
3. **Read the delivery layer as a primary source.** `Cargo.toml` and `Cargo.lock`,
   `packaging/`, the systemd units, the workflows and `rustfmt.toml` carry findings
   that never appear in `src/`.
4. **Verify, don't assume.** If you claim a guard is absent, grep the crate. If you
   claim a socket message can crash the daemon, trace `server.rs` dispatch to the
   parser and the handler and show the unchecked step.
5. **Mark inference explicitly.** Where you infer rather than confirm, say so. Do
   not invent file names, constants or versions. Anything that needs a running
   Omarchy (Quickshell behaviour, D-Bus, real devices) goes under "Not Verified".
6. **State verdicts plainly.** "This daemon is safe against a malicious plugin" or
   "a plugin can hold the writer thread forever" — no hedging, no advice that
   applies to every codebase.
7. Skip `target/`, generated artboards under `docs/design/*.dc.html` and lockfile
   *contents* — but note committed build artifacts or conflicting lockfiles.

## Rating scale

| Marker | Meaning |
|---|---|
| ✅ GOOD | No material concerns |
| 🔵 LOW | Minor; cleanup, not risk |
| 🟡 MEDIUM | Should be fixed; not urgent |
| 🟠 MEDIUM-HIGH | Fix before the next significant change |
| 🔴 HIGH | Fix now; exploitable, data-losing, or will crash the daemon |

Rate the dimension by its **worst unmitigated finding**, not by an average.

## Output structure

````markdown
# Five-Dimension Codebase Audit: kiki

**Branch:** `<branch>`
**Date:** <today>
**Components:** kikid <version> (Rust <toolchain>), plugin SDK, <n> plugins, QML shell (Qt <version> / Quickshell)
**Targets:** x86_64 and aarch64 Linux (Arch/Omarchy); macOS for tests only
**Delivery:** pacman package from PKGBUILD, AUR kiki-bin, systemd user socket + service

---

## Summary Scorecard

| Dimension | Rating | Key Risk |
|-----------|--------|----------|
| **Security** | <rating> | <one sentence — the single worst thing> |
| **Code Quality** | <rating> | <one sentence> |
| **Performance** | <rating> | <one sentence> |
| **Maintainability** | <rating> | <one sentence> |
| **Operational Readiness** | <rating> | <one sentence> |

---

## Purpose

Three to five lines: what the daemon, the plugins and the shell do and what
depends on them (the portal, FileManager1 callers, Hyprland keybindings).
Derived from the code, not from the plans' aspirations.

---

## 1. Security

Cover, as tables wherever the content is comparable:
- **Trust boundaries** — the socket (any local process in the user's session can
  connect), plugin stdout (a plugin is another process; its frames are input),
  D-Bus callers of FileManager1 and the portal, URIs from other apps
  (`x-scheme-handler`, `xdg-open`), the files being listed (names are bytes).
  For each: what parses it, what bounds it, what happens on malformed input.
- **Filesystem safety** — every delete, rename, trash, restore, mirror-delete and
  integration write: path traversal guards (`..`, absolute paths, symlinks),
  atomic writes, the blast-radius and empty-master refusals in `mirror.rs`, the
  trash restore target.
- **Command construction** — every `Command::new` and every remote shell string
  (the SFTP `find`/`stat` commands and `shell_quote`, `udisksctl`, `xdg-mime`,
  `hyprctl`, `secret-tool`, share CLIs): is user data ever interpolated unquoted?
- **Secrets** — keyring only via `secret-tool`; nothing in `locations.toml`,
  logs, JSON replies, or `ps` argv; passwords passed to plugins only in `Connect`.
- **Transport** — TLS in `ftps` and `share-mail`: pinning, verification, the
  fallback to web PKI, and whether a cached session can be handed to a server it
  was not made for; SSH host-key handling in `sftp`.
- **`unsafe` and FFI** — every `unsafe` block in `kikid` (`libc`, `mallopt`,
  `getdents64`, `lseek`, `pre_exec`) and the device plugins' hand-written
  struct layouts: what happens if a layout is wrong, and whether the plan says
  so.
- **Integration writers** — `integrate.rs`: does removal restore exactly, does a
  failed Hyprland reload roll back, can a crafted config value escape a line?

## 2. Code Quality

- **Structure** — a directory tree annotated with each crate's and module's role.
- **Findings table** — `unwrap`/`expect` on paths a peer controls, `Mutex` locks
  held across blocking I/O or plugin calls, cloned `Value` trees on hot paths,
  duplicated framing or JSON code between `kikid`, the SDK and the stub plugin,
  dead protocol messages, modules over ~1,000 lines (`listing.rs`, `mirror.rs`,
  `server.rs`) and whether they split along a seam. Note what is done *well*.

## 3. Performance

- **The listing path** — string pool, two-phase readdir, windows, sort keys,
  parallel sort threshold, incremental patch: quote the constants and the
  `bench/` numbers, and say whether the plan-01 budgets are met on the recorded
  baseline.
- **Allocation discipline** — `mallopt` settings, reused buffers (getdents,
  copy, thumbnail), `Value` construction per row per window, arena use in
  `string_pool.rs`, `index.rs`, `mirror.rs`.
- **Concurrency limits** — stat pool size, thumbnail pool, job slots, plugin
  `MAX_CONCURRENT`, `READ_IN_FLIGHT`, the per-client writer channel: bounded or
  unbounded, and what backs up when the shell stops reading.
- **Timeouts and cancellation** — `REQUEST_TIMEOUT` on plugin replies, the probe
  timeout, cancellation flags on scans, jobs and mirror runs: each present or
  ABSENT. Any plugin call with no timeout is a finding.
- **Memory** — `rss_*` metrics in the baseline; unbounded growth candidates
  (listing cache cap, journal cap, access-log compaction, index size).

## 4. Maintainability

- **Docs versus code** — the plans, `API-DAEMON.md` and `API-PLUGIN.md` against
  `server.rs` dispatch and the SDK: messages documented but unhandled, handled
  but undocumented, fields renamed on one side.
- **Tests** — unit (`cargo test -p kikid`), the SDK loop test, mock-server suites
  (SFTP, FTPS, SMTP), the real-server suites (`sshd`, `vsftpd`, `smbd` with GVfs),
  the stub-plugin contract test, QML leaf tests with the Quickshell stubs, the real
  drags under Qt's `minimal` platform (`tests/qml-drag`), and the e2e harness under
  `cage`. Assess whether tests assert behaviour or execute lines; cite one strong and
  one weak example. Name what has never run (the device plugins; the physical drag).
- **Dependencies table** — crate versions with release ages, crates pulled by a
  single plugin, `rustls` with `ring` versus `aws-lc`, tokio confined to plugins,
  duplicated versions in `Cargo.lock`.
- **Build & tooling** — `rustfmt.toml`, `cargo clippy -D warnings` in CI, the
  macOS exclude list, whether the lint job gates merges, the `paths-ignore`.

## 5. Operational Readiness

- **Logging** — where `eprintln!` goes under systemd (the journal), levels,
  whether URIs or secrets appear in output, plugin stderr inheritance.
- **Startup and failure behaviour** — socket activation versus manual bind,
  config parse failure at boot (fail fast or silent defaults), a missing plugin
  binary, a crashed plugin (respawn, failed request reported), a crashed shell
  (listings and jobs survive in the daemon?), index rebuild cost on first start.
- **Deployment** — PKGBUILD install layout, the units, `kiki.install` hooks,
  portal and D-Bus files, the AUR package, the release workflow and how a bad
  release is rolled back (previous package in the pacman cache).
- **Configuration** — `settings.toml`, `locations.toml`, `views.toml`,
  `integration.toml`, `devices.toml`, the access log, the index file: written
  atomically or not, tolerant of a partial write, cleared by Reset all.
- **Observability** — `Ping`, `Version`, `About`, `PluginStatus`, job events,
  the Activity popover: is there enough to diagnose a stuck plugin without a
  debugger?

## 6. Daemon and Plugin Host — Isolation and Protocol Robustness

The deep-dive that matters most for this codebase. A factor-by-factor table:

| Factor | What to check |
|---|---|
| Frame parsing | `proto.rs` and the SDK reader: length cap (16 MiB), type byte, JSON grammar limits (depth, string length), behaviour on a truncated frame |
| Reply matching | pending-request maps keyed by id: a reply with an unknown id, a duplicate id, a reply that never comes (`REQUEST_TIMEOUT`), binary frames when no stream is active |
| Stream discipline | one binary stream per pipe: what enforces it on both sides, and what happens if a plugin interleaves |
| Cancellation | `Cancel { target }` end to end: reader thread, flag, handler loop, daemon side on listing close and job cancel |
| Process lifecycle | spawn on use, idle reaper (5 min), `Shutdown` then kill, respawn after crash, `dbus` plugin kept alive, zombie handling |
| Concurrency | `MAX_CONCURRENT` workers, session mutexes in `sftp`/`ftps`/`gio`, the current-thread versus multi-thread tokio runtimes |
| Resource bounds | listing cache entries, journal, access log, index, thumbnail cache, temp files from mirror and share |
| Failure reporting | typed error codes (`NotFound`, `Denied`, `Auth`, `Network`, `Cancelled`) preserved from plugin to shell to toast |

End with a plain verdict: can a misbehaving plugin take the daemon down, and can
a misbehaving shell client starve other clients?

If a different question is clearly higher-stakes for the change under review
(for instance the Hyprland integration writer), pick that instead and say why
in one line.

## Priority Remediation

A single numbered list across all dimensions, ordered by severity then effort,
each tagged 🔴/🟡/🔵. Each item is one actionable sentence naming the file or
component to change. Aim for 8–12; fewer if the codebase is clean.

## Not Verified

Anything you could not determine and why (needs Omarchy, needs a real server or
device, needs a running session bus). Keep this section even when short — its
absence implies false completeness.
````

## Where to look first

Read whichever of these exist before opening application code:

- **Architecture:** `docs/0.1.0/CORE.md`, `API-DAEMON.md`, `API-PLUGIN.md`
- **Manifests:** root `Cargo.toml` (workspace members, profile), `kikid/Cargo.toml`,
  `crates/*/Cargo.toml`, `plugins/*/Cargo.toml`; `Cargo.lock` for duplicate
  versions only
- **Entry points:** `kikid/src/main.rs` (socket activation, `mallopt`, threads
  started), `server.rs` (dispatch table), `crates/kiki-plugin-sdk/src/lib.rs`
  (`run_on`, `dispatch`), `qml/shell.qml` and `qml/kiki/Daemon.qml`
- **Trust boundaries:** `proto.rs`, `plugin.rs`, `dbus.rs` and
  `plugins/kiki-plugin-dbus`, `desktop.rs`, `vfs/uri.rs`
- **Filesystem writers:** `ops.rs`, `jobs.rs`, `mirror.rs`, `integrate.rs`,
  `config.rs`, `index.rs`, `access.rs`
- **Delivery:** `packaging/PKGBUILD`, `packaging/systemd/*`, `packaging/*.service`,
  `packaging/*.portal`, `.github/workflows/*.yml`, `rustfmt.toml`
- **Tests:** `kikid/tests/`, `plugins/*/tests/`, `tests/qml/`, `tests/e2e/`,
  `bench/`

## Absence Checklist — confirm each present or ABSENT

Frame length cap on both sides of every pipe · JSON depth/size limits ·
timeout on every plugin reply · cancellation on every long loop · bounded
channels to the shell writer · typed errors end to end · path traversal guard on
every delete and restore · atomic writes for every config file · rollback on a
failed Hyprland reload · secrets never on disk or in argv · TLS verification or
an explicit, scoped exception · host-key pinning · `unsafe` blocks each with a
safety comment · FFI layouts flagged as unverified · plugin respawn after crash
· idle reaping · socket activation · fail-fast on an unreadable settings file ·
listing cache cap · journal cap · log compaction · thumbnail cache bound ·
clippy and rustfmt gating CI · a test for every parser that reads untrusted
bytes · a mock-server test per network plugin · QML leaf tests runnable without
Quickshell · benchmark baseline per target · release rollback path

Do not pad the audit by listing every checklist item. Report the ones that are
absent **and matter**, plus the ones whose presence is genuinely reassuring.
