# kiki

A fast file manager for [Omarchy](https://omarchy.org): a lean Rust daemon (`kikid`) does the work, a
Quickshell (QML) front end draws it. Locations (SFTP, FTPS), one-way mirroring, search over a name
index, git badges, project mode beside your editor and agent, sharing, and Jarvis, which asks the AI you use in Omarchy about a file.

## Layout

| Path | What |
|---|---|
| `kikid/` | the daemon: string-pool listings, windows, jobs and undo, archives, mirror, index, git, plugin and helper hosts |
| `crates/kiki-json`, `crates/kiki-plugin-sdk` | the shared JSON reader/writer and the SDK for location and share plugins |
| `plugins/` | every plugin, one process each: locations (`sftp`, `ftps`), services (`dbus`, `highlight`), share (`share-mail`, `share-tailscale`, `share-localsend`); the stub lives in `kikid/src/bin/` |
| `qml/` | the Quickshell shell |
| `tests/` | QML component tests and the e2e harness (`tests/e2e/run.sh`) |
| `docs/0.1.0/` | the core plan, feature plans and the two API documents |
| `docs/design/` | mockups (`gen.py` regenerates the artboards) |
| `packaging/` | PKGBUILD, systemd units, desktop entry, portal registration |

## Build and run (Omarchy)

```
cargo build --release
KIKI_PLUGIN_DIR=target/release target/release/kikid &
qs -p qml/shell.qml
```

The default build ships the `sftp` and `ftps` location plugins. Which protocols a build speaks is `plugin::LOCATION_KINDS` in `kikid/src/plugin.rs` plus the workspace's `default-members`; the device (`mtp`, `ptp`, `afc`) and GIO (`smb`, `dav`, `afp`) plugins stay in the tree and build with `cargo build --release -p kiki-plugin-<name>` once both lists name them. See `docs/0.1.0/06-remote-locations.md`.

Benchmarks: `kikid bench gen all /tmp/kb && kikid bench run /tmp/kb --json out.json`, then `kikid bench compare bench/baseline-<os>-<arch>.json out.json` (plan 26).

Tests: `make test` runs all three suites — `make test-rust`, `make test-qml` (leaf and interaction tests, no compositor) and `make test-e2e` (a real daemon and a real tree; the flows that drive the shell need `cage`, and are skipped by name without it).

Packaging: `cd packaging && makepkg -f`. See `docs/0.1.0/10-polish-and-packaging.md`.

## Status

Plans 01 to 20 in `docs/0.1.0/CORE.md` are generated except 17 (devices), which needs the device
libraries on an Omarchy machine. The Rust side is type-checked for Linux and unit-tested on macOS;
the QML has not yet been run under Quickshell. First run on Omarchy: verify the Quickshell socket
parser names in `qml/kiki/Daemon.qml` and the inotify event parsing in `kikid/src/watch.rs`.
