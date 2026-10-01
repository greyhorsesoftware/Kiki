# 04 — `Shell.qml` split along its seams

**Status:** planned, 2026-09-29. The audit called `Shell.qml` "the one file carrying more than
its subject" at 945 lines; it is 1 866 today. Every feature since — the chooser map, the
drag ghost and its badge, the fly, the tunnel, the `Launch` verb, the dbus IPC — landed in it
because there was nowhere else to land. Mechanical, no behaviour change, and the thing that
makes every later UI item cheaper.

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
