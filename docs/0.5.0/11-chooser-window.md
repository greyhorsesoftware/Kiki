# 11 — The chooser becomes a window of its own

**Status:** **built 2026-10-04.** The chooser is a layer surface on the overlay layer
(`ui/ChooserWindow.qml`), above the application that asked rather than painted inside the file
manager's window and behind it. Driven under sway by `tests/e2e/flows/chooser.py`, where a
screenshot says it is over that application and gone after an answer.

## The fault

Owner, 2026-10-04: "save panel needs to show up above all other windows… did a screenshot,
clicked save as and the panel showed up behind the window I was in."

The chooser was not a window. `ui/PortalDialog.qml` is an Item, and it filled kiki's own window
(`ShellChooser.qml`), so another application's Save dialog was painted inside the kiki window —
behind the application that asked, and on whatever workspace kiki happened to be on. A window on
workspace 3 while the person is on workspace 1 showed the dialog where nobody was looking, and
the asking application waited.

This was the 0.1.0 design and it was written down as temporary
(`docs/0.1.0/09-omarchy-integration.md`): the dialog is "an overlay inside the kiki window… so
there is no `parent_window` to honour and no second window to place — the Hyprland rules for the
`kiki-chooser` class stay inert". `kikid/src/integrate.rs` had written three `windowrulev2` lines
(float, center, size 860 560) for a window class that never existed, since 0.1.0.

`startChooser` called `win.raise()` — `hyprctl dispatch focuswindow pid:…` — and it did not win:
on Wayland an ordinary window cannot put itself above the one that asked.

## The decision

**An overlay-layer surface** (owner, asked which kind: "layer is what it should be"). Above every
window including a fullscreen one, by the protocol rather than by a compositor rule; on the
output the person is looking at, whatever workspace the kiki window is on; and it leaves the kiki
window alone instead of taking it over.

## As built

`ui/ChooserWindow.qml` is a `WlrLayershell` — the window type, not a `PanelWindow` carrying the
same settings as attached properties: the two are the same surface, and a window type reads
plainly and can be stood in for by a plain `Window` where the leaf tests load the shell with no
compositor under them (`tests/qml/stubs/Quickshell/Wayland`). On it:

- `layer: WlrLayer.Overlay`, `keyboardFocus: WlrKeyboardFocus.Exclusive`,
  `namespace: "kiki-chooser"`, `exclusiveZone: 0`, and no anchors, which the protocol centres on
  both axes. Measured: a 400 × 200 probe lands exactly centred on a 1280 × 720 output.
- `Math.min(860, screen.width - 48)` × `Math.min(560, screen.height - 48)` — the size the box
  inside the window used to be is now the surface's.
- The surface exists only while a chooser is up, so the exclusive keyboard focus is held then and
  at no other time.

`PortalDialog.qml` keeps everything it had — `open`, `pick`, `finish`, `chooser`, `req`, the
filters, the `dialogs.portal` flag — and gains two small things: `ownWindow`, which drops the
frosted scrim (there is no window behind to frost) and lets the box be the whole surface, and a
`closed()` signal. No QML test moved.

`ShellChooser.qml` builds the window behind a `Loader` on first use, so a session that never
opens a chooser never builds its pane, listing, sidebar or search; everything that opens one goes
through `chooserWindow()`. `win.raise()` is gone from the chooser path — the surface is already
above everything, and raising would have pulled the person to the file manager's workspace — and
stays in "Show in folder", which does want the window in front. What used to read
`portal.visible` now asks `win.chooserUp`, which never builds the window by looking at it.

**Which output.** Hyprland is asked where the focus is (`Hyprland.focusedMonitor`) when a chooser
opens — read then, not bound, so a wandering focus cannot move a dialog mid-choice. Off Hyprland
the module answers nothing after one warning and Quickshell's default screen is used; on one
monitor that is the only answer there is, and on two monitors off Hyprland the chooser opens on
the first.

**The trap that cost the afternoon.** The window was first bound `visible: dialog.visible`. An
Item's `visible` is its parent's as well, so the moment the first chooser was answered the window
hid, the dialog's `visible` went false with it — and no later assignment could raise it, because
the window it was in was hidden. The second chooser opened on to nothing. Found with three
choosers in a row under sway; the window owns the state now (`up`), the dialog's visibility
follows from it, and `closed()` is what tells the window an answer happened.

The three `windowrulev2 … class:^(kiki-chooser)$` lines are gone from `hypr_block()`: Hyprland
matches a layer surface by its namespace with `layerrule`, never with a window rule, and the
chooser wants nothing a layer rule gives. A test asserts the class never comes back.

## Where it is tested, and what that costs

`cage`, which every other flow runs inside, has no `wlr-layer-shell` at all — measured both ways
with a throwaway overlay panel: under sway the surface is created and drawn, under cage
`Failed to initialize layershell integration`, twice, and a black screen. So the chooser is
driven under **sway**, locally (owner: "sway only — as long as they run locally when testing, we
don't need them in ci"):

| what runs | where | when |
|---|---|---|
| every flow in `FLOWS` | `cage`, as before | `run.sh`, `--part local`, `--part servers`, `--flow <any>` |
| `flows/chooser.py` (`PARTS["chooser"]`, not in `FLOWS`) | `sway`, headless | `run.sh` with no arguments, after the cage block; `--part chooser`; `KIKI_E2E_COMPOSITOR=sway` |

**The cost, paid knowingly:** the Save and Open answers proven on 2026-10-04 no longer run in CI,
because CI runs cage and cage cannot show the surface. What CI keeps is the dialog's content
(`tst_PortalPick`), the bus round trip (`plugins/kiki-plugin-dbus/tests/activation.rs`) and the
listener reaching the window (`flows/dbus_activation.py`, which no longer touches a chooser).

## What the sway flow proves that nothing else can

`tests/e2e/flows/chooser.py` starts a second window — `tests/e2e/asking-app.qml`, flat magenta,
full screen — to play the application that asked, covering the file manager's window completely.
Then, read off `grim` screenshots rather than believed:

- with a chooser up, the middle of the screen is the chooser's and the corners are still magenta
  — it is a surface **over** that application, not the whole screen and not inside kiki;
- once it is answered, the middle is magenta again — the surface really goes, rather than hanging
  about invisible with the keyboard.

Beside that it drives a Save to the end (the URI is the folder given and the name typed, and the
file written there lands), an Open (the URI is the file chosen, and it reads back what the
fixture wrote), an Open asked for several (every row, in order), and a cancel with a real Escape
key, which must reach the surface holding the keyboard and answer the bus.

## By hand, on the real desktop

Hyprland is the only place the original fault can be seen, and no test here runs under it:

1. Take a screenshot, click **Save as**: the chooser must appear centred and **above** the
   screenshot tool, not behind it, and not pull you to kiki's workspace.
2. An application's **Open** — a browser's file upload: the same, and the file picked arrives.
3. **Escape** closes it and the keyboard goes back to the application that asked.
