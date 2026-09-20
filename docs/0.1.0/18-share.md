# 18 — Share

Builds on: `06-remote-locations.md` (plugin processes, framing, keyring), `04-operations-and-undo.md` (jobs, Activity), `05-archives.md` (compressing folders), `14-open-in.md` (menus, settings page pattern).

## Goal

Select files or folders, open Share, pick a destination, done. Destinations are **share plugins**: separate processes that describe themselves, list targets (peers, contacts, devices) and send with progress. 0.1.0 ships three: **Mail**, **Tailscale** and, for nearby devices, **LocalSend**. A new one is a binary that speaks the contract below, in any language.

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
- The plugin receives the user's environment and may spawn tools (`tailscale`, `xdg-email`); it must not write outside `$XDG_CACHE_HOME/kiki-share-<id>/`.

## Shipped plugins

**Mail** (`kiki-share-mail`) — no targets, `compose` = To, Subject, Message.
- Default backend `xdg-email --attach <file> --subject … --body …`, which opens the desktop mail handler with attachments (Thunderbird, Evolution, Geary).
- If the mail handler is a web app, which cannot receive attachments from `xdg-email`, the plugin's Describe form offers **SMTP** mode: server, port, security (STARTTLS, implicit TLS, none), username, password (keyring), from address, and an optional pinned certificate fingerprint for self-signed servers; then Share sends the message itself (its own SMTP client over rustls: EHLO, STARTTLS, AUTH PLAIN or LOGIN, one `multipart/mixed` message with base64 parts and RFC 2231 filenames, dot-stuffed) and reports `sent`. Tested against an in-process mock SMTP server in both TLS modes (`plugins/kiki-plugin-share-mail/tests/mock_smtp.rs`).
- Attachment total above 20 MB warns in the share sheet and suggests Tailscale or a compressed folder.

**Messages** and **AirDrop** — removed from 0.1.0 (2026-09-19). Messages wrapped `kdeconnect-cli` and `signal-cli`: LocalSend covers "to my phone" with nothing to pair, and few people have `signal-cli` linked. AirDrop wrapped OpenDrop over OWL, which needs a Wi-Fi adapter in active monitor mode and an unmaintained stack: a menu entry that rarely works is worse than none. Both are in the git history (`plugins/kiki-plugin-share-{messages,airdrop}`) if either is wanted back.

**Tailscale** (`kiki-share-tailscale`) — Taildrop.
- Targets are the tailnet's peers from `tailscale status --json` (name, OS, online), filtered to those that can receive files; refreshed on every open.
- Share runs `tailscale file cp <file>… <peer>:` per file with progress parsed from its output, and reports `sent`. Folders are compressed first since Taildrop takes files only.
- The form has one field, the `tailscale` binary path (auto-detected), and a check that the daemon is running and the user is logged in; otherwise Targets returns an error with the `tailscale up` hint.
- The **Taildrop inbox** (`tailscale file get`) is a later addition as a location.

**LocalSend** (`kiki-share-localsend`) — the open nearby-transfer protocol with apps for iOS, Android, macOS and Windows.
- Discovery by multicast on the LAN (and the protocol's HTTP fallback); targets are the devices found, with their type icon, refreshed on open.
- Share implements the protocol's sender side in the plugin with its own HTTP/1.1 client over rustls (self-signed receiver certificates are accepted, as the protocol intends): `prepare-upload` with every file's name, size and type, then one `upload` per accepted file with byte progress; `401` asks for the PIN (from the `compose` sheet), `403` is a decline, files the receiver skipped are reported in the result. A typed `host` or `host:port` in the target search is a manual target for receivers that do not answer multicast. Tested against an in-process mock receiver (`plugins/kiki-plugin-share-localsend/tests/mock_receiver.rs`).
- This is the recommended "AirDrop to a phone" path on Linux: it works on any Wi-Fi.
- **Why it did not work, and what changed (2026-09-19).** No HTTPS send to a real LocalSend had ever succeeded; the tests passed because their receivers spoke plain HTTP. Tried against the app itself (`localsend` 1.18, which Omarchy ships):
  1. **Receivers require a client certificate** (mutual TLS) and end the handshake with "certificate required" without one. The plugin presented none. It now makes a self-signed certificate once, keeps it in `$XDG_CACHE_HOME/kiki-share-localsend/` (key `0600`), presents it, and announces that certificate's SHA-256 as its `fingerprint` — it used to announce a hash of the hostname, which matched nothing (`identity.rs`).
  2. **Discovery found nothing while the LocalSend app was running on the same machine**: the app holds UDP 53317 and the plugin's plain bind failed, silently. The socket is now shared (`SO_REUSEADDR`/`SO_REUSEPORT`).
  3. **Multicast alone is not enough anyway.** A device answers an announce by POSTing to the announcer's port 53317, and only falls back to a multicast reply if that fails — with the app running here, the app takes that POST and kiki hears nothing; and many access points pass no multicast between clients at all. Discovery now also does what the app's own fallback does: `POST /api/localsend/v2/register` to every address of the /24, HTTPS then HTTP, 0.9 s each, in parallel with the two-second multicast window (`scan_subnet`, `probe`). Outbound only, so the firewall is not involved (Omarchy's ufw already opens 53317 both ways).
  4. The device's fingerprint now rides in the target id and the send is held to it (`30-code-health.md`, W1).
  Verified by hand against the running app: `register` answers 200 with its alias and fingerprint; a send reaches "waiting for the receiver to accept" (before: handshake failure); a wrong fingerprint is refused. `tests/pinned_tls.rs`'s receiver now demands a client certificate, as the real one does. **Still untried: a phone** — section D of `29`.

- Receiving is out of scope in 0.1.0.

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

**Mockups**: the Share dropdown open on the toolbar (Mail; Tailscale ▸ three peers with online dots), and the share sheet for Tailscale with two files and one folder to be compressed. Add before building.

**Plugins that need a program** say so in `Describe` (`requires: ["tailscale"]`). The daemon looks for it on `PATH` every time it lists the plugins and adds `unavailable: "tailscale is not installed"`; the Share menu keeps the entry, dimmed, with "not installed" at its right. Installing the program brings it to life without restarting anything. (Not logged in to Tailscale is a different thing and is still said when the peers are asked for.)

## Verification

- LocalSend: against the reference app on a phone on the same Wi-Fi, the phone appears within 3 s, two files arrive with correct names, a PIN receiver accepts the PIN from the sheet.
- Contract tests run every share plugin binary: `Describe` well-formed, `Configure` rejects a bad field with the field name, `Targets` returns within 2 s, `Share` streams progress and ends with a result, `Cancel` mid-send stops the child process.
- Mail: with Thunderbird as handler, Share opens a composer with the two selected files attached and the subject filled; in SMTP mode against a local `smtp4dev`-style test server, the message arrives with correct MIME parts and filenames with spaces intact.
- Tailscale: with two test nodes, the peer list shows both, sending three files reports per-file progress and they arrive in the peer's Taildrop inbox; a folder is compressed first and arrives as one `.zip`; with the daemon stopped, Targets returns the `tailscale up` hint and nothing is spawned.
- A selection on an SFTP location is fetched to a temp directory with progress before the plugin sees it, and the temp files are removed after the result.
- Dropping a stub `kiki-share-stub` binary into the user directory adds it to the menu without restart.
