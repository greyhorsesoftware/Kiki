# 01 — The engine belongs to the window; only the listener is on demand

**Status:** planned, 2026-09-26 (owner: "for 0.3, I want to look at restructure daemon, open
handling etc… I think the kiki app should start its daemon on demand, removing the need for a
systemd entry"; earlier the same day, on pairing: "why not just have kiki start it as part of
its startup — that way kiki and kikid can be paired"; then, on the shape: "does it make sense to
just build most functionality into main app and separate out the listening stuff into the on
demand version?").

**The shape, in one line.** `kikid` is not a service and not something the bus starts: it is the
window's engine, started by the window and gone with it. The only thing that must answer with no
window is the D-Bus listener — and that is already a separate 188-line binary
(`plugins/kiki-plugin-dbus`), which becomes the activated entry point and starts a window rather
than the bus starting an engine to start a window. What is wrong today is not the process
boundary but the *lifetime*: a system service where a worker process belongs.

## Today

- `kikid` is a user service: `kiki.socket` (socket activation, named for the version since
  0.2.1) and `kikid.service`, enabled for every user at install with `systemctl --global`; the
  `kiki` command reloads the units and starts or restarts them before opening a window. Two of
  0.2.x's point releases were about these units (a socket that never listened, an upgrade that
  needed a restart by hand), and the fixes are a launcher tending systemd on the user's behalf —
  the wrong layer doing the work.
- The window pairs with a daemon of its own version (`kiki-<version>.sock`, `Hello`'s
  `kikid`), retrying every 700 ms until one answers. It never starts one.
- Two things reach the daemon with no window: `org.freedesktop.FileManager1` (ShowItems from
  other applications) and the file-chooser portal — both through `kiki-plugin-dbus`, a 188-line
  binary the daemon spawns and bridges over a pipe, which claims both names itself
  (`src/main.rs:180-181`); the portal's D-Bus service file names `SystemdService=kikid.service`.
  So the listener is already a program apart — it is simply started by the wrong thing.
- Opening a file is five verbs and three modules: `OpenDefault` (a double-click), `Launch`
  (Open with… an application), `OpenIn` (a tool from `open-in.toml`: editors, terminals),
  `OpenTerminal`, `AiOpen`, on top of `desktop.rs` (MIME, desktop entries, spawning),
  `openin.rs` (tools, window classes, sessions) and `fetched.rs`; in the shell, `openSelected`,
  `openExternal`, `openIn`, `openWithMenu`, `hereItems`. The rule "a server's file is Quick
  Look, never an application" is applied in three places.
- `server.rs` is one 1 000-line `match` over ninety verbs; a slow verb that must not block the
  connection (requests from one window are handled in order) hand-rolls a thread and a reply —
  `Thumbnail`, `Arrange`, `PdfPage` each their own way.

## Decisions

1. **The window starts the daemon.** `Daemon.qml`, finding nothing on its socket, runs `kikid`
   — the one installed beside the shell (`/usr/bin/kikid`; `KIKI_DAEMON` names another, which
   `make run` sets to the checkout's) — with its own environment, then connects. `kikid` is
   single per socket: it binds the versioned socket, and one that finds the socket alive
   (another window won the race a moment earlier) exits 0 and lets that one serve. Nothing
   else starts it: no unit, no launcher logic. The `kiki` command becomes `qs -p …` and the
   `present` call, nothing more.
2. **The daemon leaves with its last window.** When the last connection closes it waits a
   grace (10 s — a window closed and reopened does not pay a cold start) and exits; jobs are
   already stopped when the window goes (the quit prompt's rule), so nothing is cut short.
   One rule, no exceptions: nothing but a window is ever a client, because after decision 4
   nothing but a window ever connects. (An earlier draft had the portal helper count as a client
   so a chooser would not lose its daemon under it; the split below removes the case.)
3. **No systemd units.** `packaging/systemd/` goes, with the `install -Dm644` lines, the
   `systemctl --global enable/disable` in `kiki.install` and every "restart kikid.service" note.
   Upgrades need nothing of the user: the new window looks for `kiki-<new>.sock`, finds none,
   starts the new daemon; the old daemon leaves when its old window does.

   **But the hook's own litter must be swept up.** `systemctl --global enable kiki.socket`
   (0.1.0–0.2.x, `kiki.install:11`) did not flip a switch — it made a symlink,
   `/etc/systemd/user/sockets.target.wants/kiki.socket → /usr/lib/systemd/user/kiki.socket`,
   for every account on the machine. The hook made it, so pacman does not own it and will not
   remove it; when this release deletes the unit file, which pacman *does* own, the symlink
   is left pointing at nothing and systemd logs a failed `kiki.socket` at every login, for ever,
   on every machine that ever had 0.2.x. So `post_upgrade` runs
   `systemctl --global disable kiki.socket` — `--global` writes under `/etc/systemd/user`, which
   a root hook can reach (`--user` is what it cannot: the note at the top of `kiki.install`) —
   and falls back to removing that one known path if a `disable` with no unit file behind it
   refuses. It is the path our own hook created, and nobody else's to clean. The hook stays for
   two releases after this one, because an upgrade straight from 0.2.2 to 0.3.2 runs only
   0.3.2's hook, and then it goes. **To confirm when implementing**: whether this systemd
   removes a dangling symlink on `disable` or only warns — the fallback is there either way, but
   the answer decides whether the fallback is dead code.
4. **The listener is the on-demand part, and it is not the engine.** `kiki-plugin-dbus`
   becomes `kiki-dbus` in `/usr/libexec/kiki` — a program in its own right rather than a
   plugin the daemon spawns — and it is what the bus activates: the portal's service file
   `Exec=/usr/libexec/kiki/kiki-dbus` with no `SystemdService=`. It already claims both names
   (`plugins/kiki-plugin-dbus/src/main.rs:180-181`); what changes is who starts whom. A request
   arrives, the listener starts or reaches a window the way the `kiki` command does, hands it
   over, relays the answer, and exits when it has no request outstanding and has held no name
   for a minute.

   **Through the window's IPC, not a socket of its own.** The window is a Quickshell process and
   has no socket to connect to: the listener reaches it exactly as `packaging/bin/kiki` does,
   `qs -p <shell.qml> ipc call shell <fn> …`, whose functions return a string — so `ShowItems`
   is a `present` and the chooser's answer comes back the same way. The listener therefore never
   connects to the daemon at all, which is what keeps decision 2 ("nothing but a window is ever a
   client") true rather than nearly true. Where a request needs a window and none is running, it
   starts one the way the command does and the window starts its own engine (decision 1).

   **It leaves when it is idle.** A request served and nothing outstanding for a minute
   (`KIKI_DBUS_IDLE_MS` for the tests) and it exits; the bus starts another for the next request.
   A chooser standing open counts as outstanding, however long the person takes. The first cut
   ended `main` in a future that never finished, and every test run left a listener behind
   holding a private bus open — the e2e flow is what found it.

   **`org.freedesktop.FileManager1` is a name kiki cannot simply take.** On an Omarchy box that
   service file belongs to another package — `Exec=/usr/bin/nautilus --gapplication-service`
   here — and two packages cannot own one path: shipping ours would be a file conflict, and
   pacman would refuse the install. Nor should it be taken silently. So it follows kiki's own
   rule for integration, the one `kiki.install` states at the top: it is **each user's choice**,
   a row in Settings › Omarchy beside mimeapps, the Hyprland bindings and the portal, which
   writes `~/.local/share/dbus-1/services/org.freedesktop.FileManager1.service` (the user's
   directory wins over `/usr/share`) and removes it again on Remove. Until a user asks, "Show in
   file manager" goes wherever it goes today.

   **A link, not a copy — or removing kiki breaks it.** The package cannot reach a user's home
   when it is removed, so whatever the row wrote stays. A *copy* naming `/usr/libexec/kiki/kiki-dbus`
   would then make every "Show in folder" fail with "Failed to execute program … No such file"
   (tried on 2026-09-26: it does, at once). So the row writes a **symlink** to the package's own
   copy under `/usr/share/kiki/dbus-1/services/`: when kiki is removed the link dangles, the bus
   skips a service file it cannot read (also tried: the system's handler ran), and nautilus has
   the name back with nothing to clean up. The row reads a dangling link as off, and Remove
   deletes it rather than leaving it. In a checkout there is no package, so it is a plain file
   naming the checkout's listener. A migration at every daemon start (`integrate::migrate_services`)
   brings 0.2.x's files up to date — copies naming the daemon and a unit become the link, and the
   per-user portal file goes, since the package's own file serves that name now — so a user who
   had the row on is healed by the upgrade without touching it. Verified on the owner's login,
   2026-09-26: the copy the row had written became the link at the next daemon start, and "Show
   in folder" still reached kiki.

   **Why this way round.** Both headless entry points need a *window* in the end — ShowItems
   shows a folder, and the file chooser **is** a picker window. Activating the engine means
   starting something whose only job is to bootstrap a window, which gives the daemon a second
   mode — running with no window, deciding whether to open one — that exists for nothing else.
   Cutting that mode out leaves one lifetime rule for the engine (decision 2) and puts the
   waiting where the waiting belongs. The cost is honest and small: two names on the bus from a
   second binary, and the listener lives as long as a chooser does — which it must, since it
   holds the reply path the portal is talking to.

   **The bridge stays what it is.** The listener still speaks kiki's framed JSON, now over the
   window's socket rather than a pipe from its parent: `ShowItems` becomes a `present` to the
   window, and the chooser's answer comes back as `ChooserResult` from the same window. It reads
   the socket name from its own version, as everything else does (decision 7), so an activated
   listener of one version never drives a window of another — it starts its own.
5. **Open handling in one place.** A daemon module `open.rs` with one verb:
   `Open { uris, with, line? }` where `with` is `"default"` (the type's application),
   `"app:<desktop id>"`, `"tool:<id>"` (an `open-in.toml` tool), `"terminal"` or `"ai"`; the
   reply `{ pid }` or `{ class, reused }` as the tool paths answer today. One rule table, applied
   once: a folder with `default` is the shell's to enter; a server's file with anything is 1330
   ("Quick Look shows it") — the shell sends a double-click there without asking;
   `access::record` happens here and nowhere else. `desktop.rs` keeps MIME and desktop entries,
   `openin.rs` keeps the tools; `OpenDefault`, `OpenWith`, `Launch`, `OpenIn`, `OpenTerminal`
   and `AiOpen` go in the same change — no aliases. A window and its daemon are the same build
   by construction (decision 7: the socket is named for the version and `Hello` refuses any
   other), so there is nobody an alias would protect: plugins never send requests, and the
   tests move with the code. `API-DELTA.md` records the change. The shell keeps one
   `open(uris, with)` and the menus build `with` strings.
6. **`server.rs` becomes a router.** The match splits by area — `handlers/{session,listing,
   tree,jobs,locations,open,quicklook,settings,integrate,share,ai,mirror,git,index,devices}.rs`
   — each `pub fn handle(cx: &mut Cx, name: &str, body: &Value) -> Option<Reply>`, tried in
   turn; the connection loop, `Hello`, `reply` and the numbered-error plumbing stay in
   `server.rs`. Slow verbs answer through one helper, `cx.later(|| …)`: a thread, the reply and
   its number sent from there — so no handler hand-rolls it, and the rule "a handler that can
   take longer than a listing answers through `later`" is written once, at the helper. The
   per-connection order stays as it is (it is what keeps a window's requests coherent); it is
   named in the module doc so nobody discovers it again by a pane that waits.
7. **The socket stays named for the version and `Hello` keeps checking** (0.2.1): the pairing
   is what makes "the window starts the daemon" safe — a checkout's window starts the
   checkout's daemon on `kiki-dev.sock`, never the installed one's.

## Layers

| | | |
|---|---|---|
| L1 | **Leaving and single-instance**: the client count with the grace, `kikid` exiting 0 when it finds the socket alive; `dbus.rs` loses the spawn-and-bridge, since nothing spawns the listener now. A Rust test that a daemon with no client exits after the grace, and one with a window connected does not. | ½ day |
| L2 | **Starting**: `Daemon.qml` runs `kikid` when the socket does not answer (`KIKI_DAEMON`), the `kiki` command reduced to `qs` + `present`, `make run` on the checkout's daemon by the same path; `tst_DaemonStart` against a fake. | ½ day |
| L3 | **The listener stands alone**: `kiki-plugin-dbus` → `kiki-dbus` under `/usr/libexec/kiki`, reaching the window through its IPC instead of a parent's pipe, starting one when there is none, exiting when idle; the portal's service file `Exec=` it; `packaging/systemd/` gone, `kiki.install` reduced to the `--global disable` sweep of decision 3 and two lines that ask for nothing; the FileManager1 row in Settings › Omarchy. **The test comes first, not last**: the chooser is a live D-Bus round trip with a window in the middle and has never had one, so the `dbus-run-session` harness is built and failing before the code moves — the daemon already runs under a private bus there for gvfsd (`tests/e2e/run.sh:123`), so it needs `XDG_DATA_DIRS` pointed at a fixture `dbus-1/services/` rather than new machinery. Then: no daemon → `kiki` → a daemon and a window; close → the daemon gone after the grace; ShowItems and a chooser with nothing at all running. **Done** (`tests/e2e/flows/dbus_activation.py`, 8 checks against a real window; it found the listener never exiting and the chooser in `open` mode taking no focus, so Escape reached nobody). | 2 days |
| L4 | **The router, and it comes before `open.rs`**: `handlers/`, `cx.later`, the hand-rolled threads moved onto it, module docs naming the per-connection order. No behaviour change: the whole gate is the test. First because `open.rs` is then written once, in its final home, instead of into the thousand-line match and moved out of it a day later. **Done 2026-09-26**: `server.rs` 1047 → 265 lines, fifteen modules under `handlers/` (99 verbs; `Hello` stays with the connection). `Arrange` and `PdfPage` answer through `later`. `Thumbnail` was never a thread of its own — it rides the thumber's queue, which has an order and a "still wanted" of its own — so parking a thread per tile would have changed how a gallery of hundreds behaves; it answers through `cx.answer_later`, the same reply builder handed to the queue's callback, which keeps the goal (no handler hand-rolls a reply) without the threads. | 1½ days |
| L5 | **`open.rs`**: the one verb and its rule table as one more handler, the old verbs removed outright (the pairing makes aliases pointless), the shell's one `open`, `API-DELTA.md`; Rust tests for every `with` on a local file, a folder, a server's file; the `ops_menu`/`launcher`/`quick_look` flows through the new verb. **Done 2026-09-26.** The verb is `Launch`, not `Open`: the daemon already had an `Open` — a listing's, asked before this handler — so an opening verb of that name would never have been reached (the Rust tests call the table directly and passed; the protocol test's sample table is what caught it). Two more things the plan had wrong: `OpenWith` was never an opening verb — it is the menu's *question* (which applications open these?) — so it stays; and `terminal`/`ai` act on a folder, so they keep 1251/1252 ("a folder on this machine"), which say the right thing, rather than 1330's "Quick Look shows it". The window's one `open` is `Daemon.open(uris, how, opts, cb)` — `how`, because `with` is a word JavaScript keeps. The e2e flows drive the window, not the verbs, and needed no change. | 1 day |
| L6 | **Docs and the rest**: API notes for 0.3.0, `docs/SKILL.md`'s transport paragraph, `README` (install has no units to speak of). **Done 2026-09-26**: `API-DELTA.md` (with L5), `SKILL.md`'s daemon and delivery paragraphs, the store plan's §4/§5 rows closed, the one stale "socket-activated" comment in `Daemon.qml`; the `README` never spoke of units. | ½ day |

About **6 working days**. L1–L3 are the owner's ask — the engine belonging to the window and
the listener standing alone; L4–L5 the restructure that makes the daemon and the open paths
readable again. Either half stands on its own.

**The order matters in one place.** L1 and L2 are reversible and can land while the units are
still installed: a daemon that leaves after its grace is simply socket-activated again by the
unit that is still there, so the gate passing after them means kiki works the same with systemd
and without it. L3 is where that stops being true. Everything before it is a step back if it
has to be; nothing after it is, which is why the manual pass on a real login — no units,
ShowItems from another application, a file chooser — belongs at the end of L3 and not later.
**Done, 2026-09-26**: the owner upgraded 0.2.2 → 0.3.0 on their own login; a browser's "Save
image as" went through the portal to a kiki chooser and back, and "Show in folder" opened kiki
with the file selected once the Settings › Omarchy row had written the user-level service file.

## Acceptance

- A fresh install: `kiki` opens a window, a `kikid` of the same version appears under it; close
  the window, ten seconds later no `kikid` is running; no `systemctl --user` anything.
- Upgrade 0.3.0 → 0.3.1 with a window open: the old pair keeps working; the next `kiki` starts a
  new pair on `kiki-0.3.1.sock`; when the old window closes its daemon leaves.
- A checkout's `make run` beside an installed kiki: two daemons, two sockets, neither aware of
  the other.
- Another application's "Show in file manager" with nothing kiki running: the bus starts
  `kiki-dbus`, which starts a window, which starts its daemon, and the folder appears with the
  file in it. The file chooser likewise; when the chooser is answered the listener exits, and
  when the window closes the daemon follows it.
- With a window already up, an activated listener uses it: no second window, no second daemon.
- `ps` while a chooser is open shows exactly one `kikid`, and its client is the window —
  `kiki-dbus` never connects as one.
- `Open` with `default` on `notes.txt` starts the editor; on a folder the shell enters it; on
  `sftp://…/notes.txt` the reply is 1330 and the shell shows Quick Look; `tool:nvim` and
  `terminal` likewise refuse a server's file; every old verb name still answers for one release.
- `server.rs` under 300 lines; every handler file under 400; no `thread::spawn` outside `later`.
- `make lint`, `cargo test`, `make test-qml`, `tests/e2e/run.sh` green.
