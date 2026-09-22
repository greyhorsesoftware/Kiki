# 28 — UI test coverage for file operations

**Status:** built; drags are covered by an IPC hook (`shell drop`), by real in-process drags under Qt's `minimal` platform (`tests/qml-drag/tst_RealDrag.qml`, part of `make test`), and by hand on Hyprland.

Builds on: `11-testing.md` (the layers, the harness, the design rules), `02-shell-and-views.md` (keymap, component inventory), `04-operations-and-undo.md` (the operations and the journal), `23-ui-refinement.md` (context menu, shortcut bar).

## Goal

Every file operation the interface offers is exercised automatically, **through the interface, by keyboard and by mouse**, and the assertion is the filesystem — not a property, not a screenshot. `11-testing.md` sets out the layers; this plan is the work that fills them for file operations, and the inventory of what is missing today.

The split stays as plan 11 has it: interaction tests prove the *request* the UI emits, the daemon's own tests prove the request does the right thing to disk, and a small number of end-to-end flows prove the two halves are wired together and undo brings the tree back.

## Where this starts

| | State on 2026-09-18 |
|---|---|
| `tests/qml/` | 8 files, 26 cases: `Format`, `Selection`, `WindowCache`, `Pane`, `ContextMenu`, `ViewSwitcher`, `SidebarItem`, `ConfirmDialog` |
| Pointer input there | Works. The five click tests failed for months because the `TestCase` lacked `when: windowShown` and `visible: true`; with those two lines they pass |
| `objectName` in `qml/` | None at all, so nothing can be addressed by name for a click or a geometry query |
| Views | `IconPane`, `ListPane`, `ColumnsPane`, `GalleryPane`, the rename editor, the collision prompt and every drag path have no test |
| `tests/e2e/` | `run.sh` + an 83-line `driver.py`; without `cage` it silently degrades to daemon-only checks, and `cage` is not installed. The compositor half has never run |
| Waiting | `driver.py` sleeps; there is no `wait_for` |
| Entry point | No `Makefile`, so plan 11's "one `make test`" does not exist |

## Layer A — interaction tests in qmltestrunner

No compositor, about a second for the suite, runs on every push. This is where most of the coverage belongs, because it can drive both input methods and read back exactly what the UI asked the daemon to do.

**The recording socket.** `tests/qml/stubs/Quickshell/Io/Socket.qml` swallows writes. Replace it with one that records every frame and lets the test push replies and events back, and add `tests/qml/FakeDaemon.qml` over it: `listing(rows)` to seed a folder, `lastRequest()`, `expect(op)`, `emitEvent(e)`. A test then reads: select row 2, press `F2`, type `notes.txt`, press `Enter`, assert one `Submit { op: "rename", uri, name }`. This single piece unblocks everything else in this layer.

**Drag and drop, honestly.** The rows set `Drag.dragType: Drag.Automatic` (`ListRow.qml`, `IconPane.qml`), which hands the drag to the compositor, so a synthetic pointer cannot drive it in-process — activating it in a test would start a real platform drag and block the run. What the drag *offers* (`dragMime`) and what a drop *decides* (`dropInto`: move within a place, copy across one, `Ctrl` forces copy, self-drops refused, and the job it submits) are covered here by handing `dropInto` a DragEvent-shaped object.

**Amended 2026-09-20/21: the platform decides, and `minimal` performs a real drag.** Under `offscreen` a drag ends the moment it begins; under **`minimal`** Qt performs one in-process, so a test can press, move, hold a key and look. `tests/qml-drag/` runs under it as the second half of `make test-qml` — `tst_RealDrag.qml`, 11 real drags: no key within a disk moves; `Ctrl` copies; `Alt` copies; `Shift` within a disk moves; a key pressed **mid-drag** decides the drop; a refused target shows refusing during the drag and takes nothing; across machines no key and `Shift` both copy; the receiving pane says it received; a pressed row of a selection carries the whole selection; Escape drops nothing. It found what no hand-built event could: **a real `DragEvent` has no `modifiers` property**, so every drop read `undefined` and `Ctrl` and `Shift` had never once reached the code, while 81 e2e checks and two QML tables passed on a field that does not exist. The hand-built events are shaped as Qt shapes a real one now (`proposedAction`, no `modifiers`).

**Addressing.** Every element a test clicks gets an `objectName`, and the names are a convention, not ad hoc: `row-<index>`, `tile-<index>`, `column-<depth>`, `sidebar-<name>`, `menu-<label>`, `dialog-<name>`, `field-<key>`, `chip-<key>`. The same names serve the IPC geometry query in layer B and the layout assertions in plan 11.

**Files to add**, one per component, each covering the keyboard path and the mouse path:

`tst_ListPane`, `tst_IconPane`, `tst_ColumnsPane`, `tst_GalleryPane`, `tst_RenameEditor`, `tst_ContextMenuActions`, `tst_Clipboard`, `tst_DragDrop`, `tst_CollisionPrompt`, `tst_Toolbar`, `tst_Breadcrumb`, `tst_FilterBar`, `tst_Inspector` (permissions grid ↔ octal), ~~`tst_ShortcutsOverlay`~~ (the overlay was removed 2026-09-21 — it duplicated the rebinding window; plan 23), `tst_LocationDialog`. The suite as it stands is wider than this list — `ls tests/qml` is the inventory.

**The trap, written down so it is not rediscovered:** a `TestCase` that drives pointer or key input must declare `when: windowShown` and `visible: true`. Without them the events are delivered nowhere and every assertion fails with a zero count, which reads exactly like a broken component.

## Layer B — end to end, with the filesystem as the oracle

`cage`, a real kikid, a real temp home. What layer A cannot prove: that the job ran, that the bytes moved, that undo restores the tree, that the watcher refreshed the view.

Prerequisites, in order:

1. `cage` installed, and a `Makefile` whose `make test` runs the Rust tests, the QML suite and this harness.
2. **A temp home per flow**, built by a fixture helper, instead of one shared 10 000-file tree; the 10k tree stays, but only for the performance flow.
3. `wait_for(predicate, timeout)` in the driver, and every `time.sleep` deleted. Each sleep is a flake waiting for a slower machine.
4. `diff -r expected/ actual/` after every operation and again after its undo, so the assertion is the whole tree rather than one path.

**Input.** Keys through `wtype`; pointer through the wlroots virtual pointer (`wlrctl`), aimed with the IPC geometry query. Every flow runs twice where both paths exist — once driven by keys, once by the pointer — and asserts the same tree.

**Never synthesise a bare `Super` chord on an Omarchy session.** Hyprland binds `Super` on its own to the system menu, and `Super+C`/`X`/`V` to the universal clipboard shortcuts; an injected chord fires the binding, the menu takes the seat, and the window under test silently stops receiving keys. Tests that mean "the user pressed Super+C" send the `Ctrl+C` that Omarchy rewrites it to, and the Super path itself is covered by the one-line binding assertion in the keymap test.

## The matrix

Each row is a test in layer A (the request), a flow in layer B (the tree), and both input methods.

| Operation | Keyboard | Mouse | Oracle |
|---|---|---|---|
| New folder | `Ctrl+Shift+N` | context menu | folder exists, rename editor open on it |
| Rename | `F2`, type, `Enter` | menu → editor | old name gone, new name present |
| Copy / Cut / Paste | `Super+C` `Super+X` `Super+V` | context menu | tree diff; cut leaves nothing behind |
| Move by drag | — | row → folder row, row → sidebar favourite, pane → pane | tree diff both ends |
| Copy by drag | — | `Ctrl`-drag | source intact |
| Move to Trash | `Del` | context menu | gone from folder, present in `trash:///`, `trashinfo` written |
| Delete permanently | `Shift+Del` + confirm | Empty Trash (there is no menu item for one file: too easy to hit by accident) | gone, and not in trash |
| Restore / Empty Trash | keys in `trash:///` | menu + confirm | restored to the original path |
| Compress / Extract | — | context menu | archive round-trips to an identical tree |
| Permissions | inspector, tab-navigable | checkbox grid | `stat` mode matches the octal shown |
| Cross-pane transfer | `F6` | drag between panes | tree diff both panes |
| Undo / Redo | `Ctrl+Z` / `Ctrl+Shift+Z` | toast button | tree returns to the previous state exactly |
| Collision | prompt keys | prompt buttons | each choice (skip, replace, keep both) produces the right tree |

Plus the cases that actually break file managers, each a flow of its own: a name collision on paste into the source folder, a move across filesystems, a read-only destination, a symlink (moved, never followed), a file deleted by another process mid-job, and a paste of 5 000 files that is cancelled halfway.

## What the shell has to expose

`state()` already answers path, view, count, selection, inspector, sidebar, filter, sort, toast and, since this plan, the clipboard. Still missing, and each is a line in `Shell.qml`'s `IpcHandler`:

- `geometry(objectName)` — the rectangle to aim a pointer at, and the input to plan 11's layout assertions.
- `snapshot(objectName)` — `grabToImage` to a PNG, for the visual layer.
- `dialogState()` — which dialog is open, its fields and their errors, the focused field.
- `renaming()` — the row being renamed and the editor's text.
- `jobs()` — already `activity()`; the driver needs a `wait_for` over it rather than a sleep.
- Timer intervals (toast dismissal, search debounce) readable and settable, per plan 11's rule 6. Toast duration lost its settings row in plan 23 and needs a test-only override.

## The focus rule

A keymap that stops working is untestable and unusable, and the shell lost its focus twice during this plan's work. The rule is the binding and nothing else: **`Shell.qml`'s keymap item owns the focus whenever no overlay, editor or dialog does**, and the binding names every legitimate taker — filter, search, breadcrumb, menus, dialogs, ~~the AI panel,~~ project mode and the inline rename editor. (The in-app AI panel was removed 2026-09-19 — plan 31, decision 4.)

Taking the focus back whenever the item loses it was tried and **removed**: it races every legitimate hand-over. The inline editor opens, the focus moves to it, the grab pulls it straight back, the editor's own "focus lost" rule closes it, and `F2` does nothing perhaps one time in three. Quickshell's window reports no activation property to hang a safer condition on. If the shell is ever seen not to recover the focus after a compositor overlay takes the seat, the fix belongs on window activation, with a flow that proves it — not on a blanket re-grab.

Every shell flow now starts with `keyFocus` as a precondition and stops if it is false, so a swallowed keystroke fails loudly instead of leaving a trail of vacuously green checks.

## CI

`cargo test` and `qmltestrunner` on every push, both already green and both fast. The `cage` job — e2e, layout, visual, performance — on every pull request, in the container image plan 11 specifies, with pinned fonts. Drag flows may be marked flaky-retry; nothing else may.

## Order of work

1. Recording socket, `FakeDaemon.qml`, `objectName`s, a view test harness.
2. Layer A files, in the order of the matrix.
3. `cage`, `Makefile`, driver rework (fixtures, `wait_for`, `diff -r`).
4. Layer B flows, one per matrix row, then the edge cases. **Done**, except the rows that need a pointer: drag to move or copy, the toast's undo button, and the permissions grid.
5. `wlrctl` and the three flows layer A cannot reach: drag out of the app, rubber-band selection, click-at-a-point. **Done for clicks**; ~~drag still wants a pointer that can hold a button down~~ — **amended 2026-09-20:** no headless compositor gives a real drag with the tools we have, so drags are proven by the IPC hook and under `minimal`, and the physical press-move-release is a phase 10 line. Icon view's lasso is `tst_IconPane`'s (2026-09-21).
6. CI job.

Steps 1 and 2 give every operation both input methods on every push; 3 and 4 prove the bytes moved.

**Landed so far.**

*Step 1, the harness*: `tests/qml/stubs/KikiTest/Wire.qml` and the recording `Socket.qml` behind it; `tests/qml/FakeDaemon.qml` with an in-memory tree that answers `Open`/`Window`/`Sort`/`Filter`/`ShowHidden`/`SeekName`/`Refresh` and pushes the `Reset` that follows each of them; `objectName`s on rows, tiles, columns, list headers, menu items, form fields, key chips, sidebar items, the rename editor, the collision buttons and the permissions grid.

*The enabling refactor*: the operations moved out of `Shell.qml` into `qml/kiki/Ops.qml` — a plain object over one pane that imports nothing from Quickshell, so the tests drive the real code. The two things that need a window leave as signals (`confirmNeeded` for a confirmation, `copyText` for `wl-copy`). `Shell.qml` keeps one-line wrappers, so the keymap, the menu items and the IPC are unchanged.

*Step 2, coverage*: `tst_ListPane` (rows, click, double-click, right-click, inline rename, sort round trip, hidden files), `tst_IconPane` (tiles, click, Ctrl-click, double-click, right-click, zoom geometry), `tst_Ops` (the whole operations matrix as requests: copy/cut/paste, copy path, new folder including the free-name search and the select-and-rename that follows, rename, trash, delete with and without the confirmation, empty trash, restore, extract here, extract to, compress, chmod, cross-pane transfer), `tst_DragDrop` (the drop policy), `tst_CollisionPrompt` (each choice and apply-to-all), `tst_InspectorPermissions` (the checkbox grid ↔ octal ↔ symbolic, Apply, Revert, recursive), `tst_Wire` (the recorder itself). **The suite went from 32 passing with 6 failing to 112 passing.**

*Step 3, the end-to-end harness*: `Makefile` (`make`, `make test`, `test-rust`, `test-qml`, `test-e2e`, `run`, `daemon`, `shell`, `fmt`, `lint`, `clean`); `tests/e2e/harness.py` with a `Daemon` that frames its own lines (a timed-out socket poisons a `makefile()` object, and this one has to survive every drain), a `Shell` over `qs ipc` with `wait_state`, `wait_for`, a fixture builder, and tree `snapshot`/`diff` so an assertion reads `same_tree` rather than a path at a time; `tests/e2e/flows/` with one module per flow, each declaring what it needs (`daemon`, `shell`, `keyboard`) so a run without a compositor skips by name instead of failing; `driver.py` as the runner, giving each flow its own folder and its own listing ids. Every `time.sleep` is gone. `run.sh` builds a throwaway `XDG_RUNTIME_DIR`, config, state, thumbnail and trash directory, and a fake `secret-tool`, so a run leaves the machine as it found it.

Flows so far: `listing` (first rows inside 100 ms, natural order, the count reaching 10 000, metadata for the live window, sort after enrichment, the second open served from the cache), `trash` (trash, the `trashinfo` record, undo byte for byte, delete for good), `archive` (compress, preview the members, extract, and the tree that comes out equals the one that went in), `ops_menu` (new folder, copy/paste, cut/paste, trash, undo — each asserted as a tree difference), `ops_keyboard` (the same by `Ctrl+C`/`Ctrl+V`, `F2`, `Ctrl+Shift+N`, `Del`, `Ctrl+Z`).

*What the end-to-end run covered on 2026-09-19* (**85 checks, green three runs in a row**) — the table is kept as the record of that day; `ls tests/e2e/flows` is the inventory now, and what has been added since is listed under it:

| Flow | What it proves |
|---|---|
| `listing` | first rows inside 100 ms, natural order, the count reaching 10 000, metadata for the live window, sort after enrichment, a second open served from the cache |
| `trash` | trash, the `trashinfo` record, undo byte for byte, delete for good |
| `archive` | compress, preview the members, extract, and the tree that comes out equals the one that went in |
| `collisions` | skip, replace and keep both, each against the bytes on disk |
| `edge_cases` | a symlink moved as a link, a read-only destination failing the job and changing nothing, a copy into its own folder, a cancelled copy |
| `cross_fs` | a move from `/tmp` to `/dev/shm` — a real cross-device move — and undo bringing every byte back |
| `ops_menu` | new folder, copy/paste, cut/paste, trash and undo, through the context menu |
| `ops_keyboard` | the same by `Ctrl+C`, `Ctrl+V`, `F2`, `Ctrl+Shift+N`, `Escape`, `Del`, `Ctrl+Z` |
| `confirm_ops` | `Shift+Del` asks; Escape means the file stays; Enter deletes for good and nothing reaches the trash |
| `trash_view` | `trash:///` lists what was trashed, Enter restores it where it came from, Empty Trash asks and then leaves nothing |
| `archive_ui` | Compress… through the menu and its dialog, then Extract here, and the tree matches |
| `view_state` | `Ctrl+H`, the filter narrowing and clearing, Escape closing the filter bar, `Ctrl+1`/`Ctrl+2` keeping the selection |
| `pointer_ops` | click selects, click moves the selection, double click opens a folder, right click opens the menu and a menu item runs, Undo from the toast, and chmod by ticking the permissions grid and clicking Apply — 14 checks, green against a real session |

**Added since** (plan 31, phases 2–8), each in the default run unless it says otherwise: `git_status`, `launcher`, `two_windows`, `listing_live`, `activity`, `side_by_side`, `columns_ops`, `mirror_local`, `mirror_sftp` (needs `sshd`; its FTPS half needs `vsftpd`), `remote_transfers`, `drag_between_panes`, `smb` (needs `smbd`), and by name `scroll_perf`, `gallery_perf`, `transfer_local`, `transfer_remote`. A flow whose server is not installed skips by name rather than failing.

Two things the harness learned the hard way, both of which made a run lie:

- **cage does not hand back its client's exit status**, so `run.sh` reported 0 whatever the driver said — a whole-suite run printed "653 passed, 1 failed" and `make` called it a pass. The driver's verdict is written down inside the compositor and read outside it; no verdict at all is a failure.
- **cage stays up while anything is drawing in it**, so two detached terminals started by a project-mode check outlived the shell and the run sat until the compositor's deadline (`Error 124`, with every check passed). `run.sh` now ends whatever was started inside the run, known by the run's own `XDG_RUNTIME_DIR`.

**The pointer.** `wlrctl` drives it through the wlroots virtual-pointer protocol, and three things had to be true before a click meant anything:

- **The compositor has to deliver the events.** cage accepts the protocol and then logs `wlr_virtual_pointer_v1 cannot be mapped to an output device`: the events go nowhere. `pointer_ops` therefore `probe()`s before it runs — one click, and if the aimed row is not the row that ends up selected, the flow steps aside with the reason instead of reporting eight failures that say nothing about kiki. Under cage it skips; against a real compositor it runs.
- **The aim has to be absolute.** `wlrctl` only moves relatively, so the harness parks at the corner and steps to the target — and where the compositor can report the cursor (`KIKI_E2E_CURSORPOS_CMD`, `hyprctl cursorpos`) it reads back and corrects until the pointer is actually there, because on a multi-output desktop "park at the corner" clamps to the current output instead.
- **The target has to be the right one.** A name search finds pooled list delegates that still answer to the name they had in the last folder, which put every row click one row out. `rowGeometry(i)` asks the view which delegate is showing row `i` (`ListView.itemAtIndex`), and the aim became exact.

~~**Dragging is still out of reach**: `wlrctl` has only `click`, which presses and releases together … the last three matrix rows wait on it.~~

**Amended 2026-09-20: dragging is covered, by three routes instead of a virtual pointer** (plan 31, decision 5 and phase 5):

- **An IPC hook.** `shell drop <uris> <dest> <modifiers>` hands `Pane.dropInto` an object shaped like the `DragEvent` Qt would give it, so everything a drag does after the button goes down is what runs, and it answers what the drop decided (`{accepted, action, items}`) so a flow need not guess. `tests/e2e/flows/drag_between_panes.py` — in the default run — drives every local / SFTP / FTPS pair in both directions and each to itself, plain and with each modifier, a multiple selection, the refusals, and drops onto `trash:///` from every end: **98 checks**, each asserted on disk. Two more IPCs make that possible without a keyboard: `shell question [yes|no]` reads and answers the yes/no dialog, and `shell action <id>` runs a keymap action as its key would. (The URIs go newline-separated, as a `text/uri-list`: Quickshell's IPC strips square brackets out of an argument, so JSON does not survive the trip.)
- **Real in-process drags under `minimal`**, above.
- **By hand on Hyprland**, on plan 31's phase 10 checklist: the cursor the theme gives for copy, move and "not allowed"; a right-button drag; Escape mid-drag; a drop into the unfocused pane; the key table confirmed under QtWayland.

*Four traps the harness had to learn*, all of which made checks pass while nothing happened:
- **A fresh config means the first-run dialog is up**, over everything, swallowing every key. `run.sh` answers that question in `settings.toml` before the shell starts — a test run must never be asked to change the machine's defaults.
- **Job events go only to clients that ask**: without a `JobEvents` request the harness saw no job ever finish, so "wait for the job" returned instantly and the assertions raced it.
- **`wtype` uploads a keymap per invocation**, and sending the chord in the same breath races the compositor into dropping it; `-s 40` gives it a beat.
- **An assertion that something is absent passes when it was never there.** Every step that removes a file now checks it is present first, and a flow stops at its first failed precondition rather than reporting five green checks about a file that does not exist.

*Bugs the tests found, in the application rather than in themselves*:
- The collision prompt kept "apply to all" ticked across prompts, so a tick meant for one job would have silently replaced files in the next.
- Its Apply button read the recursive flag through `parent.parent.children[4]`, a positional lookup that any reordering would have broken silently.
- **A folder deleted and rebuilt at the same path can be served from the cache as it was.** The harness sidesteps it by never reusing a fixture path, but a file manager that shows the old contents of a recreated folder is worth a daemon-side look.
- **New folder raced its own watcher**: `renameSoon` was set in the mkdir reply, but on a fast filesystem the `Reset` from the watcher arrives first and the row lands with nothing waiting to rename it. It is set before submitting now.
- **And raced the view**: the editor opens on a row the list is still building, and a recycled delegate takes the focus away, which the editor reads as "the user clicked elsewhere" and closes itself. Opening it now retries every 40 ms until it sticks, and gives up after a second rather than spinning.

**Two traps the harness has to work around**, both singletons that outlive a test: `Settings.viewPrefs` carries per-folder view memory from one test into the next (clear it in `init()`), and `Wire.sent` accumulates across tests (`Wire.reset()`).

## Acceptance

- `make test` from a clean checkout runs all three suites and is green.
- Every row of the matrix has a passing test in both input methods, and each destructive one has an undo assertion.
- Deliberately breaking one operation (make paste drop the last file) fails a named test rather than a screenshot diff.
- Deleting `when: windowShown` from any test file makes that file fail loudly, not silently pass.
- The e2e suite runs three times in a row with no flake on a loaded machine.
