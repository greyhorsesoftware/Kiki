import QtQuick
import Quickshell
import Quickshell.Io
import "." as Kiki
import "ui" as UI
import "views" as Views
import "viewmenu.js" as ViewMenu

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
    // Plan 03: list and icon views show the inspector only when asked for it (Get info,
    // Ctrl+I). Columns view has its own inspector column and ignores this.
    property bool inspectorRequested: false
    readonly property bool inspector: inspectorRequested && win.inspectedUri !== ""
    // Favorites panel: hidden by default (Settings → General), shown from the far-left toolbar
    // button or Ctrl+Shift+B.
    property bool sidebarShown: Kiki.Settings.view.sidebar === true
    property bool sidebarPeek: false
    // What the panel takes from the panes, pinned or peeked: the two must agree or the panes
    // shrink without moving over.
    readonly property bool sidebarRail: Kiki.Settings.view.sidebarStyle !== "traditional"
    readonly property int sidebarFull: Math.min(Kiki.Theme.sidebarWidth, Math.floor(win.width * 0.32))
    // The rail reserves only its own width; widening on hover floats over the files rather than
    // shoving them sideways every time the pointer passes.
    readonly property int sidebarSpace: sidebarPanel.visible ? (sidebarRail ? 44 : sidebarFull) : 0
    // Side by side is how many folders the WINDOW shows; `Pane.view` is only how one pane draws
    // its folder. (It used to be a view — `left.view === "mirror"` — which left the left pane
    // without a view of its own and wrote "mirror" into views.toml as the folder's preference.)
    property bool sideBySide: false
    readonly property bool split: sideBySide
    property Kiki.Pane left: Kiki.Pane { view: win.defaultView(); focused: true; rememberViews: !win.sideBySide }
    property Kiki.Pane right: Kiki.Pane { view: "list"; focused: false; rememberViews: !win.sideBySide }
    function defaultView() { const d = Kiki.Settings.view["default"]; return !d || d === "mirror" ? "list" : d }
    // `[view] default = "mirror"` in an old settings.toml still means: start side by side.
    Connections { target: Kiki.Settings; function onLoadedChanged() { if (Kiki.Settings.loaded && Kiki.Settings.view["default"] === "mirror") win.enterMirror() } }
    /// The pane put away when side by side was turned off from the right: it went to `right`,
    /// and goes back to the left when the layout returns, so both folders are where they were.
    property bool _swappedOnLeave: false
    function _swapPaneObjects() { const l = left; left = right; right = l }
    property Kiki.Pane pane: left
    property var lastLocation: null
    /// Side by side: how much of the width the left pane takes. A ratio, not a width, so resizing
    /// the window keeps the proportion. `sideRatioLive` is the drag in progress (0 = none); the
    /// setting is written once, when the drag ends — like `inspectorWidth`.
    property real sideRatioLive: 0
    readonly property real sideRatio: sideRatioLive > 0 ? sideRatioLive : (Kiki.Settings.view.sideBySideRatio > 0 ? Kiki.Settings.view.sideBySideRatio : 0.5)
    /// Neither side can be dragged shut: closing one is what the toolbar button is for.
    readonly property int sideMin: 280
    function leftPaneWidth(total) {
        const room = Math.max(0, total - 1)
        const min = Math.min(sideMin, Math.floor(room / 2))
        return Math.max(min, Math.min(room - min, Math.round(room * sideRatio)))
    }
    function dragDivider(total, leftWidth) {
        const room = Math.max(1, total - 1)
        const min = Math.min(sideMin, Math.floor(room / 2))
        sideRatioLive = Math.max(min, Math.min(room - min, leftWidth)) / room
    }
    function endDividerDrag() { if (sideRatioLive > 0) { const r = sideRatioLive; Kiki.Settings.set("view", "sideBySideRatio", Math.round(r * 1000) / 1000); sideRatioLive = 0 } }
    function resetDivider() { sideRatioLive = 0; Kiki.Settings.set("view", "sideBySideRatio", 0.5) }

    /// Side by side, "the local pane": the `file://` one — the
    /// left by default, the right after Swap — and the left when both (or neither) are local.
    readonly property var localPane: !split ? pane : (left.uri.indexOf("file://") === 0 || right.uri.indexOf("file://") !== 0 ? left : right)
    /// A path from the title bar, which only shows when there is one pane.
    function navigateFromTitle(uri) { pane.open(uri) }
    /// The breadcrumb that is on screen for the focused pane: the title bar's with one pane,
    /// that pane's own header side by side. Ctrl+L and the path menu go to it.
    function activeCrumb() { return !split ? toolbar.breadcrumb : (pane === right ? rightHeader.breadcrumb : leftHeader.breadcrumb) }
    function focusPane(p) { left.focused = p === left; right.focused = p === right; pane = p }
    // Selecting a location opens Mirror view: local_uri on the left, remote_uri on the right.
    function openLocation(loc) {
        lastLocation = loc
        _enterSplit()
        if (loc.localUri) left.open(loc.localUri)
        right.open(loc.remoteUri)
        focusPane(right)
    }
    /// A location that was added without being checked: the daemon will not connect until its
    /// server's key has been seen, and says so. Offer exactly that, rather than a bare error.
    function offerVerification(p) {
        if (!p || p.listing.error.indexOf("has not been verified yet") < 0 || locationDialog.visible) return
        const m = p.uri.match(/^([a-z0-9]+):\/\/([^/]+)/); if (!m) return
        const loc = locations.find(l => l.plugin === m[1] && l.name === decodeURIComponent(m[2]))
        if (loc) locationDialog.verify(loc)
    }
    Connections { target: win.left.listing; function onErrorChanged() { win.offerVerification(win.left) } }
    Connections { target: win.right.listing; function onErrorChanged() { win.offerVerification(win.right) } }

    /// Both panes start in List, and from here on neither reads nor writes view memory (their
    /// `rememberViews` follows `sideBySide`, which is set FIRST so the change to List is not
    /// itself remembered).
    function _enterSplit() {
        if (sideBySide) return
        if (_swappedOnLeave) { _swapPaneObjects(); _swappedOnLeave = false; pane = left.focused ? left : right }
        sideBySide = true
        left.view = "list"; right.view = "list"
    }
    // Entering Mirror view from a local folder: the right pane gets the last location, else home.
    function enterMirror() {
        if (sideBySide) return
        _enterSplit()
        if (!right.uri) right.open(lastLocation && lastLocation.remoteUri ? lastLocation.remoteUri : "file://" + home)
    }
    /// Back to one pane — the one you were in. The single layout draws `left`, so when the focus
    /// was on the right the two Pane objects change places: its folder, selection and history
    /// come across whole (copying the URI over would lose all three and re-list a remote folder
    /// for nothing), and the other folder waits in `right` for the layout to return.
    function leaveMirror() {
        if (!sideBySide) return
        const keepRight = pane === right
        if (keepRight) _swapPaneObjects()
        _swappedOnLeave = keepRight
        sideBySide = false
        focusPane(left)
        left.applyPref()          // the folder opens the way it is remembered, not the way it was split
    }
    /// The view menu and Ctrl+1/2/3 change the focused pane, side by side or not.
    function setView(v) { win.pane.view = v }
    function toggleMirrorView() { if (win.split) win.leaveMirror(); else win.enterMirror() }
    function swapPanes() { const l = left.uri, r = right.uri; if (!l || !r) return; left.open(r); right.open(l) }
    // A remote URI opened directly (breadcrumb, IPC, Show in folder) opens in Mirror view with its location's local path beside it.
    Connections { target: win.left; function onNavigated(uri) { const m = uri.match(/^([a-z]+):\/\/([^/]+)/); if (!m || m[1] === "file" || m[1] === "trash" || win.sideBySide || win.left.hasPref) return
        if (Kiki.Settings.view.smartView === false) return
        const loc = win.locations.find(l => l.plugin === m[1] && l.name === m[2]); if (!loc) return
        win.lastLocation = loc; win._enterSplit(); win.right.open(uri); if (loc.localUri) win.left.open(loc.localUri); win.focusPane(win.right) } }
    // Last mirror time per remote root, kept in settings so the bar can say "last mirrored 2 h ago".
    property var lastMirror: Kiki.Settings.mirror && Kiki.Settings.mirror.last ? Kiki.Settings.mirror.last : ({})
    function remoteUri() { return left.uri.startsWith("file://") ? right.uri : left.uri }
    function localUri() { return left.uri.startsWith("file://") ? left.uri : right.uri }
    function recordMirror() { const m = Object.assign({}, lastMirror); m[remoteUri()] = Date.now(); lastMirror = m; Kiki.Daemon.request("SetSettings", { patch: { mirror: { last: m } } }) }
    function mirrorOptions(pos) {
        menu.open([
            { label: "Upload · local → " + (remoteUri().match(/^[a-z]+:\/\/([^/]+)/) || [])[1], key: "Ctrl+M", action: () => win.startMirror(true) },
            { label: "Download · " + (remoteUri().match(/^[a-z]+:\/\/([^/]+)/) || [])[1] + " → local", action: () => win.startMirror(false) },
            { label: "Swap sides", sep: true, action: () => win.swapPanes() },
            { label: "Open remote alone", action: () => { const r = win.remoteUri(); win.left.view = "list"; win.left.open(r) } },
        ], pos)
    }
    function startMirror(upload) { if (!split) enterMirror(); mirrorWs.upload = upload; mirrorOpen = true }
    function otherPane() { return pane === left ? right : left }
    function transfer(move) { if (split) ops.transferTo(otherPane().uri, move) }
    onSplitChanged: if (!split) mirrorOpen = false
    property bool mirrorOpen: false
    function toggleMirror() { if (!split) { startMirror(true); return } mirrorOpen = !mirrorOpen }
    property string toast: ""
    // Search (plan 12). Folder scope filters the listing; other scopes open a results view.
    property Kiki.WindowCache results: Kiki.WindowCache { padAhead: 100; padBehind: 50 }
    // The field above the listing, opened by the toolbar's glass, "/" or Ctrl+F.
    property string searchScope: "everywhere"
    // Filtering (the strip above the listing) and global search (the overlay) are separate.
    property bool filterOpen: false
    property int filterTotal: 0
    function openFilter() {
        filterTotal = pane.listing.count
        filterOpen = true
        const bar = win.pane === win.right ? rightFilter : leftFilter
        bar.focusInput()
    }
    function closeFilter() { filterOpen = false; leftFilter.clear(); rightFilter.clear(); pane.setFilter(""); keys.forceActiveFocus() }
    function openSearch(seed) { searchOverlay.open(seed !== undefined ? seed : "") }
    function runSearch(text, scope) {
        searchScope = scope
        if (!text) return
        if (!results.lid) { results.lid = Kiki.Daemon.allocLid(); Kiki.Daemon.bind(results.lid, results) }
        const req = { lid: results.lid, scope: scope === "everywhere" ? "everywhere" : "location", query: text, mode: "substring" }
        if (scope !== "everywhere") { const l = locations.find(x => x.name === scope); if (l) req.uri = l.remoteUri }
        Kiki.Daemon.request("Search", req, (ok, err) => {
            if (err) { indexInfo = err.message; return }
            indexInfo = scope === "everywhere" ? "index " + Math.round(ok.indexAge / 60) + " min old" + (ok.capped ? " · capped" : "") : ""
        })
    }
    property string indexInfo: ""
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
        // Each tool's window is known by the class the daemon gave its terminal and by its pid;
        // one class for both used to make Arrange take the editor's window for the agent too. A
        // tool that will not start says so — the error was dropped, and `e` on a folder did nothing.
        const started = (role, ok, err) => {
            if (ok) spawned.push({ role: role, class: ok["class"] || "", pid: ok.pid })
            else Kiki.Jobs.showToast({ text: "Project mode: the " + role + " did not start — " + ((err && err.message) || "unknown error"), undoable: false })
        }
        Kiki.Daemon.request("OpenIn", { role: "editor", uris: [uri] }, (ok, err) => {
            started("editor", ok, err)
            if (Kiki.Settings.project.agent) Kiki.Daemon.request("OpenIn", { role: "agent", uris: [uri] }, (ok2, err2) => { started("agent", ok2, err2); arrangeProject(spawned) })
            else arrangeProject(spawned)
        })
    }
    function arrangeProject(spawned) {
        if (!Kiki.Settings.project.arrange) return
        // kiki's own window is found by this process's pid: its class is Quickshell's, not "kiki".
        const windows = [{ role: "kiki", class: "", pid: Quickshell.processId }].concat(spawned)
        Kiki.Daemon.request("Arrange", { layout: "project", root: projectRoot, windows: windows, leftWidth: Kiki.Settings.project.width || 320 }, (ok, err) => { if (ok && ok.missing.length) Kiki.Jobs.showToast({ text: "Could not place: " + ok.missing.join(", "), undoable: false }) })
    }
    function leaveProject() { projectMode = false; win.width = savedWidth; pane.open(projectRoot) }

    // Share (plan 18)
    property var sharePlugins: []
    function loadShare() { Kiki.Daemon.request("SharePlugins", {}, ok => { if (ok) sharePlugins = ok.plugins.filter(p => p.enabled !== false) }) }
    /// One entry per share plugin, with its own icon. A plugin that names no targets is sent to
    /// at once — Mail opens a composer with the files attached, and a form in front of that only
    /// asks for what the composer is about to ask for.
    function shareItems() {
        const uris = selectedUris()
        // One that cannot work here — its program is not installed — stays in the menu, dimmed,
        // saying what is missing: it tells the user the thing exists and what it would take.
        // Each way of sending is a row of the menu itself — "Send via LocalSend ▸" — rather than
        // all of them behind one "Share ▸": it is one level less to the device, and a plugin's
        // targets fit in the one submenu the menu has. They are asked for when the row is opened
        // (LocalSend looks for two seconds). One that cannot work here stays, dimmed, saying why.
        const items = sharePlugins.map(p => {
            const it = { label: "Send via " + p.name, icon: p.icon || "share", enabled: uris.length > 0 && !p.unavailable, key: p.unavailable ? "not installed" : "" }
            if (p.unavailable) return it
            if (p.targets === "none") { it.action = () => { if (uris.length) shareNow(p, null, uris) }; return it }
            it.items = [{ label: "Looking…", enabled: false, action: () => {} }]
            it.load = fill => shareTargetItems(p, uris, fill)
            return it
        })
        if (items.length) items[0].sep = true
        return items
    }
    function shareMenu() {
        if (!selectedUris().length) return
        const items = shareItems()
        menuUnder(toolbar.viewButton, items.length ? items : [{ label: "No share plugins installed", enabled: false, action: () => {} }])
    }
    /// Share without asking anything first. If the plugin needs more than the files — an SMTP
    /// account wants a recipient — it says so, and the sheet opens to collect it.
    function shareNow(p, target, uris) {
        Kiki.Daemon.request("Share", { plugin: p.id, uris: uris, target: target ? target.id : undefined, compose: {} }, (ok, err) => {
            if (err) shareSheet.open(p, target, uris)
        })
    }
    /// For scripts (`shell share <plugin>`): the targets as a menu of their own.
    function shareTargets(p, uris) { shareTargetItems(p, uris, items => menuUnder(toolbar.viewButton, items)) }
    /// A share plugin's targets as menu items: online ones first as the plugin sorted them,
    /// offline ones greyed, and the reason when there are none.
    function shareTargetItems(p, uris, fill) {
        Kiki.Daemon.request("ShareTargets", { plugin: p.id }, (ok, err) => {
            if (err) { fill([{ label: err.message, enabled: false, action: () => {} }]); return }
            const items = ok.targets.map(t => ({ label: t.name + (t.detail ? "  ·  " + t.detail : ""), key: t.online ? "" : "offline", icon: t.icon || p.icon || "share", enabled: t.online, action: () => shareSheet.open(p, t, uris) }))
            fill(items.length ? items : [{ label: "Nothing found", enabled: false, action: () => {} }])
        })
    }
    // AI (plan 19)
    /// "Open AI here…" and "Open Terminal here…": both lead OUT of kiki, to a terminal window
    /// started in `dir`. "Here" is the folder under the pointer when that is what was clicked,
    /// else the folder being shown; the AI — the one chosen in Settings, as its own command-line
    /// tool — is also told which `files` were selected. Local folders only.
    function hereItems(dir, files) {
        const local = dir.indexOf("file://") === 0
        return [
            { label: "Open AI here…", key: keymap.chordFor("ai"), sep: true, enabled: local, action: () => win.openAiHere(dir, files) },
            { label: "Open Terminal here…", enabled: local, action: () => win.openTerminalHere(dir) },
        ]
    }
    /// A failure says why (the tool is not installed, the files are remote) and, when no tool
    /// is set at all, goes to where one is chosen.
    function openAiHere(dir, files) {
        Kiki.Daemon.request("AiOpen", { dir: dir, uris: files || [] }, (ok, err) => {
            if (!err) return
            Kiki.Jobs.showToast({ text: err.message, undoable: false })
            if (err.message.indexOf("Settings") >= 0) settingsWin.open("ai")
        })
    }
    function openTerminalHere(dir) {
        Kiki.Daemon.request("OpenTerminal", { dir: dir }, (ok, err) => { if (err) Kiki.Jobs.showToast({ text: err.message, undoable: false }) })
    }
    // Git (plan 15): the branch chip for the focused pane
    property var repo: null
    function loadRepo() { if (!pane.uri.startsWith("file://")) { repo = null; return } Kiki.Daemon.request("Repo", { uri: pane.uri }, ok => { repo = ok || null }) }
    Connections { target: win.pane; function onNavigated(uri) { win.loadRepo(); if (win.filterOpen) win.closeFilter() } }
    Connections { target: Kiki.Daemon; function onEvent(msg) { if (msg.event === "RepoChanged") win.loadRepo(); if (msg.event === "OpenInChanged") win.loadOpenIn(); if (msg.event === "ShowChooser") portal.open(msg); if (msg.event === "ShowItems") win.showItems(msg) } }
    /// Bring the window to the front of the workspace it is on. Asked for by whatever opened
    /// something in it from outside — a second `kiki`, "Show in folder" from a browser — which
    /// is otherwise answered by a window nobody can see.
    function raise() { Quickshell.execDetached(["hyprctl", "dispatch", "focuswindow", "pid:" + Quickshell.processId]) }
    /// Open a folder, or a file's folder with the file selected, and come to the front: what a
    /// second launch of kiki does to the one already running.
    function present(uri) {
        if (uri) win.pane.open(uri)
        raise()
    }
    /// org.freedesktop.FileManager1: ShowFolders, ShowItems, ShowItemProperties.
    function showItems(msg) {
        const uris = msg.uris || []; if (!uris.length) return
        const first = uris[0]
        raise()
        if (msg.folders) { win.pane.open(first); return }
        const parent = first.replace(/\/[^/]*$/, "") || first
        win.pane.open(parent)
        // Selected once the folder has listed, and found by the daemon: the file may be far past
        // the rows the window holds. (Set after `open`, which decides this for itself.)
        win.pane.selectAfterLoad = decodeURIComponent(first.split("/").pop())
        win.selectCameFrom()
        if (msg.properties) win.inspectorRequested = true
    }
    // The inspected item follows the selection's current row.
    /// The info panel's width, dragged by its edge and remembered between sessions.
    property int inspectorW: Kiki.Settings.view.inspectorWidth || Kiki.Theme.inspectorWidth
    function setInspectorWidth(w, room) { inspectorW = Math.max(260, Math.min(Math.floor(room * 0.7), Math.round(w))) }
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
    function submitChmod(uri, mode, recursive) { ops.chmod(uri, mode, recursive) }

    // Operations (plan 04) live in Ops.qml, which knows nothing about windows and dialogs, so
    // the interaction tests can drive them; the window supplies the confirmation and wl-copy.
    Kiki.Ops {
        id: ops
        pane: win.pane
        onConfirmNeeded: (spec, reply) => confirm.ask(spec, reply)
        onFolderNeeded: (spec, reply) => portal.pick(
            { mode: "open", directory: true, title: spec.title, currentFolder: (spec.start || "").replace(/^file:\/\//, "") || win.home },
            uris => reply(uris && uris.length ? uris[0].replace(/\/+$/, "") : ""))
        onCopyText: text => Quickshell.execDetached(["wl-copy", text])
    }
    property alias clipboard: ops.clipboard
    function copySelection(cut) { ops.copySelection(cut) }
    function paste() { ops.paste() }
    function trashSelection() { if (win.galleryPane()) win.galleryPane().keepPlace(); ops.trashSelection() }
    function newFolder() { ops.newFolder() }
    function renameSelected() { ops.renameSelected() }
    function copyPath() { ops.copyPath() }
    // View menu (plan 02): one toolbar button, the three views, then hidden files.
    // A menu hung under the toolbar item that opened it, in window coordinates.
    function menuUnder(item, items) {
        const p = item.mapToItem(menu.parent, 0, item.height + 4)
        menu.open(items, Qt.point(p.x, p.y))
    }
    // Clicking the path offers the folders above this one, and the way into typing one.
    function pathMenu() {
        const crumb = activeCrumb()
        const items = crumb.ancestors().map(a => ({ label: a.label, action: () => win.pane.open(a.uri) }))
        items.push({ label: "Type a path…", key: "Ctrl+L", sep: items.length > 0, action: () => crumb.edit() })
        menuUnder(crumb, items)
    }
    /// The same menu for a pane's own header: the folders above, and typing a path.
    function paneHeaderPathMenu(target, crumb) {
        const items = crumb.ancestors().map(a => ({ label: a.label, action: () => target.open(a.uri) }))
        items.push({ label: "Type a path…", sep: items.length > 0, action: () => crumb.edit() })
        menuUnder(crumb, items)
    }
    /// The gear: settings, the keymap, and who made this.
    function gearMenu() {
        menuUnder(toolbar.gearButton, [
            { label: "Settings…", key: keymap.chordFor("settings"), action: () => settingsWin.open("general") },
            { label: "Keyboard shortcuts…", key: keymap.chordFor("shortcuts"), action: () => keysWin.open() },
            { label: "About kiki…", sep: true, action: () => aboutDlg.open() },
        ])
    }
    /// The rows and their ticks are `viewmenu.js`'s (tested there); what each does is here.
    /// Ticked for the FOCUSED pane's view, side by side as well — each pane has its own. (The
    /// ticks used to be withheld when split: a leftover from when Mirror was itself a view and
    /// none of these was the current one.)
    function viewMenuItems() {
        const act = { gallery: () => win.enterGallery(), hidden: () => pane.setHidden(!pane.showHidden) }
        return ViewMenu.items(pane.view, pane.showHidden).map(it => Object.assign(it, { action: act[it.id] || (() => win.setView(it.id)) }))
    }
    function viewMenu() {
        const items = viewMenuItems()
        menuUnder(toolbar.viewButton, items)
    }
    // Sidebar keyboard focus (plan 23): Ctrl+B, then Up/Down/Enter, Esc back to the pane.
    property bool sidebarFocus: false
    function focusSidebar(on) { sidebarFocus = on; if (on && sidebarPanel.keyIndex < 0) sidebarPanel.keyIndex = 0; if (!on) sidebarPanel.keyIndex = -1 }
    // Type-ahead (plan 23): letters jump to the next name starting with what was typed; the
    // prefix resets after 800 ms. Off when Vim keys are on, since h j k l e are bound then.
    readonly property bool vimKeys: Kiki.Settings.view.vimKeys === true
    property string typed: ""
    Timer { id: typedTimer; interval: 800; onTriggered: win.typed = "" }
    function typeAhead(ch) {
        typed += ch; typedTimer.restart()
        const after = typed.length > 1 ? null : (pane.selection.current >= 0 ? pane.selection.current : null)
        Kiki.Daemon.request("SeekName", { lid: pane.listing.lid, prefix: typed, after: after }, ok => { if (ok && ok.index !== null && ok.index !== undefined) win.selectAt(ok.index, false) })
    }
    // Open with (plans 02/03 and 14): one list holding the desktop entries for the file's MIME
    // type and kiki's own tools, so there is a single way to open something elsewhere.
    /// Open with is the desktop's applications. The terminal tools and agents of plan 14 are
    /// their own thing (`Alt+Enter`, the editor keys) and do not belong in a list of apps.
    function openWithItems() { return [{ label: "Looking…", enabled: false, action: () => {} }] }
    function loadOpenWith(uris, apply) {
        apply(win.openWithItems())
        if (uris.length !== 1) { apply([{ label: "Select one file", enabled: false, action: () => {} }]); return }
        Kiki.Daemon.request("OpenWith", { uri: uris[0] }, (ok, err) => {
            if (!ok) { apply([{ label: err ? err.message : "Nothing offered", enabled: false, action: () => {} }]); return }
            const apps = ok.apps.map(a => ({ label: a.name + (a.default ? "  ·  default" : ""), icon: "open",
                                             action: () => Kiki.Daemon.request("Launch", { app: a.id, uris: uris },
                                                 (r, e) => { if (e) Kiki.Jobs.showToast({ text: e.message, undoable: false }) }) }))
            apply(apps.length ? apps : [{ label: "No application for this kind", enabled: false, action: () => {} }])
        })
    }
    function openWithMenu(pos) {
        const u = selectedUris(); if (!u.length) return
        win.loadOpenWith(u, list => {
            const items = list.length ? list : [{ label: "Nothing to open it with", enabled: false, action: () => {} }]
            if (menu.visible) menu.items = items
            else if (pos) menu.open(items, pos)
            else menuUnder(toolbar.viewButton, items)
        })
    }
    // Trash view (plan 04): restore to the original path, delete for good, or empty everything.
    property var trashInfo: ({})
    function loadTrashInfo() { Kiki.Daemon.request("TrashInfo", {}, ok => { if (ok) { const m = {}; for (const it of ok.items) m[it.name] = it; win.trashInfo = m } }) }
    function trashNames() { return ops.selectedNames() }
    function restoreSelection() { ops.restoreSelection() }
    function deleteForever() { ops.deleteForever() }
    function emptyTrash() { ops.emptyTrash() }
    Connections { target: win.pane; function onNavigated(uri) { if (uri.startsWith("trash://")) win.loadTrashInfo() } }
    Connections { target: win.pane.listing; function onReset() { if (win.pane.isTrash) win.loadTrashInfo() } }

    property var openWithSub: []
    /// The menu for one row addressed by URI rather than by a position in the pane's listing:
    /// columns view shows several folders at once, so the row that was clicked need not be in
    /// the folder the pane is on. Same actions, aimed at that one file.
    function contextItemsForUri(uri, row) {
        const uris = [uri]
        win.openWithSub = win.openWithItems()
        win.loadOpenWith(uris, list => { win.openWithSub = list; if (menu.visible) menu.refill("Open with", list) })
        const folder = uri.replace(/\/[^/]*$/, "")
        return [
            { label: "Open", key: "Enter", action: () => row && row.isDir ? win.pane.open(uri) : win.openExternal(uri) },
            { label: "Open with", items: win.openWithSub },
            { label: "Get info", key: "Ctrl+I", action: () => { win.inspectedUri = uri; win.inspectedRow = row; win.inspectorRequested = true } },
            { label: "Copy", key: "Super+C", sep: true, action: () => ops.copySelection(false, uris) },
            { label: "Cut", key: "Super+X", action: () => ops.copySelection(true, uris) },
            { label: "Compress…", sep: true, action: () => compressDialog.open(uris, folder) },
            { label: "Extract here", enabled: !!row && row.kind === "archive", action: () => Kiki.Jobs.submit({ op: "extract", archive: uri, dest: folder }) },
            { label: "Copy path", action: () => ops.copyPath(uris) },
            ...win.hereItems(row && row.isDir ? uri : folder, row && row.isDir ? [] : uris),
            { label: "Move to Trash", key: "Del", danger: true, sep: true, action: () => ops.trashSelection(uris) },
        ]
    }
    /// The menu for the background of a folder — nothing under the pointer. The same list as a
    /// row's, so nothing moves about, with everything that needs a file already greyed out by
    /// `contextItems`; only the two actions that need somewhere to put things are re-aimed, since
    /// in columns view the folder clicked need not be the one the pane is on.
    function folderItems(folderUri) {
        pane.selection.clear()
        const items = win.contextItems(-1)
        if (folderUri && folderUri !== pane.uri) {
            for (const it of items) {
                if (it.label === "Paste") it.action = () => ops.paste(folderUri)
                else if (it.label === "New folder") it.action = () => ops.newFolder(folderUri)
            }
        }
        return items
    }
    function contextItems(index) {
        // A placeholder goes in at once so the submenu is never empty; the applications land a
        // moment later and replace it, even if the submenu is already showing.
        win.openWithSub = win.openWithItems()
        const chosen = win.selectedUris()
        if (chosen.length) win.loadOpenWith(chosen, list => { win.openWithSub = list; if (menu.visible) menu.refill("Open with", list) })
        const r = index >= 0 ? pane.listing.row(index) : null
        const sel = pane.selection.count() > 0
        if (pane.isTrash) {
            const info = r && win.trashInfo[r.name]
            return [
                { label: info ? "Restore to " + Kiki.Format.display(info.path.replace(/\/[^/]*$/, "") || "/", win.home) : "Restore", key: "Enter", enabled: sel, action: () => win.restoreSelection() },
                { label: "Copy path", enabled: sel && !!info, action: () => Quickshell.execDetached(["wl-copy", info.path]) },
                { label: "Empty Trash", danger: true, sep: true, enabled: pane.listing.count > 0, action: () => win.emptyTrash() },
            ]
        }
        const items = [
            { label: "Open", key: "Enter", enabled: sel, action: () => win.openSelected() },
            { label: "Open with", enabled: sel, items: win.openWithSub },
            { label: "Get info", key: "Ctrl+I", enabled: sel, action: () => win.inspectorRequested = true },
            { label: "Copy", key: "Super+C", sep: true, enabled: sel, action: () => win.copySelection(false) },
            { label: "Cut", key: "Super+X", enabled: sel, action: () => win.copySelection(true) },
            { label: "Paste", key: "Super+V", enabled: win.clipboard.uris.length > 0, action: () => win.paste() },
            { label: "New folder", key: "Ctrl+Shift+N", sep: true, action: () => win.newFolder() },
            { label: "Rename", key: "F2", enabled: sel && pane.selection.count() === 1, action: () => win.renameSelected() },
            { label: "Compress…", enabled: sel, action: () => compressDialog.open(win.selectedUris(), pane.uri) },
            { label: "Extract here", enabled: !!r && r.kind === "archive", action: () => ops.extractHere(r.name) },
            { label: "Extract to…", enabled: !!r && r.kind === "archive", action: () => ops.extractTo(r.name) },
            { label: "Copy path", enabled: sel, action: () => win.copyPath() },
            ...win.shareItems(),
            ...win.hereItems(r && r.isDir && pane.selection.count() === 1 ? pane.childUri(r.name) : pane.uri, win.selectedUris().filter(u => !(r && r.isDir && pane.selection.count() === 1))),
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
    function selectedUris() { return ops.selectedUris() }
    /// An item's rectangle in window coordinates, as the geometry queries report it.
    function rectOf(it) {
        const p = it.mapToItem(null, 0, 0)
        // `visible` is what QML says; `onscreen` is whether it is actually inside the window —
        // a row scrolled past the bottom of a list is still "visible" and still reports a
        // rectangle, and clicking there hits whatever is really at those coordinates.
        // Inside the window, and inside every ancestor that clips — a row scrolled out of a list
        // is clipped away by the list, not by the window.
        let inside = p.x >= 0 && p.y >= 0 && p.x + it.width <= win.width && p.y + it.height <= win.height
        for (let a = it.parent; a && inside; a = a.parent) {
            if (a.clip !== true) continue
            const q = a.mapToItem(null, 0, 0)
            inside = p.x + it.width > q.x && p.x < q.x + a.width && p.y + it.height > q.y && p.y < q.y + a.height
        }
        return JSON.stringify({ x: Math.round(p.x), y: Math.round(p.y), w: Math.round(it.width), h: Math.round(it.height),
                                cx: Math.round(p.x + it.width / 2), cy: Math.round(p.y + it.height / 2),
                                visible: it.visible === true, onscreen: it.visible === true && inside })
    }
    /// Depth-first search of the scene for an objectName; the IPC geometry query's legs.
    function findByName(item, name) {
        if (!item) return null
        if (item.objectName === name && item.visible !== false) return item
        const kids = item.children || []
        for (let i = 0; i < kids.length; i++) {
            const found = findByName(kids[i], name)
            if (found) return found
        }
        return null
    }
    /// Entering a folder from the keyboard puts the cursor on its first row.
    function openFolder(uri) { pane.open(uri); pane.selectFirstAfterLoad = true }
    function openSelected() {
        const p = pane.selection.current; const r = p >= 0 ? pane.listing.row(p) : null
        if (!r) return
        if (pane.isTrash) { win.restoreSelection(); return }
        if (r.isDir) pane.open(pane.childUri(r.name))
        else openExternal(pane.childUri(r.name))
    }
    function openExternal(uri) { Quickshell.execDetached(["xdg-open", uri]) }
    /// Right: step into the selected folder. A file has nothing to step into.
    function enterSelected() {
        const r = pane.listing.row(pane.selection.current)
        if (r && r.isDir) win.openFolder(pane.childUri(r.name))
        else if (r && (r.kind === "image" || r.kind === "video")) win.enterGallery()
    }
    /// Ask the daemon which row a name is on and go there. The listing is virtual, so a name
    /// outside the loaded window cannot be found by looking.
    function seekName(name) {
        Kiki.Daemon.request("SeekName", { lid: pane.listing.lid, prefix: name }, ok => {
            if (ok && ok.index !== null && ok.index !== undefined) win.selectAt(ok.index, false)
        })
    }
    /// Put the cursor back on the folder we just came out of, once its row exists.
    function selectCameFrom() {
        const name = pane.selectAfterLoad
        if (!name || !pane.listing.done || !pane.listing.lid) return
        pane.selectAfterLoad = ""
        Kiki.Daemon.request("SeekName", { lid: pane.listing.lid, prefix: name }, ok => {
            if (ok && ok.index !== null && ok.index !== undefined) win.selectAt(ok.index, false)
        })
    }
    /// Landing in a folder from the keyboard: start on the first row.
    function selectFirstIfAsked() {
        if (!pane.selectFirstAfterLoad || !pane.listing.done) return
        pane.selectFirstAfterLoad = false
        if (pane.listing.count > 0 && pane.selection.current < 0) win.selectAt(0, false)
    }
    Connections {
        target: win.pane ? win.pane.listing : null
        function onDoneChanged() { win.selectCameFrom(); win.selectFirstIfAsked() }
        function onReset() { win.selectCameFrom(); win.selectFirstIfAsked() }
    }
    Kiki.Keymap { id: keymap }
    /// Run a rebindable action by id. Returns false when the action is not available now, so the
    /// keypress can fall through to whatever the view makes of it.
    function runAction(id) {
        switch (id) {
        case "filter": case "filterAlt": win.openFilter(); return true
        case "search": win.openSearch(leftFilter.text); return true
        case "typePath": win.activeCrumb().edit(); return true
        case "addLocation": locationDialog.open(null); return true
        case "viewIcon": win.setView("icon"); return true
        case "viewList": win.setView("list"); return true
        case "viewColumns": win.setView("columns"); return true
        case "viewMirror": win.toggleMirrorView(); return true
        case "viewGallery": win.enterGallery(); return true
        case "hidden": pane.setHidden(!pane.showHidden); return true
        case "inspector": win.inspectorRequested = !win.inspectorRequested; return true
        case "refresh": pane.listing.refresh(); return true
        case "sidebar": win.sidebarShown = !win.sidebarShown; return true
        case "focusSidebar": win.focusSidebar(!win.sidebarFocus); return true
        case "settings": settingsWin.open("general"); return true
        case "shortcuts": keysWin.open(); return true
        case "copy": win.copySelection(false); return true
        case "cut": win.copySelection(true); return true
        case "paste": win.paste(); return true
        case "copyPath": win.copyPath(); return true
        case "newFolder": win.newFolder(); return true
        case "rename": win.renameSelected(); return true
        case "edit": win.editSelected(); return true
        case "trash": if (pane.isTrash) win.deleteForever(); else win.trashSelection(); return true
        case "deleteForever": win.deleteForever(); return true
        case "undo": Kiki.Jobs.undo(); return true
        case "redo": Kiki.Jobs.redo(); return true
        case "selectAll": for (let i = 0; i < pane.listing.count; i++) pane.selection.rows[i] = true; pane.selection.changed(); return true
        case "openWith": win.openWithMenu(); return true
        case "openDefault": win.openIn(""); return true
        case "share": win.shareMenu(); return true
        case "ai": win.openAiHere(win.pane.uri, win.selectedUris()); return true
        case "mirror": win.toggleMirror(); return true
        case "transfer": if (!win.split) return false; win.transfer(true); return true
        case "project": if (win.projectMode) win.leaveProject(); else { const u = win.selectedUris(); win.enterProject(u.length && win.pane.listing.row(win.pane.selection.current).isDir ? u[0] : win.pane.uri) } return true
        case "eject": { const d = win.devices.find(d => pane.uri.startsWith(d.uri.replace(/\/$/, ""))); if (!d) return false; Kiki.Daemon.request("Eject", { uri: d.uri }); return true }
        }
        return false
    }
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
    /// Jobs a person would call running: not the machinery (an undo's inverse ops, a preflight).
    property int runningShown: 0
    Connections { target: Kiki.Jobs; function onChanged() { win.runningShown = Kiki.Jobs.running().filter(j => !j.hidden).length } }
    /// What the bottom bar says about the folder. In the gallery one thing is on the stage at a
    /// time, so it is where you are among them — "7 of 31" — rather than how many are selected.
    function countText() {
        const n = pane.listing.count, more = pane.listing.done ? "" : " …"
        if (pane.filterText) return n + " match"
        if (pane.view === "gallery" && !win.split && pane.selection.current >= 0 && n > 0) return (pane.selection.current + 1) + " of " + n + more
        return n + " items" + more + (pane.selection.count() ? " · " + pane.selection.count() + " selected" : "")
    }
    /// The focused pane's view item when it is the gallery, else null.
    /// The view item the focused pane is showing, whichever side it is on.
    function currentView() { return (win.pane === win.right && rightLoader.item) ? rightLoader.item : viewLoader.item }
    function galleryPane() {
        const v = (win.pane === win.right && rightLoader.item) ? rightLoader.item : viewLoader.item
        return v && v.step ? v : null
    }
    property string galleryFrom: "icon"
    function enterGallery() { if (pane.view !== "gallery") { galleryFrom = pane.view; pane.view = "gallery" } }
    /// The focused pane's view item when it is the columns view, else null.
    function columnsPane() {
        const v = (win.pane === win.right && rightLoader.item) ? rightLoader.item : viewLoader.item
        return v && v.focusLeft ? v : null
    }
    // Up/Down step one row: in the icon grid that is one row of tiles, elsewhere one entry.
    readonly property int rowStep: viewLoader.item && viewLoader.item.perRow ? viewLoader.item.perRow : 1
    readonly property int pageStep: viewLoader.item && viewLoader.item.pageSize ? viewLoader.item.pageSize : 20

    Connections {
        target: Kiki.Daemon
        function onReadyChanged() { if (Kiki.Daemon.ready) { win.loadSidebar(); win.loadOpenIn(); win.loadShare(); if (!win.pane.uri) win.start(Quickshell.env("KIKI_START")) } }
        // A restarted daemon knows nothing of the listings this window had open: their ids died
        // with it, so every pane opens its folder again.
        function onReconnected() {
            for (const p of [win.left, win.right]) if (p && p.uri) p.listing.open(p.uri)
            win.loadTrashInfo()
        }
    }
    Connections { target: Kiki.Daemon; function onEvent(msg) { if (msg.event === "FavoritesChanged" || msg.event === "VolumesChanged" || msg.event === "LocationsChanged" || msg.event === "DeviceAdded" || msg.event === "DeviceRemoved") win.loadSidebar(); if (msg.event === "DeviceRemoved" && win.pane.uri.startsWith(msg.uri.replace(/\/$/, ""))) win.pane.open("file://" + win.home) } }

    // Keymap (plan 02). Every action here is also reachable over IPC.
    Item {
        id: keys
        anchors.fill: parent
        focus: !win.filterOpen && !searchOverlay.visible && !shortcuts_.visible && !toolbar.breadcrumb.editing && !leftHeader.breadcrumb.editing && !rightHeader.breadcrumb.editing && !menu.visible && !settingsWin.visible
            && !locationDialog.visible && !portal.visible && !confirm.visible && !integrationDialog.visible
            && !shareSheet.visible && !compressDialog.visible && !win.projectMode
            && !keysWin.visible && !aboutDlg.visible && !jobLog.visible
            && win.pane.renamingIndex < 0
        // No re-grab here on purpose: taking the focus back whenever this item loses it races
        // every legitimate hand-over — the inline editor opens, the focus moves, the grab pulls
        // it straight back and the editor closes again. The binding above is the whole rule.
        Keys.onPressed: event => {
            const ctrl = event.modifiers & Qt.ControlModifier, shift = event.modifiers & Qt.ShiftModifier, alt = event.modifiers & Qt.AltModifier
            // The rebindable shortcuts come first, from the table the shortcuts window edits.
            // Whatever is left is contextual — arrows, Enter, type-ahead — and belongs to the view.
            const action = keymap.idFor(event.key, event.modifiers, event.text)
            if (action && win.runAction(action)) { event.accepted = true; return }
            switch (event.key) {
            case Qt.Key_L: if (win.vimKeys && win.columnsPane()) win.columnsPane().focusRight(); else return; break
            case Qt.Key_1: if (win.galleryPane()) win.galleryPane().actual(); else return; break
            // Bare keys are free in the gallery: type-ahead is off there.
            case Qt.Key_0: if (win.galleryPane()) win.galleryPane().fit(); else return; break
            case Qt.Key_Plus: case Qt.Key_Equal: if (win.galleryPane()) win.galleryPane().zoomBy(1.25); else return; break
            case Qt.Key_Minus: if (win.galleryPane()) win.galleryPane().zoomBy(0.8); else return; break
            case Qt.Key_Space: if (win.galleryPane()) win.galleryPane().step(1); else return; break
            case Qt.Key_J: if (win.vimKeys && win.columnsPane()) win.columnsPane().moveKey(1); else if (win.vimKeys) win.moveSelection(1, shift); else return; break
            case Qt.Key_K: if (win.vimKeys && win.columnsPane()) win.columnsPane().moveKey(-1); else if (win.vimKeys) win.moveSelection(-1, shift); else return; break
            case Qt.Key_Down: if (win.sidebarFocus) sidebarPanel.moveKey(1); else if (win.galleryPane()) win.galleryPane().step(1); else if (win.columnsPane()) win.columnsPane().moveKey(1); else win.moveSelection(win.rowStep, shift); break
            case Qt.Key_Up: if (win.sidebarFocus) sidebarPanel.moveKey(-1); else if (alt) pane.up(); else if (win.galleryPane()) win.galleryPane().step(-1); else if (win.columnsPane()) win.columnsPane().moveKey(-1); else win.moveSelection(-win.rowStep, shift); break
            case Qt.Key_F: if (win.galleryPane()) win.galleryPane().filmstrip = !win.galleryPane().filmstrip; else return; break
            case Qt.Key_Home: win.selectAt(0, shift); break
            case Qt.Key_End: win.selectAt(pane.listing.count - 1, shift); break
            case Qt.Key_PageDown: win.moveSelection(win.pageStep, shift); break
            case Qt.Key_PageUp: win.moveSelection(-win.pageStep, shift); break
            case Qt.Key_H: if (win.vimKeys && win.columnsPane()) { if (!win.columnsPane().focusLeft()) pane.up() } else return; break
            case Qt.Key_Return: case Qt.Key_Enter: if (win.sidebarFocus) { sidebarPanel.activateKey(); win.focusSidebar(false); break } if (win.columnsPane()) { win.columnsPane().activateKey(); break } { const rr = pane.listing.row(pane.selection.current); if (rr && rr.isDir && !pane.isTrash) { win.openFolder(pane.childUri(rr.name)); break } } win.openSelected(); break
            case Qt.Key_Backspace: pane.up(); break
            // In columns, Left walks back through the columns the inspector pushed off screen and
            // only leaves the folder once it runs out of them.
            // Left leaves a folder, Right enters one, whichever view is showing.
            case Qt.Key_Left: if (alt) pane.back(); else if (win.galleryPane()) { if (!win.galleryPane().step(-1)) pane.up() } else if (win.columnsPane()) { if (!win.columnsPane().focusLeft()) pane.up() } else pane.up(); break
            case Qt.Key_Right: if (alt) pane.forward(); else if (win.galleryPane()) win.galleryPane().step(1); else if (win.columnsPane()) win.columnsPane().focusRight(); else win.enterSelected(); break
            case Qt.Key_E: if (win.vimKeys) win.editSelected(); else return; break
            case Qt.Key_Menu: menu.open(win.contextItems(pane.selection.current), Qt.point(400, 200)); break
            case Qt.Key_Escape: if (activity.visible) activity.close(); else if (win.sidebarFocus) win.focusSidebar(false); else if (win.galleryPane()) pane.view = win.galleryFrom; else pane.selection.clear(); break
            case Qt.Key_I: if (win.vimKeys) win.inspectorRequested = !win.inspectorRequested; else return; break
            case Qt.Key_Tab: if (win.split) win.focusPane(win.otherPane()); else return; break
            default:
                // type-ahead: printable characters without Ctrl/Alt (Vim keys off)
                // Not in the gallery: its bare keys are commands (0, 1, +, -, F, Space), and a letter
                // that jumped the stage to another picture would be a surprise beside them.
                if (!ctrl && !alt && !win.vimKeys && !win.sidebarFocus && !win.galleryPane() && event.text && event.text.length === 1 && event.text.charCodeAt(0) > 32) { win.typeAhead(event.text); break }
                return
            }
            event.accepted = true
        }
    }

    UI.ScrollProbe { id: scrollProbe }
    IpcHandler {
        target: "shell"
        function open(uri: string): void { win.pane.open(uri) }
        /// What the `kiki` launcher calls on a running instance: open it, and come to the front.
        function present(uri: string): void { win.present(uri) }
        function enter(): void { win.enterSelected() }
        /// Mirrors the gallery's keys, fallback included, so the harness can drive them.
        function gallery(action: string): void {
            const g = win.galleryPane(); if (!g) return
            if (action === "prev") { if (!g.step(-1)) win.pane.up() }
            else if (action === "next") g.step(1)
            else if (action === "open") g.activateKey()
            else if (action === "play") g.togglePlay()
        }
        /// What the gallery cost: how many pictures it has decoded and how long they took. The
        /// perf flow reads this; nothing in the window depends on it.
        function galleryStats(): string {
            const g = win.galleryPane()
            return JSON.stringify(g ? g.stats() : {})
        }
        /// Scroll the focused pane's view from top to bottom in `ms`, measuring; `scrollStats`
        /// says `running` until it is over. The scroll_perf flow; nothing here depends on it.
        function scrollRun(ms: string): void {
            const v = win.currentView(), s = v && v.scroller ? v.scroller() : null
            scrollProbe.start(s ? s.view : null, s ? s.cache : null, parseInt(ms) || 4000)
        }
        function scrollStats(): string { return JSON.stringify(scrollProbe.result) }
        function back(): void { win.pane.back() }
        function forward(): void { win.pane.forward() }
        function setView(v: string): void { win.pane.view = v }
        function search(text: string): void { if (text) win.openFilter(); const bar = win.pane === win.right ? rightFilter : leftFilter; bar.text = text; win.pane.setFilter(text) }
        function searchEverywhere(text: string): void { if (searchOverlay.visible && !text) searchOverlay.close(); else win.openSearch(text) }
        function select(name: string): void {
            for (let i = 0; i < win.pane.listing.count; i++) {
                const r = win.pane.listing.row(i)
                if (r && r.name === name) { win.pane.selection.set(i); return }
            }
            // Past the rows the window happens to hold — it keeps a few hundred either side of
            // the viewport, not the whole folder — only the daemon knows where a name sits.
            if (win.pane.listing.lid) win.seekName(name)
        }
        function selection(): string { return JSON.stringify(win.selectedUris()) }
        function uri(pane: string): string { return win.pane.uri }
        function split(on: string): void { if (on === "on") win.enterMirror(); else win.leaveMirror() }
        function project(action: string, uri: string): void { if (action === "enter") win.enterProject(uri || win.pane.uri); else win.leaveProject() }
        function projectState(): string { return JSON.stringify({ root: win.projectRoot, active: win.projectMode, width: win.width }) }
        function edit(uri: string, line: string): void { win.editAt(uri, parseInt(line) || 1) }
        function reveal(uri: string): void { if (win.projectMode) projectTree.reveal(uri); else { const p = uri.replace(/\/[^/]*$/, ""); win.pane.open(p); const name = decodeURIComponent(uri.split("/").pop()); Qt.callLater(() => { for (let i = 0; i < win.pane.listing.count; i++) { const r = win.pane.listing.row(i); if (r && r.name === name) { win.pane.selection.set(i); break } } }) } }
        function saved(uri: string): void { win.pane.listing.refresh() }
        /// `share` alone opens the menu; `share <plugin>` sends to it, as clicking it would.
        function share(plugin: string, target: string): void {
            if (!plugin) { win.shareMenu(); return }
            const p = win.sharePlugins.find(x => x.id === plugin); if (!p) return
            const uris = win.selectedUris(); if (!uris.length) return
            if (p.targets === "none" && !target) win.shareNow(p, null, uris)
            else win.shareTargets(p, uris)
        }
        function settings(action: string, page: string): void { if (action === "open") settingsWin.open(page || "general"); else { settingsWin.close(); keys.forceActiveFocus() } }
        function mirror(on: string): void { if (on === "open") win.startMirror(true); else win.mirrorOpen = false }
        function mirrorScreen(): string { return win.mirrorOpen ? mirrorWs.screen : "" }
        function focusPane(side: string): void { win.focusPane(side === "right" ? win.right : win.left) }
        function transfer(kind: string): void { win.transfer(kind === "move") }
        function openLocation(name: string): void { const l = win.locations.find(x => x.name === name); if (l) win.openLocation(l) }
        function state(): string {
            return JSON.stringify({ uri: win.pane.uri, view: win.pane.view, count: win.pane.listing.count, done: win.pane.listing.done, selection: win.selectedUris(), inspector: win.inspector, sidebar: win.sidebarShown, keyFocus: keys.activeFocus, filterOpen: win.filterOpen, searchOpen: searchOverlay.visible, settingsVisible: settingsWin.visible, menuVisible: menu.visible, clipboard: win.clipboard.uris, clipboardCut: win.clipboard.cut === true, renaming: win.pane.renamingIndex,
                daemon: { ready: Kiki.Daemon.ready, connected: Kiki.Daemon.connected },
                dialogs: { confirm: confirm.visible, compress: compressDialog.visible, location: locationDialog.visible, shortcuts: shortcuts_.visible, integration: integrationDialog.visible, portal: portal.visible, share: shareSheet.visible }, split: win.split, filter: win.pane.filterText, sort: [win.pane.sortRole, win.pane.sortOrder], toast: win.toast })
        }
        /// Side by side, for scripts and tests: `toggle`; `drag <px>` is what dragging the line
        /// between the panes to that x does, `end` lets go, `reset` is the double click.
        function sideBySide(action: string): string {
            if (action === "toggle") win.toggleMirrorView()
            else if (action.indexOf("drag ") === 0) win.dragDivider(paneRow.width, parseInt(action.slice(5)) || 0)
            else if (action === "end") win.endDividerDrag()
            else if (action === "reset") win.resetDivider()
            return JSON.stringify({ split: win.split, total: paneRow.width, left: win.split ? win.leftPaneWidth(paneRow.width) : paneRow.width,
                ratio: win.sideRatio, dragging: win.sideRatioLive > 0, min: win.sideMin,
                leftView: win.left.view, rightView: win.right.view, remembering: win.left.rememberViews,
                titlePathShown: toolbar.pathShown, leftPath: leftHeader.breadcrumb.visible ? leftHeader.breadcrumb.uri : "", rightPath: rightHeader.breadcrumb.visible ? rightHeader.breadcrumb.uri : "", focused: win.pane === win.right ? "right" : "left", leftUri: win.left.uri, rightUri: win.right.uri })
        }
        function viewMenu(): void { if (menu.visible) menu.close(); else win.viewMenu() }
        /// What the open menu says, for scripts and tests: each row's label, tick and whether it is live.
        function menuItems(): string { return JSON.stringify(menu.visible ? menu.items.map(i => ({ label: i.label, checked: i.checked === true, enabled: !menu.off(i) })) : []) }
        function pathMenu(): void { if (menu.visible) menu.close(); else win.pathMenu() }
        function toggleSearch(): void { win.toggleSearch() }
        function inspector(on: string): void { win.inspectorRequested = on === "" ? !win.inspectorRequested : on === "on" }
        function shortcuts(): void { if (shortcuts_.visible) shortcuts_.close(); else shortcuts_.open() }
        function openWith(): void { if (menu.visible) menu.close(); else win.openWithMenu() }
        function columns(action: string): string {
            const c = win.columnsPane(); if (!c) return ""
            if (action === "down") c.moveKey(1); else if (action === "up") c.moveKey(-1)
            else if (action === "left") c.focusLeft(); else if (action === "right") c.focusRight()
            else if (action === "open") c.activateKey()
            // `info <px>` is what a drag on the info column's edge does, for scripts and tests.
            else if (action.indexOf("info ") === 0) c.inspectorW = parseInt(action.slice(5)) || 0
            return JSON.stringify({ focusCol: c.focusCol, count: c.columns.length, selected: c.columns.map(x => x.selected),
                                    inspected: c.inspectedUri, scrollX: Math.round(c.scrollX), width: Math.round(c.stripWidth),
                                    columnWidth: c.columnWidth, infoWidth: c.inspectedUri !== "" ? c.inspectorWidth : 0 })
        }
        function sidebar(on: string): void { win.sidebarShown = on === "" ? !win.sidebarShown : on === "on" }
        function undo(): void { Kiki.Jobs.undo() }
        function redo(): void { Kiki.Jobs.redo() }
        function activity(): string { return JSON.stringify(Kiki.Jobs.list) }
        /// The orb and its popup, for the harness: "open" | "close" | "toggle" | "clear", and what is showing.
        function activityView(action: string): string {
            if (action === "open") activity.open(); else if (action === "close") activity.close(); else if (action === "toggle") activity.toggle(); else if (action === "clear") Kiki.Jobs.clear()
            return JSON.stringify({ open: activity.visible, orb: Kiki.Jobs.orbState(), tip: Kiki.Jobs.orbTip(), entries: Kiki.Jobs.shown().map(j => ({ id: j.id, headline: Kiki.Jobs.headline(j), state: j.state, line: Kiki.Jobs.live(j) ? Kiki.Jobs.statusLine(j) : Kiki.Jobs.completion(j) })) })
        }
        function contextMenu(action: string): void { const it = win.contextItems(win.pane.selection.current).find(i => i.label === action); if (it && it.enabled !== false && it.action) it.action() }
        function addLocation(): void { locationDialog.open(null) }
        /// The Add-location form, for scripts and tests: pick a kind by scheme, a page or a
        /// credentials tab by name.
        function locationForm(what: string, name: string): string {
            if (what === "kind") locationDialog.selectKind(locationDialog.plugins.findIndex(p => p.scheme === name))
            else if (what === "page") locationDialog.page = name
            else if (what === "auth") locationDialog.chooseGroup(name)
            return JSON.stringify({ kind: locationDialog.current() ? locationDialog.current().scheme : "", page: locationDialog.page, auth: locationDialog.authGroup, pages: locationDialog.pages(), groups: locationDialog.groups() })
        }
        function about(): void { if (aboutDlg.visible) aboutDlg.close(); else aboutDlg.open() }
        /// The palette in force, for scripts and for checking a theme change landed.
        function theme(): string {
            return JSON.stringify({ name: Kiki.Theme.name, icons: Kiki.Theme.iconTheme,
                                    folderIcon: Quickshell.iconPath("folder", true), bg: String(Kiki.Theme.bg),
                                    fg: String(Kiki.Theme.fg), accent: String(Kiki.Theme.accent), surface: String(Kiki.Theme.surface) })
        }
        function keymap(): void { if (keysWin.visible) keysWin.close(); else keysWin.open() }
        /// Where an element is, in window coordinates, for a test that drives the pointer: the
        /// rectangle of the first item with this objectName, or an empty object when nothing has
        /// it. Names follow plan 28: row-N, tile-N, column-N, menu-LABEL, perm-WHO-BIT …
        function geometry(name: string): string {
            const it = win.findByName(win.contentItem, name)
            return it ? win.rectOf(it) : "{}"
        }
        /// Where row `i` of the current view is, in window coordinates. Rows move as a folder
        /// loads, so this asks the view for the delegate rather than searching by name.
        function rowGeometry(index: string): string {
            const pane = win.currentView()
            const it = pane && pane.rowItem ? pane.rowItem(parseInt(index)) : null
            return it ? win.rectOf(it) : "{}"
        }
        /// Close whatever is open — editor, menu, overlay — and hand the keymap its focus back.
        /// What Escape does, for a script that cannot be sure what the last step left behind.
        function dismiss(): void {
            win.pane.renamingIndex = -1
            menu.close()
            if (shortcuts_.visible) shortcuts_.close()
            if (searchOverlay.visible) searchOverlay.close()
            win.filterOpen = false
            win.pane.selection.clear()
            keys.forceActiveFocus()
        }
        function windowState(pane: string): string { const l = win.pane.listing; return JSON.stringify({ count: l.count, viewport: [l.viewportFirst, l.viewportCount], held: Object.keys(l._rows).length }) }
        function timestamps(): string { return JSON.stringify({ now: Date.now() }) }
    }

    property alias toolbar: toolbar

    Views.ProjectTree {
        id: projectTree
        visible: win.projectMode
        anchors.fill: parent
        rootUri: win.projectMode ? win.projectRoot : ""
        // The keys are the tree's while it is up. `keys` lets go of the focus in project mode
        // and nothing took it, so no key did anything — Esc and Ctrl+Shift+P included, which
        // left the mouse as the only way out.
        focus: win.projectMode
        onVisibleChanged: if (visible) forceActiveFocus()
        home: win.home; repo: win.repo
        onOpenFile: uri => win.editAt(uri, 1)
        onSendToAgent: uri => Kiki.Daemon.request("OpenIn", { role: "agent", uris: [uri] })
        onLeave: win.leaveProject()
    }
    // The toolbar spans the window above everything, so the path has the full width to use, and
    // the favorites panel sits under it and can be hidden.
    Column {
        visible: !win.projectMode
        anchors.fill: parent
        UI.Toolbar {
            id: toolbar
            objectName: "toolbar"
            width: parent.width
            pane: win.pane; home: win.home
            onToggleSplit: win.toggleMirrorView()
            // Over the line between the two panes, wherever that line is: the panes start after
            // the sidebar, and the line goes where it is dragged (and is remembered there), so
            // neither the middle of the bar nor the middle of the panes is where it is.
            mirrorCenterX: win.sidebarSpace + win.leftPaneWidth(paneRow.width) + 0.5
            onToggleMirror: win.toggleMirror()
            onNavigate: uri => win.navigateFromTitle(uri)
            split: win.split; mirror: win.mirrorOpen
            locations: win.locations
            repo: win.repo
            onViewMenu: win.viewMenu()
            onPathMenu: win.pathMenu()
            onSettings: win.gearMenu()
            onToggleSearch: win.openSearch(leftFilter.text)
            sidebarShown: win.sidebarShown
            onToggleSidebar: win.sidebarShown = !win.sidebarShown
        }
        Row {
            width: parent.width; height: parent.height - toolbar.height - bar.height
            // Space the pinned panel occupies; while peeking it floats over the panes instead.
            Item { width: win.sidebarSpace; height: parent.height }
        Column {
            width: parent.width - win.sidebarSpace; height: parent.height
            UI.MirrorWorkspace {
                id: mirrorWs
                objectName: "mirror-workspace"
                visible: win.mirrorOpen
                width: parent.width; height: parent.height
                localUri: win.localUri(); remoteUri: win.remoteUri(); home: win.home
                onClosed: win.mirrorOpen = false
                onRelist: { win.left.listing.refresh(); win.right.listing.refresh(); win.recordMirror() }
            }
            // Off for now: the strip above the two panes is gone, and "Mirror to …", Swap and
            // "last mirrored" will be given a place of their own later. Kept wired so that is a
            // matter of showing it somewhere, not of rebuilding it. The way into a mirror run is
            // the button in the middle of the toolbar, and Ctrl+M; Swap has no other way in yet
            // and waits for that place.
            UI.MirrorBar {
                id: mirrorBar
                objectName: "mirror-bar"
                visible: false
                width: parent.width
                leftPane: win.left; rightPane: win.right; home: win.home
                lastMirrored: win.lastMirror[win.remoteUri()] || null
                onSwap: win.swapPanes()
                onMirror: upload => win.startMirror(upload)
                onOptions: pos => { const p = mirrorBar.mapToItem(menu.parent, pos.x, pos.y); win.mirrorOptions(Qt.point(p.x, p.y)) }
            }
            Row {
                id: paneRow
                objectName: "pane-row"
                visible: !win.mirrorOpen
                width: parent.width; height: parent.height - (mirrorBar.visible ? mirrorBar.height : 0)
                // Left pane (the only pane when not split)
                Column {
                    id: leftCol
                    objectName: "pane-left"
                    width: Math.max(0, (win.split ? win.leftPaneWidth(parent.width) : parent.width) - (inspectorPanel.visible && !win.split ? inspectorPanel.width : 0)); height: parent.height
                    UI.PaneHeader { objectName: "pane-header-left"; visible: win.split; width: parent.width; pane: win.left; home: win.home; id: leftHeader; onClicked: win.focusPane(win.left); onPathMenu: c => win.paneHeaderPathMenu(win.left, c) }
                    UI.FilterBar {
                        id: leftFilter
                        visible: win.filterOpen && win.pane === win.left
                        width: parent.width; pane: win.left; total: win.filterTotal
                        onPromote: text => { win.closeFilter(); win.openSearch(text) }
                        onClosed: win.closeFilter()
                    }
                    Loader {
                        id: viewLoader
                        width: parent.width; height: parent.height - (win.split ? 34 : 0) - (leftFilter.visible ? leftFilter.height : 0)
                        sourceComponent: win.left.view === "icon" ? iconView : (win.left.view === "columns" ? columnsView : (win.left.view === "gallery" ? galleryView : listView))
                        onLoaded: item.pane = win.left
                        // A different Pane object (the two change places when side by side is
                        // left from the right): build the view again rather than re-point one
                        // whose rows and caches were made for the other pane.
                        readonly property var shownPane: win.left
                        onShownPaneChanged: if (status === Loader.Ready) { active = false; active = true }
                        // Side by side a press anywhere in the view — a file or white space — gives
                        // its pane the focus; the pointer passing over does not (see PaneFocus). It
                        // lies over the view as the view's sibling. (It used to hang from a holder
                        // of no size at the top of the pane's Column, and in the running app its
                        // handler was never asked: a press outside the holder's own 0×0 is not
                        // offered to what the holder contains.)
                        UI.PaneFocus { objectName: "pane-focus-left"; anchors.fill: parent; z: 100; active: win.split; onWanted: if (win.pane !== win.left) win.focusPane(win.left) }
                        UI.PaneError { anchors.fill: parent; z: 50; pane: win.left; onRetry: win.left.listing.open(win.left.uri) }
                    }
                }
                // The line between the panes is a grip: drag it to give one side more room,
                // double-click for half and half. The same grip the info panel has.
                Rectangle {
                    id: divider
                    objectName: "pane-divider"
                    visible: win.split; width: 1; height: parent.height; z: 10
                    color: dividerGrip.containsMouse || dividerGrip.pressed ? Kiki.Theme.accent : Kiki.Theme.line
                    MouseArea {
                        id: dividerGrip
                        objectName: "pane-divider-grip"
                        x: -3; width: 7; height: parent.height
                        cursorShape: Qt.SplitHCursor
                        hoverEnabled: true
                        preventStealing: true
                        onPositionChanged: mouse => { if (pressed) win.dragDivider(paneRow.width, mapToItem(paneRow, mouse.x, 0).x) }
                        onReleased: win.endDividerDrag()
                        onDoubleClicked: win.resetDivider()
                    }
                }
                Column {
                    id: rightCol
                    objectName: "pane-right"
                    visible: win.split
                    width: win.split ? parent.width - win.leftPaneWidth(parent.width) - 1 : 0; height: parent.height
                    UI.PaneHeader { objectName: "pane-header-right"; width: parent.width; pane: win.right; home: win.home; id: rightHeader; onClicked: win.focusPane(win.right); onPathMenu: c => win.paneHeaderPathMenu(win.right, c) }
                    UI.FilterBar {
                        id: rightFilter
                        visible: win.filterOpen && win.pane === win.right
                        width: parent.width; pane: win.right; total: win.filterTotal
                        onPromote: text => { win.closeFilter(); win.openSearch(text) }
                        onClosed: win.closeFilter()
                    }
                    Loader {
                        id: rightLoader
                        active: win.split
                        width: parent.width; height: parent.height - 34 - (rightFilter.visible ? rightFilter.height : 0)
                        sourceComponent: win.right.view === "icon" ? iconView : (win.right.view === "columns" ? columnsView : (win.right.view === "gallery" ? galleryView : listView))
                        onLoaded: item.pane = win.right
                        readonly property var shownPane: win.right
                        onShownPaneChanged: if (status === Loader.Ready) { active = false; active = true }
                        // Side by side a press anywhere in the view — a file or white space — gives
                        // its pane the focus; the pointer passing over does not (see PaneFocus). It
                        // lies over the view as the view's sibling. (It used to hang from a holder
                        // of no size at the top of the pane's Column, and in the running app its
                        // handler was never asked: a press outside the holder's own 0×0 is not
                        // offered to what the holder contains.)
                        UI.PaneFocus { objectName: "pane-focus-right"; anchors.fill: parent; z: 100; active: win.split; onWanted: if (win.pane !== win.right) win.focusPane(win.right) }
                        UI.PaneError { anchors.fill: parent; z: 50; pane: win.right; onRetry: win.right.listing.open(win.right.uri) }
                    }
                }
                UI.Inspector {
                    id: inspectorPanel
                    // Columns view supplies its own inspector column; icon and list show it with the selection.
                    visible: win.pane.view !== "columns" && win.inspector
                    width: Math.min(win.inspectorW, Math.floor(parent.width * 0.7)); height: parent.height
                    uri: win.inspectedUri; row: win.inspectedRow; home: win.home
                    onClosed: win.inspectorRequested = false
                    // Dragging the grip leftwards makes the panel wider.
                    onResized: dx => win.setInspectorWidth(win.inspectorW - dx, parent.width)
                    onResizeEnded: Kiki.Settings.set("view", "inspectorWidth", win.inspectorW)
                    onEdit: (u, line) => win.editAt(u, line)
                    onOpen: u => win.openExternal(u)
                    onChmod: (mode, recursive) => win.submitChmod(win.inspectedUri, mode, recursive)
                }
            }
        }
        }
        UI.ShortcutBar {
            id: bar
            width: parent.width
            // Side by side, Ctrl+M is the thing the layout is for, so it leads the hints.
            keys: (win.split ? [{ key: "^M", label: "mirror" }] : []).concat(win.vimKeys ? [{ key: "h j k l", label: "move" }] : []).concat([
                { key: "Enter", label: "open" }, { key: "←", label: "up" }, { key: "→", label: "into" },
                { key: "^I", label: "info" }, { key: "F2", label: "rename" }, { key: "Del", label: "trash" },
                { key: "❖C", label: "copy" }, { key: "❖V", label: "paste" }, { key: "/", label: "filter" },
                { key: "?", label: "keys" }])
            statusInset: 38      // the orb stands at the right end
            status: (win.runningShown ? win.runningShown + " running · " : "") + win.countText()
        }

    }

    UI.Sidebar {
        id: sidebarPanel
        x: 0; y: toolbar.height; z: 60
        // Peeking ends when the pointer leaves the panel.
        HoverHandler { onHoveredChanged: if (!hovered) win.sidebarPeek = false }
        visible: win.sidebarShown || win.sidebarPeek
        compact: win.sidebarRail
        width: win.sidebarRail ? 44 : win.sidebarFull
        height: win.height - toolbar.height - bar.height
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
        onFavoriteMenu: (index, pos) => menu.open([
            { label: "Remove from Sidebar", action: () => {
                const list = win.favorites.slice()
                list.splice(index, 1)
                Kiki.Daemon.request("SetFavorites", { items: list }, () => win.loadSidebar())
            } },
        ], pos)
        onAddFavorites: (uris, index) => {
            const add = uris.filter(u => u.startsWith("file://") && !win.favorites.some(f => f.uri === u)).map(u => ({ name: decodeURIComponent(u.replace(/\/+$/, "").split("/").pop()) || "/", uri: u }))
            if (!add.length) return
            const list = win.favorites.slice()
            list.splice(Math.max(0, Math.min(list.length, index)), 0, ...add)
            Kiki.Daemon.request("SetFavorites", { items: list }, () => win.loadSidebar())
        }
        onMountVolume: vol => Kiki.Daemon.request("Mount", { device: vol.device }, (ok, err) => { if (ok) win.pane.open(ok.uri); else Kiki.Jobs.showToast({ text: "Mount failed: " + (err ? err.message : ""), undoable: false }) })
        onVolumeMenu: vol => menu.open([
            { label: vol.mounted === false ? "Mount" : "Open", action: () => vol.mounted === false ? Kiki.Daemon.request("Mount", { device: vol.device }, ok => { if (ok) win.pane.open(ok.uri) }) : win.pane.open(vol.uri) },
            { label: "Unmount", enabled: vol.mounted !== false && vol.uri !== "file:///", action: () => Kiki.Daemon.request("Unmount", { device: vol.device }, (ok, err) => { if (err) Kiki.Jobs.showToast({ text: "Unmount failed: " + err.message, undoable: false }) }) },
            { label: "Eject", enabled: !!vol.removable, action: () => Kiki.Daemon.request("Eject", { device: vol.device }, (ok, err) => { if (err) Kiki.Jobs.showToast({ text: "Eject failed: " + err.message, undoable: false }) }) },
        ], Qt.point(40, 200))
        onEditLocation: loc => menu.open([{ label: "Open", action: () => win.pane.open(loc.remoteUri) }, { label: "Edit…", action: () => locationDialog.open(loc) }].concat(win.locationImageItems(loc)).concat([{ label: "Connection Log…", sep: true, action: () => jobLog.openLocation(loc.name) }, { label: "Disconnect", action: () => Kiki.Daemon.request("Disconnect", { name: loc.name }) }, { label: "Remove", danger: true, sep: true, action: () => Kiki.Daemon.request("RemoveLocation", { name: loc.name }, () => win.loadSidebar()) }]), Qt.point(40, 200))
    }

    // Mousing into the left edge brings the hidden favorites panel back.
    MouseArea {
        x: 0; y: toolbar.height; width: 6; height: win.height - toolbar.height - bar.height
        z: 61; hoverEnabled: true; acceptedButtons: Qt.NoButton
        enabled: !win.sidebarShown && !win.sidebarPeek && !win.projectMode
        onEntered: win.sidebarPeek = true
    }

    Component { id: listView; Views.ListPane { pane: win.left; onActivate: i => { win.focusPane(pane); pane.selection.set(i); win.openSelected() }; onContextMenu: (i, pos) => { win.focusPane(pane); menu.open(win.contextItems(i), pos) } } }
    Component { id: galleryView; Views.GalleryPane { pane: win.left; home: win.home; onActivate: i => { win.focusPane(pane); pane.selection.set(i); win.openSelected() }; onContextMenu: (i, pos) => { win.focusPane(pane); menu.open(win.contextItems(i), pos) } } }
    Component { id: iconView; Views.IconPane { pane: win.left; onActivate: i => { win.focusPane(pane); pane.selection.set(i); win.openSelected() }; onContextMenu: (i, pos) => { win.focusPane(pane); menu.open(win.contextItems(i), pos) } } }

    Views.SearchOverlay {
        id: searchOverlay
        parent: win.contentItem
        results: win.results; locations: win.locations; home: win.home; indexInfo: win.indexInfo
        onSearch: (text, scope) => win.runSearch(text, scope)
        onOpenUri: uri => { const r = uri.replace(/\/[^/]*$/, "") || uri; win.pane.open(uri.endsWith("/") ? uri : r) }
        onRevealUri: uri => { const r = uri.replace(/\/[^/]*$/, "") || uri; win.pane.open(r) }
        onClosed: keys.forceActiveFocus()
    }
    UI.ShortcutsOverlay { id: shortcuts_; parent: win.contentItem; onClosed: keys.forceActiveFocus() }
    UI.KeymapWindow { id: keysWin; parent: win.contentItem; keymap: keymap; onClosed: keys.forceActiveFocus() }
    UI.AboutDialog { id: aboutDlg; parent: win.contentItem; onClosed: keys.forceActiveFocus() }
    UI.ContextMenu { id: menu; parent: win.contentItem; onClosed: keys.forceActiveFocus() }
    UI.Toast { anchors.horizontalCenter: parent.horizontalCenter; anchors.bottom: parent.bottom; anchors.bottomMargin: 44 }
    UI.CollisionPrompt { anchors.fill: parent }
    UI.LocationDialog {
        id: locationDialog; anchors.fill: parent; onSaved: win.loadSidebar(); onVisibleChanged: if (!visible) keys.forceActiveFocus()
        // "Add and Connect": the list of locations is asked for afresh (the one just saved is
        // not in ours yet), and the new one is opened from the answer.
        onOpenRequested: name => Kiki.Daemon.request("Locations", {}, ok => {
            if (!ok) return
            win.locations = ok.locations
            const loc = ok.locations.find(l => l.name === name)
            if (loc) win.openLocation(loc)
        })
        // kiki's own folder chooser — the one it gives other apps through the portal — over the
        // form, starting where the field points (or at home), answering with a plain path.
        onChooseImage: (start, reply) => win.pickLocationImage(start, path => { reply(path); locationDialog.forceActiveFocus() })
        onChooseFolder: (start, reply) => portal.pick(
            { mode: "open", directory: true, title: "Choose the local folder", currentFolder: (start || "").replace(/^~/, win.home) || win.home },
            uris => { if (uris && uris.length) reply(decodeURIComponent(uris[0].replace(/^file:\/\//, "")).replace(/\/+$/, "") || "/"); locationDialog.forceActiveFocus() })
    }
    /// kiki's own chooser, asked for one picture; answers with a plain path.
    function pickLocationImage(start, reply) {
        const dir = start ? start.replace(/\/[^\/]*$/, "") : ""
        portal.pick({ mode: "open", title: "Choose an image", currentFolder: dir || win.home,
                      filters: [{ name: "Images", patterns: ["*.png", "*.jpg", "*.jpeg", "*.webp", "*.svg", "*.gif", "*.bmp"] }] },
            uris => { if (uris && uris.length) reply(decodeURIComponent(uris[0].replace(/^file:\/\//, ""))) })
    }
    /// The sidebar menu's part for a location's picture: set one, and take it off again.
    function locationImageItems(loc) {
        const set = image => Kiki.Daemon.request("SetLocationImage", { name: loc.name, image: image }, () => win.loadSidebar())
        const items = [{ label: loc.image ? "Change Image…" : "Set Image…", sep: true, action: () => win.pickLocationImage(loc.image || "", set) }]
        if (loc.image) items.push({ label: "Remove Image", action: () => set("") })
        return items
    }
    UI.PortalDialog { id: portal; anchors.fill: parent; home: win.home; favorites: win.favorites; locations: win.locations }
    UI.SettingsWindow { id: settingsWin; parent: win.contentItem; onVisibleChanged: if (!visible) keys.forceActiveFocus() }
    UI.IntegrationDialog { id: integrationDialog; parent: win.contentItem }
    UI.ConfirmDialog { id: confirm; parent: win.contentItem; onVisibleChanged: if (!visible) keys.forceActiveFocus() }
    Connections { target: Kiki.Settings; function onLoadedChanged() { if (Kiki.Settings.loaded && Kiki.Settings.integration.asked === false) integrationDialog.open() } }
    UI.ShareSheet { id: shareSheet; anchors.fill: parent }
    UI.CompressDialog { id: compressDialog; anchors.fill: parent; onSubmit: (archive, format) => ops.compress(items, archive, format) }
    // Activity (plan 32). The orb is the window's, not the key-hint row's: it is there in every
    // mode, project mode's narrow tree and the gallery included. It replaced an unmarked 200 px
    // click area at the right of the bottom bar, which nothing said could be clicked.
    UI.ActivityOrb {
        id: orb
        objectName: "activity-orb"
        anchors.right: parent.right; anchors.bottom: parent.bottom; anchors.rightMargin: 6; z: 61
        open: activity.visible
        onClicked: activity.toggle()
    }
    UI.ActivityPopover {
        id: activity
        objectName: "activity"
        aimX: orb.x + orb.width / 2; aimY: orb.y
        onLogRequested: job => { activity.close(); jobLog.openJob(job) }
        onRevealRequested: uri => win.revealUri(uri)
    }
    UI.JobLogWindow { id: jobLog; objectName: "joblog"; onCopyText: text => Quickshell.execDetached(["wl-copy", text]) }
    /// Show a file where it is: its folder, with it selected.
    function revealUri(uri) {
        const parent = uri.replace(/\/[^/]*$/, "") || uri
        win.pane.open(parent)
        win.pane.selectAfterLoad = decodeURIComponent(uri.split("/").pop())
        win.selectCameFrom()
    }
    Component { id: columnsView; Views.ColumnsPane { pane: win.left; home: win.home; onActivate: uri => win.openExternal(uri); onEdit: (u, line) => win.editAt(u, line)
        onContextMenu: (uri, row, pos) => { win.focusPane(pane); menu.open(win.contextItemsForUri(uri, row), pos) }
        onContextMenuFolder: (uri, pos) => { win.focusPane(pane); menu.open(win.folderItems(uri), pos) } } }
}
