# 18 — Share

Builds on: `06-remote-locations.md` (plugin processes, framing, keyring), `04-operations-and-undo.md` (jobs, Activity), `05-archives.md` (compressing folders), `14-open-in.md` (menus, settings page pattern).

## Goal

Select files or folders, open Share, pick a destination, done. Destinations are **share plugins**: separate processes that describe themselves, list targets (peers, contacts, devices) and send with progress. 0.1.0 ships three by default, **Mail**, **Messages** and **Tailscale**, plus two nearby-transfer plugins, **LocalSend** and an experimental **AirDrop**. A new one is a binary that speaks the contract below, in any language.

## Share plugin contract

A share plugin is an executable named `kiki-plugin-share-<id>` in the plugin directory, spawned on first use, idle-exit after 5 minutes, with the framing and request/reply rules of `API-PLUGIN.md`. Environment: `KIKI_PLUGIN_PROTOCOL=1`, `KIKI_SHARE_ID=<id>`.

| Request | Fields | Reply |
|---|---|---|
| `Describe` | | `{ id, name, icon, version, accepts: { files: bool, folders: bool, multiple: bool, maxBytes: u64 \| null }, targets: "list" \| "search" \| "none", form: [Field], secretFields: [string], compose: [Field] }` |
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
- The plugin receives the user's environment and may spawn tools (`tailscale`, `xdg-email`, `kdeconnect-cli`, `signal-cli`); it must not write outside `$XDG_CACHE_HOME/kiki-share-<id>/`.

## Shipped plugins

**Mail** (`kiki-share-mail`) — no targets, `compose` = To, Subject, Message.
- Default backend `xdg-email --attach <file> --subject … --body …`, which opens the desktop mail handler with attachments (Thunderbird, Evolution, Geary).
- If the mail handler is a web app, which cannot receive attachments from `xdg-email`, the plugin's Describe form offers **SMTP** mode: server, port, security (STARTTLS, implicit TLS, none), username, password (keyring), from address, and an optional pinned certificate fingerprint for self-signed servers; then Share sends the message itself (its own SMTP client over rustls: EHLO, STARTTLS, AUTH PLAIN or LOGIN, one `multipart/mixed` message with base64 parts and RFC 2231 filenames, dot-stuffed) and reports `sent`. Tested against an in-process mock SMTP server in both TLS modes (`plugins/kiki-plugin-share-mail/tests/mock_smtp.rs`).
- Attachment total above 20 MB warns in the share sheet and suggests Tailscale or a compressed folder.

**Messages** (`kiki-share-messages`) — targets from whichever messaging backends are present:
- **KDE Connect** devices (`kdeconnect-cli -l --id-only`, share via `--share`), which covers sending to a paired phone and, through the phone, SMS and any app; online state from the CLI.
- **Signal** recipients when `signal-cli` is installed and linked, from its configured contacts, sent with `signal-cli send -a <file> -m <text>`.
- **Matrix** rooms when a `matrix-commander`-style CLI is configured in the form.
- Each backend is a section in the form with an enable switch and its command paths; targets are listed grouped by backend with an icon; `compose` = Message. Sending goes through the CLI with progress by byte count where the CLI reports it, else indeterminate.

**Tailscale** (`kiki-share-tailscale`) — Taildrop.
- Targets are the tailnet's peers from `tailscale status --json` (name, OS, online), filtered to those that can receive files; refreshed on every open.
- Share runs `tailscale file cp <file>… <peer>:` per file with progress parsed from its output, and reports `sent`. Folders are compressed first since Taildrop takes files only.
- The form has one field, the `tailscale` binary path (auto-detected), and a check that the daemon is running and the user is logged in; otherwise Targets returns an error with the `tailscale up` hint.
- The **Taildrop inbox** (`tailscale file get`) is a later addition as a location.

**LocalSend** (`kiki-share-localsend`) — the open nearby-transfer protocol with apps for iOS, Android, macOS and Windows.
- Discovery by multicast on the LAN (and the protocol's HTTP fallback); targets are the devices found, with their type icon, refreshed on open.
- Share implements the protocol's sender side in the plugin with its own HTTP/1.1 client over rustls (self-signed receiver certificates are accepted, as the protocol intends): `prepare-upload` with every file's name, size and type, then one `upload` per accepted file with byte progress; `401` asks for the PIN (from the `compose` sheet), `403` is a decline, files the receiver skipped are reported in the result. A typed `host` or `host:port` in the target search is a manual target for receivers that do not answer multicast. Tested against an in-process mock receiver (`plugins/kiki-plugin-share-localsend/tests/mock_receiver.rs`).
- This is the recommended "AirDrop to a phone" path on Linux: it works on any Wi-Fi.

**AirDrop** (`kiki-share-airdrop`, experimental) — wraps OpenDrop on top of OWL, the open AWDL implementation.
- Only usable on Wi-Fi adapters whose driver supports active monitor mode and with the `owl` daemon running; the plugin's Describe reports those checks and the menu hides it otherwise, with the reason on the Settings page.
- Targets are the Apple devices OpenDrop discovers; the receiver must have AirDrop set to Everyone (recent iOS caps that at ten minutes) and sees the sender as an unnamed device.
- Share sends with per-file progress and reports `sent` on the receiver's accept, `Cancelled` on decline.
- Receiving is out of scope for both nearby plugins in 0.1.0.

## UI

- **Share button** in the toolbar (share icon) with a dropdown: one row per installed plugin, then, expanding on hover or right arrow, that plugin's targets; the button itself reopens the last used target. Disabled with a tooltip when the selection is empty or exceeds a plugin's `accepts`.
- **Context menu**: "Share ▸" submenu with the same structure.
- **Share sheet**: a small dialog with the selection summary (count, total size, "3 folders will be compressed"), the plugin's `compose` fields, the chosen target, Cancel and Send. Skipped entirely when `compose` is empty and a target was chosen from the menu.
- **Progress** appears in the Activity popover and the shortcut bar as a job; a toast confirms `sent`, `opened` or `queued` with the target name.
- **Settings page "Share"**: installed plugins with detected state, each plugin's form, enable switch, and reorder for the menu.
- **Key**: `Alt+S` opens the Share dropdown on the selection.

## Daemon

| Request | Fields | Reply |
|---|---|---|
| `SharePlugins` | | `{ plugins: [Describe results + enabled, configured] }` |
| `ShareTargets` | `plugin`, `query?` | `{ targets }` |
| `Share` | `plugin`, `uris`, `target?`, `compose` | `{ job }` (a plan-04 job: fetch and compress if needed, then the plugin's `Share`; progress on `JobEvents`) |
| `ShareConfigure` | `plugin`, `config`, `secrets` | `{}` |

Event: `SharePluginsChanged {}`.

**IPC added**: `share(plugin?, target?)`.

**Mockups**: the Share dropdown open on the toolbar (Mail; Messages ▸ two KDE Connect devices and a Signal contact; Tailscale ▸ three peers with online dots), and the share sheet for Tailscale with two files and one folder to be compressed. Add before building.

## Verification

- LocalSend: against the reference app on a phone on the same Wi-Fi, the phone appears within 3 s, two files arrive with correct names, a PIN receiver accepts the PIN from the sheet.
- AirDrop: on a supported adapter with `owl` running, a Mac set to Everyone appears and accepts a file; on an unsupported adapter the plugin is hidden and Settings shows why.
- Contract tests run every share plugin binary: `Describe` well-formed, `Configure` rejects a bad field with the field name, `Targets` returns within 2 s, `Share` streams progress and ends with a result, `Cancel` mid-send stops the child process.
- Mail: with Thunderbird as handler, Share opens a composer with the two selected files attached and the subject filled; in SMTP mode against a local `smtp4dev`-style test server, the message arrives with correct MIME parts and filenames with spaces intact.
- Messages: with a paired KDE Connect phone, the target lists it as online and the file appears on the phone; with `signal-cli` linked to a test number, a file and text arrive.
- Tailscale: with two test nodes, the peer list shows both, sending three files reports per-file progress and they arrive in the peer's Taildrop inbox; a folder is compressed first and arrives as one `.zip`; with the daemon stopped, Targets returns the `tailscale up` hint and nothing is spawned.
- A selection on an SFTP location is fetched to a temp directory with progress before the plugin sees it, and the temp files are removed after the result.
- Dropping a stub `kiki-share-stub` binary into the user directory adds it to the menu without restart.
