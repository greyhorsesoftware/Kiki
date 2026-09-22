# 07 — Side by Side (was "Split mode")

**Status:** built and tested as Side by Side (plan 29 J).

> Superseded in part by `24-mirror-view.md`, and renamed by plan 29 J: the split toggle and `Ctrl+Shift+S` are gone, and the two panes are **Side by Side** (`Ctrl+4`), entered automatically when a location is opened. It was called "Mirror view" for a while; **Mirror** now names only the operation (plan 08), never the layout. Side by Side is **not a view**: it is a toolbar button of its own, absent from the view menu, and it writes nothing into per-folder view memory. The pane behaviour, transfers and `Tab`/`F6` below still apply.

Builds on: `02-shell-and-views.md`, `06-remote-locations.md`.

Mockup: `SplitView.dc.html`.

## Goal

Selecting a location opens it side by side: left pane at the location's `local_uri`, right pane at its `remote_uri`. Both are ordinary plan-02 panes; either can be navigated to any URI afterwards. Copy, move and drag between panes are transfers with progress and undo.

## Design

- **Panes**: two independent listing models with their own history and view mode; one focused pane at a time (blue header underline). The toolbar's breadcrumb and search follow the focused pane; the view switcher applies to the focused pane.
- **Entering** (**amended 2026-09-21**, the owner's rules of that evening): clicking a location in the sidebar — or arriving at a server URI any other way, typed into the breadcrumb included — lays it out with the location's local folder left, the server right, the focus on the right. It fires on *arriving* at a server, not on every step inside one, so a folder opened in a server after the layout was turned off does not bring it back. The button and `Ctrl+4` are offered **only while a server is open in a pane, or while the layout is on**, so there is always a way back; **with no server open the key does nothing**. Turning it **off** leaves the **server's** pane, whichever had the focus; turning it **on** again from one pane on a server puts the server right and its location's local folder — or home — left. The divider between the panes still drags (7 px, the system's resize cursor, a double click resets), and its ratio is one window-level setting (`settings.toml [view] sideBySideRatio`), written when the drag ends. A script may still put two local folders side by side (`shell sideBySide toggle`), which the flows use.
- **Transfers**: Super+C / Super+V and drag between panes submit a copy or move job (plan 04) between the two backends. Progress shows in the status bar with a per-job bar and speed; the job list (Activity) shows all running jobs.
- **Undo** (**amended 2026-09-21**): a copy **to a server** is undoable — it journals `deleteCopies`, which removes only what the server still says is what it put there, leaves what has changed and names it, and says in both toasts that the delete is permanent (plan 04). A copy between two local folders undoes as ever. A **move that touches a server is not undoable**, deliberately.
- ~~**Badges**: the pane header badge and colour come from the location's plugin (`display_name`), so a new plugin shows correctly here without UI changes.~~ **Struck 2026-09-21** (plan 29 J dropped the chip): each pane header carries a **place icon and nothing else** — one drive for this machine, a green server for a remote — because the path beside it already begins with the location's name. Each header also shows that pane's own path (columns view's deepest open folder when that is what it is showing), and the focused pane has the accent underline.
- **Mirror** (plan 08) lives here: its two roots are the two pane URIs, whatever their schemes. The Mirror… button stands over the line between the panes.
- **IPC added**: `split(on|off)`, `focusPane(left|right)`, `transfer(copy|move)`, and `sideBySide toggle|drag <px>|end|reset` — the button, the divider drag and the double click, each answering the layout's whole state.
- **The keys follow the focused pane** (fixed 2026-09-21): `Up`/`Down`, `Home`/`End` and the page keys used to reach for the left view whichever pane had the focus, so on the right the selection walked out of sight.

## Verification

- Selecting a location opens both panes at their configured paths; `state()` shows both paths and both listings complete within 200 ms on a local test server.
- A cross-pane copy shows progress and Ctrl+Z removes the copy on the destination; a cross-pane move and its undo leave both trees as they started. (A move that touches a server has no undo: the flow asserts the copy's, not the move's.)
- Collapsing and re-expanding the split restores both pane paths and selections — both directions, since collapsing from the right swaps the two `Pane` objects and collapsing from the left destroys the right pane's view.

All of it is driven by `tests/e2e/flows/side_by_side.py` (75 checks, in the default run; the three that want a server run on the `sshd` fixture and skip by name without it), with `tst_SideBySideChrome` and `tst_ToolbarDisconnect` under it.
