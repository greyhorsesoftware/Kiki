# 04 — `Shell.qml` split along its seams

**Status:** built, 2026-10-02 — five files out, in the order planned, each with the gate green
behind it; no test edited, no name changed. `Shell.qml` 1 871 → 1 194 lines:

| step | file out | lines | `Shell.qml` after |
|---|---|---|---|
| 1 | `ShellIpc.qml` — the `IpcHandler`, target `shell`, functions unchanged | 310 | 1 603 |
| 2 | `ShellKeys.qml` — `handleKey`, `runAction`, the Vim letters, the `dd` timer | 128 | 1 499 |
| 3 | `ShellMenus.qml` — the context, folder, gear, hamburger, view, Open with, path and mirror-bar tables; `menuUnder` | 203 | 1 328 |
| 4 | `ShellDrag.qml` — the ghost, the badge, the fan and its flight, the drop area, the tunnel-back, `fakeDrop` | 129 | 1 227 |
| 5 | `ShellChooser.qml` — `startChooser`, the token map, `showItems`, the `PortalDialog` | 59 | 1 194 |

Each is an `Item` or `QtObject` with a `required property var win`, declared in `Shell.qml`
where what it replaced stood, and registered in `qmldir`. What they reach of the window's
document — the dialogs, the probes, the two columns, the filters, the menu — is exported once
each as a `property alias` on `win` (33 of them, in one block after `toolbar`), never copied;
what the tests and the IPC reach of *them* (`runAction`, `contextItems…`, `lastFly`,
`fakeDrop`, `openWithSub`, `startChooser`, `chooserAnswer`, `showItems`, `portal`) is a
one-line forwarder or alias on `win`, so no caller changed. The badge, the ghost, the fan and
the drop area keep `parent: win.contentItem`; `Keys.onPressed` stays on the focus item and is
one line, `shellKeys.handleKey(event)`.

Under 700 was the target; 1 194 is where it stopped, and what is left is of a piece: the
window and its two panes, the side-by-side and mirror layout and the pane swapping, the
filter and search, the share and open-in and project glue, the inspector, the operations
facade over `Ops`, the view accessors, and the layout tree itself (the `Column` of toolbar,
pane row and bar, 300 lines of it). The next seam is `Pane.qml`'s drop and tunnel logic, which
the plan named for a third day and which was not needed to make this file readable.

**One defect found by the split, not made by it.** Each pane's `onReceived` (a drop landed
here) focused the pane by the *side it was declared on* — `win.focusPane(win.left)` inside
`left`'s handler — but the two Pane objects change places (`_swapPaneObjects`, on arriving at
a server and on leaving side by side), after which "left" is the other one, and a drop on the
right pane focused the left. `drag_between_panes` run alone fails on the committed tree
(`the pane a drop lands in takes the focus … focused: left`); in the full suite the flows
before it happen to leave the objects back in their declared places, which is why it was
never seen. Each pane now focuses itself by its own id (`paneA`/`paneB`) and tunnels the
*other* one back, whichever side that is. The e2e count is unchanged (the check was already
there; it now passes alone as well as in company).

## Today

One file holds: the window and its two panes' layout; `focusPane`, split and mirror mode;
the context-menu tables (part already in `viewmenu.js`); the keymap dispatcher (`Keys.onPressed`,
~100 cases); the IPC surface (`IpcHandler`, ~90 functions, `qml/kiki/Shell.qml:1150–1420`);
the chooser bookkeeping (`startChooser`, `chooserFinished`, `chooserAnswer`, `chooserDone`);
`showItems`; the drag glue (`dragGhost`, the badge, `flyPoints`/`flyFiles`, `DragTrack`
connections, the tunnel-back on `wentOut`); Quick Look's window and its key relay; `transfer`,
`newFolder`, `openSelected`, `present`, `editAt`; the dialogs' instances. The QML tests drive
almost all of it through the IPC, which is what makes a split safe: the tests do not change.

## Decisions

1. **Five files out, one in.** Each is an `Item` or a `QtObject` with a `win` property, placed
   in the same `qml/kiki/` folder, named for its subject:
   - `ShellIpc.qml` — the `IpcHandler` and nothing else. Its functions call `win.*`; no logic
     of its own beyond argument parsing.
   - `ShellKeys.qml` — the keymap dispatcher: `Keys.onPressed` becomes `handleKey(event)` here,
     with the `case` table; `Keymap.qml` stays what it is (the chords).
   - `ShellMenus.qml` — the context-menu and menu-bar item tables, joining `viewmenu.js`; they
     return arrays of `{ label, key, enabled, action }` as today.
   - `ShellDrag.qml` — the ghost, the badge, the fly, `DragTrack` wiring, the tunnel-back.
   - `ShellChooser.qml` — `startChooser`, the token map, `showItems`, and the `PortalDialog`
     instance.
   What stays in `Shell.qml`: the window, the panes, split/mirror, focus, the dialogs that are
   one line each, and the properties everything else reads. Target: under 700 lines.
2. **No renames on the way.** Every `objectName` the tests find, every IPC function name and
   every signal keeps its name; the split is `git mv`-shaped even where it is not a move. A
   reader of `git blame` after this sees one commit per file.
3. **One commit per file out**, each with the full gate green, so a regression bisects to a
   subject. The order: `ShellIpc` first (largest, most mechanical), then keys, menus, drag,
   chooser.
4. **The QML test count does not change, and no test is edited** — that is the acceptance.
   If a test *must* change, the split changed behaviour, and the commit is not right yet.

## Size

A day or two; the third day, if there is one, is the `Pane.qml` seam that shows up once
`Shell.qml` is readable (the tunnel and drop logic at `Pane.qml:213–310` want a `PaneDrop.qml`
of their own). Not in this plan unless it is obvious by then.
