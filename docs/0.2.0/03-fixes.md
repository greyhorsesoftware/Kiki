# 03 — Fixes in 0.2.0

Defects found after 0.1.1 shipped and fixed in this release, each with the test that holds it.
Newest last.

- **Double-clicking a file opens it with the default application (2026-09-24); a file on a
  server opens in Quick Look (2026-09-25).** It was `xdg-open` on the URI from the shell, which
  for an `sftp://` file had nothing to work with. Now the daemon's `OpenDefault` picks the
  default application for the file's type (`xdg-open` when none is registered) for a local file.
  For a day a remote file was fetched to the cache, opened in the application and its save sent
  back to the server; the owner withdrew that ("double click should just open the quicklook
  window for remote") — a double-click or `Enter` on a server's file opens Quick Look
  (`05-quicklook.md`), and the daemon refuses to open one in an application with 1330. Remote
  editing proper is a plan of its own, `docs/0.3.0/01-remote-edit.md`. `fetched.rs` (the fetch to
  the cache, kept for Quick Look), `desktop::open_default`, `Shell.openSelected`;
  `kikid/tests/open_default.rs`, `quick_look.py`.
- **Search everywhere before the index was ever built said "index 29,839,291 min old"
  (2026-09-25).** The daemon answered `Search` with `indexAge` counted from an index built at
  second 0. It now leaves `indexAge` out until there is an index, and the window says "no index
  yet" (`search.noIndex`). `server.rs`, `Shell.qml`; `API-DELTA.md`.
- **The e2e FTPS mock is held to 100 MB/s (2026-09-25).** `remote_transfers` uploads a 1.5 GB
  file to list beside it and cancel it mid-file; over loopback into tmpfs the whole file was in
  before the folder beside it had listed, and the flow's second try proved nothing either. vsftpd's
  `anon_max_rate` makes the upload 15 s long on any machine, so what the flow checks is checked
  every run, not on the machine's mood (owner: "there should be no timing issues in a test").
  `tests/e2e/servers.py`. The photographs flow likewise opens its own empty folder in the right
  pane before the mirror compare, instead of whatever the pane opened at (one run compared the
  real home: 550,500 entries).
- **No gallery on a server (2026-09-25).** The view menu greys the gallery out when the pane's
  folder is on a server (owner: "disallow gallery view for remote sftp/ftps views, just dim it
  in menu"); `Ctrl+5`, the arrow into a picture and the smart pick of a pictures folder follow
  the menu; a folder there remembered as a gallery opens as a list and is remembered as a list
  from then on (owner: "if there is a preference, make it list view"), a default of gallery is
  simply not applied there. Every picture in a remote gallery was a fetch; Quick Look (`05-quicklook.md`) is the
  way to look at one. `viewmenu.js`, `Shell.enterGallery`, `Pane._smart`/`isLocal`;
  `tst_ViewMenu`, `tst_PaneSmartView`.
- **The e2e harness works under `target/e2e/`, not `/tmp` (2026-09-25).** `/tmp` is a tmpfs of
  7.7 GB on this machine; a full run — two 1.5 GB upload fixtures among the rest — filled it,
  and with it every shell on the machine stopped answering (writes under `/tmp` failing with
  `EDQUOT`), including the run itself. The work directory is on disk now, under `target/`
  (ignored, gone with `cargo clean`), removed at the end of a run as before. `tests/e2e/run.sh`.
- **A server's folder no longer holds up the other pane (2026-09-25).** Requests from one
  window are handled in order, and `Open` on a remote folder connected to the server — the
  plugin's connect, seconds on a slow link — before answering, so the local pane's own `Open`
  queued behind it (owner: "why does local file listing wait for remote to fill in? can't we do
  it in parallel?"). The connect now happens on the listing's scan thread, the first time the
  folder is read (`RemoteDir::lazy`): `Open` answers at once for both panes, and a connect
  that fails is the scan's failure, carried by number in the `Count` event (`errorN`,
  `errorParams`) and said in the window's language. `vfs/remote.rs`, `listing/cache.rs`,
  `listing/scan.rs`, `WindowCache.qml`; `kikid/tests/remote_open.rs`.
- **A window and a daemon are a pair (2026-09-25).** The socket is named for the version —
  `$XDG_RUNTIME_DIR/kiki-0.2.0.sock` — on both sides (`config::socket_name`, `Daemon.qml`,
  the systemd socket unit templated at package time), and `Hello`'s reply carries `kikid`, the
  daemon's version, which the window compares with its own (`qml/kiki/version.js`): a daemon of
  another version is refused, every request answering "kikid is {daemon} but this window is
  {window}: restart kiki after an upgrade" in the window's language. So a checkout's window
  never lands on the installed daemon and a freshly upgraded window never on the daemon still
  running from before (owner: "that way kiki and kikid can be paired"; today's dev window had
  got "unknown request type" from the installed daemon). `make lint` checks Cargo.toml, the
  window and both PKGBUILDs say one version (`tests/version_check.py`); the e2e harness pins
  `KIKI_SOCKET` for its run. `tst_DaemonPairing`, `config::socket_tests`.
