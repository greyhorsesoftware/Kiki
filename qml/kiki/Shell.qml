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
    property bool inspector: Kiki.Settings.view.inspector
    property bool split: false
    property Kiki.Pane pane: Kiki.Pane { view: Kiki.Settings.view["default"]; focused: true }
    property string toast: ""
    // The inspected item follows the selection's current row.
    property string inspectedUri: ""
    property var inspectedRow: null
    Connections {
        target: win.pane.selection
        function onChanged() {
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
            { label: "Move to Trash", key: "Del", danger: true, sep: true, enabled: sel, action: () => win.trashSelection() },
        ]
        return items
    }

    function start(uri) { pane.open(uri || ("file://" + home)) }
    function loadSidebar() {
        Kiki.Daemon.request("Favorites", {}, ok => { if (ok) favorites = ok.items })
        Kiki.Daemon.request("Volumes", {}, ok => { if (ok) volumes = ok.items })
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

    Connections { target: Kiki.Daemon; function onReadyChanged() { if (Kiki.Daemon.ready) { win.loadSidebar(); if (!win.pane.uri) win.start(Quickshell.env("KIKI_START")) } } }
    Connections { target: Kiki.Daemon; function onEvent(msg) { if (msg.event === "FavoritesChanged" || msg.event === "VolumesChanged") win.loadSidebar() } }

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
            case Qt.Key_Return: case Qt.Key_Enter: if (alt) return; win.openSelected(); break
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
        function state(): string {
            return JSON.stringify({ uri: win.pane.uri, view: win.pane.view, count: win.pane.listing.count, done: win.pane.listing.done, selection: win.selectedUris(), inspector: win.inspector, split: win.split, filter: win.pane.filterText, sort: [win.pane.sortRole, win.pane.sortOrder], toast: win.toast })
        }
        function undo(): void { Kiki.Jobs.undo() }
        function redo(): void { Kiki.Jobs.redo() }
        function activity(): string { return JSON.stringify(Kiki.Jobs.list) }
        function contextMenu(action: string): void { const it = win.contextItems(win.pane.selection.current).find(i => i.label === action); if (it && it.enabled !== false) it.action() }
        function windowState(pane: string): string { const l = win.pane.listing; return JSON.stringify({ count: l.count, viewport: [l.viewportFirst, l.viewportCount], held: Object.keys(l._rows).length }) }
        function timestamps(): string { return JSON.stringify({ now: Date.now() }) }
    }

    property alias toolbar: toolbar

    Row {
        anchors.fill: parent
        UI.Sidebar {
            id: sidebar
            height: parent.height
            favorites: win.favorites; volumes: win.volumes; currentUri: win.pane.uri
            onOpen: uri => win.pane.open(uri)
        }
        Column {
            width: parent.width - sidebar.width; height: parent.height
            UI.Toolbar {
                id: toolbar
                width: parent.width
                pane: win.pane; home: win.home
                inspector: win.inspector; split: win.split
                onToggleInspector: win.inspector = !win.inspector
                onToggleSplit: win.split = !win.split
            }
            Row {
                width: parent.width; height: parent.height - toolbar.height - bar.height
                Loader {
                    id: viewLoader
                    width: parent.width - (inspectorPanel.visible ? inspectorPanel.width : 0); height: parent.height
                    sourceComponent: win.pane.view === "icon" ? iconView : (win.pane.view === "columns" ? columnsView : listView)
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
                keys: win.pane.view === "columns"
                    ? [{ key: "Enter", label: "open" }, { key: "h l", label: "columns" }, { key: "F2", label: "rename" }, { key: "Del", label: "trash" }, { key: "^C", label: "copy" }, { key: "^V", label: "paste" }, { key: "^Z", label: "undo" }, { key: "?", label: "all keys" }]
                    : [{ key: "Enter", label: "open" }, { key: "F2", label: "rename" }, { key: "Del", label: "trash" }, { key: "^C", label: "copy" }, { key: "^V", label: "paste" }, { key: "/", label: "search" }, { key: "^Z", label: "undo" }, { key: "?", label: "all keys" }]
                MouseArea { anchors.right: parent.right; width: 200; height: parent.height; onClicked: activity.toggle() }
                status: (Kiki.Jobs.running().length ? Kiki.Jobs.running().length + " running · " : "") + (win.pane.filterText ? (win.pane.listing.count + " match") : (win.pane.listing.count + " items" + (win.pane.listing.done ? "" : " …"))) + (win.pane.selection.count() ? " · " + win.pane.selection.count() + " selected" : "")
            }
        }
    }

    Component { id: listView; Views.ListPane { pane: win.pane; onActivate: i => { win.pane.selection.set(i); win.openSelected() }; onContextMenu: (i, pos) => menu.open(win.contextItems(i), Qt.point(pos.x + 224, pos.y + 48)) } }
    Component { id: iconView; Views.IconPane { pane: win.pane; onActivate: i => { win.pane.selection.set(i); win.openSelected() }; onContextMenu: (i, pos) => menu.open(win.contextItems(i), Qt.point(pos.x + 224, pos.y + 48)) } }

    UI.ContextMenu { id: menu; parent: win.contentItem }
    UI.Toast { anchors.horizontalCenter: parent.horizontalCenter; anchors.bottom: parent.bottom; anchors.bottomMargin: 44 }
    UI.CollisionPrompt { anchors.fill: parent }
    UI.CompressDialog { id: compressDialog; anchors.fill: parent; onSubmit: (archive, format) => Kiki.Jobs.submit({ op: "compress", items: items, archive: archive, format: format }) }
    UI.ActivityPopover { id: activity; anchors.right: parent.right; anchors.bottom: parent.bottom; anchors.bottomMargin: Kiki.Theme.barHeight + 4; anchors.rightMargin: 8 }
    Component { id: columnsView; Views.ColumnsPane { pane: win.pane; home: win.home; onActivate: uri => win.openExternal(uri) } }
}
