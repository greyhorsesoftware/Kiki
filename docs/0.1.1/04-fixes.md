# 04 — Fixes in 0.1.1

Defects found after 0.1.0 shipped and fixed in this release, each with the test that holds it.
Newest last.

- **Columns view: a blank strip after the info column (2026-09-24).** With two columns dragged
  to a width (`[view.columnsWidths]`, `c0 = 255`, `c1 = 203`) and a file chosen in the second,
  a full-screen window showed the two columns, the info column at its own width, and some 400 px
  of nothing to the right: hand-set widths were kept to the pixel, and the columns only shared
  the room when none of them had one. Now the last column takes whatever the others and the
  info column leave — a hand-set width is its floor, not its size — so the info column always
  meets the strip's right edge, and with no panel up the last column does.
  `ColumnsPane.widthOf`; `tst_ColumnsPane::test_the_last_column_fills_to_the_info_column_at_the_strips_edge`.
- **Columns view: Apply in the info column did nothing (2026-09-24).** The last column's
  inspector had no `chmod` handler, so its Apply button emitted into nothing; found wiring the
  selection's chmod through it. The column now forwards `chmod` and `chmodMany` to the shell like
  the docked panel does. `ColumnsPane.qml`; covered by `tst_ColumnsPane`'s info-column tests.
- **`plugin_contract` failed on any machine with kiki installed (2026-09-24).** The test asserted
  the plugin inventory held exactly one kind, the stub; `plugin_dirs()` also walks
  `/usr/lib/kiki/plugins`, so the installed package's six plugins made it seven, and `available()`
  four. It now asserts the stub is among what is found. `kikid/tests/plugin_contract.rs`.
- **The info card opened too tall and shrank on the switch to Permissions (2026-09-24).** The
  card is as tall as the taller of its two tabs, the other tab laid out in an unseen copy; that
  copy was `visible: false`, and in a hidden subtree a row's `visible` change never reaches its
  Column, so the copy kept the layout it was created with — the single-file Symbolic row over a
  selection, a git-tracked file's rows over a plain one — until a tab switch remade it. The copy
  is now unseen by opacity and disabled, and lays out like the seen one. `Inspector.qml`;
  `tst_InspectorSelection::test_the_compact_card_keeps_one_height_across_tabs_after_the_rows_change`.
- **SFTP rows had no owner or group (2026-09-24).** The plugin sends the names; the daemon's
  `meta_from` dropped them, `Meta` holding only a uid and gid resolved through local `passwd`.
  Remote names are now interned into that same id space (`listing::names`, above `0x8000_0000`)
  so a row stays two u32s and `user`/`group` answer both. `vfs/remote.rs`, `listing/mod.rs`;
  `listing::tests::remote_owner_and_group_names_survive_the_round_trip`.
- **A selection of one kind showed no fan (2026-09-24).** The fan dealt one card per distinct
  kind, so three folders on the local side were one card — an icon, not a fan — while a remote
  selection of four kinds fanned. It now deals one card per item up to five, the distinct kinds
  first, with "+n" at its foot for the rest. `KindFan.qml`;
  `tst_InspectorSelection::test_one_kind_many_times_is_still_a_fan`.
- **Clicking a location that is already open re-opened both panes (2026-09-24).** The daemon
  reused the live session, but the panes lost their place and selection. Now a click on an open
  location (a pane anywhere inside it) only focuses the remote pane; a pane outside it is
  brought there. `Shell.openLocation`;
  `tst_SideBySideFocus::test_clicking_an_open_location_only_focuses_its_pane`.
- **Open: a remote file listed as a folder at a daemon start (2026-09-24).** The new wire log
  showed `find '…/images/greyhorse-dark.png' -mindepth 1` on opening the sftp location — a
  client had sent `Open` for a file's URI, before the folder's. Seen twice, both with a shell
  connecting as the daemon came up; not reproduced in six replays (fresh shell, shell started
  before the daemon, location opened while disconnected, the single-pane `open` IPC). The
  daemon now has an opt-in request trace (`KIKI_TRACE=1`, stderr: client, kind, body) so the
  next occurrence names the request and the client. `server.rs`.
- **Arriving at a server in one pane listed it twice (2026-09-24).** The pane listed the
  server, then the arrival handler opened the same folder again in the right pane and the local
  folder over the first — a wasted listing, and the selection and history just made were lost.
  The pane objects now change places, as they do on leaving side by side, so the listing is
  kept. `Shell.qml`;
  `tst_SideBySideFocus::test_arriving_at_a_server_keeps_its_listing_and_opens_the_local_folder_beside_it`.
- **Double work found with the request trace (2026-09-24).** `KIKI_TRACE=1` over the common
  flows showed: `Preview` and `GitStatus` asked three or four times per selection — the docked
  panel, the card and the columns' info column each follow the selection and each asked, seen or
  not (now an inspector asks only while it can be seen, and asks when it becomes seen with a
  selection it has not asked about; `Inspector._loadedFor`); every `SetViewPref` answered by a
  `ViewPrefsChanged` that made the writer fetch all the prefs again (the writer skips the one it
  wrote; `Settings._written`); an unchanged pref written anyway (skipped); the same `Filter`
  sent twice, by the bar's debounce and the direct call (`applyFilter` returns when it is already
  in force); `Repo` reloaded once per listing's `RepoChanged` — the other pane's, every column's
  (only the pane's own listing's counts). Covered by the existing suites (720 green). Left as
  they are, explained: the second `Window` after an `Open` is the refetch on the scan-end
  `Reset` (rows shown before the sort settles), and `Repo` on navigate plus once more when the
  listing reads the repo.
- **Settings pages cut off at half a window (2026-09-24).** Fixed-width fields and 560 px
  texts ran past a 400 px page: the Search boxes, the AI command field and its explanations,
  the Omarchy rows' file paths and their two buttons, the Share and About tables. A row's
  control is now sized to what is left beside the label (`Row2.slot`), texts take the page's
  width, and the two Omarchy buttons wrap. `SettingsWindow.qml`; `tst_SettingsWindow` (8).
- **The rail was cut off in a short window (2026-09-24).** Its sections are now in a vertical
  Flickable clipped where the bottom bar begins: what does not fit is under the edge and the
  wheel, or a drag, brings it up. `Sidebar.qml` (`sidebar-scroll`);
  `tst_Sidebar::test_a_short_rail_scrolls_to_what_is_below_the_edge`.
