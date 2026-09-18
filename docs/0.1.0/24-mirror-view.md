# 24 — Mirror view

Builds on: `07-split-mode.md` (two panes, transfers), `08-mirror.md` (the mirror engine and screens), `21-view-memory-and-columns.md` (per-folder memory), `23-ui-refinement.md` (view menu).

Mockup: `SplitView.dc.html` (titled "Mirror view").

## Goal

Split mode and the mirror toggle were two toolbar buttons with four combinations. Mirror is now simply the fourth view: **Icon, List, Columns, Mirror**. Mirror view is the two panes, local on the left and remote on the right, with the mirror run one click away in a bar above them. Remote locations open in it by default, a local folder can switch to it, and per-folder memory remembers the choice like any other view.

## Behaviour

- **Entering**: `Ctrl+4`, the view menu, clicking a location in the sidebar (local path left, remote path right, as before), or opening a remote URI directly (breadcrumb, IPC, Show in folder): the shell finds the location by scheme and name and pairs it with its local path. Choosing Mirror on a plain local folder puts the last used location on the right, or the home folder when there is none, so it doubles as a two-pane local commander.
- **Leaving**: pick any other view. The split toggle and `Ctrl+Shift+S` are gone.
- **The mirror bar**: left path, a swap control between the paths, right path, "last mirrored 2 h ago" (from `settings mirror.last`, written when a run finishes) or "not mirrored yet", and the action: an accent split button **Mirror to \<location\>** with the direction arrow, a `⌃M` chip and a chevron. The main part starts an upload run (Configure → Review → Running from plan 08); the chevron opens Upload, Download, Swap sides and Open remote alone. `Ctrl+M` outside Mirror view enters it and starts the run.
- **Swap** exchanges the two panes' URIs; the workspace always treats the `file://` side as local, whichever pane holds it.
- **Transfers** between panes (drag, `Super+C`/`Super+V`, `F6`) and `Tab` to switch panes are unchanged from plan 07.

## Default view by contents

When a folder has no memory (plan 21) and **Pick the view by contents** is on (Settings → General, default on):

- a remote URI opens in Mirror view;
- a folder named Pictures, Photos, DCIM, Screenshots, Wallpapers, Camera or Camera Roll, or one where at least 60 percent of the first 200 entries are images or videos (checked once the listing is done and at least twelve rows are held), opens in Icon view;
- everything else uses the global default.

Memory wins over the rule, the rule wins over the default. A view the user picks is remembered as before, so the rule never fights a choice.

## Settings

General: Default view gains "mirror"; **Pick the view by contents**. `settings mirror.last` maps a remote root to the time of its last completed run.

## Protocol and keys

No daemon messages change. Keys: `Ctrl+4` Mirror view; `Ctrl+M` mirror to the remote; `Ctrl+Shift+S` removed. IPC `split on|off` now enters and leaves Mirror view; `mirror open` starts an upload run.

## Verification

- Clicking a location shows Mirror view with the mirror bar; the toolbar has no split or mirror buttons.
- `Ctrl+4` on `~/Projects` opens the last location on the right; with none, home.
- Opening `sftp://homelab/srv/kiki` from the breadcrumb pairs it with `~/Projects/kiki` on the left.
- Swap moves local to the right; "Mirror to homelab" still uploads from the local side.
- A folder of 40 photos with no memory opens in Icon view; after switching it to List, it reopens in List.
- After a run finishes the bar reads "last mirrored just now" and survives a restart.
