# kiki 0.1.0 — core plan

kiki is a file manager for Omarchy: extremely fast, keyboard-first, native to Hyprland and the Omarchy themes. A Rust daemon does all the work; Quickshell (QML) draws it.

Mockups: `docs/design/` (canvas at https://claude.ai/code/artifact/197478a0-0fcf-4a90-99eb-1b3b9ed978d5). Wire protocols: `API-DAEMON.md` (shell ↔ kikid) and `API-PLUGIN.md` (kikid ↔ location plugins); feature plans describe behaviour, the API documents are the authority on messages.

This file fixes the architecture, the decisions every feature relies on, and the build order. Each feature has its own plan in this folder. A feature plan may only depend on plans that come before it in the order below; it may leave a hook for a later plan (a field that becomes editable, a preview that appears) as long as it works without it and says so.

## Architecture

```
┌──────────────────────────────┐          ┌──────────────────────────────┐        ┌────────────────────────┐
│ kiki (Quickshell / QML)      │          │ kikid (Rust, std + rustix)   │        │ kiki-plugin-sftp       │
│ window · sidebar · toolbar   │  socket  │ string pool · windows        │  pipe  │ kiki-plugin-ftps       │
│ views · inspector · dialogs  │◄────────►│ stat · watch · cache         │◄──────►│ kiki-plugin-smb (gio)  │
│ window cache · theme · IPC   │  JSON    │ previews · jobs · journal    │  JSON  │ share-mail · -tailscale│
│ Hyprland · no data parsing   │          │ mirror · portal              │        │ dbus · kiki-thumber    │
│ beyond one window at a time  │          │ FileManager1                 │        │ (spawned on use, idle  │
└──────────────────────────────┘          └──────────────────────────────┘        │  exit after 5 minutes) │
                                                                                  └────────────────────────┘
```

- **kikid** is a lean daemon: standard library plus `rustix` for syscalls, `inotify` through the kernel API, a hand-written JSON writer and reader for the small protocol, and no async runtime. It owns every listing as an **string pool** (names in one contiguous buffer, a kind byte per entry, index arrays for sort orders), answers **window** requests for the rows a view can see, stats only those rows, watches directories, caches everything, and runs jobs, undo, mirror, the portal backend and "Show in folder". Its dependency graph is deliberately small enough to read.
- **One plugin directory, three kinds.** Every out-of-process piece is `kiki-plugin-<name>` under `/usr/lib/kiki/plugins/` (or `~/.local/lib/kiki/plugins/`), speaks the same framing, and answers `Describe` with a `kind`: `location` (a URI scheme), `share` (`share-mail`, `share-tailscale`), or `service` (a fixed name the daemon uses itself: `dbus`, for FileManager1 and the portal). The Settings window's Plugins page lists all of them with running state and a Ping. None of their dependencies enter kikid.
- **A location kind is the suffix of the name the plugin is run by**, and the kinds a build speaks are `plugin::LOCATION_KINDS` in `kikid/src/plugin.rs`: `ftps`, `sftp`, `smb`. `smb` is `kiki-plugin-gio` installed under that name — `gio` itself is never a kind — and the same binary can speak WebDAV and AFP, neither of which 0.1.0 installs. The list is compiled in, so a location plugin dropped into the directory is inventoried and then ignored: discovery, `Describe`, the Add-location dialog and `AddLocation` never see a kind this build does not name. Share and service plugins are not kinds and are found by name.
- **`kiki-thumber` is a child of the same shape and not a plugin**: it speaks the framing (`Plugin::spawn_path`) and answers `Thumb`, with no `Describe`, no `Connect` and no scheme. It is the only part of kiki that decodes a file's contents, so a malformed picture costs that file its thumbnail rather than the daemon.
- **Location plugins are separate executables** (`kiki-plugin-<scheme>`) that the daemon spawns on first use of a scheme and lets exit when idle. They speak the same window/stat/read/write protocol over a pipe. Their dependencies (SSH, TLS, whatever a future S3 plugin needs) never enter the daemon; a plugin crash never takes the daemon down; and the protocol is the plugin ABI, so an out-of-tree plugin is an ordinary binary in a directory — plus, for a location, one line of `LOCATION_KINDS`.
- **kiki** is the Quickshell app: pure QML, a normal toplevel window (Hyprland tiles it, Omarchy draws the border, no client decorations). Every view is an integer-count model whose delegates render from a **window cache** the shell fills with `Window` requests as the viewport moves. Thumbnails, previews and PDF pages arrive as file paths into on-disk caches and load through QML's asynchronous `Image`. The UI never parses more than one window of rows at a time.
- **Transport**: a Unix socket at `$XDG_RUNTIME_DIR/kiki.sock` between kiki and kikid, and a stdin/stdout pipe between kikid and each plugin. Both carry length-prefixed JSON messages; the pipe also carries length-prefixed binary frames for file bytes, and the socket does not — file contents never cross it. Every feature plan states which messages it adds.
- **Decision recorded (2026-09-13, superseding the in-process model of the same morning)**: the reason for in-process models was parsing whole listings in QML. Two-phase virtualized readdir removes that: nothing whole ever crosses the socket, a window of 30 to 60 rows parses in microseconds, and thumbnails travel as paths. Dropping cxx-qt removes the Qt build dependency from Rust, the plugin-loading risk in Quickshell, and most of the crate graph.

## Development environment

Everything is developed and run on an Omarchy x86_64 machine: kikid, the plugin helpers, the Quickshell front end, the portal backend and the test harness. No cross-compilation, no macOS builds. CI builds and tests on an Arch x86_64 container and, for the daemon and plugins, on an Arch Linux ARM aarch64 container on GitHub's ARM runners; aarch64 packages ship from CI, and the shell harness runs on x86_64 only until Quickshell is built for ARM in CI. Prerequisites on the machine: `rustup` stable, `quickshell`, `qt6-declarative` (`qmltestrunner`), `cage`, `wtype`, `ffmpeg`, `poppler` (`pdftoppm`), `xdg-desktop-portal`, `gnome-keyring` and `libsecret`. The e2e servers are the real ones, each run as the user running the tests: `openssh` (`sshd`), **`vsftpd` — the FTPS server, and the only one** (a Python one was used until 2026-09-20 and stopped answering under a few hundred files in quick succession; `vsftpd` is what real servers behave like, session reuse included), and `samba` (`smbd`, `pdbedit`) with `gvfs` and `dbus-run-session` for the SMB flow. A flow whose server is not installed skips by name.

## Cross-cutting rules

- **Every write is a job.** Copy, move, rename, trash, chmod, compress, extract, transfers, mirror runs: all go through the job queue (see `04-operations-and-undo.md`) with progress and cancellation. Jobs that can be undone record a journal entry. Two cannot, and both say so in the UI: remote deletes (no remote trash) and mirror runs (the Review screen is the safeguard).
- **Undo is journaled, not diffed.** Each job records its inverse when it runs. Ctrl+Z replays the inverse as a new job.
- **Everything is a URI.** Every file, folder and pane is addressed by a URI: `file:///home/david/Projects`, `sftp://homelab/srv/kiki`, `ftps://nas/volume1`. A resolver in kikid turns a URI into a backend (local, or a plugin process) plus a path (see `01`). Job ops, IPC calls, the CLI, the portal and "Show in folder" all take URIs. A pane is a view of one URI, so any number of panes can show any mix of schemes with the same code; 0.1.0 lays out at most two (Side by Side).
- **One backend interface, protocols as processes.** Inside kikid, `Backend` is a trait; the local backend implements it directly and `PluginBackend` implements it by forwarding to a plugin process over the pipe (see `06-remote-locations.md`). Views, operations and the mirror engine never contain protocol-specific code; capability differences are flags the plugin reports, and the Add-location dialog is generated from the form the plugin describes.
- **Secrets never touch disk.** Credentials live in the Omarchy keyring via the Secret Service D-Bus API. Config files store lookup attributes only.
- **Theme comes from Omarchy.** kiki reads `~/.config/omarchy/current/theme/` and reloads on change. The mockups use Tokyo Night as the reference palette.
- **Threading: a few threads, no runtime.** kikid runs one reader thread and one writer thread per client connection (clients are few: the shell, a portal request, a test) and a `poll`-based thread for plugin pipes, a scanner thread per active phase-1 listing, one stat worker pool (core count) for phase-2 windows, thumbnails and previews, one watcher thread on the inotify fd, and one thread per running job (N slots, default 3). Threads talk through `std::sync::mpsc` channels; cancellation is an `AtomicBool` per job checked between actions and inside byte loops. A mirror run's N concurrent actions are N threads owned by that job. `ffmpeg` is a child process, one per core, low priority. Plugins are processes, so they bring their own runtime without it touching kikid; the daemon only ever does blocking reads and writes on their pipes from the I/O thread. Quickshell's QML thread does nothing but render and issue window requests.
- **Minimal dependencies.** kikid's allowed crates: `rustix` (syscalls), `libc` where rustix lacks a call, `inotify`-free direct kernel use, a small zbus-free D-Bus client for FileManager1 and the portal, `png` for thumbnail files and `image` for decoding — which run in `kiki-thumber`, a second binary of the same crate, never in the daemon process. MD5, for digests and thumbnail keys, is `src/md5.rs`, hand-written like the JSON. JSON is `src/json.rs`: a writer and a reader for this protocol only, fuzzed. No tokio, no serde, no async. Plugins may depend on whatever they need. Adding a crate to kikid is a review question, not a default.
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
| 10 | `10-polish-and-packaging.md` | all | Theme coverage, PKGBUILD, Omarchy install hook (the cheat sheet was removed on 2026-09-21: `Ctrl+?` opens the rebinding window, which held the same rows) |
| 12 | `12-search.md` | 1, 2, 6 | In-folder filter, the name index for Everywhere search, remote walks (ships after 6, before 10) |
| 13 | `14-open-in.md` | 2, 9 | "Open in…": one TOML list of command templates for editors, AI harnesses and other tools; sessions with a reuse channel; toolbar button, context submenu, `e`, settings page (ships after 9) |
| 17 | `17-devices.md` | 1, 3, 4, 6 | **Not in 0.1.0.** Devices over MTP, AFC and PTP: the three plugins left the tree on 2026-09-23 (git history has them); no kind ships, detection does not run and the sidebar draws no Devices section |
| 18 | `18-share.md` | 4, 5, 6, 14 | Share menu with share plugins as processes; **Mail and Tailscale (Taildrop) ship**. LocalSend was built and removed on 2026-09-21; Messages and AirDrop (OpenDrop over OWL) were removed before that (ships after 14, before 10) |
| 19 | `19-ai-query.md` | 3, 4, 6, 13, 14 | **Not in 0.1.0.** The Jarvis panel — a chat panel with local answers and a CLI in print mode — is gone; what remains is "Open AI here…", which starts the chosen AI's own command-line tool in a terminal beside the files |
| 20 | `20-settings.md` | every plan with a setting | One Settings window with a page per area; keymap served from one table (ships last before 10) |
| 21 | `21-view-memory-and-columns.md` | 1, 2, 20 | Each folder remembers its view and sort; optional list columns |
| 22 | `22-access-heat-map.md` | 1, 20, 21 | Accessed column: relative time over a recency heat swatch, atime sort, the relatime caveat, and an opt-in log of kiki's own opens |
| 23 | `23-ui-refinement.md` | 2, 20, 21 | One view button with a menu, hidden files per pane, settings gear, relative Modified dates, keyboard coverage audit |
| 24 | `24-mirror-view.md` | 7, 8, 21, 23 | Side by Side: a server and the folder kept beside it, in two panes, with Mirror and Disconnect in the toolbar. It is a layout, not a view — there is no mirror bar, no Swap and no "last mirrored". A picture folder with no memory opens in Gallery |
| 25 | `25-smb.md` | 6, 8, 9, 24 | SMB locations through the GIO/GVfs plugin, installed as `kiki-plugin-smb`: mounts, discovery, attribute-rich listings, mirror. SMB2 or later only. No `smb://` scheme handler; WebDAV and AFP are not in 0.1.0 |
| 26 | `26-benchmarks.md` | 1, 11 | Synthetic trees, one timed pass over the daemon's hot paths, JSON results, baselines, CI comparison |
| 27 | `27-gallery-view.md` | 3, 13, 21, 23, 24 | Gallery is the fifth view: one image filling the pane over a filmstrip of its neighbours; arrows page, zoom and trash in place |
| 28 | `28-ui-test-coverage.md` | 2, 4, 11, 23 | Every file operation tested through the interface by keyboard and by mouse: a recording socket for the QML suite, `objectName`s, the `cage` harness, the filesystem as the oracle |
| 29 | `29-release-readiness.md` | all | What stood between the tree and 0.1.0: Mirror view renamed Side by Side with its own toolbar button and a draggable divider, cut loose from view preferences, Side by Side and mirroring driven by hand and then by tests, the location kinds a build actually ships, large transfers in three directions, drag and drop between the panes with modifiers and local/remote defaults, the activity orb in the window's lower right corner, the pointer path, the manual checklist. Spring-loaded folders and link-on-drop are not in 0.1.0. Closed by `31-final-implementation-plan.md`, which is the order of record |
| 30 | `30-code-health.md` | all | What the 2026-09-19 code scan left open: the daemon's three unbounded caches (mirror plans, git status, git roots) and the theme's 2 s poll, both closed; the duplication (`Label`, `Rule`, Shell's repeated handlers, the Rust repeats) is W6–W8 and waits for 0.1.1. Its LocalSend item went with LocalSend on 2026-09-21 |
| 15 | `15-git-status.md` | 1, 2, 3 | Git badges in every view from `git status --porcelain=v2`, branch chip in the breadcrumb, inspector git detail (ships after 3, any time before 10) |
| 16 | `16-project-mode.md` | 2, 9, 13, 14, 15 | `e` on a folder: kiki becomes a narrow project tree and arranges editor and agent beside it through Hyprland (ships after 15, before 10) |
| 14 | `13-code-viewer-and-editor.md` | 3, 14 | Editor bridge: the Open in entry marked `role = "editor"`, `F4`, and the Neovim session with its reuse socket (ships after Open in, before 10) |
| — | `11-testing.md` | applies from 1 onward | Test layers, the design rules that make the UI testable, harness, visual and performance budgets, manual checklist |

Why this order: listing is the performance floor everything sits on, so it is measured first. The inspector precedes operations because chmod lives in it. Remote backends come after the job queue so transfers are jobs from day one. Split mode needs remote backends; mirror needs split mode (its two roots are the location's local and remote paths) and the job queue. Integration is independent of most features but is deferred so a broken daemon is never the system's default file manager during early development.

## Out of scope for 0.1.0

Tabs, bulk rename, further location plugins (S3 and other object stores; SMB is plan 25), iPhone app document folders, bidirectional sync, following symlinks, mirroring permissions.

**Locations and devices.** MTP (Android), AFC (iPhone) and PTP (cameras): the three plugins are in git history, not the tree, since 2026-09-23; no device kind ships and detection does not run (`17-devices.md`). AFP. The `smb://` scheme handler and its dialog prefill — an `smb://` URI opens in whatever else claims it (`25-smb.md`). `ssh-agent` and `~/.ssh/config`: an SFTP location carries its own key or password, and nothing reads the agent or the config file (`06-remote-locations.md`).

**Sending.** Messages and AirDrop (OpenDrop over OWL).

**Panes and dragging.** Spring-loaded folders — no hover-to-open anywhere. Link on drop: the daemon has no `link` operation at all, so `Ctrl+Shift` and "Link here" are not a QML change but a new op with its own undo and journal entry. Swap, "last mirrored" and the mirror options — the strip that held them is off and stays off; the `Ctrl+Alt+←/→` divider nudge; the git branch chip on a pane's header (the branch capsule in the breadcrumb is what was built instead, `15-git-status.md`).

**Deferred by the audit of 2026-09-19** (D-numbers are `31-final-implementation-plan.md`'s): the rest of project mode beyond the four defects fixed (D2); Open in's Settings page, TOML watcher, `tab` placement, `follow` / `on_save`, `local_uri` fallback and malformed-entry report (D3); the Settings pages for Locations, Open in and Plugins (D4); search as it was written — the sorted index, the streaming remote walk, the scope menu, cancel, and `IndexProgress` — and a location search still runs in the client's request thread (D5); the gallery's `Ctrl`+wheel zoom, video poster, remote large preview and Filmstrip setting (D6); the inspector's Created field (D7); D-Bus activation of `org.kiki.App` (D12); plan 11's visual-diff layer and shell performance probes, and plan 26's shell half (D20); copying small files with parallel workers (D22); `30-code-health.md`'s W6–W8, which are 0.1.1's (D23). A remote **move** is not undoable (a copy to a server is, since 2026-09-21); thumbnails of remote files; a sandbox on `kiki-thumber`; multi-select in Columns.

Removed from 0.1.0 after being built (owner, 2026-09-21), each in the git history and each one install line from coming back: **WebDAV** — the `dav` kind of the GIO plugin, never opened against a server; a location saved as `dav` stays listed and says "WebDAV is not in this version" (`25-smb.md`); **LocalSend** — nearby transfers, fixed against the desktop app but never sent to a phone, removed entirely: plugin, workspace member, PKGBUILD line and tests (`18-share.md`). The **Jarvis panel** went the same way on 2026-09-19 — the in-app chat is gone and "Open AI here…" replaces it (`19-ai-query.md`). AFP, from the same GIO plugin, was never in 0.1.0 either. **SMB1 (NT1)** is not supported and will not be: kiki speaks SMB2 or later (`25-smb.md`).

## Decisions already taken

- Remote delete is a confirmed, non-undoable delete in 0.1.0; there is no remote trash.
- A mirror run is not undoable.
- The daemon never uses `io_uring` in 0.1.0; `copy_file_range` with a partial-write loop, falling back to buffered copy.
- All data goes through kikid over the socket; the UI parses at most one window of rows per request. Listings never cross whole.
- `io_uring` for batched `statx`: not in 0.1.0. A window is 30 to 60 stats on the worker pool, microseconds each on a warm cache; the case it helps (cold metadata on slow storage) is measured first. The stat scheduler is written behind one function so a batched submission can be dropped in without touching callers.
- Thumbnails cross to the UI as file paths, not shared memory or DMABUF. Pure QML can only load images from a URL; memfd or a GPU buffer would need a C++ or Rust plugin inside Quickshell, which is exactly what this design removed. The on-disk cache is required anyway for freedesktop sharing, and the kernel page cache makes the second read a memory read. Revisit only if a Quickshell-side plugin is ever accepted.
- Location plugins are separate processes spawned on use, not linked crates.
- The keyring is reached through `secret-tool` (libsecret's CLI), not a D-Bus client in the daemon.
- **The package depends on `gnome-keyring` and `libsecret`** (2026-09-21, closing the first open question). Omarchy is not relied on to ship a Secret Service provider: `libsecret` is where `secret-tool` lives, `gnome-keyring` is the provider behind it, and both are `depends` in `packaging/PKGBUILD` rather than `optdepends`. A location without a keyring is a location whose password has nowhere to go.
- **kiki's chooser sits beside GTK's, per user** (2026-09-21, closing the second). The package only makes the chooser available (`/usr/share/xdg-desktop-portal/portals/kiki.portal`); whether it is used is each user's choice, made in the first-run dialog, which writes `~/.config/xdg-desktop-portal/portals.conf` with `FileChooser=kiki;gtk` — gtk second, so it stays the fallback. Nothing system-wide is replaced, and nothing is written for root.

### Taken 2026-09-19, from the audit (`31-final-implementation-plan.md`)

1. **Location kinds** are `sftp`, `ftps` and gio. gio shipped as `smb` and `dav`; `dav` went with WebDAV on 2026-09-21, so a release build speaks `ftps`, `sftp`, `smb`. `mtp`, `afc` and `ptp` are not in 0.1.0.
2. **Drag and drop ships** — between panes, with modifiers and local/remote defaults. **Spring-loaded folders do not**; a later release.
3. **The activity popup carries the full entry anatomy** of `32-activity.md`: an orb at the window's bottom right, the popup anchored above it.
4. **The in-app Jarvis panel is removed.** "Open AI here…" and "Open Terminal here…" stay.
5. **Pointer testing**: no headless compositor delivers a real drag with the tools to hand — `wlrctl` cannot hold a button down, and cage delivers no virtual-pointer events, so `pointer_ops` skips by name. Drags are driven through an IPC drop hook (`shell drop`) and proved by hand on Hyprland.
6. **Everything in `29-release-readiness.md` F is fixed**, the video player included.

### Taken since

- **kiki has no built-in code viewer and will not grow one** (2026-09-19). Code is read in an editor, which is what Open in and the nvim bridge are for. The Code tab, `kiki-plugin-highlight` and its sixteen grammars, `OpenText` and `TextFind` are deleted; `13-code-viewer-and-editor.md` is the editor bridge and nothing else. This is what kiki is, not something waiting its turn.
- **Side by Side is a server and the folder beside it** (2026-09-21). The button and `Ctrl+4` exist only while a server is open in a pane, or while the layout is on — so there is always a way back to one pane; with no server open the key does nothing. Turned off, the one pane that stays is **the server's**, whichever had the focus. Turned on again from a pane on a server, the server goes right and its location's local folder — or home — left, as a location opened from the sidebar is laid out. A **Disconnect** button stands beside Mirror while a pane is on a server: it disconnects that location and moves the pane to the local folder or home.

## Open questions

None. Both — the Secret Service provider and whether kiki's portal replaces GTK's — were answered on 2026-09-21 and are recorded under Decisions above.
