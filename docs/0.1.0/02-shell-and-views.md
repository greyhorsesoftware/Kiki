# 02 — Shell and views

Builds on: `01-daemon-and-listing.md`.

Mockups: `Main.dc.html`, `IconView.dc.html`, `ListView.dc.html` in `docs/design/`.

## Goal

The kiki window: sidebar, toolbar, the three views, search, and the Omarchy theme. After this plan kiki is usable as a read-only browser.

## Design

**Quickshell usage**: the window, the IPC handler (`qs ipc` for scripts, Omarchy keybindings and tests), Hyprland hooks, `Process` for `xdg-open` and git, `FileView` for the theme directory and `DesktopEntries` for Open with… are all Quickshell. Data comes from kikid over `Quickshell.Io.Socket` through the plan-01 `WindowCache`: views are integer-count models whose delegates read rows from the cache, and the shell never parses more than one window at a time. Images, thumbnails and previews are file paths loaded by `Image { asynchronous: true }`. Views, rows, tiles, the toolbar controls and the sidebar sections are plain QtQuick components fed by properties, per the rules in `11-testing.md`.

**Window**: a Quickshell toplevel with no client decorations. Layout is the mockup: 224 px sidebar, 48 px toolbar, content, 28 px shortcut bar.

**Shortcut bar**: the bottom strip shows the keys that apply right now as key chips with labels, and the item count or transfer progress on the right. The set changes with context: browsing (open, rename, trash, copy, paste, search, undo), columns view (adds `h`/`l`), split mode (switch pane, transfer, mirror, unsplit), each mirror screen (preflight, toggle row, filter tab, save report, cancel run, concurrency). `?` is always last and opens the full cheat sheet (plan 10). The bar is a plain QtQuick component fed a list of `(key, label)` pairs by the shell, so each screen declares its own set.

**Sidebar**: two sections plus a transient third. **Favorites** lists Home, Desktop, Documents, Downloads, Pictures, Projects, Trash (`trash:///`, plan 04); folders dropped on the section header are added; stored in `~/.config/kiki/favorites.toml`. **Locations** lists local volumes (mounted ones from `/proc/self/mounts`, unmounted filesystems from `lsblk -J` shown dimmed; a click mounts through `udisksctl` and opens, the context menu has Unmount and Eject) and, from plan 06, remote locations; its `+` button is inert until plan 06. **Devices** (plan 17) appears while a phone or camera is plugged in, with an eject icon on hover. Every favorite, volume and device row is a drop target: files dropped there move within a scheme and copy across it (Ctrl forces copy), dropping on Trash trashes. A free-space bar sits at the bottom.

**View button**: one toolbar button shows the current view's icon and opens a menu: Icon view (`Ctrl+1`), List view (`Ctrl+2`), Columns view (`Ctrl+3`) with the active one checked, then **Show hidden files** (`Ctrl+H`), a per-pane toggle whose starting value is the General setting of the same name. Hidden means a leading dot; the daemon filters them out of the view like a name filter (`ShowHidden { lid, show }`, a `Reset` follows), so the string pool keeps them and toggling is instant.

**View memory per folder**: each folder remembers its own view mode, sort role and order in `~/.config/kiki/views.toml` (the most recent 1,000 folders, keyed by URI, so remote and trash listings work the same way; exact folder only, never inherited by subfolders). `Pane.open` restores the entry or keeps the current view; changing the view or sort writes one entry through `SetViewPref` and other windows pick it up from `ViewPrefsChanged`. Settings → General has the switch (on by default) and Forget all.

**List columns**: Name is fixed; Modified, Size, Kind and **Accessed** are optional (Settings → General → List columns). Accessed shows the last access time as a relative phrase ("5 min ago", "3 weeks ago") over a heat swatch in the accent colour that fades on a log scale from the last hour to a year, and sorts by `atime`. It is only as fresh as the filesystem keeps it: Linux's default `relatime` updates atime at most once a day unless the file changed, so minute accuracy needs `strictatime`; remote plugins that do not report access times show a dash.

**Drag and drop**: rows and tiles start a platform drag carrying `text/uri-list` (so drops into and from other Wayland apps work); folder rows, tiles, the pane background and the sidebar are `DropArea`s. The daemon's `Submit` does the work; nothing is moved by the shell itself.

**Open with…**: the daemon's `OpenWith { uri }` lists the desktop entries for the file's MIME type (from `mimeapps.list` defaults and `mimeinfo.cache`, removed associations honoured, `text/*` falling back to `text/plain` handlers) and `Launch { app, uris }` runs one with its `Exec` field codes expanded (`%f %F %u %U %i %c %k`, `Terminal=true` through `$TERMINAL -e`), detached from the daemon's session. The menu item and the inspector button both use it.

**Toolbar** (plan 14 adds an "Open in" split button after the view switcher; plan 18 adds a Share button beside it): back / forward (history per pane), breadcrumb (the pane URI rendered by `Uri::display`: `~ › Projects › kiki` locally, `homelab › srv › kiki` on a location; click a crumb to jump, `Ctrl+L` edits the full URI, so typing `sftp://homelab/srv` navigates there; plan 15 adds a branch chip at the right end inside a git repository), search box (`/`), view switcher (`Ctrl+1/2/3`), and two toggles that plans 07 and 03 wire up (split, inspector).

**Component inventory**: every area is its own QtQuick component with a props-in, signals-out contract, under `qml/`. Names match the mockup generator's functions so a mockup region maps to one file.

| Area | Components |
|---|---|
| Shell | `Shell` (the Quickshell window, IPC, theme), `Pane`, `PaneHeader`, `ShortcutBar`, `KeyChip`, `StatusArea` |
| Sidebar | `Sidebar`, `SidebarSection`, `SidebarItem`, `FreeSpaceBar` |
| Toolbar | `Toolbar`, `NavButtons`, `Breadcrumb`, `SearchBox`, `ViewSwitcher`, `ToggleButton` |
| Views | `IconView`, `IconTile`, `ListView`, `ListHeader`, `ListRow`, `ColumnsView`, `Column`, `ColumnRow` |
| Inspector (plan 03) | `Inspector`, `InspectorHeader`, `InspectorTabs`, `GeneralTab`, `PermissionsTab`, `PreviewBox`, `FieldRow`, `PermissionGrid` |
| Operations (plans 04, 05) | `ContextMenu`, `MenuItem`, `Toast`, `ActivityPopover`, `JobRow`, `CollisionPrompt`, `CompressDialog` |
| Locations (plan 06) | `LocationDialog`, `ProtocolTabs`, `FormField` (one per `Field` kind), `KeyringNote` |
| Mirror (plan 08) | `MirrorWorkspace`, `MirrorHeader`, `DirectionArrows`, `ConfigureScreen`, `PlanBox`, `ReviewScreen`, `ReviewTabs`, `ReviewRow`, `RunningScreen`, `RunRow` |
| Portal (plan 09) | `PortalDialog` reusing `Sidebar`, `Breadcrumb`, `SearchBox`, `ListRow` |
| Primitives | `Button`, `Checkbox`, `Select`, `TextField`, `ProgressBar`, `Badge`, `Icon` (inline SVG set from the mockups) |

Dialogs, the mirror workspace, the context menu and the inspector tabs sit behind `Loader`s and are instantiated on first use, so window start-up compiles only the shell, sidebar, toolbar and one view.

Rules: a component never reaches for a model it was not given; lists take a model and a delegate; every component has an `objectName` and a QtTest file (plan 11); primitives own the theme tokens so a restyle touches one directory.

**Panes**: a pane is a view of one URI with its own history, view mode, selection and `ListingModel`. This plan lays out one pane; plan 07 lays out two. Nothing in a pane knows which scheme it shows.

**Views** share one selection model and one sort order:
- **Icon**: `GridView`, 5–7 columns depending on width, 44 px icons, two-line labels. Images and video show thumbnails in place of the kind icon once plan 03 lands.
- **List**: `ListView` with Name, Modified, Size, Kind; click a header to sort, arrow shows direction.
- **Columns**: Miller columns, 220 px each. Selecting a folder pushes a column; selecting a file makes the next column the inspector (plan 03). Ancestor selections render grey, the active one blue. The column strip scrolls horizontally when it exceeds the width.

**Search**: filters the current listing by substring as you type; `Enter` on a result opens it; `Esc` clears; the scope is typed as a prefix (`everywhere:`, `homelab:`) that becomes a chip, or picked from the menu behind the chip, per `12-search.md`.

**Icons**: kiki ships its own. UI icons (toolbar, sidebar sections, view switcher, inspector tabs, key chips, mirror arrows) are the stroke SVG set from the mockups, exported from `docs/design/gen.py` into `qml/icons/` and recoloured by theme tokens. File-kind icons are twelve SVGs in the same style, one per plan-01 `Kind`, coloured per kind as in the mockups; they render from phase 1 on extension alone. A setting switches file-kind icons to the freedesktop icon theme Omarchy configures, resolved through Quickshell's icon lookup from the mime type, which fills in after phase 2 and shows the kind icon until then. Application icons in Open with… always come from the system theme via `DesktopEntries`. Thumbnails replace kind icons for images, video and PDFs once generated (plan 03).

**Theme**: read `~/.config/omarchy/current/theme/` and map to the tokens used in the mockups (background, surface, highlight, foreground, muted, accent, plus the file-kind colours). `current` is a symlink that Omarchy replaces, so watch its parent directory `~/.config/omarchy/` for the rename, then re-read; reload live.

**Models**: every view binds to a plan-01 `WindowCache` (one `Open` per pane, one per column in columns view) and shares a `Selection` object that holds selected row indices and their URIs. Sorting and search filtering are `Sort` and `Filter` requests; the daemon answers with a `Reset` and the cache refetches its window. The sidebar's Favorites list is a plain QML `ListModel` loaded from `favorites.toml` through the daemon's `Favorites` request; Locations come from the `Locations` request (plan 06).

**Settings**: `~/.config/kiki/settings.toml` holds view preferences (default view, sort, inspector and split state per location) and the timer intervals that `11-testing.md` shrinks in tests (toast dismissal, search debounce, mirror poll).

**IPC handler** (`qs ipc call kiki <fn>`), used by scripts, Omarchy keybindings and tests. Later plans add to this list where noted:
- navigation: `open(uri)`, `back()`, `forward()`, `setView(icon|list|columns)`, `search(text)`, `select(name)`, `selection()` (URIs), `uri(pane)`
- toggles: `inspector(on|off)`, `split(on|off)` (plan 07), `focusPane(left|right)` (plan 07)
- state: `state()` returns paths, selection, view, split, inspector, focused pane, open dialog, mirror screen and spec, toast text as one JSON object
- test hooks: `geometry(objectName)`, `snapshot(objectName)`, `timestamps()`, `windowState(pane)` (count, cached range, pending requests); every element a test touches has an `objectName`
- actions from later plans: `undo()`, `redo()`, `activity()` (plan 04), `mirror(open|close)` (plan 08)

**Keymap** (the single source for the cheat sheet in plan 10):

| Key | Action |
|---|---|
| `/`, `Ctrl+F` | search |
| `Ctrl+L` | edit path |
| `Ctrl+1` `Ctrl+2` `Ctrl+3` `Ctrl+4` | icon / list / columns / mirror |
| `Up` `Down` | move selection; in icon view by a row of tiles |
| `Left` `Right` | columns: pop/push; icon view: previous/next tile |
| letters | type-ahead to the next matching name (plan 23; with Vim keys on, `h j k l` move and `e` edits instead) |
| `F4` | edit (plan 13); folder: project mode (plan 16) |
| `Shift+Del` | delete permanently, after confirming (plan 23) |
| `Ctrl+B` | focus the sidebar: Up/Down/Enter, Esc back (plan 23) |
| `Home` `End`, `PgUp` `PgDn` | first, last, page (Shift extends the selection) |
| `Alt+Up` | parent folder |
| `Ctrl+H` | show hidden files (per pane) |
| `Ctrl+A`, `Esc` | select all, clear selection |
| `Ctrl+Shift+C` | copy path |
| `Enter` | open |
| `Backspace`, `Alt+Left` | back |
| `F5` | refresh (re-list, bypassing the cache) |
| `Ctrl+I` | inspector (plan 03) |
| `Super+C` `Super+X` `Super+V` | copy, cut, paste (plan 04); `Ctrl` works too, since Omarchy's universal clipboard binding rewrites the Super chord to it |
| `F2` | rename (plan 04) |
| `Del` | move to trash (plan 04) |
| `Ctrl+Z` `Ctrl+Shift+Z` | undo, redo (plan 04) |
| `Ctrl+4` | Mirror view: two panes, local and remote (plan 24) |
| `Ctrl+M` | mirror to the remote from Mirror view (plan 08) |
| `e` | file: edit in the chosen editor (plan 13); folder: project mode (plan 16) |
| `Ctrl+Shift+P` | toggle project mode on the current folder (plan 16) |
| `Alt+Enter`, `Alt+Shift+Enter` | open in the default tool, open the tool list (plan 14) |
| `Alt+S` | share the selection (plan 18) |
| `Alt+Q` | AI query on the selection (plan 19) |
| `Ctrl+,` | settings (plan 20) |
| `?` | keybinding cheat sheet (plan 10) |

## Verification

- Each of these states is reached by a recorded key script with no pointer input: icon view, list view sorted by size, columns three deep, inspector on in icon and list, search active with a match selected, path edited via `Ctrl+L`.
- Switching views keeps the selection.
- Changing the Omarchy theme recolours the window without restart.
- The window opens and paints `~/` in under 100 ms from launch on a warm daemon.
