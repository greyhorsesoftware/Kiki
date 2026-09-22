# 10 — Polish and packaging

**Status:** built; installed from the package once.

Builds on: all previous plans.

## Goal

Ship 0.1.0 as an Omarchy package.

## Design

- **Themes**: verify every Omarchy theme (Tokyo Night, Catppuccin, Nord, Gruvbox, Everforest, Kanagawa, Rose Pine, Matte Black) against the token map; fix any that lack a colour by deriving it in OKLCH from the theme's accent.
- ~~**Cheat sheet**: `?` opens a keybinding overlay generated from the keymap table in plan 02.~~ **Removed 2026-09-21** (owner): its rows were a subset of `Keymap.qml`'s and nothing but an IPC call opened it. `^?` opens the rebinding window, which is what the shortcut bar now says.
- **Performance pass**: re-run the plan-01 benchmarks with all features on; list, icon and columns views must still paint the first chunk within 16 ms. *(Amended 2026-09-21: the daemon half is `bench` and runs in CI, with a transfer profile; the shell-side probes are not in 0.1.0 — D20, plan 26.)*
- **Packaging**: one package, `kiki`, one command, `kiki`. Installed layout:

  **Amended 2026-09-21 — the table below is `packaging/PKGBUILD` as it installs today.** It named plugins that are not in 0.1.0 (`highlight`, `share-localsend`, `ptp`, `mtp`, `afc`), a D-Bus service for a name nobody owns, and a portal drop-in that was never read.

  | Path | Contents |
  |---|---|
  | `/usr/bin/kiki` | launcher: finds the running instance by the path of its `shell.qml`, hands it the URI over `shell present` and raises it; otherwise sets `KIKI_START` and runs `qs -n`. `kiki --ipc <fn> …` is how a tool kiki started calls back |
  | `/usr/bin/kikid` | the daemon |
  | `/usr/lib/kiki/kiki-thumber` | the thumbnailer's own process — the only part of kiki that decodes a file's contents. Without it: no thumbnails |
  | `/usr/lib/kiki/plugins/kiki-plugin-sftp`, `-ftps`, `-smb` | the location kinds in `plugin::LOCATION_KINDS`. `-smb` is the gio binary installed under that name: a kind is the suffix of the name a plugin is run by |
  | `/usr/lib/kiki/plugins/kiki-plugin-dbus` | FileManager1 and the portal FileChooser backend (plan 09) |
  | `/usr/lib/kiki/plugins/kiki-plugin-share-mail`, `-share-tailscale` | the share plugins (plan 18) |
  | `/usr/share/kiki/` | QML shell, `qml/icons/`, `qmldir` |
  | `/usr/lib/systemd/user/kiki.socket`, `kikid.service` | socket activation; `systemctl --global enable kiki.socket` in post-install, which takes effect from the user's next login (the launcher starts it before that) |
  | `/usr/share/applications/org.kiki.App.desktop` | menu entry, `inode/directory`, `x-scheme-handler/sftp`, `x-scheme-handler/ftps`; `Exec=kiki %U`, no `DBusActivatable` |
  | `/usr/share/icons/hicolor/scalable/apps/org.kiki.App.svg` | the mark the desktop entry names |
  | `/usr/share/dbus-1/services/org.freedesktop.impl.portal.desktop.kiki.service` | activation for the portal backend name (`Exec=/usr/bin/kikid`, `SystemdService=kikid.service`) |
  | `/usr/share/xdg-desktop-portal/portals/kiki.portal` | makes kiki's chooser *available*; whether it is used is each user's choice, written per user as `FileChooser=kiki;gtk` |
  | `/usr/share/licenses/kiki/LICENSE` | |

  The PKGBUILD builds all Rust binaries from one workspace (`cargo build --release --locked`, the workspace's `default-members`). **Post-install does nothing per user**: the hooks run as root, where `xdg-mime default` sets *root's* folder handler and `systemctl --user` reaches nobody's session — becoming the folder handler and the file chooser is each user's choice, offered by the first-run dialog (plan 09) and undone from Settings → Omarchy. It updates the desktop and icon caches, enables the socket globally, and says what to do next; post-remove reverses the cache updates and says how to pick another handler.

  **Depends**: `quickshell`, `xdg-desktop-portal`, `ffmpeg`, `gnome-keyring`, `libsecret` (`secret-tool`, with gnome-keyring as the Secret Service behind it), `libarchive` (`bsdtar`), `udisks2` (`udisksctl`), `git` (the repository overlay in every listing), `qt6-declarative`, `glib2` and `gvfs` (what the SMB plugin links and talks to). **Makedepends**: `rust`, `cargo`, `clang`, `pkgconf`, `glib2`. **Optdepends**: `poppler` (PDF thumbnails), `openssh` (SFTP host key tools), `gvfs-smb` (SMB locations), `gvfs-wsdd` (finding Windows computers), `tailscale` (Taildrop). ~~`libmtp`, `libgphoto2`, `libimobiledevice`, `usbmuxd` … `kdeconnect`, `signal-cli`, a mail client for `xdg-email`, `owl` and `opendrop` for AirDrop.~~ **Amended 2026-09-21:** the device libraries go with plan 17 (not in 0.1.0), and Messages, AirDrop and LocalSend are out of plan 18. `vsftpd` is **not** a dependency of anything: it is a prerequisite of the e2e suite only (`sudo pacman -S openssh vsftpd samba`, `tests/e2e/servers.py`). Third-party plugins are a single file in `~/.local/lib/kiki/plugins/` — though a *location* plugin dropped in there is ignored by a release build, since `LOCATION_KINDS` is compiled in (`API-PLUGIN.md`). An install line goes into `omarchy`'s package list if adopted.
- **Distribution**: a tag triggers CI to build the package for `x86_64` (Arch container) and `aarch64` (GitHub ARM runner with an Arch Linux ARM container), attach `kiki-<ver>-1-<arch>.pkg.tar.zst` for both to the GitHub release, and push an updated `PKGBUILD` to the AUR for `kiki-bin` with per-architecture sources (repackages the release binary; also a `kiki` source package for people who build). Users install with `yay -S kiki-bin`, which Omarchy's AUR helper supports, and get updates with their normal system update; `pacman -U` on the release file works without the AUR. An own pacman repository (`repo-add` on a static host, one `[kiki]` line in `pacman.conf`) is added only if Omarchy does not adopt kiki into its own repository.
- **Files**: `.github/workflows/ci.yml` (format, lint, tests and a package build on both architectures; the shell harness on x86_64), `.github/workflows/release.yml` (build both packages, publish the GitHub release, update `kiki-bin` on the AUR; needs `AUR_USERNAME`, `AUR_EMAIL`, `AUR_SSH_PRIVATE_KEY` secrets), `packaging/PKGBUILD` (source build, both architectures), `packaging/aur/kiki-bin/PKGBUILD`, and the launcher, units, desktop entry, D-Bus service and portal files under `packaging/`.
- **Docs**: README with install, keymap and the location and mirror workflows. *(Amended 2026-09-21: written — see `README.md`, which carries the install section, the keymap and both workflows. Nothing of this item is left here.)*

## Verification

- Fresh Omarchy VM: install the package, log in, `xdg-open ~/` opens kiki, Open dialogs are kiki's, the daemon starts on demand.
- Every theme eyeballed once, as part of the manual checklist in `11-testing.md`. ~~screenshot reviewed side by side with the mockup palette~~ **Amended 2026-09-21:** there are no visual baselines to review against — the visual layer is not in 0.1.0 (D20), so the theme pass stays a hand pass.
