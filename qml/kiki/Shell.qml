import QtQuick
import Quickshell
import Quickshell.Io
import "." as Kiki
import "ui" as UI
import "views" as Views

// The kiki window: sidebar, toolbar, one pane (two in plan 07), shortcut bar.
FloatingWindow {
    id: win
    title: "kiki"
    implicitWidth: 1200
    implicitHeight: 760
    color: Kiki.Theme.bg

    readonly property string home: Quickshell.env("HOME")
    property var favorites: []
    property var volumes: []
    property var locations: []
    property bool inspector: Kiki.Settings.view.inspector
    property bool split: false
    property Kiki.Pane left: Kiki.Pane { view: Kiki.Settings.view["default"]; focused: true }
    property Kiki.Pane right: Kiki.Pane { view: "list"; focused: false }
    property Kiki.Pane pane: left
    function focusPane(p) { left.focused = p === left; right.focused = p === right; pane = p }
    // Selecting a location opens it side by side: local_uri on the left, remote_uri on the right.
    function openLocation(loc) {
        if (loc.localUri) { left.open(loc.localUri) }
        right.open(loc.remoteUri)
        split = true
        focusPane(right)
    }
    function otherPane() { return pane === left ? right : left }
    function transfer(move) {
        const u = selectedUris(); if (!u.length || !split) return
        Kiki.Jobs.submit({ op: move ? "move" : "copy", items: u, dest: otherPane().uri })
    }
    onSplitChanged: if (!split) { focusPane(left); mirrorOpen = false }
    property bool mirrorOpen: false
    function toggleMirror() { if (!split) return; mirrorOpen = !mirrorOpen }
    property string toast: ""
    // Search (plan 12). Folder scope filters the listing; other scopes open a results view.
    property Kiki.WindowCache results: Kiki.WindowCache { padAhead: 100; padBehind: 50 }
    property bool searching: false
    property string searchScope: "folder"
    property string indexInfo: ""
    function runSearch(text, scope) {
        searchScope = scope
        if (scope === "folder") { searching = false; pane.setFilter(text); return }
        pane.setFilter("")
        if (!text) { searching = false; return }
        if (!results.lid) { results.lid = Kiki.Daemon.allocLid(); Kiki.Daemon.bind(results.lid, results) }
        const req = { lid: results.lid, scope: scope === "everywhere" ? "everywhere" : "location", query: text, mode: "substring" }
        if (scope !== "everywhere") { const l = locations.find(x => x.name === scope); if (l) req.uri = l.remoteUri }
        Kiki.Daemon.request("Search", req, (ok, err) => {
            if (err) { indexInfo = err.message; return }
            searching = true
            indexInfo = scope === "everywhere" ? "index " + Math.round(ok.indexAge / 60) + " min old" + (ok.capped ? " · capped" : "") : ""
        })
    }
    function closeSearch() { searching = false; toolbar.search.clear() }
    function scopeMenu() {
        const items = [{ label: "This folder", action: () => { toolbar.search.scope = "folder"; runSearch(toolbar.search.text, "folder") } }, { label: "Everywhere", action: () => { toolbar.search.scope = "everywhere"; runSearch(toolbar.search.text, "everywhere") } }]
        for (const l of locations) items.push({ label: l.name + "  ·  " + l.plugin, sep: items.length === 2, action: () => { toolbar.search.scope = l.name; runSearch(toolbar.search.text, l.name) } })
        menu.open(items, Qt.point(toolbar.x + toolbar.search.x + 224, 44))
    }
    // Open in (plan 14)
    property var openInTools: []
    function loadOpenIn() { Kiki.Daemon.request("OpenInList", {}, ok => { if (ok) openInTools = ok.tools.filter(t => t.enabled) }) }
    function openInDefaultTool() { const t = openInTools.find(x => x.role !== "editor") || openInTools[0]; return t ? t : null }
    function openIn(id) {
        const uris = selectedUris(); const target = uris.length ? uris : [pane.uri]
        const tool = id ? openInTools.find(t => t.id === id) : openInDefaultTool()
        if (!tool) return
        Kiki.Daemon.request("OpenIn", { id: tool.id, uris: target }, (ok, err) => { if (err) Kiki.Jobs.showToast({ text: err.message, undoable: false }) })
    }
    function openInMenu() { menu.open(openInTools.map(t => ({ label: t.name + (t.role ? "  ·  " + t.role : ""), action: () => win.openIn(t.id) })), Qt.point(toolbar.width - 300 + 224, 44)) }
    function editSelected() {
        const uris = selectedUris(); if (!uris.length) return
        const r = pane.listing.row(pane.selection.current)
        if (r && r.isDir) { Kiki.Jobs.showToast({ text: "Project mode is plan 16", undoable: false }); return }
        Kiki.Daemon.request("OpenIn", { role: "editor", uris: [uris[0]] }, (ok, err) => { if (err) Kiki.Jobs.showToast({ text: err.message, undoable: false }) })
    }
    // Git (plan 15): the branch chip for the focused pane
    property var repo: null
    function loadRepo() { if (!pane.uri.startsWith("file://")) { repo = null; return } Kiki.Daemon.request("Repo", { uri: pane.uri }, ok => { repo = ok || null }) }
    Connections { target: win.pane; function onNavigated(uri) { win.loadRepo(); win.searching = false } }
    Connections { target: Kiki.Daemon; function onEvent(msg) { if (msg.event === "RepoChanged") win.loadRepo(); if (msg.event === "OpenInChanged") win.loadOpenIn() } }
    // The inspected item follows the selection's current row.
    property string inspectedUri: ""
    property var inspectedRow: null
    Connections {
        target: win.pane.selection
        function onChanged() {
            if (!win.pane) return
            const p = win.pane.selection.current; const r = p >= 0 ? win.pane.listing.row(p) : null
            win.inspectedRow = r; win.inspectedUri = r ? win.pane.childUri(r.name) : ""
        }
    }
    function submitChmod(uri, mode, recursive) { Kiki.Jobs.submit({ op: "chmod", items: [uri], mode: mode, recursive: recursive }) }

    // Operations (plan 04). The clipboard holds URIs and whether it was a cut.
    property var clipboard: ({ uris: [], cut: false })
    function copySelection(cut) { const u = selectedUris(); if (u.length) clipboard = { uris: u, cut: !!cut } }
    function paste() {
        if (!clipboard.uris.length) return
        Kiki.Jobs.submit({ op: clipboard.cut ? "move" : "copy", items: clipboard.uris, dest: pane.uri })
        if (clipboard.cut) clipboard = { uris: [], cut: false }
    }
    function trashSelection() { const u = selectedUris(); if (u.length) Kiki.Jobs.submit({ op: "trash", items: u }) }
    function newFolder() {
        let name = "New folder", n = 2
        const names = new Set(); for (let i = 0; i < pane.listing.count; i++) { const r = pane.listing.row(i); if (r) names.add(r.name) }
        while (names.has(name)) name = "New folder " + n++
        Kiki.Jobs.submit({ op: "mkdir", uri: pane.childUri(name) }, ok => { if (ok) win.renameSoon = name })
    }
    property string renameSoon: ""
    function renameSelected() { if (pane.selection.current >= 0) { if (pane.view !== "list") pane.view = "list"; pane.renamingIndex = pane.selection.current } }
    Connections { target: win.pane; function onRenameRequested(uri, name) { Kiki.Jobs.submit({ op: "rename", uri: uri, name: name }) } }
    // After a new folder lands in the listing, select it and start renaming.
    Connections { target: win.pane.listing; function onReset() { if (!win.renameSoon) return; const name = win.renameSoon; win.renameSoon = ""; Qt.callLater(() => { for (let i = 0; i < win.pane.listing.count; i++) { const r = win.pane.listing.row(i); if (r && r.name === name) { win.pane.selection.set(i); win.renameSelected(); break } } }) } }
    function copyPath() {
        const u = selectedUris(); if (!u.length) return
        const text = u.map(x => x.startsWith("file://") ? decodeURIComponent(x.slice(7)) : x).join("\n")
        Quickshell.execDetached(["wl-copy", text])
    }
    function contextItems(index) {
        const r = index >= 0 ? pane.listing.row(index) : null
        const sel = pane.selection.count() > 0
        const items = [
            { label: "Open", key: "Enter", enabled: sel, action: () => win.openSelected() },
            { label: "Open with…", enabled: false, action: () => {} },
            { label: "Copy", key: "Ctrl+C", sep: true, enabled: sel, action: () => win.copySelection(false) },
            { label: "Cut", key: "Ctrl+X", enabled: sel, action: () => win.copySelection(true) },
            { label: "Paste", key: "Ctrl+V", enabled: win.clipboard.uris.length > 0, action: () => win.paste() },
            { label: "New folder", key: "Ctrl+Shift+N", sep: true, action: () => win.newFolder() },
            { label: "Rename", key: "F2", enabled: sel && pane.selection.count() === 1, action: () => win.renameSelected() },
            { label: "Compress…", enabled: sel, action: () => compressDialog.open(win.selectedUris(), pane.uri) },
            { label: "Extract here", enabled: r && r.kind === "archive", action: () => Kiki.Jobs.submit({ op: "extract", archive: pane.childUri(r.name), dest: pane.uri }) },
            { label: "Extract to…", enabled: r && r.kind === "archive", action: () => { const folder = r.name.replace(/\.(tar\.(gz|xz|zst|bz2)|tgz|txz|tzst|zip|7z|tar)$/i, ""); Kiki.Jobs.submit({ op: "mkdir", uri: pane.childUri(folder) }, ok => { if (ok) Kiki.Jobs.submit({ op: "extract", archive: pane.childUri(r.name), dest: pane.childUri(folder) }) }) } },
            { label: "Copy path", enabled: sel, action: () => win.copyPath() },
            { label: "Open in…", enabled: win.openInTools.length > 0, action: () => win.openInMenu() },
            { label: "Move to Trash", key: "Del", danger: true, sep: true, enabled: sel, action: () => win.trashSelection() },
        ]
        return items
    }

    function start(uri) { left.open(uri || ("file://" + home)) }
    function loadSidebar() {
        Kiki.Daemon.request("Favorites", {}, ok => { if (ok) favorites = ok.items })
        Kiki.Daemon.request("Volumes", {}, ok => { if (ok) volumes = ok.items })
        Kiki.Daemon.request("Locations", {}, ok => { if (ok) locations = ok.locations })
    }
    function selectedUris() { return pane.selection.positions().map(p => { const r = pane.listing.row(p); return r ? pane.childUri(r.name) : null }).filter(u => u) }
    function openSelected() {
        const p = pane.selection.current; const r = p >= 0 ? pane.listing.row(p) : null
        if (!r) return
        if (r.isDir) pane.open(pane.childUri(r.name))
        else openExternal(pane.childUri(r.name))
    }
    function openExternal(uri) { Quickshell.execDetached(["xdg-open", uri]) }
    function moveSelection(delta, extend) {
        const n = pane.listing.count; if (!n) return
        const cur = pane.selection.current < 0 ? (delta > 0 ? -1 : n) : pane.selection.current
        const next = Math.max(0, Math.min(n - 1, cur + delta))
        if (extend) pane.selection.range(next); else pane.selection.set(next)
        viewLoader.item && viewLoader.item.ensureVisible && viewLoader.item.ensureVisible(next)
    }

    Connections { target: Kiki.Daemon; function onReadyChanged() { if (Kiki.Daemon.ready) { win.loadSidebar(); win.loadOpenIn(); if (!win.pane.uri) win.start(Quickshell.env("KIKI_START")) } } }
    Connections { target: Kiki.Daemon; function onEvent(msg) { if (msg.event === "FavoritesChanged" || msg.event === "VolumesChanged" || msg.event === "LocationsChanged") win.loadSidebar() } }

    // Keymap (plan 02). Every action here is also reachable over IPC.
    Item {
        id: keys
        anchors.fill: parent
        focus: !toolbar.search.active && !toolbar.breadcrumb.editing
        Keys.onPressed: event => {
            const ctrl = event.modifiers & Qt.ControlModifier, shift = event.modifiers & Qt.ShiftModifier, alt = event.modifiers & Qt.AltModifier
            switch (event.key) {
            case Qt.Key_Slash: toolbar.search.focus(); break
            case Qt.Key_L: if (ctrl) toolbar.breadcrumb.edit(); else if (pane.view === "columns") win.openSelected(); else return; break
            case Qt.Key_1: if (ctrl) pane.view = "icon"; else return; break
            case Qt.Key_2: if (ctrl) pane.view = "list"; else return; break
            case Qt.Key_3: if (ctrl) pane.view = "columns"; else return; break
            case Qt.Key_J: case Qt.Key_Down: win.moveSelection(1, shift); break
            case Qt.Key_K: case Qt.Key_Up: win.moveSelection(-1, shift); break
            case Qt.Key_H: if (pane.view === "columns") pane.up(); else return; break
            case Qt.Key_Return: case Qt.Key_Enter: if (alt && shift) { win.openInMenu(); break } if (alt) { win.openIn(""); break } if (win.searching) { resultsView.activate(); break } win.openSelected(); break
            case Qt.Key_Backspace: pane.back(); break
            case Qt.Key_Left: if (alt) pane.back(); else return; break
            case Qt.Key_Right: if (alt) pane.forward(); else return; break
            case Qt.Key_I: if (ctrl) win.inspector = !win.inspector; else return; break
            case Qt.Key_S: if (ctrl && shift) win.split = !win.split; else return; break
            case Qt.Key_F5: pane.listing.refresh(); break
            case Qt.Key_F2: win.renameSelected(); break
            case Qt.Key_Delete: win.trashSelection(); break
            case Qt.Key_C: if (ctrl) win.copySelection(false); else return; break
            case Qt.Key_X: if (ctrl) win.copySelection(true); else return; break
            case Qt.Key_V: if (ctrl) win.paste(); else return; break
            case Qt.Key_Z: if (ctrl && shift) Kiki.Jobs.redo(); else if (ctrl) Kiki.Jobs.undo(); else return; break
            case Qt.Key_N: if (ctrl && shift) win.newFolder(); else return; break
            case Qt.Key_Menu: menu.open(win.contextItems(pane.selection.current), Qt.point(400, 200)); break
            case Qt.Key_A: if (ctrl) { for (let i = 0; i < pane.listing.count; i++) pane.selection.rows[i] = true; pane.selection.changed() } else return; break
            case Qt.Key_Escape: pane.selection.clear(); break
            case Qt.Key_Tab: if (win.split) win.focusPane(win.otherPane()); else return; break
            case Qt.Key_F5: pane.listing.refresh(); break
            case Qt.Key_F6: if (win.split) win.transfer(true); else return; break
            case Qt.Key_M: if (ctrl) win.toggleMirror(); else return; break
            case Qt.Key_E: win.editSelected(); break
            case Qt.Key_Q: if (alt) Kiki.Jobs.showToast({ text: "AI query is plan 19", undoable: false }); else return; break
            default: return
            }
            event.accepted = true
        }
    }

    IpcHandler {
        target: "shell"
        function open(uri: string): void { win.pane.open(uri) }
        function back(): void { win.pane.back() }
        function forward(): void { win.pane.forward() }
        function setView(v: string): void { win.pane.view = v }
        function search(text: string): void { win.toolbar.search.text = text; win.pane.setFilter(text) }
        function select(name: string): void { for (let i = 0; i < win.pane.listing.count; i++) { const r = win.pane.listing.row(i); if (r && r.name === name) { win.pane.selection.set(i); return } } }
        function selection(): string { return JSON.stringify(win.selectedUris()) }
        function uri(pane: string): string { return win.pane.uri }
        function inspector(on: string): void { win.inspector = on === "on" }
        function split(on: string): void { win.split = on === "on" }
        function mirror(on: string): void { win.mirrorOpen = on === "open" && win.split }
        function mirrorScreen(): string { return win.mirrorOpen ? mirrorWs.screen : "" }
        function focusPane(side: string): void { win.focusPane(side === "right" ? win.right : win.left) }
        function transfer(kind: string): void { win.transfer(kind === "move") }
        function openLocation(name: string): void { const l = win.locations.find(x => x.name === name); if (l) win.openLocation(l) }
        function state(): string {
            return JSON.stringify({ uri: win.pane.uri, view: win.pane.view, count: win.pane.listing.count, done: win.pane.listing.done, selection: win.selectedUris(), inspector: win.inspector, split: win.split, filter: win.pane.filterText, sort: [win.pane.sortRole, win.pane.sortOrder], toast: win.toast })
        }
        function undo(): void { Kiki.Jobs.undo() }
        function redo(): void { Kiki.Jobs.redo() }
        function activity(): string { return JSON.stringify(Kiki.Jobs.list) }
        function contextMenu(action: string): void { const it = win.contextItems(win.pane.selection.current).find(i => i.label === action); if (it && it.enabled !== false) it.action() }
        function addLocation(): void { locationDialog.open(null) }
        function windowState(pane: string): string { const l = win.pane.listing; return JSON.stringify({ count: l.count, viewport: [l.viewportFirst, l.viewportCount], held: Object.keys(l._rows).length }) }
        function timestamps(): string { return JSON.stringify({ now: Date.now() }) }
    }

    property alias toolbar: toolbar

    Row {
        anchors.fill: parent
        UI.Sidebar {
            id: sidebar
            height: parent.height
            favorites: win.favorites; volumes: win.volumes; locations: win.locations; currentUri: win.pane.uri
            onOpen: uri => win.pane.open(uri)
            onAddLocation: locationDialog.open(null)
            onOpenLocation: loc => win.openLocation(loc)
            onEditLocation: loc => menu.open([{ label: "Open", action: () => win.pane.open(loc.remoteUri) }, { label: "Edit…", action: () => locationDialog.open(loc) }, { label: "Disconnect", action: () => Kiki.Daemon.request("Disconnect", { name: loc.name }) }, { label: "Remove", danger: true, sep: true, action: () => Kiki.Daemon.request("RemoveLocation", { name: loc.name }, () => win.loadSidebar()) }], Qt.point(40, 200))
        }
        Column {
            width: parent.width - sidebar.width; height: parent.height
            UI.Toolbar {
                id: toolbar
                width: parent.width
                pane: win.pane; home: win.home
                inspector: win.inspector; split: win.split; mirror: win.mirrorOpen
                locations: win.locations
                repo: win.repo
                openInDefault: win.openInDefaultTool() ? win.openInDefaultTool().name : ""
                onSearch: (text, scope) => win.runSearch(text, scope)
                onScopeMenu: win.scopeMenu()
                onOpenIn: id => win.openIn(id)
                onOpenInMenu: win.openInMenu()
                onToggleInspector: win.inspector = !win.inspector
                onToggleSplit: win.split = !win.split
                onToggleMirror: win.toggleMirror()
            }
            Views.SearchResults {
                id: resultsView
                visible: win.searching
                width: parent.width; height: parent.height - toolbar.height - bar.height
                results: win.results; query: toolbar.search.text; home: win.home; indexInfo: win.indexInfo
                scopeLabel: win.searchScope === "everywhere" ? "Everywhere" : win.searchScope
                onOpen: uri => { win.closeSearch(); const r = uri.replace(/\/[^/]*$/, "") || uri; win.pane.open(uri.endsWith("/") ? uri : r); }
            }
            UI.MirrorWorkspace {
                id: mirrorWs
                visible: win.mirrorOpen
                width: parent.width; height: parent.height - toolbar.height - bar.height
                localUri: win.left.uri; remoteUri: win.right.uri; home: win.home
                onClosed: win.mirrorOpen = false
                onRelist: { win.left.listing.refresh(); win.right.listing.refresh() }
            }
            Row {
                visible: !win.mirrorOpen && !win.searching
                width: parent.width; height: parent.height - toolbar.height - bar.height
                // Left pane (the only pane when not split)
                Column {
                    width: (win.split ? Math.floor((parent.width - 1) / 2) : parent.width) - (inspectorPanel.visible && !win.split ? inspectorPanel.width : 0); height: parent.height
                    UI.PaneHeader { visible: win.split; width: parent.width; pane: win.left; home: win.home; onClicked: win.focusPane(win.left) }
                    Loader {
                        id: viewLoader
                        width: parent.width; height: parent.height - (win.split ? 34 : 0)
                        sourceComponent: win.left.view === "icon" ? iconView : (win.left.view === "columns" ? columnsView : listView)
                        onLoaded: item.pane = win.left
                    }
                }
                Rectangle { visible: win.split; width: 1; height: parent.height; color: Kiki.Theme.line }
                Column {
                    visible: win.split
                    width: win.split ? parent.width - Math.floor((parent.width - 1) / 2) - 1 : 0; height: parent.height
                    UI.PaneHeader { width: parent.width; pane: win.right; home: win.home; onClicked: win.focusPane(win.right) }
                    Loader {
                        id: rightLoader
                        active: win.split
                        width: parent.width; height: parent.height - 34
                        sourceComponent: win.right.view === "icon" ? iconView : (win.right.view === "columns" ? columnsView : listView)
                        onLoaded: item.pane = win.right
                    }
                }
                UI.Inspector {
                    id: inspectorPanel
                    // Columns view supplies its own inspector column; icon and list use the toggle.
                    visible: win.pane.view !== "columns" && win.inspector && win.inspectedUri !== ""
                    width: Kiki.Theme.inspectorWidth; height: parent.height
                    uri: win.inspectedUri; row: win.inspectedRow; home: win.home
                    onOpen: win.openSelected()
                    onChmod: (mode, recursive) => win.submitChmod(win.inspectedUri, mode, recursive)
                }
            }
            UI.ShortcutBar {
                id: bar
                width: parent.width
                keys: win.searching ? [{ key: "Enter", label: "open" }, { key: "↑ ↓", label: "move" }, { key: "Tab", label: "cycle scope" }, { key: "⌫", label: "remove scope" }, { key: "Esc", label: "close" }, { key: "?", label: "all keys" }] : win.split ? [{ key: "Tab", label: "switch pane" }, { key: "^C ^V", label: "transfer" }, { key: "F6", label: "move across" }, { key: "^M", label: "mirror" }, { key: "^⇧S", label: "unsplit" }, { key: "?", label: "all keys" }] : win.pane.view === "columns"
                    ? [{ key: "Enter", label: "open" }, { key: "h l", label: "columns" }, { key: "F2", label: "rename" }, { key: "Del", label: "trash" }, { key: "^C", label: "copy" }, { key: "^V", label: "paste" }, { key: "^Z", label: "undo" }, { key: "?", label: "all keys" }]
                    : [{ key: "Enter", label: "open" }, { key: "F2", label: "rename" }, { key: "Del", label: "trash" }, { key: "^C", label: "copy" }, { key: "^V", label: "paste" }, { key: "/", label: "search" }, { key: "^Z", label: "undo" }, { key: "?", label: "all keys" }]
                MouseArea { anchors.right: parent.right; width: 200; height: parent.height; onClicked: activity.toggle() }
                status: (Kiki.Jobs.running().length ? Kiki.Jobs.running().length + " running · " : "") + (win.pane.filterText ? (win.pane.listing.count + " match") : (win.pane.listing.count + " items" + (win.pane.listing.done ? "" : " …"))) + (win.pane.selection.count() ? " · " + win.pane.selection.count() + " selected" : "")
            }
        }
    }

    Component { id: listView; Views.ListPane { pane: win.left; onActivate: i => { win.focusPane(pane); pane.selection.set(i); win.openSelected() }; onContextMenu: (i, pos) => { win.focusPane(pane); menu.open(win.contextItems(i), Qt.point(pos.x + 224, pos.y + 48)) } } }
    Component { id: iconView; Views.IconPane { pane: win.left; onActivate: i => { win.focusPane(pane); pane.selection.set(i); win.openSelected() }; onContextMenu: (i, pos) => { win.focusPane(pane); menu.open(win.contextItems(i), Qt.point(pos.x + 224, pos.y + 48)) } } }

    UI.ContextMenu { id: menu; parent: win.contentItem }
    UI.Toast { anchors.horizontalCenter: parent.horizontalCenter; anchors.bottom: parent.bottom; anchors.bottomMargin: 44 }
    UI.CollisionPrompt { anchors.fill: parent }
    UI.LocationDialog { id: locationDialog; anchors.fill: parent; onSaved: win.loadSidebar() }
    UI.CompressDialog { id: compressDialog; anchors.fill: parent; onSubmit: (archive, format) => Kiki.Jobs.submit({ op: "compress", items: items, archive: archive, format: format }) }
    UI.ActivityPopover { id: activity; anchors.right: parent.right; anchors.bottom: parent.bottom; anchors.bottomMargin: Kiki.Theme.barHeight + 4; anchors.rightMargin: 8 }
    Component { id: columnsView; Views.ColumnsPane { pane: win.left; home: win.home; onActivate: uri => win.openExternal(uri) } }
}
