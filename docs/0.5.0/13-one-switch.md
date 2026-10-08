# 13 — One switch, and what it does in a person's words

**Status:** built, 2026-10-06 (owner: "lets also simplify the omarchy tab. just one switch on/off
w/ list of stuff we bind (in human terms) no file nonsense" … "yes do the dialog too").

## What was there

Two screens asked the same question in the same shape, part by part, in kiki's vocabulary rather
than a person's.

Settings → Omarchy was four rows — a label, the word on or off, an Apply or Remove button, and
the file that part writes, elided to fit: `~/.config/mimeapps.list`,
`~/.local/share/dbus-1/services`, `~/.config/hypr/bindings.lua`,
`~/.config/xdg-desktop-portal/portals.conf` — under a sentence about per-user integration, over
"Make kiki the default" and "Remove kiki from Omarchy". The first-run dialog was the same four as
checkboxes, each with a line beneath it like `inode/directory in ~/.config/mimeapps.list` or
`FileChooser=kiki;gtk in ~/.config/xdg-desktop-portal/portals.conf`.

Which file a desktop integration lands in is kiki's business. A person deciding whether to let a
file manager have the Super key does not want a path; they want to know what will be different.

## What is there now

One switch on the page, two buttons in the dialog, and the same list under both — built once, in
`T.omarchyDoes()`, so the two cannot drift:

- Super+Shift+F opens kiki
- Super+Alt+Shift+F opens the folder your terminal is in
- Folders open in kiki
- Another app's Open and Save dialogs are kiki's
- "Show in folder" from another app comes to kiki
- Quick Look floats above the window

Six lines for four parts: the keys are two sentences because they are two keys, and the Hyprland
part carries Quick Look's window rule, which is worth saying and is not a keybinding. The list is
what it *does*, not what kiki writes.

## Decisions

1. **All of it or none.** `Integrate` and `Unintegrate` go out with no `parts`, and the first-run
   dialog no longer lets three of four through. A desktop with kiki on the keys but GTK's dialog
   on Open is a desktop nobody chose; it was only ever reachable because the screen offered it.
   The verb still takes a list for anyone driving it over the wire — this is the UI's decision,
   not the protocol's.
2. **The switch is on only when every part is in place.** A half-applied desktop — one step
   refused, one file edited by hand since — must not read as done, and the switch is the only
   thing on the page that can say so now that the per-part words are gone.
3. **What fails is named in the words of the list**, not by its file: "Could not set: the keyboard
   shortcuts." The daemon's own message says which file it could not write; that is the thing
   this page stopped showing. The one exception kept is the Hyprland config-error line, which
   quotes what Hyprland itself reported — a real reason, in Hyprland's words, for the one step
   that can be refused by something outside kiki.
4. **The strings are in all three catalogs** (`omarchy.does.*`, `settings.omarchyPart.*`,
   `settings.omarchyTrouble`, `settings.omarchyWhatItDoes`, `settings.omarchyHint`,
   `integration.makeDefault`, `integration.applying`), and the thirteen keys that fell out of use
   are gone from them — `settings.mime`, `settings.dbus`, `settings.hypr`, `settings.portal`,
   `settings.omarchyIntro`, `settings.apply`, `settings.remove`, `settings.on`, `settings.off`,
   `settings.stepsFailed`, `settings.removeFromOmarchy` and the four `integration.*` labels.

## Verification

`tst_SettingsWindow`: the switch is on with every part and off with one missing; turning it on
sends one `Integrate` with no `parts`; turning it off sends `Unintegrate`; a failed part is said
in the list's words and the sentence carries no `/`. The page and the dialog are drawn by the
`photographs` flow in all three languages, which is where the words are looked at.
