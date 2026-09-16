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
    property var devices: []
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
        if (r && r.isDir) { enterProject(uris[0]); return }
        editAt(uris[0], 1)
    }
    function editAt(uri, line) { Kiki.Daemon.request("OpenIn", { role: "editor", uris: [uri], line: line }, (ok, err) => { if (err) Kiki.Jobs.showToast({ text: err.message, undoable: false }) }) }
    // Project mode (plan 16): kiki becomes the tree; editor and agent are arranged beside it.
    property bool projectMode: false
    property string projectRoot: ""
    property int savedWidth: 1200
    function enterProject(uri) {
        projectRoot = uri; projectMode = true; savedWidth = win.width
        win.width = Kiki.Settings.project.width || 320
        const spawned = []
        Kiki.Daemon.request("OpenIn", { role: "editor", uris: [uri] }, (ok, err) => {
            if (ok) spawned.push({ role: "editor", class: "kiki-tool", pid: ok.pid })
            if (Kiki.Settings.project.agent) Kiki.Daemon.request("OpenIn", { role: "agent", uris: [uri] }, (ok2, err2) => { if (ok2) spawned.push({ role: "agent", class: "kiki-tool", pid: ok2.pid }); arrangeProject(spawned) })
            else arrangeProject(spawned)
        })
    }
    function arrangeProject(spawned) {
        if (!Kiki.Settings.project.arrange) return
        const windows = [{ role: "kiki", class: "kiki", pid: 0 }].concat(spawned)
        Kiki.Daemon.request("Arrange", { layout: "project", root: projectRoot, windows: windows, leftWidth: Kiki.Settings.project.width || 320 }, (ok, err) => { if (ok && ok.missing.length) Kiki.Jobs.showToast({ text: "Could not place: " + ok.missing.join(", "), undoable: false }) })
    }
    function leaveProject() { projectMode = false; win.width = savedWidth; pane.open(projectRoot) }

    // Share (plan 18)
    property var sharePlugins: []
    function loadShare() { Kiki.Daemon.request("SharePlugins", {}, ok => { if (ok) sharePlugins = ok.plugins.filter(p => p.enabled !== false) }) }
    function shareMenu() {
        const uris = selectedUris(); if (!uris.length) return
        const items = sharePlugins.map(p => ({ label: p.name, action: () => { if (p.targets === "none") shareSheet.open(p, null, uris); else shareTargets(p, uris) } }))
        if (!items.length) items.push({ label: "No share plugins installed", enabled: false, action: () => {} })
        menu.open(items, Qt.point(toolbar.width - 200 + 224, 44))
    }
    function shareTargets(p, uris) {
        Kiki.Daemon.request("ShareTargets", { plugin: p.id }, (ok, err) => {
            if (err) { Kiki.Jobs.showToast({ text: err.message, undoable: false }); return }
            const items = ok.targets.map(t => ({ label: t.name + (t.online ? "" : "  (offline)") + (t.detail ? "  ·  " + t.detail : ""), enabled: t.online, action: () => shareSheet.open(p, t, uris) }))
            if (!items.length) items.push({ label: "Nothing found", enabled: false, action: () => {} })
            menu.open(items, Qt.point(toolbar.width - 200 + 224, 44))
        })
    }
    // AI (plan 19)
    property bool aiOpen: false
    property var aiStatus: ({ configured: false })
    function loadAi() { Kiki.Daemon.request("AiStatus", {}, ok => { if (ok) aiStatus = ok }) }
    function aiQuery() { const u = selectedUris(); if (!u.length) return; if (!aiStatus.configured) { settingsWin.open("ai"); return } aiOpen = true; inspector = false; aiPanel.openFor(u) }
    function aiCanned(q) { aiQuery(); if (aiOpen) aiPanel.ask(q) }
    // Git (plan 15): the branch chip for the focused pane
    property var repo: null
    function loadRepo() { if (!pane.uri.startsWith("file://")) { repo = null; return } Kiki.Daemon.request("Repo", { uri: pane.uri }, ok => { repo = ok || null }) }
    Connections { target: win.pane; function onNavigated(uri) { win.loadRepo(); win.searching = false } }
    Connections { target: Kiki.Daemon; function onEvent(msg) { if (msg.event === "RepoChanged") win.loadRepo(); if (msg.event === "OpenInChanged") win.loadOpenIn(); if (msg.event === "ShowChooser") portal.open(msg); if (msg.event === "ShowItems") win.showItems(msg) } }
    function showItems(msg) {
        const uris = msg.uris || []; if (!uris.length) return
        const first = uris[0]
        if (msg.folders) { win.pane.open(first); return }
        const parent = first.replace(/\/[^/]*$/, "") || first
        win.pane.open(parent)
        const name = decodeURIComponent(first.split("/").pop())
        Qt.callLater(() => { for (let i = 0; i < win.pane.listing.count; i++) { const r = win.pane.listing.row(i); if (r && r.name === name) { win.pane.selection.set(i); break } } })
        if (msg.properties) win.inspector = true
    }
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
    // View menu (plan 02): one toolbar button, the three views, then hidden files.
    function viewMenu() {
        const items = [
            { label: "Icon view", key: "Ctrl+1", checked: pane.view === "icon", action: () => pane.view = "icon" },
            { label: "List view", key: "Ctrl+2", checked: pane.view === "list", action: () => pane.view = "list" },
            { label: "Columns view", key: "Ctrl+3", checked: pane.view === "columns", action: () => pane.view = "columns" },
            { label: "Show hidden files", key: "Ctrl+H", sep: true, checked: pane.showHidden, action: () => pane.setHidden(!pane.showHidden) },
        ]
        menu.open(items, Qt.point(toolbar.x + toolbar.viewButton.x, 44))
    }
    // Open with… (plan 02/03): the daemon lists the desktop entries for the file's MIME type.
    function openWithMenu(pos) {
        const u = selectedUris(); if (u.length !== 1) return
        Kiki.Daemon.request("OpenWith", { uri: u[0] }, ok => {
            if (!ok) return
            const items = ok.apps.map(a => ({ label: a.name + (a.default ? "  ·  default" : ""), action: () => Kiki.Daemon.request("Launch", { app: a.id, uris: u }) }))
            if (!items.length) items.push({ label: "No application for " + ok.mime, enabled: false, action: () => {} })
            menu.open(items, pos || Qt.point(toolbar.width - 300 + 224, 44))
        })
    }
    // Trash view (plan 04): restore to the original path, delete for good, or empty everything.
    property var trashInfo: ({})
    function loadTrashInfo() { Kiki.Daemon.request("TrashInfo", {}, ok => { if (ok) { const m = {}; for (const it of ok.items) m[it.name] = it; win.trashInfo = m } }) }
    function trashNames() { return pane.selection.positions().map(p => { const r = pane.listing.row(p); return r ? r.name : null }).filter(n => n) }
    function restoreSelection() { const n = trashNames(); if (n.length) Kiki.Jobs.submit({ op: "restore", names: n }) }
    function deleteForever() { const u = selectedUris(); if (u.length) Kiki.Jobs.submit({ op: "delete", items: u }) }
    function emptyTrash() { Kiki.Jobs.submit({ op: "emptyTrash" }) }
    Connections { target: win.pane; function onNavigated(uri) { if (uri.startsWith("trash://")) win.loadTrashInfo() } }
    Connections { target: win.pane.listing; function onReset() { if (win.pane.isTrash) win.loadTrashInfo() } }

    function contextItems(index) {
        const r = index >= 0 ? pane.listing.row(index) : null
        const sel = pane.selection.count() > 0
        if (pane.isTrash) {
            const info = r && win.trashInfo[r.name]
            return [
                { label: info ? "Restore to " + Kiki.Format.display(info.path.replace(/\/[^/]*$/, "") || "/", win.home) : "Restore", key: "Enter", enabled: sel, action: () => win.restoreSelection() },
                { label: "Delete permanently", key: "Del", danger: true, enabled: sel, action: () => win.deleteForever() },
                { label: "Copy path", enabled: sel && !!info, action: () => Quickshell.execDetached(["wl-copy", info.path]) },
                { label: "Empty Trash", danger: true, sep: true, enabled: pane.listing.count > 0, action: () => win.emptyTrash() },
            ]
        }
        const items = [
            { label: "Open", key: "Enter", enabled: sel, action: () => win.openSelected() },
            { label: "Open with…", enabled: sel && pane.selection.count() === 1 && r && !r.isDir, action: () => win.openWithMenu() },
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
            { label: "Share…", key: "Alt+S", enabled: sel && win.sharePlugins.length > 0, action: () => win.shareMenu() },
            { label: win.aiStatus.configured ? "Jarvis: Query…" : "Jarvis: Set up…", key: "Alt+Q", enabled: sel && r && (r.kind === "code" || r.kind === "text" || r.kind === "document" || r.kind === "pdf" || r.isDir), action: () => win.aiQuery() },
            { label: "Jarvis: Summarise", enabled: sel && win.aiStatus.configured && r && !r.isDir, action: () => win.aiCanned("Summarise this file in a few sentences.") },
            { label: "Jarvis: Explain this file", enabled: sel && win.aiStatus.configured && r && !r.isDir, action: () => win.aiCanned("Explain what this file does and how it is structured.") },
            { label: "Move to Trash", key: "Del", danger: true, sep: true, enabled: sel, action: () => win.trashSelection() },
        ]
        return items
    }

    function start(uri) { left.open(uri || ("file://" + home)) }
    function loadSidebar() {
        Kiki.Daemon.request("Favorites", {}, ok => { if (ok) favorites = ok.items })
        Kiki.Daemon.request("Volumes", {}, ok => { if (ok) volumes = ok.items })
        Kiki.Daemon.request("Locations", {}, ok => { if (ok) locations = ok.locations })
        Kiki.Daemon.request("Devices", {}, ok => { if (ok) devices = ok.devices })
    }
    function selectedUris() { return pane.selection.positions().map(p => { const r = pane.listing.row(p); return r ? pane.childUri(r.name) : null }).filter(u => u) }
    function openSelected() {
        const p = pane.selection.current; const r = p >= 0 ? pane.listing.row(p) : null
        if (!r) return
        if (pane.isTrash) { win.restoreSelection(); return }
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
    function selectAt(index, extend) {
        const n = pane.listing.count; if (!n) return
        const i = Math.max(0, Math.min(n - 1, index))
        if (extend) pane.selection.range(i); else pane.selection.set(i)
        viewLoader.item && viewLoader.item.ensureVisible && viewLoader.item.ensureVisible(i)
    }
    // Up/Down step one row: in the icon grid that is one row of tiles, elsewhere one entry.
    readonly property int rowStep: viewLoader.item && viewLoader.item.perRow ? viewLoader.item.perRow : 1
    readonly property int pageStep: viewLoader.item && viewLoader.item.pageSize ? viewLoader.item.pageSize : 20

    Connections { target: Kiki.Daemon; function onReadyChanged() { if (Kiki.Daemon.ready) { win.loadSidebar(); win.loadOpenIn(); win.loadShare(); win.loadAi(); if (!win.pane.uri) win.start(Quickshell.env("KIKI_START")) } } }
    Connections { target: Kiki.Daemon; function onEvent(msg) { if (msg.event === "FavoritesChanged" || msg.event === "VolumesChanged" || msg.event === "LocationsChanged" || msg.event === "DeviceAdded" || msg.event === "DeviceRemoved") win.loadSidebar(); if (msg.event === "DeviceRemoved" && win.pane.uri.startsWith(msg.uri.replace(/\/$/, ""))) win.pane.open("file://" + win.home) } }

    // Keymap (plan 02). Every action here is also reachable over IPC.
    Item {
        id: keys
        anchors.fill: parent
        focus: !toolbar.search.active && !toolbar.breadcrumb.editing
        Keys.onPressed: event => {
            const ctrl = event.modifiers & Qt.ControlModifier, shift = event.modifiers & Qt.ShiftModifier, alt = event.modifiers & Qt.AltModifier
            switch (event.key) {
            case Qt.Key_Slash: toolbar.search.focus(); break
            case Qt.Key_F: if (ctrl) toolbar.search.focus(); else return; break
            case Qt.Key_L: if (ctrl) toolbar.breadcrumb.edit(); else if (pane.view === "columns") win.openSelected(); else return; break
            case Qt.Key_1: if (ctrl) pane.view = "icon"; else return; break
            case Qt.Key_2: if (ctrl) pane.view = "list"; else return; break
            case Qt.Key_3: if (ctrl) pane.view = "columns"; else return; break
            case Qt.Key_J: win.moveSelection(1, shift); break
            case Qt.Key_K: win.moveSelection(-1, shift); break
            case Qt.Key_Down: win.moveSelection(win.rowStep, shift); break
            case Qt.Key_Up: if (alt) pane.up(); else win.moveSelection(-win.rowStep, shift); break
            case Qt.Key_Home: win.selectAt(0, shift); break
            case Qt.Key_End: win.selectAt(pane.listing.count - 1, shift); break
            case Qt.Key_PageDown: win.moveSelection(win.pageStep, shift); break
            case Qt.Key_PageUp: win.moveSelection(-win.pageStep, shift); break
            case Qt.Key_H: if (ctrl) pane.setHidden(!pane.showHidden); else if (pane.view === "columns") pane.up(); else return; break
            case Qt.Key_Return: case Qt.Key_Enter: if (alt && shift) { win.openInMenu(); break } if (alt) { win.openIn(""); break } if (win.searching) { resultsView.activate(); break } win.openSelected(); break
            case Qt.Key_Backspace: pane.back(); break
            case Qt.Key_Left: if (alt) pane.back(); else if (pane.view === "icon") win.moveSelection(-1, shift); else if (pane.view === "columns") pane.up(); else return; break
            case Qt.Key_Right: if (alt) pane.forward(); else if (pane.view === "icon") win.moveSelection(1, shift); else if (pane.view === "columns") win.openSelected(); else return; break
            case Qt.Key_I: if (ctrl) win.inspector = !win.inspector; else return; break
            case Qt.Key_F5: pane.listing.refresh(); break
            case Qt.Key_E: if (ctrl) { const d = win.devices.find(d => pane.uri.startsWith(d.uri.replace(/\/$/, ""))); if (d) Kiki.Daemon.request("Eject", { uri: d.uri }) } else win.editSelected(); break
            case Qt.Key_F2: win.renameSelected(); break
            case Qt.Key_Delete: if (pane.isTrash) win.deleteForever(); else win.trashSelection(); break
            case Qt.Key_C: if (ctrl && shift) win.copyPath(); else if (ctrl) win.copySelection(false); else return; break
            case Qt.Key_X: if (ctrl) win.copySelection(true); else return; break
            case Qt.Key_V: if (ctrl) win.paste(); else return; break
            case Qt.Key_Z: if (ctrl && shift) Kiki.Jobs.redo(); else if (ctrl) Kiki.Jobs.undo(); else return; break
            case Qt.Key_N: if (ctrl && shift) win.newFolder(); else return; break
            case Qt.Key_Menu: menu.open(win.contextItems(pane.selection.current), Qt.point(400, 200)); break
            case Qt.Key_A: if (ctrl) { for (let i = 0; i < pane.listing.count; i++) pane.selection.rows[i] = true; pane.selection.changed() } else return; break
            case Qt.Key_Escape: pane.selection.clear(); break
            case Qt.Key_Tab: if (win.split) win.focusPane(win.otherPane()); else return; break
            case Qt.Key_F6: if (win.split) win.transfer(true); else return; break
            case Qt.Key_M: if (ctrl) win.toggleMirror(); else return; break
            case Qt.Key_Comma: if (ctrl) settingsWin.open("general"); else return; break
            case Qt.Key_Question: settingsWin.open("keys"); break
            case Qt.Key_P: if (ctrl && shift) { if (win.projectMode) win.leaveProject(); else { const u = win.selectedUris(); win.enterProject(u.length && win.pane.listing.row(win.pane.selection.current).isDir ? u[0] : win.pane.uri) } } else return; break
            case Qt.Key_Q: if (alt) win.aiQuery(); else return; break
            case Qt.Key_S: if (alt) win.shareMenu(); else if (ctrl && shift) win.split = !win.split; else return; break
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
        function project(action: string, uri: string): void { if (action === "enter") win.enterProject(uri || win.pane.uri); else win.leaveProject() }
        function projectState(): string { return JSON.stringify({ root: win.projectRoot, active: win.projectMode, width: win.width }) }
        function edit(uri: string, line: string): void { win.editAt(uri, parseInt(line) || 1) }
        function reveal(uri: string): void { if (win.projectMode) projectTree.reveal(uri); else { const p = uri.replace(/\/[^/]*$/, ""); win.pane.open(p); const name = decodeURIComponent(uri.split("/").pop()); Qt.callLater(() => { for (let i = 0; i < win.pane.listing.count; i++) { const r = win.pane.listing.row(i); if (r && r.name === name) { win.pane.selection.set(i); break } } }) } }
        function saved(uri: string): void { win.pane.listing.refresh() }
        function share(plugin: string, target: string): void { win.shareMenu() }
        function aiQuery(uri: string, question: string): void { win.pane.selection.clear(); win.aiOpen = true; aiPanel.openFor([uri]); if (question) aiPanel.ask(question) }
        function aiClose(): void { win.aiOpen = false }
        function settings(action: string, page: string): void { if (action === "open") settingsWin.open(page || "general"); else settingsWin.visible = false }
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

    Views.ProjectTree {
        id: projectTree
        visible: win.projectMode
        anchors.fill: parent
        rootUri: win.projectMode ? win.projectRoot : ""
        home: win.home; repo: win.repo
        onOpenFile: uri => win.editAt(uri, 1)
        onSendToAgent: uri => Kiki.Daemon.request("OpenIn", { role: "agent", uris: [uri] })
        onLeave: win.leaveProject()
    }
    Row {
        visible: !win.projectMode
        anchors.fill: parent
        UI.Sidebar {
            id: sidebar
            height: parent.height
            favorites: win.favorites; volumes: win.volumes; locations: win.locations; devices: win.devices; currentUri: win.pane.uri
            onEjectDevice: dev => Kiki.Daemon.request("Eject", { uri: dev.uri }, (ok, err) => { if (err) Kiki.Jobs.showToast({ text: "Eject failed: " + err.message, undoable: false }) })
            onDeviceMenu: dev => menu.open([
                { label: "Open", enabled: !dev.busy, action: () => win.pane.open(dev.uri) },
                { label: "Eject", key: "Ctrl+E", action: () => Kiki.Daemon.request("Eject", { uri: dev.uri }) },
                { label: dev.busy ? "In use by " + dev.busy : dev.kind.toUpperCase() + " · " + dev.vendor + " " + dev.model, enabled: false, sep: true, action: () => {} },
            ], Qt.point(40, 300))
            onOpen: uri => win.pane.open(uri)
            onAddLocation: locationDialog.open(null)
            onOpenLocation: loc => win.openLocation(loc)
            onDropOn: (uri, drop) => win.pane.dropInto(uri, drop)
            onAddFavorites: uris => {
                const add = uris.filter(u => u.startsWith("file://") && !win.favorites.some(f => f.uri === u)).map(u => ({ name: decodeURIComponent(u.replace(/\/+$/, "").split("/").pop()) || "/", uri: u }))
                if (add.length) Kiki.Daemon.request("SetFavorites", { items: win.favorites.concat(add) }, () => win.loadSidebar())
            }
            onMountVolume: vol => Kiki.Daemon.request("Mount", { device: vol.device }, (ok, err) => { if (ok) win.pane.open(ok.uri); else Kiki.Jobs.showToast({ text: "Mount failed: " + (err ? err.message : ""), undoable: false }) })
            onVolumeMenu: vol => menu.open([
                { label: vol.mounted === false ? "Mount" : "Open", action: () => vol.mounted === false ? Kiki.Daemon.request("Mount", { device: vol.device }, ok => { if (ok) win.pane.open(ok.uri) }) : win.pane.open(vol.uri) },
                { label: "Unmount", enabled: vol.mounted !== false && vol.uri !== "file:///", action: () => Kiki.Daemon.request("Unmount", { device: vol.device }, (ok, err) => { if (err) Kiki.Jobs.showToast({ text: "Unmount failed: " + err.message, undoable: false }) }) },
                { label: "Eject", enabled: !!vol.removable, action: () => Kiki.Daemon.request("Eject", { device: vol.device }, (ok, err) => { if (err) Kiki.Jobs.showToast({ text: "Eject failed: " + err.message, undoable: false }) }) },
            ], Qt.point(40, 200))
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
                onViewMenu: win.viewMenu()
                onSettings: settingsWin.open("general")
                onShare: win.shareMenu()
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
                    width: (win.split ? Math.floor((parent.width - 1) / 2) : parent.width) - (inspectorPanel.visible && !win.split ? inspectorPanel.width : 0) - (aiPanel.visible && !win.split ? aiPanel.width : 0); height: parent.height
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
                UI.AiPanel { id: aiPanel; visible: win.aiOpen; width: 420; height: parent.height; home: win.home; onClose: win.aiOpen = false }
                UI.Inspector {
                    id: inspectorPanel
                    // Columns view supplies its own inspector column; icon and list use the toggle.
                    visible: !win.aiOpen && win.pane.view !== "columns" && win.inspector && win.inspectedUri !== ""
                    width: Kiki.Theme.inspectorWidth; height: parent.height
                    uri: win.inspectedUri; row: win.inspectedRow; home: win.home
                    onOpen: win.openSelected()
                    onOpenWith: win.openWithMenu()
                    onEdit: (u, line) => win.editAt(u, line)
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
    UI.PortalDialog { id: portal; anchors.fill: parent; home: win.home; favorites: win.favorites; locations: win.locations }
    UI.SettingsWindow { id: settingsWin }
    UI.IntegrationDialog { id: integrationDialog; parent: win.contentItem }
    Connections { target: Kiki.Settings; function onLoadedChanged() { if (Kiki.Settings.loaded && Kiki.Settings.integration.asked === false) integrationDialog.open() } }
    UI.ShareSheet { id: shareSheet; anchors.fill: parent }
    UI.CompressDialog { id: compressDialog; anchors.fill: parent; onSubmit: (archive, format) => Kiki.Jobs.submit({ op: "compress", items: items, archive: archive, format: format }) }
    UI.ActivityPopover { id: activity; anchors.right: parent.right; anchors.bottom: parent.bottom; anchors.bottomMargin: Kiki.Theme.barHeight + 4; anchors.rightMargin: 8 }
    Component { id: columnsView; Views.ColumnsPane { pane: win.left; home: win.home; onActivate: uri => win.openExternal(uri); onEdit: (u, line) => win.editAt(u, line) } }
}
