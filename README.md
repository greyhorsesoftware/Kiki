# kiki

A fast file manager for [Omarchy](https://omarchy.org): a lean Rust daemon (`kikid`) does the work, a
Quickshell (QML) front end draws it. Locations (SFTP, FTPS, SMB), one-way mirroring, search over a
name index, git badges, project mode beside your editor and agent, sharing, and "Open AI here…",
which starts the AI you use in Omarchy in a terminal beside your files.

A **location kind** is the suffix of the name a plugin is run by, and the kinds a build speaks are
`plugin::LOCATION_KINDS`: `sftp`, `ftps` and `smb`. `smb` is `kiki-plugin-gio` installed under that
name, talking to GVfs. Every plugin is a process of its own under `plugins/`; a kind the list does
not name is ignored, whatever binaries are in the plugin directory.

## Layout

| Path | What |
|---|---|
| `kikid/` | the daemon: string-pool listings, windows, jobs and undo, archives, mirror, index, git, plugin and helper hosts |
| `crates/kiki-json`, `crates/kiki-plugin-sdk` | the shared JSON reader/writer and the SDK for location and share plugins |
| `plugins/` | every plugin, one process each: locations (`sftp`, `ftps`, `gio`), the `dbus` service, share (`share-mail`, `share-tailscale`); the device plugins (`mtp`, `ptp`, `afc`) stay in the tree unbuilt; the stubs live in `kikid/src/bin/` |
| `qml/` | the Quickshell shell |
| `tests/` | QML component tests and the e2e harness (`tests/e2e/run.sh`) |
| `docs/0.1.0/` | the core plan, feature plans and the two API documents |
| `docs/design/` | mockups (`gen.py` regenerates the artboards) |
| `packaging/` | PKGBUILD, systemd units, desktop entry, portal registration |

## Install

On Arch / Omarchy, from the latest release:

```
curl -fsSL https://raw.githubusercontent.com/greyhorsesoftware/Kiki/main/install.sh | bash
```

It fetches the package for your architecture (x86_64 or aarch64) from GitHub Releases, checks the
published sha256, and runs `pacman -U`, which resolves the dependencies from the official
repositories. Run it again to upgrade; `KIKI_VERSION=v0.1.0` picks a release. Or download the
package from the Releases page and `sudo pacman -U` it yourself. (kiki is not on the AUR: it was
not taking new accounts when 0.1.0 shipped.)

From a checkout:

```
cd packaging && makepkg -f      # -fi builds and installs it
```

The package installs `kikid` and `kiki-thumber`, the plugins this build ships, the QML under
`/usr/share/kiki/`, the desktop entry, the portal registration, and the user units
`kiki.socket` / `kikid.service` — the daemon is socket-activated and starts on demand. `kiki.socket`
is enabled `--global`, so it is live from the next login; the `kiki` command starts it itself until
then. Nothing system-wide is changed for you: making kiki the folder handler and the file chooser is
offered by its own first-run dialog, per user, and undone from Settings → Omarchy.

Dependencies are in `packaging/PKGBUILD`: `quickshell`, `qt6-declarative`, `xdg-desktop-portal`,
`ffmpeg`, `gnome-keyring` and `libsecret` (the keyring), `libarchive` (`bsdtar`), `udisks2`, `git`,
`glib2` and `gvfs`. Optional: `poppler` (PDF thumbnails), `openssh`, `gvfs-smb` and `gvfs-wsdd`
(SMB locations and finding Windows computers), `tailscale`.

## Build and run (Omarchy)

```
make            # cargo build --release, plus the kiki-plugin-smb symlink for gio
make run        # a daemon against this checkout and a shell on top of it
```

or by hand:

```
cargo build --release
KIKI_PLUGIN_DIR=target/release target/release/kikid &
qs -p qml/shell.qml
```

Benchmarks: `kikid bench gen all /tmp/kb && kikid bench run /tmp/kb --json out.json`, then
`kikid bench compare bench/baseline-<os>-<arch>.json out.json` (plan 26). `make scroll-perf` times
100,000 rows scrolled in each view and records the run in `bench/scroll-history.jsonl`;
`make scroll-history` prints the record.

Tests: `make test` runs clippy and all three suites — `make test-rust`, `make test-qml` (leaf and
interaction tests, no compositor) and `make test-e2e` (a real daemon and a real tree; the flows that
drive the shell need `cage`, and the ones that need a real server — `openssh`, `vsftpd`, `samba` —
skip by name when it is not installed).

## Keys

The defaults, from `qml/kiki/Keymap.qml`, which is also what the rebinding window edits. Arrows,
Enter, Backspace and type-ahead are contextual and not rebindable.

| | |
|---|---|
| Find | `/` or `Ctrl+F` filter this folder · `Ctrl+Shift+F` search everywhere · `Ctrl+L` type a path · `Ctrl+Shift+L` add a location |
| View | `Ctrl+1` icon · `Ctrl+2` list · `Ctrl+3` columns · `Ctrl+4` Side by Side · `Ctrl+5` gallery · `Ctrl+H` hidden files · `Ctrl+I` info panel · `F5` refresh · `Ctrl+B` focus favorites · `Ctrl+Shift+B` show or hide them · `Ctrl+,` settings · `Ctrl+?` keyboard shortcuts |
| Files | `Super+C` copy · `Super+X` cut · `Super+V` paste · `Super+Shift+C` copy path · `Ctrl+Shift+N` new folder · `F2` rename · `F4` edit · `Del` trash · `Shift+Del` delete for good · `Ctrl+Z` undo · `Ctrl+Shift+Z` redo · `Ctrl+A` select all · `Alt+Enter` open in the default tool · `Alt+Shift+Enter` open with… · `Alt+S` share · `Alt+Q` open AI here |
| Two panes | `Ctrl+M` mirror · `F6` move across · `Ctrl+Shift+P` project mode · `Ctrl+E` eject |

## Locations and mirroring

Add a location from the sidebar — the `+` beside Locations, or `Ctrl+Shift+L`. The dialog is
generated from the plugin's own form, so a new field needs no UI work; the password goes to the
keyring and the config file keeps lookup attributes only. A connected location wears a green dot,
and its menu has Connection Log… for one that will not connect, and Disconnect.

**Opening a location goes Side by Side**: the server on the right, the local folder kept beside it on
the left (home, when the location names none). The layout's button and `Ctrl+4` are offered only
while a server is open; turning it off leaves the server's pane, and Disconnect — beside Mirror —
lets the server go and puts the pane back on the local folder.

**Mirror** is one way, master to replica, and is not undoable; the Review screen is the safeguard.
**Mirror…** in the toolbar or `Ctrl+M` opens the workspace:

- **Configure** — direction, the change detector, delete extras, the filter rules (Edit rules… for
  the list, nine defaults such as `.git` and `node_modules`), the clock offset, and five workers.
- **Preflight** — the compare, counting as it walks; Cancel or Escape stops it.
- **Review** — every action with its reason, checkable row by row, with the copy and delete counts.
  Mirror runs it.
- **Done** — what happened, in counts and bytes, and **Save report…**, which opens kiki's own save
  dialog and writes the report where you say.

## Status

2026-09-21, evening, on the development machine: **clippy clean; `cargo test` 209; QML 662; e2e 733
passed, 1 skipped** (`pointer_ops` — cage delivers no virtual-pointer events, so the physical press
and drag are driven by hand instead; `shell drop` drives the rest).

Everything in `docs/0.1.0/CORE.md`'s build order has code and tests behind it except plan 17
(devices), which is not in 0.1.0. What is hand-verified rather than automated, and what was removed
before the tag — WebDAV, LocalSend, the Jarvis panel, the code viewer — is in `CORE.md` and in
`docs/0.1.0/31-final-implementation-plan.md`, which is the order of record.
