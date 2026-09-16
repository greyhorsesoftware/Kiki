# kiki

A fast file manager for [Omarchy](https://omarchy.org): a lean Rust daemon (`kikid`) does the work, a
Quickshell (QML) front end draws it. Locations (SFTP, FTPS), one-way mirroring, search over a name
index, git badges, project mode beside your editor and agent, sharing, and Jarvis, an AI query panel.

## Layout

| Path | What |
|---|---|
| `kikid/` | the daemon: string-pool listings, windows, jobs and undo, archives, mirror, index, git, plugin and helper hosts |
| `crates/kiki-json`, `crates/kiki-plugin-sdk` | the shared JSON reader/writer and the SDK for location and share plugins |
| `plugins/` | every plugin, one process each: locations (`sftp`, `ftps`), services (`dbus`, `highlight`, `jarvis`), share (`share-mail`, `share-messages`, `share-tailscale`, `share-localsend`, `share-airdrop`); the stub lives in `kikid/src/bin/` |
| `qml/` | the Quickshell shell |
| `tests/` | QML component tests and the e2e harness (`tests/e2e/run.sh`) |
| `docs/0.1.0/` | the core plan, feature plans and the two API documents |
| `docs/design/` | mockups (`gen.py` regenerates the artboards) |
| `packaging/` | PKGBUILD, systemd units, desktop entry, portal registration |

## Build and run (Omarchy)

```
cargo build --release --workspace
KIKI_PLUGIN_DIR=target/release target/release/kikid &
qs -p qml/shell.qml
```

Tests: `cargo test --workspace`, `qmltestrunner-qt6 -input tests/qml`, `tests/e2e/run.sh` (needs `cage`).

Packaging: `cd packaging && makepkg -f`. See `docs/0.1.0/10-polish-and-packaging.md`.

## Status

Plans 01 to 20 in `docs/0.1.0/CORE.md` are generated except 17 (devices), which needs the device
libraries on an Omarchy machine. The Rust side is type-checked for Linux and unit-tested on macOS;
the QML has not yet been run under Quickshell. First run on Omarchy: verify the Quickshell socket
parser names in `qml/kiki/Daemon.qml` and the inotify event parsing in `kikid/src/watch.rs`.
