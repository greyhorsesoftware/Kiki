# 11 — Testing

Builds on: every plan. Applied from plan 01 onward; not a phase that runs at the end.

## Goal

Automated coverage of behaviour, layout, appearance and performance, so a release needs only a short manual checklist. The target is deterministic tests for roughly 90 percent of what the UI does.

## The layers

| Layer | What it covers | Runs where | Tool |
|---|---|---|---|
| Daemon | json.rs, string pool, two-phase scan, windows and phase-2 scheduling, cache, watch, previews, jobs, undo, archives, mirror engine, portal, FileManager1 | plain Rust tests plus a fuzz target for `json.rs` | `cargo test`, `cargo fuzz`, temp trees, `dbus-run-session` |
| Plugins | every `kiki-plugin-*` binary through the pipe protocol, including the stub | Rust tests spawning the binary | an in-process mock SSH/SFTP server (russh) for SFTP, local `sshd` and `vsftpd` on Omarchy |
| Protocol | recorded request/response sequences over the socket, window semantics (count growth, `RowsChanged`, `Reset`), error typing | Rust tests against a running kikid | a socket client in the test crate |
| Window cache | `WindowCache` and `Selection` in QML: padding, debounce, refetch on `RowsChanged`, clear on `Reset`, placeholder rows | Qt test runner with a fake socket | QtTest `TestCase` with `SignalSpy` |
| Leaf components | rows, grids, tables, form controls, inspector fields, the plan sentence | Qt test runner, no compositor | `qmltestrunner`, QtTest `TestCase` |
| Shell in the real runtime | window, socket, IPC, keymap, view switching, dialogs, mirror screens, theme reload | Quickshell under a headless compositor | `cage` (or `sway --headless`), `qs ipc`, `wtype` |
| Visual | every screen in every theme | same headless runtime | `grabToImage` via IPC, `odiff` |
| Layout | geometry, elision, minimum sizes, hit targets | same | IPC geometry queries |
| Performance | first paint, frame time, input latency | same | timestamps from Quickshell, asserted in the test |
| Manual | drag feel, animation feel, Hyprland focus, third-party portal clients, first-run keyring consent | a real Omarchy session or VM | checklist below |

## Design rules that make this possible

These are constraints on the code, decided here so every plan follows them.

1. **Leaf components import only QtQuick.** Rows, grids, tables, checkboxes, selects, fields, the inspector panels, the review table and the running table take their data as properties and emit signals. No `Quickshell.*` import, no socket access. This is what lets the standard runner load them alone.
2. **Shell components own the integrations.** The window, the socket client, the IPC handler, Hyprland hooks, processes, theme watching and desktop-entry lookup live in a small set of shell files. They are tested only in the real runtime.
3. **Everything is reachable by keyboard and by IPC.** Any action a mouse can take has a key (plan 02's keymap) and an IPC function. Tests drive through IPC first, keys second, synthesised pointer input last.
4. **State is queryable.** The IPC handler exposes: current path per pane, selection, view mode, split and inspector state, focused pane, open dialog, mirror screen and spec, toast text, and any item's geometry by object name. Every visible element that a test needs has an `objectName`.
5. **The daemon is the oracle.** Assertions about files, jobs and the journal go to kikid over the socket, never through the UI. Assertions about what a view shows go to the model's roles over IPC, never to pixels.
6. **Time is injectable.** Timers for toast dismissal, search debounce and the mirror poll read their intervals from one settings object the test can shrink.
7. **Theme is a function.** Omarchy theme directory in, kiki tokens out, with no side effects.

## Daemon tests

Already specified per plan; collected here so the harness is shared.

- Fixtures: a temp tree builder, a fake-socket QML harness that replays recorded daemon replies, a local `sshd` with a throwaway key, `vsftpd` with a self-signed certificate, a `kiki-plugin-stub` binary with a two-field form, a session bus with `xdg-desktop-portal` and a fake Secret Service (`secret-service` test backend or a small mock on the bus).
- Every write operation has a round-trip test: run, undo, `diff -r` clean.
- The mirror diff is tested on synthetic maps with no I/O, per the list in `08-mirror.md`.
- Protocol tests replay recorded message sequences against the socket and compare responses, so a protocol change is caught before the UI sees it.

## Leaf component tests

`tests/qml/tst_*.qml` with QtTest, run with `qmltestrunner -import tests/qml/stubs -input tests/qml`. Present: `Format` (sizes, dates, relative times, heat), `Selection`, `WindowCache`, `ContextMenu`, `ViewSwitcher`, `SidebarItem`, `ConfirmDialog`, `Pane` (history, parent and child URIs, trash and hidden flags). One file per component. Each test:

- instantiates the component with a fixed model,
- drives it with `mouseClick`, `keyClick` and property writes,
- asserts emitted signals and resulting properties.

Coverage list (one test file per component in plan 02's inventory; the highlights): sidebar sections and the Locations `+` button, the shortcut bar (chips match the declared set for each context), breadcrumb, search box, view switcher, icon tile, list row and header sort, column and its selection states, inspector General and Permissions tabs (octal and symbolic derivation from the checkbox grid), context menu, undo toast, the generated Add-location form (renders every `Field` kind, Connect enabled state, inline field errors), Open dialog list and filter, split pane header, mirror Configure controls and the plan sentence, mirror Review tabs, rows and footer summary, mirror Running rows in every status.

Importing the `kiki` module compiles its singletons, two of which import Quickshell; `tests/qml/stubs/` provides inert `Quickshell`, `Quickshell.Io.Socket`, `SplitParser` and `FileView` stand-ins so the standard runner (`qmltestrunner -import tests/qml/stubs -input tests/qml`) loads the module without Quickshell installed. Verify early in plan 02 whether a QtTest `TestCase` also runs inside Quickshell's engine. If it does, these tests can additionally run in the real runtime and rule 1 matters less.

## Shell tests in the real runtime

Harness: `tests/e2e/run.sh` starts `cage` with a fixed 1200×760 output, a fresh `XDG_RUNTIME_DIR`, kikid, the session bus and fake keyring, then Quickshell with kiki's config. A Python driver talks to `qs ipc` and to the daemon socket. Each test starts from a known temp home.

Flows, one script each:

- open home, switch icon → list → columns, keyboard-only, assert selection survives
- columns: push three folders, select a file, assert the inspector column, pop back
- inspector toggle in icon and list views, assert grid column count and table width change
- search filters, `Enter` opens, `Esc` clears
- copy, move, rename, trash, mkdir, chmod through the context menu keys; undo each; assert via daemon
- compress and extract; undo both
- add an SFTP and an FTPS location through the dialog; assert keyring entries and no secret on disk; the stub plugin appears as a third tab
- select a location, assert split panes at local and remote paths; cross-pane copy; undo
- mirror: Configure → Preflight → Review → Mirror against local `sshd`; assert destination tree and the Done summary; repeat with deletes on and the blast-radius confirmation; cancel mid-run and assert both panes re-list
- `xdg-open ~/` and `ShowItems` over the bus raise a window at the right path with the right selection
- Open and Save through the portal from a test client (`zenity --file-selection` and a five-line PyGObject script); assert returned URIs, filters and suggested names
- change the Omarchy theme symlink; assert tokens updated without restart

Pointer input via `wtype` and the wlroots virtual pointer is used only for the two drag flows (drag into Favorites, drag between panes), and those are allowed to be marked flaky-retry.

## Visual tests

- After each e2e flow reaches a named screen, the driver calls the IPC `snapshot(objectName)` which uses `grabToImage` and writes a PNG.
- Baselines live in `tests/visual/<theme>/<screen>.png`, one per Omarchy theme: Tokyo Night, Catppuccin, Nord, Gruvbox, Everforest, Kanagawa, Rose Pine, Matte Black.
- Compare with `odiff` at a 1 percent threshold, anti-aliasing tolerant. Baselines and CI runs use the same container image with pinned fonts and hinting, since font rendering differences across machines are the usual source of flaky screenshot tests. A failure attaches the diff image to the CI run.
- Baselines are updated only by a deliberate `make visual-accept`, reviewed in the PR like code.
- The first baselines are checked by eye against `docs/design/`. After that the mockups are reference only, not a pixel target.

## Layout tests

Through the IPC geometry query, assert after each screen:

- no text elided where the mockup shows it whole at 1200 px wide,
- inspector width never below 300 px,
- control sizes match the mockups: toolbar controls 34 px, sidebar items 30 px, rows 28 px, dialog fields 32 px, buttons 30 px; nothing clickable smaller than 24 px,
- the column strip scrolls rather than compressing when four or more columns are open,
- dialogs fit their frame with nothing clipped.

## Performance tests

Run in the same headless runtime on a quiet CI machine, with generous thresholds that still catch order-of-magnitude regressions:

| Measure | Budget |
|---|---|
| first chunk of a 10k-entry listing on the socket | 16 ms |
| first paint after that chunk | 16 ms |
| frame time scrolling 10k rows in list view | under 16 ms per frame, 99th percentile |
| keypress to selection change | 8 ms |
| window open to home painted, warm daemon | 100 ms |
| mirror scan of 5k files on local `sshd` | 5 s |

Quickshell timestamps the events; the driver reads them over IPC.

## Manual checklist (per release, real Omarchy session)

- drag into Favorites and between split panes feels right
- "Show in folder" from Firefox and Signal raises kiki on the current workspace
- Open and Save dialogs from a GTK app, a Qt app and an Electron app
- first-run keyring consent prompt
- every theme, eyeballed once against the visual baselines
- Hyprland tiling and border with two kiki windows open
- cancel a mirror mid-run on a real remote and confirm the browser session still lists

## CI

- `cargo test` on every push.
- `qmltestrunner` on every push.
- e2e, visual, layout and performance in one job under `cage`, on every PR, about ten minutes.
- The manual checklist is a PR template item on release branches only.

## Verification for this plan

- The harness runs green on a fresh Omarchy VM from a single `make test`.
- A script cross-checks the IPC function list in plan 02 and every `objectName` in the QML against the test sources; each must be referenced by at least one test.
- A one-pixel colour change in a token fails the visual job for every theme that uses it.
