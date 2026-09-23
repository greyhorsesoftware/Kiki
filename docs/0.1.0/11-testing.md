# 11 — Testing

**Status:** daemon, QML and e2e layers built; visual, layout and shell-performance layers not in 0.1.0 (D20); `pointer_ops` skips — no headless compositor here delivers a virtual pointer.

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

**Which of those exist (2026-09-21).** Built: Daemon, Plugins, Protocol, Window cache, Leaf components, Shell in the real runtime, Manual. **Not in 0.1.0** (D20): Visual, Layout and the shell half of Performance — a suite that cannot be built in time must not hold the tag; the daemon's own budgets live in `bench` (plan 26) and the theme pass stays on the manual list. The compositor is `cage`, not `sway --headless`; `wtype` is unused — flows drive through IPC and the keyboard, and `wlrctl` only appears in `pointer_ops`, which skips.

## The suites as they are (2026-09-21)

`make test` is four things in order, and any one of them failing fails it:

1. **`clippy`** — `cargo clippy --all-targets -- -D warnings`. A warning is a failure, not a chore (plan 30 W5).
2. **`cargo test`** — the daemon, the plugin binaries through the pipe protocol, the protocol suite against a real socket, and the JSON property tests. **209 passed.**
3. **`make test-qml`** — `qmltestrunner -import tests/qml/stubs -input tests/qml` under `QT_QPA_PLATFORM=offscreen` (the environment wins), then **`tests/qml-drag` under `minimal`**. Two platforms because offscreen ends a drag the moment it begins, while `minimal` performs a real in-process drag — the only way to hold `Ctrl` down and read what Qt actually does with it (phase 5). **662 passed.**
4. **`make test-e2e`** — `tests/e2e/run.sh`: one `cage` client at a fixed 1200 × 760 with its own `XDG_RUNTIME_DIR` and session bus, kikid, Quickshell and the Python driver inside it. **733 passed, 1 skipped.**

**The servers are real, not mocks.** `tests/e2e/servers.py` starts OpenSSH's `sshd`, `vsftpd` and Samba's `smbd` as the launching user, each on a high port with its own throwaway config, keys, passdb and certificate (`sudo pacman -S openssh vsftpd samba`). A flow whose server is not installed skips **by name**, so a machine without one still runs everything else. SMB additionally needs the real GVfs stack, and the daemon under it runs in a `dbus-run-session` with a `gvfsd --no-fuse` of its own on the run's private runtime directory — gvfsd-smb asks the keyring before it asks the server anything, and the ambient keyring is the developer's own.

**Two things about running under cage, both learned the hard way.** It stays up while anything is drawing in it, so `run.sh` ends whatever the run started (known by the run's own `XDG_RUNTIME_DIR`) before the script leaves — a detached terminal used to hold the run to the compositor's deadline. And it does not hand back its client's exit status, so the driver's verdict is written down inside the compositor and read outside it; no verdict at all is a failure. Without cage, `run.sh` runs the daemon-only flows and says which it skipped.

**Why `pointer_ops` skips.** cage accepts `wlr_virtual_pointer_v1` and then logs *"cannot be mapped to an output device"*: the events go nowhere. The flow probes with a single click and steps aside with that sentence rather than failing fourteen checks that say nothing about kiki. `wlrctl` cannot hold a button down either, so no headless compositor available here gives a real drag: drags are proven three other ways — the `shell drop` IPC hook (`drag_between_panes.py`, every local/remote pair in both directions with each modifier), real in-process drags under `minimal` (`tests/qml-drag/tst_RealDrag.qml`), and by hand on Hyprland on the phase-10 checklist.

**The sway spike** (plan 31, decision 5) was a time-boxed half day to find out whether sway's headless backend delivers virtual-pointer events to Quickshell. **It was not run, and no answer is recorded**: there is no `run_in_sway` in `run.sh` and no mention of sway anywhere in the tree. The skip therefore stands for 0.1.0, and `sway` is not in CI.

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

~~Pointer input via `wtype` and the wlroots virtual pointer is used only for the two drag flows (drag into Favorites, drag between panes), and those are allowed to be marked flaky-retry.~~ **Amended 2026-09-21:** the virtual pointer does not work under cage at all (above), so no flow depends on it; the drag flows go through the `shell drop` IPC hook, which hands `Pane.dropInto` the object shape Qt would have given it, so everything after the button goes down is what runs. Nothing is marked flaky-retry; the one retry in the suite is `remote_transfers`' race between a 1.5 GB upload and a listing, which forgives the machine and not the bug.

## Visual tests

**Not in 0.1.0 (D20, 2026-09-21.)** No baselines exist, `odiff` is not a dependency and there is no visual CI job; every theme is eyeballed once by hand at phase 10 instead. The section stands as the design for a later release.

- After each e2e flow reaches a named screen, the driver calls the IPC `snapshot(objectName)` which uses `grabToImage` and writes a PNG.
- Baselines live in `tests/visual/<theme>/<screen>.png`, one per Omarchy theme: Tokyo Night, Catppuccin, Nord, Gruvbox, Everforest, Kanagawa, Rose Pine, Matte Black.
- Compare with `odiff` at a 1 percent threshold, anti-aliasing tolerant. Baselines and CI runs use the same container image with pinned fonts and hinting, since font rendering differences across machines are the usual source of flaky screenshot tests. A failure attaches the diff image to the CI run.
- Baselines are updated only by a deliberate `make visual-accept`, reviewed in the PR like code.
- The first baselines are checked by eye against `docs/design/`. After that the mockups are reference only, not a pixel target.

## Layout tests

**Not in 0.1.0 (D20, 2026-09-21.)** There is no layout suite. What it would have caught is covered case by case where it bit — the chooser in a short window, the mirror Review footer at three widths, Configure at three heights, the inspector's Permissions grid — each pinned in its own QML test, and the geometry query it would have used (`shell geometry <objectName>`) exists and is what the flows aim the pointer with.

Through the IPC geometry query, assert after each screen:

- no text elided where the mockup shows it whole at 1200 px wide,
- inspector width never below 300 px,
- control sizes match the mockups: toolbar controls 34 px, sidebar items 30 px, rows 28 px, dialog fields 32 px, buttons 30 px; nothing clickable smaller than 24 px,
- the column strip scrolls rather than compressing when four or more columns are open,
- dialogs fit their frame with nothing clipped.

## Performance tests

**The shell half is not in 0.1.0 (D20, 2026-09-21)**; none of the budgets below is asserted in a test. What is measured: the daemon's side in `bench`, by hand on the baseline's machine (plan 26; CI's runners are not comparable); `scroll_perf` and `gallery_perf`, by name, which record times and peak RSS rather than failing on a threshold; and the watch budget, which *is* asserted (`kikid/tests/watch.rs`, plan 01's 100 ms). Launch-to-paint is unmeasured.

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

**As built (2026-09-23)**, `.github/workflows/ci.yml` has three jobs on every push: daemon and plugins per architecture (`make lint`, `make test-rust`, the ignored protocol and contract tests); QML under Qt with no compositor; and a package build per architecture. **The e2e flows under `cage` against the *installed* package** moved to `release.yml` the same day (owner): the job "Install the package and drive it" runs on a tag, gates the GitHub release on the x86_64 package proving itself, and runs by hand from the Actions tab (`workflow_dispatch`, which builds and drives but publishes nothing). It was the slowest job by a margin and the one most likely to go red for reasons that were not the code; the three that stay are fast, cached and deterministic. There is no visual, layout or performance job; the benchmark job that existed until 2026-09-23 compared a GitHub runner against the owner's machine and could only fail (plan 26).

- `cargo test` on every push.
- `qmltestrunner` on every push.
- ~~e2e, visual, layout and performance in one job under `cage`~~ ~~e2e in one job under `cage`, on every PR, about ten minutes.~~ e2e under `cage` on every tag and by hand (release.yml), not on every push.
- The manual checklist is a PR template item on release branches only.

## Coverage

`make coverage` instruments the daemon (`-C instrument-coverage`) and runs every Rust test —
including the plugin contract test — through `llvm-profdata`/`llvm-cov`, which the system LLVM
provides; no `cargo-llvm-cov` needed. It prints per-file lines and leaves `target/coverage` for
`llvm-cov show` on a file.

Measured 2026-09-19, after `listing.rs` and `mirror.rs` were split: **62.1% of lines** across
kikid. That number is a floor rather than the truth: `server.rs` (806 lines) reads as 0% because
nothing in the Rust suite dials the socket — the e2e flows do, against a real daemon that is not
instrumented — and the same goes for `share.rs`, `dbus.rs` and `helpers.rs`.

Where the two split modules stand, and what each number means:

| Module | Lines | What is not covered |
|---|---|---|
| `mirror/store.rs` | 100% | — |
| `mirror/filters.rs` | 97% | — |
| `mirror/detect.rs` | 87% | `pick_detector` asking a plugin's `Describe` |
| `mirror/diff.rs` | 85% | — |
| `mirror/report.rs` | 83% | the download-direction wording |
| `mirror/mod.rs` | 72% | `Spec` JSON round trips for options the UI rarely sets |
| `mirror/scan.rs` | 63% | the per-directory remote fallback's error paths |
| `mirror/execute.rs` | 57% | remote→remote transfers (two plugin processes) |
| `listing/rows.rs` | 93% | — |
| `listing/scan.rs` | 81% | remote scans and the watcher's large-batch rescan |
| `listing/mod.rs` | 78% | — |
| `listing/stats.rs` | 75% | thumbnail submission and the low-priority queue |
| `listing/cache.rs` | 75% | eviction under memory pressure |
| `listing/view.rs` | 74% | `atime` sort, and the parallel sort above 50k rows |

Writing those tests found two real faults, which is the point of doing it:

- **A sort that never answered.** `enrich_all` queued only the rows the *view* was showing while
  `enrich_progress` waited for *every* row to have metadata, so with a filter typed, a sort by
  size left the listing permanently mid-enrichment and the `Sort` reply never came.
- **A stub plugin that lied about `Scan`.** It answered `recursive: true` with the top level
  only, instead of `Unsupported` as API-PLUGIN requires, which would make a mirror re-copy every
  file it never saw. The contract test now asserts the refusal.

## Verification for this plan

- The harness runs green on a fresh Omarchy VM from a single `make test`.
- ~~A script cross-checks the IPC function list in plan 02 and every `objectName` in the QML against the test sources~~ **Amended 2026-09-21:** no such script was written; coverage of the shell is the flows plus `make coverage` on the daemon.
- ~~A one-pixel colour change in a token fails the visual job for every theme that uses it.~~ **Amended 2026-09-21:** no visual job (D20). What guards the palette instead is `tst_ThemeDanger` and `tst_GitBadges`, which assert that a badge takes the theme's colour and that a theme whose yellow is red does not draw a modified project as a conflicted one.
