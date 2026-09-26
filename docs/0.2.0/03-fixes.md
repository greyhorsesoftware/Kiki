# 03 — Fixes in 0.2.0

Defects found after 0.1.1 shipped and fixed in this release, each with the test that holds it.
Newest last.

- **Double-clicking a file opens it with the default application; a remote file is fetched
  first, and a save sends it back (2026-09-24).** It was `xdg-open` on the URI from the shell,
  which for an `sftp://` file had nothing to work with. Now the daemon's `OpenDefault` picks the
  default application for the file's type (`xdg-open` when none is registered). A remote file —
  on a double-click, in "Open with…" and in "Open in" alike — is first brought to
  `~/.cache/kiki/open/<moment>/` by an ordinary copy job (seen in the orb, cancellable, its wire
  logged) and the application started on the copy when it has arrived; the folder is watched,
  and a save of the copy (a write and close, or an editor's rename over it) sends it back by
  another copy job — "Save notes.txt back to homelab" in the orb — replacing the original without asking. The copies are for the editing session only: the
  daemon empties the folder at start (owner, 2026-09-25: no keeping, no restoring). `openback.rs`,
  `desktop::open_default`, `Shell.openExternal`; `kikid/tests/open_default.rs`,
  `tst_OpenIn::test_a_double_click_on_a_file_asks_the_daemon_to_open_it_with_the_default_application`.
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
