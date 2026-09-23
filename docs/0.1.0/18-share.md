# 18 — Share

**Status:** built and tested; each way sent once for real.

Builds on: `06-remote-locations.md` (plugin processes, framing, keyring), `04-operations-and-undo.md` (jobs, Activity), `05-archives.md` (compressing folders), `14-open-in.md` (menus, settings page pattern).

## Goal

Select files or folders, open Share, pick a destination, done. Destinations are **share plugins**: separate processes that describe themselves, list targets (peers, contacts, devices) and send with progress. 0.1.0 ships two: **Mail** and **Tailscale**. A new one is a binary that speaks the contract below, in any language.

## Share plugin contract

A share plugin is an executable named `kiki-plugin-share-<id>` in the plugin directory, spawned on first use, idle-exit after 5 minutes, with the framing and request/reply rules of `API-PLUGIN.md`. Environment: `KIKI_PLUGIN_PROTOCOL=1`, `KIKI_SHARE_ID=<id>`.

| Request | Fields | Reply |
|---|---|---|
| `Describe` | | `{ id, name, icon, version, accepts: { files: bool, folders: bool, multiple: bool, maxBytes: u64 \| null }, targets: "list" \| "search" \| "none", form: [Field], secretFields: [string], compose: [Field], requires: [string] }` — `requires` names the programs the plugin needs on `PATH`; the daemon checks them each time it lists the plugins and adds **`unavailable: "<name> is not installed"`** to what the shell sees. (`defaultEnabled` / `off_by_default` went with LocalSend, 2026-09-21: nothing ships switched off any more.) |
| `Configure` | `config`, `secrets` | `{}` or `Invalid { field, message }` (validates and stores nothing; the daemon keeps config and keyring) |
| `Targets` | `config`, `secrets`, `query: string \| null` | `{ targets: [{ id, name, detail, online: bool, icon }] }` (for `"search"`, filtered by `query`) |
| `Share` | `id` (request), `config`, `secrets`, `uris: [Uri]`, `target: string \| null`, `compose: { key: string } ` | streamed `Progress { id, done, total, bytes, bytesTotal, status }` then `{ result: "sent" \| "opened" \| "queued", detail: string \| null }` |
| `Cancel` | `id` | `{}` |
| `Ping`, `Shutdown` | | as `API-PLUGIN.md` |

- `form` is the settings the plugin needs (accounts, commands), rendered by the daemon's generic form like a location dialog; `secretFields` go to the keyring.
- `compose` is what to ask at share time (subject, message text, a note), rendered as a small **share sheet** before sending; empty means no sheet.
- `targets: "none"` means the plugin has no recipient concept (Mail opens a composer, for instance).
- `result: "opened"` means the plugin handed off to another application (a mail composer) and cannot report delivery; `"queued"` means the destination accepted it for later.
- Files are passed as URIs. For remote URIs and device URIs the daemon fetches to a temp directory first (a plan-04 job with progress) and passes `file://` paths; a plugin never sees a non-local URI. Folders are compressed to a `.zip` (plan 05) unless `accepts.folders` is true.
- The plugin receives the user's environment and may spawn tools (`tailscale`, `xdg-email`); it must not write outside `$XDG_CACHE_HOME/kiki-share-<id>/`.

## Shipped plugins

**Mail** (`kiki-share-mail`) — no targets, `compose` = To, Subject, Message.
- Default backend `xdg-email --attach <file> --subject … --body …`, which opens the desktop mail handler with attachments (Thunderbird, Evolution, Geary).
- If the mail handler is a web app, which cannot receive attachments from `xdg-email`, the plugin's Describe form offers **SMTP** mode: server, port, security (STARTTLS, implicit TLS, none), username, password (keyring), from address, and an optional pinned certificate fingerprint for self-signed servers; then Share sends the message itself (its own SMTP client over rustls: EHLO, STARTTLS, AUTH PLAIN or LOGIN, one `multipart/mixed` message with base64 parts and RFC 2231 filenames, dot-stuffed) and reports `sent`. Tested against an in-process mock SMTP server in both TLS modes (`plugins/kiki-plugin-share-mail/tests/mock_smtp.rs`).
- Attachment total above 20 MB warns in the share sheet and suggests Tailscale or a compressed folder.

**Messages** and **AirDrop** — removed from 0.1.0 (2026-09-19). Messages wrapped `kdeconnect-cli` and `signal-cli`: LocalSend covers "to my phone" with nothing to pair (LocalSend went two days later — below; 0.1.0 has no "to my phone" at all), and few people have `signal-cli` linked. AirDrop wrapped OpenDrop over OWL, which needs a Wi-Fi adapter in active monitor mode and an unmaintained stack: a menu entry that rarely works is worse than none. Both are in the git history (`plugins/kiki-plugin-share-{messages,airdrop}`) if either is wanted back.

**Tailscale** (`kiki-share-tailscale`) — Taildrop.
- Targets are the tailnet's peers from `tailscale status --json` (name, OS, online), filtered to those that can receive files; refreshed on every open.
- Share runs `tailscale file cp <file>… <peer>:` per file with progress parsed from its output, and reports `sent`. Folders are compressed first since Taildrop takes files only.
- The form has one field, the `tailscale` binary path (auto-detected), and a check that the daemon is running and the user is logged in; otherwise Targets returns an error with the `tailscale up` hint.
- The **Taildrop inbox** (`tailscale file get`) is a later addition as a location.

**LocalSend** — removed from 0.1.0 (2026-09-21). It implemented the open nearby-transfer protocol's sender side in-process — multicast discovery with a subnet-wide HTTP fallback, a client certificate of its own, `prepare-upload` then one `upload` per file, PIN and decline handled, the send held to the fingerprint the device announced — and by 2026-09-19 it was fixed against the desktop app Omarchy ships: `register` answered, a send reached "waiting for the receiver to accept", a wrong fingerprint was refused. **It was never sent to a phone**, which is the whole point of it, and it shipped switched off waiting for that session. A way of sending that nobody has watched arrive is not something to put a release's name to. It is in the git history at `plugins/kiki-plugin-share-localsend` — plugin, mock receiver and pinned-TLS tests together — if it is wanted back; bringing it back is that directory, the workspace's members, the PKGBUILD's install loop, and a phone.

Nothing ships switched off any more, so a plugin no longer says how it ships: `off_by_default` / `defaultEnabled` went with LocalSend, its only user. An installed share plugin is on until the user switches it off in Settings → Share.

## UI

**Amended 2026-09-21 — as built.** What ships is the plugins the plugin directory holds: `plugins/kiki-plugin-share-mail` and `-share-tailscale`, and nothing else (LocalSend's directory is deleted).

- ~~**Share button** in the toolbar (share icon) with a dropdown~~ **There is no toolbar Share button.** The ways in are the context menu and `Alt+S`, which raise the same list.
- **Context menu**: ~~a "Share ▸" submenu~~ **each way of sending is a row of the menu itself** — "Send via Mail", "Send via Tailscale ▸" — rather than all of them behind one "Share ▸": one level less to the device, and a plugin's targets fit in the one submenu the menu has. Targets are asked for when the row is opened; the online ones come first as the plugin sorted them, offline ones are greyed, and "Nothing found" or the plugin's own error takes their place when there are none. A plugin whose program is missing **stays in the menu, dimmed**, with "not installed" at its right (`unavailable`, below). A plugin that names no targets is sent to at once — Mail opens a composer, and a form in front of that only asks for what the composer is about to ask for.
- **Share sheet** (`ui/ShareSheet.qml`): the selection summary, the plugin's `compose` fields, the chosen target, Cancel and Send. Skipped when the send can just go: the sheet opens if the daemon says more is needed.
- **Progress** appears in the Activity popover as a job; a toast confirms with what the plugin said. Tailscale's reads **"Shared via tailscale: sent — 1 file to davids-macbook-pro, waiting in its Tailscale"** — "sent" alone left nothing to go on when a file kiki had handed over did not turn up, so the plugin names the peer and where it is waiting, and a refusal carries Tailscale's own words. The same line goes into the job's log.
- **Settings page "Share"**: installed plugins with an enable switch and each plugin's form, a secret field travelling as a secret (`tst_SettingsWindow`). ~~detected state, and reorder for the menu~~ — no reorder; the menu is in the order the plugins are found.
- **Key**: `Alt+S` opens the list on the selection.

## Daemon

| Request | Fields | Reply |
|---|---|---|
| `SharePlugins` | | `{ plugins: [Describe results + enabled, configured] }` |
| `ShareTargets` | `plugin`, `query?` | `{ targets }` |
| `Share` | `plugin`, `uris`, `target?`, `compose` | `{ job }` (a plan-04 job: fetch and compress if needed, then the plugin's `Share`; progress on `JobEvents`) |
| `ShareConfigure` | `plugin`, `config`, `secrets` | `{}` |

Event: ~~`SharePluginsChanged {}`.~~ **Amended 2026-09-21 (D10): never built, never emitted; out of `API-DAEMON.md` too.** The list is re-read when a window asks for it, which is on open and after a `ShareConfigure`.

**IPC added**: `share(plugin?, target?)`.

**Mockups**: the Share dropdown open on the toolbar (Mail; Tailscale ▸ three peers with online dots), and the share sheet for Tailscale with two files and one folder to be compressed. Add before building.

**Plugins that need a program** say so in `Describe` (`requires: ["tailscale"]`). The daemon looks for it on `PATH` every time it lists the plugins and adds `unavailable: "tailscale is not installed"`; the Share menu keeps the entry, dimmed, with "not installed" at its right. Installing the program brings it to life without restarting anything. (Not logged in to Tailscale is a different thing and is still said when the peers are asked for.)

## Verification

- Contract tests run every share plugin binary: `Describe` well-formed, `Configure` rejects a bad field with the field name, `Targets` returns within 2 s, `Share` streams progress and ends with a result, `Cancel` mid-send stops the child process.
- Mail: with Thunderbird as handler, Share opens a composer with the two selected files attached and the subject filled; in SMTP mode against a local `smtp4dev`-style test server, the message arrives with correct MIME parts and filenames with spaces intact.
- Tailscale: with two test nodes, the peer list shows both, sending three files reports per-file progress and they arrive in the peer's Taildrop inbox; a folder is compressed first and arrives as one `.zip`; with the daemon stopped, Targets returns the `tailscale up` hint and nothing is spawned.
- A selection on an SFTP location is fetched to a temp directory with progress before the plugin sees it, and the temp files are removed after the result.
- Dropping a stub `kiki-share-stub` binary into the user directory adds it to the menu without restart.
