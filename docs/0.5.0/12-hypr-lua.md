# 12 — kiki's Hyprland block is Lua

**Status:** built, 2026-10-06. Owner: "super shift F - does not open kiki ? why not ?" — and
then, on keeping the old file as a fallback: "I dont think we need the conf version anymore".

## The fault

Hyprland's configuration on Omarchy is Lua. A current `~/.config/hypr/` holds `hyprland.lua`,
`bindings.lua`, `input.lua`, `looknfeel.lua`, `monitors.lua`, `autostart.lua` — and no
`hyprland.conf`. `hyprland.lua` loads Omarchy's defaults and then requires the person's own
files, `require("hypr.bindings")` among them.

`integrate.rs` wrote its `# kiki: begin … # kiki: end` block into
`~/.config/hypr/bindings.conf`, which **nothing sources**. Measured on the owner's desktop:
`hyprctl binds` lists 228 binds and not one of them is kiki's; `hyprctl configerrors` is clean,
because there is no error to report — the file is simply never read. So the launch keys have
never worked here, and neither have Quick Look's float and pin rules, which is why its window
tiles beside the file manager instead of floating over it. The integration's own row said it had
been applied, truthfully: it had written the file it meant to write.

Two more things the measurement turned up, each of which the fix has to answer:

- **Omarchy's defaults already own both chords.** `bindings/applications.lua` binds
  `SUPER + SHIFT + F` to Nautilus and `SUPER + ALT + SHIFT + F` to Nautilus in the terminal's
  folder; `hyprctl binds` shows the live one (`key: F`, `modmask: 65`, "File manager"). Binding
  a chord that is taken *adds* a binding. Omarchy's own instructions — the comments in the
  person's `bindings.lua` — say to `hl.unbind` first and then bind.
- **`o.*` is not sugar.** `o.bind(…, { launch = "x" })` goes through `command_from` to
  `o.launch("x")` = `uwsm-app -- x`: every application Omarchy binds starts inside a uwsm
  systemd scope. A bare `hl.dsp.exec_cmd("kiki")` would start kiki outside the scope everything
  else on that desktop runs in.

## The decision

**Lua only, no fallback to the old language** (owner). What kiki writes into
`~/.config/hypr/bindings.lua`:

```lua
-- kiki: begin
pcall(hl.unbind, "SUPER + SHIFT + F")
pcall(hl.unbind, "SUPER + ALT + SHIFT + F")
if o and o.bind then
  o.bind("SUPER + SHIFT + F", "File manager (kiki)", { launch = "kiki" })
  o.bind("SUPER + ALT + SHIFT + F", "File manager here (kiki)", { launch = 'kiki "$(omarchy-cmd-terminal-cwd)"' })
else
  hl.bind("SUPER + SHIFT + F", hl.dsp.exec_cmd("kiki"), { description = "File manager (kiki)" })
  hl.bind("SUPER + ALT + SHIFT + F", hl.dsp.exec_cmd('kiki "$(omarchy-cmd-terminal-cwd)"'), { description = "File manager here (kiki)" })
end
if o and o.window then
  o.window({ title = "^(.* — Quick Look)$" }, { float = true, pin = true })
else
  hl.window_rule({ match = { title = "^(.* — Quick Look)$" }, float = true, pin = true })
end
-- kiki: end
```

Why it is shaped like that:

- **Unbind, then bind**, because the chords are taken. Both unbinds are *inside* the block,
  which is what makes Remove exact: delete the block and Omarchy's own binding is there again,
  with nothing recorded and nothing to put back.
- **`pcall` around the unbind**, so a Hyprland that never bound the chord — or one old enough
  to have no `hl.unbind` — does not fail to load a person's configuration over it.
- **Omarchy's helpers when they are loaded, Hyprland's own when they are not.** `o` exists only
  while Omarchy's defaults are in; the block must not need them. On Omarchy kiki is launched the
  way every other application there is, inside the uwsm scope. `{ launch = … }` is concatenated,
  not quoted, so the terminal-folder form survives as `uwsm-app -- kiki "$(omarchy-cmd-terminal-cwd)"`
  and the command substitution is done by the shell Hyprland runs it in.
- The three `kiki-chooser` window rules are not here: the chooser became a layer surface on
  2026-10-04 (`11-chooser-window.md`) and no window rule can match one.

## Migration

A block in `bindings.conf` written by any earlier version has never done anything, and leaving
it leaves a person reading their own configuration something that looks like it works. Applying
*or* removing sweeps it (`sweep_conf`): kiki's block goes, the file goes with it when the block
was all it held — kiki made that file — and the file stays, shorter, when it was not.

## Verification

`integrate.rs`'s own tests, over a scratch home: the block lands in `bindings.lua` beside the
person's own lines and applying twice does not double it; Remove leaves the file exactly as it
was found, unbinds and all; each chord is unbound before it is bound and no unbind survives
removal; the block carries nothing of the old language (`bindd`, `windowrulev2`) and no
`kiki-chooser`; the old file's block is swept in all three cases above; and the block is run
through `luac -p`, so what kiki writes into a person's configuration is known to parse (a
machine without `luac` says so rather than asserting nothing).

What no test here can show is Hyprland accepting it, since the suite has no Hyprland: that is
the apply-time `hyprctl configerrors` preflight, the reload, and the rollback, which are
unchanged — and the owner pressing Apply.
