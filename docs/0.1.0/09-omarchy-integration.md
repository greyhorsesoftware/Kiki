# 09 — Omarchy integration

Builds on: `01-daemon-and-listing.md` (the daemon), `02-shell-and-views.md` (the window).

Mockup: `OpenDialog.dc.html`.

## Goal

kiki is Omarchy's file manager: `xdg-open` on a folder launches it, browsers and chat apps can "Show in folder", and every app's Open and Save dialogs are kiki's.

## Design

- **Desktop entry**: `org.kiki.App.desktop` (the id must equal the bus name for D-Bus activation to work) with `MimeType=inode/directory;` and `DBusActivatable=true`. No `x-scheme-handler/file`: `xdg-open` does not route `file:` through scheme handlers. Install runs `xdg-mime default org.kiki.App.desktop inode/directory`.
- **D-Bus activation**: kikid owns `org.kiki.Daemon`; `kiki` the window owns `org.kiki.App` with `Activate`, `Open(uris)`. Launching with a path opens or focuses a window at it (Hyprland focus via `hyprctl dispatch focuswindow`).
- **URIs everywhere**: the `kiki` binary accepts any URI (`kiki sftp://homelab/srv`), the desktop entry registers `x-scheme-handler/sftp` and `x-scheme-handler/ftps` so `xdg-open sftp://…` lands in kiki, and every D-Bus entry point below passes URIs straight to `open(uri)`.
- **Becoming the default (per user, reversible)**: the package only installs files under `/usr`, so kiki asks on first launch ("Make kiki your file manager?", four checkboxes) and does the per-user work itself through `Integrate { parts }`; Settings → Omarchy shows each item with its file, an Apply/Remove button, and "Remove kiki from Omarchy". The four items: (1) `inode/directory` and the `sftp`/`ftps` scheme handlers set to `org.kiki.App.desktop` in `~/.config/mimeapps.list` through `xdg-mime`, verified by reading the file back; (2) user D-Bus activation files in `~/.local/share/dbus-1/services/` for `org.freedesktop.FileManager1` and the portal backend name, which the session bus prefers over the distro's files in `/usr/share`, with `Exec=/usr/bin/kikid` and `SystemdService=kikid.service`; (3) a `# kiki: begin … # kiki: end` block in `~/.config/hypr/bindings.conf` with `Super+Shift+F` (open kiki), `Super+Alt+Shift+F` (open the terminal's folder via `omarchy-cmd-terminal-cwd`) and the float/center/size rules for the `kiki-chooser` window class (inert while the chooser is an overlay inside the kiki window; they take effect once it becomes its own window with that class), written atomically after a `hyprctl configerrors` preflight, reloaded, and rolled back automatically if Hyprland reports errors; (4) `org.freedesktop.impl.portal.FileChooser=kiki;gtk` in `~/.config/xdg-desktop-portal/portals.conf` (existing backends kept as fallbacks, gtk last) followed by `systemctl --user restart xdg-desktop-portal`. The first apply of each item records what it replaced in `~/.config/kiki/integration.toml` (the previous `inode/directory` handler, the previous FileChooser chain, any pre-existing override file), and Remove puts those exact values back rather than merely deleting kiki's lines; a second apply never overwrites the recorded original. The Hyprland block only adds lines, so stripping it is already exact. Implemented in `kikid/src/integrate.rs` with unit tests over a scratch home.
- **Show in folder**: implement `org.freedesktop.FileManager1` (`ShowItems`, `ShowFolders`, `ShowItemProperties`) on the daemon via `zbus`. `ShowItems` opens a window at the parent and selects the file; `ShowItemProperties` opens it with the inspector on.
- **Portal FileChooser**: an `org.freedesktop.impl.portal.FileChooser` backend (`OpenFile`, `SaveFile`, `SaveFiles`) in the daemon, with a portal config that routes FileChooser to kiki. The daemon owns the D-Bus interface; the dialog itself is a Quickshell window that the daemon asks the running kiki to show over its IPC (starting kiki if it is not running) and whose result comes back over the socket. Parenting to the caller's window via the portal's `parent_window` handle (`xdg_foreign`) is to be verified against Quickshell early; if unsupported, the dialog opens centred on the caller's monitor via Hyprland. The dialog is the mockup: a trimmed window (Favorites and Locations, breadcrumb, search, list, filter dropdown, Cancel / Open or Save). Filters, multiple selection, directory mode, current folder and suggested name all follow the portal spec. Remote locations are allowed; the daemon streams the file to a temporary local path and returns that URI.
- **Trash favorite** integrates with `org.freedesktop.FileManager1`-aware apps by exposing the standard Trash directory.

## Verification

- `xdg-open ~/` opens kiki at home; `xdg-open file.txt` still opens the file's own handler.
- Firefox's "Show in folder" reveals the download selected.
- A GTK app's Open dialog and a Qt app's Save dialog are kiki's; filters and suggested names arrive correctly; picking a file on the SFTP location returns a readable local URI.
- kiki is not launched twice for two `xdg-open` calls; the second focuses the existing window.
