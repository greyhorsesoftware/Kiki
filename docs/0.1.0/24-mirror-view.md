# 24 — Mirror view (built as **Side by Side**)

**Status:** built as Side by Side; the mirror bar, Swap and "last mirrored" are dropped.

**Renamed 2026-09-21** (plan 29 J): the two-pane layout is **Side by Side** everywhere a user can read it — it is a layout, not a view, and it is not in the view menu. "Mirror" stays the name of the operation it can start (Mirror to…, `Ctrl+M`, the workspace, the report — plan 08). Read "Mirror view" below as "Side by Side".

Builds on: `07-split-mode.md` (two panes, transfers), `08-mirror.md` (the mirror engine and screens), `21-view-memory-and-columns.md` (per-folder memory), `23-ui-refinement.md` (view menu).

Mockup: `SplitView.dc.html` (titled "Mirror view").

## Goal

Split mode and the mirror toggle were two toolbar buttons with four combinations. ~~Mirror is now simply the fourth view: **Icon, List, Columns, Mirror**.~~ **Amended 2026-09-21:** it is a **toolbar toggle of its own** (`Ctrl+4`) and the view menu is four views. Side by Side is the two panes, the server's folder on the right and the folder it is kept beside on the left, with the mirror run one click away — from the button standing over the line between the panes. A server opens in it; per-folder view memory is **off** while it is on (plan 21, plan 29 J).

## Behaviour

**As built, 2026-09-21** (owner: *"when you connect to a remote location it should activate side by side … if you unclick it goes to show the remote location only; turning it back on you see both. It should not be toggleable/visible unless a remote location is open."*):

- **Offered only while a server is open in a pane, or while the layout is already on** — so there is always a way back to one pane. With no server open, `Ctrl+4` and the button do nothing. (A script may still put two local folders side by side: `shell sideBySide toggle`, which the flows use.)
- **Entering**: clicking a location in the sidebar (its local folder left, the server right, focus right), opening a server URI directly (breadcrumb, IPC, Show in folder), or the button and `Ctrl+4` from one pane standing on a server — which puts the **server on the right** and its location's local folder, or home, on the **left**, the way a location opened from the sidebar is laid out.
- **The pairing fires on _arriving_ at a server** — when the host changes — not on every step inside one. It used to fire on every navigation to a server URI, so a double click on a folder in the server brought the layout back after it had been turned off. It survives the pane swap that leaving does (`_leftHost`).
- **Leaving**: the button or `Ctrl+4`. One pane again is **the server's pane**, whichever had the focus (between two local folders it is the one in use); the other folder waits in the hidden pane for the next toggle, with its selection and history. The layout is never left by picking a view.
- **The button** stands left of Mirror while the layout is on, and where the path ends while it is off (the path gives it the room). It is lit — box and all — while on.
- **Disconnect** sits beside Mirror while a pane is on a server: a broken chain, "Disconnect from ghs". It sends `Disconnect` for that location, moves any pane standing on the server to the location's local folder (or home) and goes back to one pane. The sidebar's menu item does the same thing through the same function. `tst_ToolbarDisconnect`.
- ~~**The mirror bar**: left path, a swap control between the paths, right path, "last mirrored 2 h ago" … a chevron opening Upload, Download, Swap sides and Open remote alone.~~ **Dropped 2026-09-21** (plan 29 J): the strip above the panes is off. The way into a mirror run is the Mirror button standing over the line between the panes, and `Ctrl+M`. **Swap** and **"last mirrored"** went with the strip and are not in 0.1.0 — `settings mirror.last` is still written when a run finishes, and nothing shows it.
- **Transfers** between panes (drag, `Super+C`/`Super+V`, `F6`) and `Tab` to switch panes are unchanged from plan 07.

## Default view by contents

When a folder has no memory (plan 21) ~~and **Pick the view by contents** is on (Settings → General, default on)~~ — **amended 2026-09-21:** there is no such switch; the guess rides on "Remember view per folder", which is the setting that decides whether a pane reads and writes view memory at all:

- a remote URI opens Side by Side;
- a folder named Pictures, Photos, DCIM, Screenshots, Wallpapers, Camera or Camera Roll, or one where at least 60 percent of the first 200 entries are images or videos (checked once the listing is done and at least twelve rows are held), opens in ~~Icon view~~ **Gallery** (**amended 2026-09-21**, D18: the code opens Gallery and is right — a grid of smudges is what Gallery exists to replace. `tst_PaneSmartView`);
- everything else uses the global default.

Memory wins over the rule, the rule wins over the default. A view the user picks is remembered as before, so the rule never fights a choice — and the pane's own guess is not written back as a choice. Side by Side makes no guess at all (plan 21).

## Settings

General: ~~Default view gains "mirror"; **Pick the view by contents**.~~ **Amended 2026-09-21:** neither was built. Side by Side is a layout, so it is not offered as a default view (a `default = "mirror"` left by an older build still starts the window side by side and draws as List), and the view-by-contents guess has no switch of its own. `settings mirror.last` maps a remote root to the time of its last completed run and is read by nothing.

## Protocol and keys

No daemon messages change. Keys: `Ctrl+4` Side by Side; `Ctrl+M` mirror to the remote; `Ctrl+Shift+S` removed. IPC: `shell split on|off` and `shell sideBySide toggle` enter and leave the layout; `shell mirror open` starts an upload run.

## Verification

- ~~Clicking a location shows Mirror view with the mirror bar; the toolbar has no split or mirror buttons.~~ **Reversed 2026-09-21** (plan 29 J): clicking a location shows Side by Side with **no** bar above the panes, and the toolbar is where its controls are — Side by Side, Mirror, and Disconnect while a pane is on a server.
- With no server open the button is absent and `Ctrl+4` does nothing; opening one offers both.
- Turning it off from a server leaves the server's pane on screen, whichever side it was; turning it on again puts the server right and its local folder left.
- A double click on a folder inside the server does not bring the layout back.
- ~~`Ctrl+4` on `~/Projects` opens the last location on the right; with none, home.~~ **Struck 2026-09-21** (owner): Side by Side is offered only while a server is open in a pane, or while it is on; with no server open the key and the button do nothing. See plan 31, 2026-09-21 evening.
- Opening `sftp://homelab/srv/kiki` from the breadcrumb pairs it with `~/Projects/kiki` on the left.
- ~~Swap moves local to the right; "Mirror to homelab" still uploads from the local side.~~ **Dropped with the bar** — there is no way to Swap in 0.1.0. The workspace still treats the `file://` side as local, whichever pane holds it.
- A folder of 40 photos with no memory opens in **Gallery**; after switching it to List, it reopens in List.
- ~~After a run finishes the bar reads "last mirrored just now" and survives a restart.~~ **Dropped with the bar.**

Driven end to end by `tests/e2e/flows/side_by_side.py` (75 checks, the three that want a server on the `sshd` fixture), `tst_SideBySideChrome`, `tst_ToolbarDisconnect` and `tst_PaneSmartView`.
