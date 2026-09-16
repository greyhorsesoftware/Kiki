# 07 — Split mode

Builds on: `02-shell-and-views.md`, `06-remote-locations.md`.

Mockup: `SplitView.dc.html`.

## Goal

Selecting a location opens it side by side: left pane at the location's `local_uri`, right pane at its `remote_uri`. Both are ordinary plan-02 panes; either can be navigated to any URI afterwards. Copy, move and drag between panes are transfers with progress and undo.

## Design

- **Panes**: two independent listing models with their own history and view mode; one focused pane at a time (blue header underline). The toolbar's breadcrumb and search follow the focused pane; the view switcher applies to the focused pane.
- **Entering**: clicking a remote location in the sidebar enters split mode with the panes at `local_path` and `remote_path`. The split toggle (`Ctrl+Shift+S`) collapses to the remote pane alone, and re-enters with the same pair. A local-only split (two local folders) is allowed via the toggle when no remote is selected.
- **Transfers**: Ctrl+C / Ctrl+V and drag between panes submit a copy or move job (plan 04) between the two backends. Progress shows in the status bar with a per-job bar and speed; the job list (Activity) shows all running jobs.
- **Undo**: a cross-pane copy's inverse deletes the transferred copies on the destination; a move's inverse moves back. Because remote deletes are not trashed, the toast for a remote-side inverse says so.
- **Badges**: the pane header badge and colour come from the location's plugin (`display_name`), so a new plugin shows correctly here without UI changes.
- **Mirror** (plan 08) lives here: its two roots are the two pane URIs, whatever their schemes.
- **IPC added**: `split(on|off)`, `focusPane(left|right)`, `transfer(copy|move)`.

## Verification

- Selecting a location opens both panes at their configured paths; `state()` shows both paths and both listings complete within 200 ms on a local test server.
- A cross-pane copy shows progress and Ctrl+Z removes the copy on the destination; a cross-pane move and its undo leave both trees as they started.
- Collapsing and re-expanding the split restores both pane paths and selections.
