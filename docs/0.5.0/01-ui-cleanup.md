# 01 — UI cleanup

**Status:** planned, 2026-09-29 (owner: "under 0.5.0 lets create a ui cleanup plan"). A list,
not a design: small things seen in daily use, each with where it is in the code and what done
looks like. Items are added as they are found; each is built on its own and ticked here.

## The list

### 1. The rename box sits too far right

**Seen:** F2 on a row, or Ctrl+Shift+N for a new folder (which is a row created and put into
rename at once, `Ops.newFolder` → `pane.renamingIndex`), puts the text box a little to the right
of where the name it replaces was drawn. The eye sees the name jump.

**Where:** `views/RenameEditor.qml` is placed by each view with a literal — `x: 40` in
`views/ListRow.qml`, `x: 30` in `views/ColumnsPane.qml` — plus its own `leftMargin: 6` inside;
the name cell's text starts at the icon's width plus its gap, which is a different number, kept
elsewhere. Two literals that must agree, and do not.

**Done:** the editor's first character sits exactly over the name's first character, in the
list, the columns and the icon view (where the editor is under the icon, centred as the label
is). One place says where a name starts in a row — the row exports it (`nameX`, or the name
cell's own `x`) and the editor reads it, so a change to the icon size or gap moves both. The
frame around the box may extend left of the text (it has a padding); the *text* does not move.
A QML test puts a row into rename and compares the editor's text x with the name's.

### 2. The status message stays too long, and has an × it does not need

**Seen:** a copy done, a rename, an undo — the message rolls into the shortcut strip
(`ui/ShortcutBar.qml`, the 2026-09-24 "instead of toast" design) and sits there for eight
seconds (`Settings.timers.toastMs: 8000`, `Jobs.toastTimer`), with a small × beside it. Eight
seconds is long enough to read it three times; the × is a target nobody aims at, and the strip
comes back by itself anyway.

**Done:** the message stays about three seconds; one with **Undo** stays a little longer (five),
since Undo is the reason it is there, and a new message replaces the old at once. The × goes
(`toast-close` and its `MouseArea` in `ShortcutBar.qml`; `Jobs.dismissToast` stays for the
keyboard and for Undo). The timer's default moves in `Settings.qml` (`toastMs`, and a second
`toastUndoMs`); the setting is not surfaced in the Settings window — a number nobody would
change by hand. `tests/qml/tst_ShortcutBar.qml` loses its × case and gains the two lengths.

### 3. A drag that will copy shows the + — across machines and across mountpoints

**Seen:** dragging from a local folder to a server (or a stick) with no key held is a copy —
`Pane.dropAction`: a move within one place, a copy between places, "nobody asked for the
delete" — but the fan under the pointer shows no +. It only appears with Ctrl held.

**Where:** the badge is drawn from `DragTrack.action`, and `views/DropTarget.qml` feeds that
from `drag.proposedAction` — the *key* — rather than from `drag.action`, which `Pane.dragOver`
has just set to what the drop would really do (`Pane.qml:233`). So the badge says what was
asked, not what will happen. And "one place" is `placeOf`: scheme and authority, so every
`file://` is one place — a drag from the home to a mounted stick is a move, which the daemon
does as a copy and a delete on `EXDEV` (`ops.rs:218`), with no + because nothing called it a
copy.

**Done:** the badge follows the resolved action: `DropTarget` hands `DragTrack` the
`drag.action` that `dragOver` set, so over a server the + is up from the first pixel, over a
local folder it is not, and Ctrl still turns it on anywhere. And a different mountpoint is a
different place: a folder's listing carries the device it is on (`statx` is already read in
`listing/scan.rs`; a `device` field on the listing's header, one line in `API-DELTA.md`), the
pane knows its own, and `placeOf` for a `file://` URI the window itself is dragging is
`file://` plus that device — so home → stick is a copy with a +, and the stick keeps a copy
where before the file left the home. A drag from *another application* has no device to read
and keeps today's rule (one machine, a move). Two QML tests: a drag onto a `sftp://` target
with no key shows the badge; a `file://` target on another device does too, and on the same
device does not.

## Order and size

Each item is its own change and its own commit, in the order above: 1 is an afternoon, 2 an
hour, 3 half a day (the badge fix is two lines; the device is the rest). Only 3 touches the
daemon, by one field on something it already answers (the listing's `device`), recorded in
`API-DELTA.md`.

## Not in this plan

Anything that adds chrome (a new bar, a button, a menu entry): that is asked for first, never
built on the strength of a plan. The rename editor's *look* — its frame, colours, font — is as it is; only
its place changes.
