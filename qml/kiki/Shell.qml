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
    function submitChmod(uri, mode, recursive) { Kiki.Daemon.request("Submit", { op: { op: "chmod", items: [uri], mode: mode, recursive: recursive } }) }

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
                status: (win.pane.filterText ? (win.pane.listing.count + " match") : (win.pane.listing.count + " items" + (win.pane.listing.done ? "" : " …"))) + (win.pane.selection.count() ? " · " + win.pane.selection.count() + " selected" : "")
            }
        }
    }

    Component { id: listView; Views.ListPane { pane: win.pane; onActivate: i => { win.pane.selection.set(i); win.openSelected() } } }
    Component { id: iconView; Views.IconPane { pane: win.pane; onActivate: i => { win.pane.selection.set(i); win.openSelected() } } }
    Component { id: columnsView; Views.ColumnsPane { pane: win.pane; home: win.home; onActivate: uri => win.openExternal(uri) } }
}
