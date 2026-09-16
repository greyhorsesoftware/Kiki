# kiki 0.1.0 — core plan

kiki is a file manager for Omarchy: extremely fast, keyboard-first, native to Hyprland and the Omarchy themes. A Rust daemon does all the work; Quickshell (QML) draws it.

Mockups: `docs/design/` (canvas at https://claude.ai/code/artifact/197478a0-0fcf-4a90-99eb-1b3b9ed978d5). Wire protocols: `API-DAEMON.md` (shell ↔ kikid) and `API-PLUGIN.md` (kikid ↔ location plugins); feature plans describe behaviour, the API documents are the authority on messages.

This file fixes the architecture, the decisions every feature relies on, and the build order. Each feature has its own plan in this folder. A feature plan may only depend on plans that come before it in the order below; it may leave a hook for a later plan (a field that becomes editable, a preview that appears) as long as it works without it and says so.

## Architecture

```
┌──────────────────────────────┐          ┌──────────────────────────────┐        ┌────────────────────────┐
│ kiki (Quickshell / QML)      │          │ kikid (Rust, std + rustix)   │        │ kiki-plugin-sftp       │
│ window · sidebar · toolbar   │  socket  │ string pool · windows        │  pipe  │ kiki-plugin-ftps       │
│ views · inspector · dialogs  │◄────────►│ stat · watch · cache         │◄──────►│ (spawned on first use, │
│ window cache · theme · IPC   │  JSON    │ previews · thumbnails        │  JSON  │  exit when idle)       │
│ Hyprland · no data parsing   │          │ jobs · journal · mirror      │        └────────────────────────┘
│ beyond one window at a time  │          │ portal · FileManager1        │
└──────────────────────────────┘          └──────────────────────────────┘
```

- **kikid** is a lean daemon: standard library plus `rustix` for syscalls, `inotify` through the kernel API, a hand-written JSON writer and reader for the small protocol, and no async runtime. It owns every listing as an **string pool** (names in one contiguous buffer, a kind byte per entry, index arrays for sort orders), answers **window** requests for the rows a view can see, stats only those rows, watches directories, caches everything, and runs jobs, undo, mirror, the portal backend and "Show in folder". Its dependency graph is deliberately small enough to read.
- **One plugin directory, three kinds.** Every out-of-process piece is `kiki-plugin-<name>` under `/usr/lib/kiki/plugins/` (or `~/.local/lib/kiki/plugins/`), speaks the same framing, and answers `Describe` with a `kind`: `location` (a URI scheme: `sftp`, `ftps`), `share` (`share-mail`, `share-tailscale`, …), or `service` (fixed names the daemon uses itself: `dbus` for FileManager1 and the portal, `highlight` for the code viewer). The Settings window's Plugins page lists all of them with running state and a Ping. None of their dependencies enter kikid.
- **Location plugins are separate executables** (`kiki-plugin-<scheme>`) that the daemon spawns on first use of a scheme and lets exit when idle. They speak the same window/stat/read/write protocol over a pipe. Their dependencies (SSH, TLS, whatever a future S3 plugin needs) never enter the daemon; a plugin crash never takes the daemon down; and the protocol is the plugin ABI, so out-of-tree plugins are ordinary binaries in a directory.
- **kiki** is the Quickshell app: pure QML, a normal toplevel window (Hyprland tiles it, Omarchy draws the border, no client decorations). Every view is an integer-count model whose delegates render from a **window cache** the shell fills with `Window` requests as the viewport moves. Thumbnails, previews and PDF pages arrive as file paths into on-disk caches and load through QML's asynchronous `Image`. The UI never parses more than one window of rows at a time.
- **Transport**: a Unix socket at `$XDG_RUNTIME_DIR/kiki.sock` between kiki and kikid, and a stdin/stdout pipe between kikid and each plugin. Both carry length-prefixed JSON messages, with length-prefixed binary frames for file bytes. Every feature plan states which messages it adds.
- **Decision recorded (2026-09-13, superseding the in-process model of the same morning)**: the reason for in-process models was parsing whole listings in QML. Two-phase virtualized readdir removes that: nothing whole ever crosses the socket, a window of 30 to 60 rows parses in microseconds, and thumbnails travel as paths. Dropping cxx-qt removes the Qt build dependency from Rust, the plugin-loading risk in Quickshell, and most of the crate graph.

## Development environment

Everything is developed and run on an Omarchy x86_64 machine: kikid, the plugin helpers, the Quickshell front end, the portal backend and the test harness. No cross-compilation, no macOS builds. CI builds and tests on an Arch x86_64 container and, for the daemon and plugins, on an Arch Linux ARM aarch64 container on GitHub's ARM runners; aarch64 packages ship from CI, and the shell harness runs on x86_64 only until Quickshell is built for ARM in CI. Prerequisites on the machine: `rustup` stable, `quickshell`, `qt6-declarative` (`qmltestrunner`), `cage`, `wtype`, `openssh`, `vsftpd`, `ffmpeg`, `poppler` (`pdftoppm`), `xdg-desktop-portal`, `gnome-keyring` or the Secret Service provider chosen in the open question below.

## Cross-cutting rules

- **Every write is a job.** Copy, move, rename, trash, chmod, compress, extract, transfers, mirror runs: all go through the job queue (see `04-operations-and-undo.md`) with progress and cancellation. Jobs that can be undone record a journal entry. Two cannot, and both say so in the UI: remote deletes (no remote trash) and mirror runs (the Review screen is the safeguard).
- **Undo is journaled, not diffed.** Each job records its inverse when it runs. Ctrl+Z replays the inverse as a new job.
- **Everything is a URI.** Every file, folder and pane is addressed by a URI: `file:///home/david/Projects`, `sftp://homelab/srv/kiki`, `ftps://nas/volume1`. A resolver in kikid turns a URI into a backend (local, or a plugin process) plus a path (see `01`). Job ops, IPC calls, the CLI, the portal and "Show in folder" all take URIs. A pane is a view of one URI, so any number of panes can show any mix of schemes with the same code; 0.1.0 lays out at most two (split mode).
- **One backend interface, protocols as processes.** Inside kikid, `Backend` is a trait; the local backend implements it directly and `PluginBackend` implements it by forwarding to a plugin process over the pipe (see `06-remote-locations.md`). Views, operations and the mirror engine never contain protocol-specific code; capability differences are flags the plugin reports, and the Add-location dialog is generated from the form the plugin describes.
- **Secrets never touch disk.** Credentials live in the Omarchy keyring via the Secret Service D-Bus API. Config files store lookup attributes only.
- **Theme comes from Omarchy.** kiki reads `~/.config/omarchy/current/theme/` and reloads on change. The mockups use Tokyo Night as the reference palette.
- **Threading: a few threads, no runtime.** kikid runs one reader thread and one writer thread per client connection (clients are few: the shell, a portal request, a test) and a `poll`-based thread for plugin pipes, a scanner thread per active phase-1 listing, one stat worker pool (core count) for phase-2 windows, thumbnails and previews, one watcher thread on the inotify fd, and one thread per running job (N slots, default 3). Threads talk through `std::sync::mpsc` channels; cancellation is an `AtomicBool` per job checked between actions and inside byte loops. A mirror run's N concurrent actions are N threads owned by that job. `ffmpeg` is a child process, one per core, low priority. Plugins are processes, so they bring their own runtime without it touching kikid; the daemon only ever does blocking reads and writes on their pipes from the I/O thread. Quickshell's QML thread does nothing but render and issue window requests.
- **Minimal dependencies.** kikid's allowed crates: `rustix` (syscalls), `libc` where rustix lacks a call, `inotify`-free direct kernel use, a small zbus-free D-Bus client for FileManager1 and the portal, `md5` for digests and thumbnail keys, `png` for thumbnail files, `image` for decoding. JSON is `src/json.rs`: a writer and a reader for this protocol only, fuzzed. No tokio, no serde, no async. Plugins may depend on whatever they need. Adding a crate to kikid is a review question, not a default.
- **Keyboard first.** Every action in the mockups has a key; the keymap is one table in `02-shell-and-views.md`.
- **Quickshell for the system, plain QtQuick for the leaves.** Window, the daemon socket (`Quickshell.Io.Socket`), IPC, Hyprland, processes, theme watching and desktop entries use Quickshell components. Rows, grids, tables, forms and panels import only QtQuick and take their data as properties, so they can be tested alone. Every action is reachable by keyboard and by `qs ipc`. See `11-testing.md`.

## Build order

Each step ships on its own and is verified before the next starts.

| # | Feature plan | Builds on | Delivers |
|---|---|---|---|
| 1 | `01-daemon-and-listing.md` | — | kikid with the string pool, two-phase readdir, window protocol, stat cache, inotify watch, listing cache, `src/json.rs`; a QML window cache scrolling 200k rows |
| 2 | `02-shell-and-views.md` | 1 | Quickshell window, sidebar (Favorites), toolbar, icon / list / columns views, search, theme, keymap |
| 3 | `03-inspector.md` | 2 | Tabbed inspector (General, Permissions), toolbar toggle, columns-view last column, thumbnails for images and video in every view |
| 4 | `04-operations-and-undo.md` | 1, 3 | Job queue, journal, copy / move / rename / trash / mkdir / chmod, context menu, undo toast |
| 5 | `05-archives.md` | 4 | Compress and extract as jobs, undoable |
| 6 | `06-remote-locations.md` | 1, 4 | Plugin process protocol, `kiki-plugin-sftp` and `kiki-plugin-ftps`, Locations section with `+`, generated Add-location dialog, keyring |
| 7 | `07-split-mode.md` | 2, 6 | Local | remote split, cross-pane transfers |
| 8 | `08-mirror.md` | 4, 6, 7 | One-way mirror engine (scan → diff → execute), review workspace, queued job, report |
| 9 | `09-omarchy-integration.md` | 1, 2 | Default file manager, FileManager1 "Show in folder", portal Open / Save dialogs |
| 10 | `10-polish-and-packaging.md` | all | Theme coverage, cheat sheet, PKGBUILD, Omarchy install hook |
| 12 | `12-search.md` | 1, 2, 6 | In-folder filter, the name index for Everywhere search, remote walks (ships after 6, before 10) |
| 13 | `14-open-in.md` | 2, 9 | "Open in…": one TOML list of command templates for editors, AI harnesses and other tools; sessions with a reuse channel; toolbar button, context submenu, `e`, settings page (ships after 9) |
| 17 | `17-devices.md` | 1, 3, 4, 6 | Devices section: Android over MTP, iPhone over AFC, cameras over PTP as plugin processes plus uevent hotplug and eject (ships after 6, any time before 10) |
| 18 | `18-share.md` | 4, 5, 6, 14 | Share menu with share plugins as processes; Mail, Messages (KDE Connect, Signal, Matrix), Tailscale (Taildrop), LocalSend and experimental AirDrop (OpenDrop over OWL) shipped (ships after 14, before 10) |
| 19 | `19-ai-query.md` | 3, 4, 6, 13, 14 | Jarvis ▸ Query… on text files: a chat panel, local answers for counts, otherwise the selected AI's own CLI in print mode (ships after 14, before 10) |
| 20 | `20-settings.md` | every plan with a setting | One Settings window with a page per area; keymap served from one table (ships last before 10) |
| 21 | `21-view-memory-and-columns.md` | 1, 2, 20 | Each folder remembers its view and sort; optional list columns |
| 22 | `22-access-heat-map.md` | 1, 20, 21 | Accessed column: relative time over a recency heat swatch, atime sort, the relatime caveat, and an opt-in log of kiki's own opens |
| 23 | `23-ui-refinement.md` | 2, 20, 21 | One view button with a menu, hidden files per pane, settings gear, relative Modified dates, keyboard coverage audit |
| 24 | `24-mirror-view.md` | 7, 8, 21, 23 | Mirror is the fourth view: two panes with a mirror bar; remote locations open in it; picture folders open in icon view |
| 25 | `25-smb.md` | 6, 8, 9, 24 | SMB (and WebDAV, AFP) locations through one GIO/GVfs plugin: mounts, discovery, attribute-rich listings, mirror; `smb://` handler |
| 26 | `26-benchmarks.md` | 1, 11 | Synthetic trees, one timed pass over the daemon's hot paths, JSON results, baselines, CI comparison |
| 15 | `15-git-status.md` | 1, 2, 3 | Git badges in every view from `git status --porcelain=v2`, branch chip in the breadcrumb, inspector git detail (ships after 3, any time before 10) |
| 16 | `16-project-mode.md` | 2, 9, 13, 14, 15 | `e` on a folder: kiki becomes a narrow project tree and arranges editor and agent beside it through Hyprland (ships after 15, before 10) |
| 14 | `13-code-viewer-and-editor.md` | 3, 14 | Highlighted code viewer in the inspector; the editor bridge as the Open in entry marked `role = "editor"` (ships after Open in, before 10) |
| — | `11-testing.md` | applies from 1 onward | Test layers, the design rules that make the UI testable, harness, visual and performance budgets, manual checklist |

Why this order: listing is the performance floor everything sits on, so it is measured first. The inspector precedes operations because chmod lives in it. Remote backends come after the job queue so transfers are jobs from day one. Split mode needs remote backends; mirror needs split mode (its two roots are the location's local and remote paths) and the job queue. Integration is independent of most features but is deferred so a broken daemon is never the system's default file manager during early development.

## Out of scope for 0.1.0

Tabs, bulk rename, further location plugins (S3 and other object stores, WebDAV; SMB is plan 25), iPhone app document folders, bidirectional sync, following symlinks, mirroring permissions.

## Decisions already taken

- Remote delete is a confirmed, non-undoable delete in 0.1.0; there is no remote trash.
- A mirror run is not undoable.
- The daemon never uses `io_uring` in 0.1.0; `copy_file_range` with a partial-write loop, falling back to buffered copy.
- All data goes through kikid over the socket; the UI parses at most one window of rows per request. Listings never cross whole.
- `io_uring` for batched `statx`: not in 0.1.0. A window is 30 to 60 stats on the worker pool, microseconds each on a warm cache; the case it helps (cold metadata on slow storage) is measured first. The stat scheduler is written behind one function so a batched submission can be dropped in without touching callers.
- Thumbnails cross to the UI as file paths, not shared memory or DMABUF. Pure QML can only load images from a URL; memfd or a GPU buffer would need a C++ or Rust plugin inside Quickshell, which is exactly what this design removed. The on-disk cache is required anyway for freedesktop sharing, and the kernel page cache makes the second read a memory read. Revisit only if a Quickshell-side plugin is ever accepted.
- Location plugins are separate processes spawned on use, not linked crates.
- The keyring is reached through `secret-tool` (libsecret's CLI), not a D-Bus client in the daemon.
- Plan 17 (devices) is specified but not generated: its three plugins need libmtp, libgphoto2 and libimobiledevice headers, which are only on the Omarchy machine. Everything else in the build order has code.

## Open questions

- Does Omarchy ship a Secret Service provider (gnome-keyring) on every install, or does kiki depend on it explicitly in the PKGBUILD?
- Should the portal backend replace the GTK portal wholesale (`xdg-desktop-portal` config) or be offered as an option?
