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
    // Closing the window closes kiki. Quickshell only hides a closed window and keeps the shell
    // up behind it, and a hidden one cannot be shown again (setting `visible` does not map it):
    // the next launch found a running kiki, asked it to come to the front, and nothing came —
    // no window until the process was killed by hand.
    property bool _wasShown: false
    onVisibleChanged: { if (visible) _wasShown = true; else if (_wasShown) Qt.quit() }
    // Nothing runs on behind a window that is gone, so a close with jobs going is a question
    // first. The `closing` that can be refused is the real window's — Quickshell's proxy has only
    // `closed`, which is after the fact. Yes stops the jobs here and now; a kiki that goes any
    // other way (killed, crashed) has them stopped by the daemon, a few seconds after it has gone.
    readonly property var _backing: win.contentItem ? win.contentItem.Window.window : null
    property bool _quitting: false
    property var _quit: () => Qt.quit()     // a test's to replace: it would take the runner along
    function closeAsked(ev) {
        const live = Kiki.Jobs.running(), seen = live.filter(j => !j.hidden)
        if (win._quitting || !seen.length) return
        ev.accepted = false
        const what = seen.length === 1 ? Kiki.T.tr("quit.oneRunning", { job: Kiki.Jobs.headline(seen[0]) }) : Kiki.T.tr("quit.manyRunning", { n: seen.length })
        confirm.ask({ title: Kiki.T.tr("quit.title"), message: what + " " + Kiki.T.tr("quit.finishedStays"), label: Kiki.T.tr("quit.confirm") }, yes => {
            if (!yes) return
            for (const j of Kiki.Jobs.running()) Kiki.Jobs.cancel(j.id)
            win._quitting = true
            win._quit()
        })
    }
    Connections { target: win._backing; ignoreUnknownSignals: true; function onClosing(ev) { win.closeAsked(ev) } }

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
    readonly property int sidebarSpaceWanted: sidebarPanel.visible ? (sidebarRail ? 44 : sidebarFull) : 0
    // Eased: pinning or hiding the panel slides the panes over rather than snapping them.
    property int sidebarSpace: sidebarSpaceWanted
    Behavior on sidebarSpace { NumberAnimation { duration: 160; easing.type: Easing.OutCubic } }
    // Side by side is how many folders the WINDOW shows; `Pane.view` is only how one pane draws
    // its folder. (It used to be a view — `left.view === "mirror"` — which left the left pane
    // without a view of its own and wrote "mirror" into views.toml as the folder's preference.)
    property bool sideBySide: false
    readonly property bool split: sideBySide
    // A drop hands the focus to the pane it landed in: that is where the files now are.
    property Kiki.Pane left: Kiki.Pane { view: win.defaultView(); focused: true; rememberViews: !win.sideBySide; onReceived: win.focusPane(win.left) }
    property Kiki.Pane right: Kiki.Pane { view: "list"; focused: false; rememberViews: !win.sideBySide; onReceived: win.focusPane(win.right) }
    /// The drop the IPC makes, shaped as Qt shapes a real one — which has no `modifiers` at all:
    /// the keys held arrive folded into `proposedAction`, by the table measured in
    /// `Pane.wantsCopy` (no key and Shift → Move, the source's proposal; Ctrl and Alt → Copy).
    /// What the drop decided comes back. `ontoPane` names a pane as the target, which is what
    /// tells the two trashes apart: `trash:///` on its own is the Trash in the SIDEBAR, which
    /// trashes what is dropped on it, while the same URI aimed at a pane is the trash VIEW's own
    /// folder — and nothing goes into that.
    function fakeDrop(target, uris, dest, modifiers, ontoPane) {
        const list = (uris || "").split(/[\r\n]+/).filter(u => u && !u.startsWith("#"))
        if (!ontoPane && dest.startsWith("trash:")) {
            if (list.length) ops.trashSelection(list)
            return JSON.stringify({ accepted: list.length > 0, action: list.length ? "move" : "none", items: list.length })
        }
        const keys = (modifiers || "").toLowerCase().split(/[+,\s]+/)
        const copy = keys.indexOf("ctrl") >= 0 || keys.indexOf("control") >= 0 || keys.indexOf("alt") >= 0
        let took = Qt.IgnoreAction
        const ev = {
            accepted: false,
            hasUrls: list.length > 0,
            urls: list.map(u => ({ toString: () => u })),
            hasText: false, text: "",
            proposedAction: copy ? Qt.CopyAction : Qt.MoveAction,
            accept: a => { ev.accepted = true; took = a === undefined ? ev.proposedAction : a }
        }
        target.dropInto(dest, ev)
        return JSON.stringify({ accepted: ev.accepted && took !== Qt.IgnoreAction,
                                action: took === Qt.CopyAction ? "copy" : took === Qt.MoveAction ? "move" : "none",
                                items: list.length, focused: win.pane === win.right ? "right" : "left" })
    }
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
    /// A pill in the title bar's path. In columns view one that is among the columns brings them
    /// back to it instead of starting again from that folder.
    function navigateFromTitle(uri) { const c = columnsPane(); if (c && c.backTo(uri)) return; pane.open(uri) }
    /// The breadcrumb that is on screen for the focused pane: the title bar's with one pane,
    /// that pane's own header side by side. Ctrl+L and the path menu go to it.
    function activeCrumb() { return !split ? toolbar.breadcrumb : (pane === right ? rightHeader.breadcrumb : leftHeader.breadcrumb) }
    function focusPane(p) { left.focused = p === left; right.focused = p === right; pane = p }
    // Selecting a location opens Mirror view: local_uri on the left, remote_uri on the right.
    function openLocation(loc) {
        lastLocation = loc
        _enterSplit()
        // Already showing it: the click is a way back to the remote pane, not a request to
        // open it again — the panes keep their folders and selections (owner, 2026-09-24).
        // (The daemon reuses a live session either way; what a re-open cost was the pane's
        // place and selection.)
        const within = (u, root) => !!u && !!root && (u === root || u.startsWith(root.replace(/\/$/, "") + "/"))
        if (!within(right.uri, loc.remoteUri)) right.open(loc.remoteUri)
        if (loc.localUri && !within(left.uri, loc.localUri)) left.open(loc.localUri)
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
    function isRemote(uri) { return !!uri && !/^(file|trash):/.test(uri) }
    function locationOf(uri) { const m = (uri || "").match(/^([a-z0-9]+):\/\/([^/]+)/); return m ? locations.find(l => l.plugin === m[1] && l.name === decodeURIComponent(m[2])) : undefined }
    /// Side by side is a server and the folder it is kept beside (owner, 2026-09-21): there is a
    /// button and a key for it only while a server is open — or while it is on, so there is
    /// always a way out. Scripts can still put two local folders side by side (`shell split on`).
    readonly property bool remoteOpen: split ? (isRemote(left.uri) || isRemote(right.uri)) : isRemote(pane.uri)
    function enterMirror() {
        if (sideBySide) return
        _enterSplit()
        // Coming from one pane on a server: the server goes to the right, as a location opened
        // from the sidebar is laid out, and the folder it is paired with — or home — to the left.
        if (isRemote(left.uri) && !isRemote(right.uri)) {
            const u = left.uri, loc = locationOf(u)
            if (loc) lastLocation = loc
            right.open(u); left.open(loc && loc.localUri ? loc.localUri : "file://" + home)
            focusPane(right)
        } else if (!right.uri) right.open(lastLocation && lastLocation.remoteUri ? lastLocation.remoteUri : "file://" + home)
    }
    /// Back to one pane — the one you were in. The single layout draws `left`, so when the focus
    /// was on the right the two Pane objects change places: its folder, selection and history
    /// come across whole (copying the URI over would lose all three and re-list a remote folder
    /// for nothing), and the other folder waits in `right` for the layout to return.
    function leaveMirror() {
        if (!sideBySide) return
        // One pane again is the SERVER's pane, whichever had the focus; between two folders on
        // this machine (a script's doing) it is the one in use.
        const keepRight = isRemote(right.uri) ? true : (isRemote(left.uri) ? false : pane === right)
        if (keepRight) _swapPaneObjects()
        _swappedOnLeave = keepRight
        sideBySide = false
        focusPane(left)
        left.applyPref()          // the folder opens the way it is remembered, not the way it was split
    }
    /// The view menu and Ctrl+1/2/3 change the focused pane, side by side or not.
    function setView(v) { win.pane.view = v }
    /// The button and Ctrl+4. `force` is the IPC's: a script may split two local folders.
    function toggleMirrorView(force) { if (win.split) win.leaveMirror(); else if (force || win.remoteOpen) win.enterMirror() }
    function swapPanes() { const l = left.uri, r = right.uri; if (!l || !r) return; left.open(r); right.open(l) }
    // A remote URI opened directly (breadcrumb, IPC, Show in folder) opens in Mirror view with its location's local path beside it.
    /// A server URI opened in one pane pairs itself with its location's local folder — on
    /// ARRIVING at the server, not on every step inside it: side by side turned off with the
    /// server left on its own must stay off while folders in it are opened (owner, 2026-09-21:
    /// "turned off side by side, double clicked a folder — it went back into side by side").
    property string _leftHost: ""
    function _hostOf(uri) { const m = (uri || "").match(/^([a-z]+):\/\/([^/]+)/); return m && m[1] !== "file" && m[1] !== "trash" ? m[1] + "://" + m[2] : "" }
    // Leaving side by side can swap the two pane objects, so "the left pane" is another one now.
    onLeftChanged: _leftHost = _hostOf(left.uri)
    Connections { target: win.left; function onNavigated(uri) {
        const from = win._leftHost, m = uri.match(/^([a-z]+):\/\/([^/]+)/)
        win._leftHost = win._hostOf(uri)
        if (!win._leftHost || win._leftHost === from || win.sideBySide || win.left.hasPref) return
        if (Kiki.Settings.view.smartView === false) return
        const loc = win.locations.find(l => l.plugin === m[1] && l.name === m[2]); if (!loc) return
        // The pane that just listed the server BECOMES the right pane — the objects change
        // places, as they do on leaving — and the local folder opens in the other. It used to
        // open the server again in the right pane and the local folder over it in the left:
        // one listing wasted, and the selection and history the arrival had just made lost.
        const arrived = win.left
        win.lastLocation = loc; win._enterSplit()
        if (win.left === arrived) win._swapPaneObjects()      // `_enterSplit` may have swapped them back already
        if (loc.localUri) win.left.open(loc.localUri); win.focusPane(win.right) } }
    // Last mirror time per remote root, kept in settings so the bar can say "last mirrored 2 h ago".
    property var lastMirror: Kiki.Settings.mirror && Kiki.Settings.mirror.last ? Kiki.Settings.mirror.last : ({})
    function remoteUri() { return left.uri.startsWith("file://") ? right.uri : left.uri }
    /// The server a pane is on, "" when neither is: what the toolbar's Disconnect names.
    function remoteHost() { const u = [left.uri, right.uri].find(x => x !== "" && !/^(file|trash):/.test(x)); return u ? Kiki.Format.authority(u) : "" }
    /// Let go of that server. A pane standing on it goes to the folder the location is paired
    /// with, or home: left where it was it would only say the server had gone.
    function disconnectRemote() { disconnectLocation(remoteHost()) }
    /// Let go of a location by name: the toolbar's Disconnect and the sidebar's menu item both.
    function disconnectLocation(host) {
        if (!host) return
        const loc = win.locations.find(l => l.name === host)
        const to = loc && loc.localUri ? loc.localUri : "file://" + win.home
        for (const p of [left, right]) if (p.uri !== "" && Kiki.Format.authority(p.uri) === host && !/^(file|trash):/.test(p.uri)) p.open(to)
        Kiki.Daemon.request("Disconnect", { name: host }, (ok, err) => Kiki.Jobs.showToast({ text: err ? err.message : Kiki.T.tr("toast.disconnected", { host: host }), undoable: false }))
        // Side by side was that server and its folder: with the server gone it is one pane again.
        if (!remoteOpen) leaveMirror()
    }
    function localUri() { return left.uri.startsWith("file://") ? left.uri : right.uri }
    function recordMirror() { const m = Object.assign({}, lastMirror); m[remoteUri()] = Date.now(); lastMirror = m; Kiki.Daemon.request("SetSettings", { patch: { mirror: { last: m } } }) }
    function mirrorOptions(pos) {
        menu.open([
            { label: Kiki.T.tr("menu.mirrorUpload", { host: (remoteUri().match(/^[a-z]+:\/\/([^/]+)/) || [])[1] }), key: "Ctrl+M", action: () => win.startMirror(true) },
            { label: Kiki.T.tr("menu.mirrorDownload", { host: (remoteUri().match(/^[a-z]+:\/\/([^/]+)/) || [])[1] }), action: () => win.startMirror(false) },
            { id: "swapSides", label: Kiki.T.tr("menu.swapSides"), sep: true, action: () => win.swapPanes() },
            { id: "openRemoteAlone", label: Kiki.T.tr("menu.openRemoteAlone"), action: () => { const r = win.remoteUri(); win.left.view = "list"; win.left.open(r) } },
        ], pos)
    }
    function startMirror(upload) { if (!split) enterMirror(); mirrorWs.upload = upload; mirrorOpen = true }
    function otherPane() { return pane === left ? right : left }
    function transfer(move) { if (split) ops.transferTo(otherPane().uri, move) }
    onSplitChanged: if (!split) mirrorOpen = false
    property bool mirrorOpen: false
    // Leaving goes through the workspace's own `leave`, so Ctrl+M out of it puts it back at
    // Configure exactly as the Cancel button does — and not on the table of the last run.
    function toggleMirror() { if (!split) { startMirror(true); return } if (mirrorOpen) mirrorWs.leave(); else mirrorOpen = true }
    property string toast: ""
    // Search (plan 12). Folder scope filters the listing; other scopes open a results view.
    property Kiki.WindowCache results: Kiki.WindowCache { padAhead: 100; padBehind: 50 }
    // The field above the listing, opened by the toolbar's glass, "/" or Ctrl+F.
    property string searchScope: "everywhere"
    // Filtering (the strip above the listing) and global search (the overlay) are separate.
    property bool filterOpen: false
    property int filterTotal: 0
    /// What the filter narrows: the focused column in the columns view (0.1.1), the pane's own
    /// listing anywhere else. Asked each time rather than kept — the focus moves.
    function filterTarget() {
        const v = currentView()
        return (win.pane.view === "columns" && v && v.focusCache) ? v.focusCache : win.pane.listing
    }
    /// Its name for the bar's placeholder: "Filter src" in columns, "Filter this folder" elsewhere.
    function filterPlaceholder() {
        const v = currentView()
        if (win.pane.view === "columns" && v && v.focusUri) return Kiki.T.tr("filter.named", { name: decodeURIComponent(v.focusUri.replace(/\/+$/, "").split("/").pop() || "/") })
        return "Filter this folder"
    }
    /// The listing the bar's text is on right now. Kept, not asked for: the focus can move
    /// between typing a filter and clearing it (Left out of a filtered column), and the clear
    /// has to reach the listing that was narrowed, not the one the keyboard is in now.
    property var filteredCache: null
    function applyFilter(text) {
        const t = filterTarget()
        // Already in force: the bar's debounce and a direct call (the `search` IPC, closing the
        // bar) both land here, and the second used to send the same filter again.
        if (text === win.pane.filterText && filteredCache === (text ? t : null)) return
        if (filteredCache && filteredCache !== t && filteredCache.filter) filteredCache.filter("")
        filteredCache = text ? t : null
        win.pane.filterText = text
        t.filter(text)
    }
    function openFilter() {
        filterTotal = filterTarget().count
        filterOpen = true
        const bar = win.pane === win.right ? rightFilter : leftFilter
        bar.focusInput()
    }
    function closeFilter() { filterOpen = false; leftFilter.clear(); rightFilter.clear(); applyFilter(""); keys.forceActiveFocus() }
    function openSearch(seed) { searchOverlay.open(seed !== undefined ? seed : "") }
    /// The rail's Search entry: up if it is down, and down if it is up.
    function toggleSearch() { if (searchOverlay.visible) searchOverlay.close(); else openSearch(leftFilter.text) }
    function runSearch(text, scope) {
        searchScope = scope
        if (!text) return
        if (!results.lid) { results.lid = Kiki.Daemon.allocLid(); Kiki.Daemon.bind(results.lid, results) }
        const req = { lid: results.lid, scope: scope === "everywhere" ? "everywhere" : "location", query: text, mode: "substring" }
        if (scope !== "everywhere") { const l = locations.find(x => x.name === scope); if (l) req.uri = l.remoteUri }
        Kiki.Daemon.request("Search", req, (ok, err) => {
            if (err) { indexInfo = err.message; return }
            // No `indexAge` at all: the index has never been built (it used to read as 29 million minutes)
            indexInfo = scope === "everywhere" ? (ok.indexAge === undefined ? Kiki.T.tr("search.noIndex") : Kiki.T.tr("search.indexAge", { n: Math.round(ok.indexAge / 60) })) + (ok.capped ? Kiki.T.tr("search.capped") : "") : ""
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
        // `tool`, not `id`: the request's own id is a number the daemon replies with, and a
        // second `id` beside it took its place — every Alt+Enter was answered "missing id".
        Kiki.Daemon.request("OpenIn", { tool: tool.id, uris: target }, (ok, err) => { if (err) Kiki.Jobs.showToast({ text: err.message, undoable: false }) })
    }
    function editSelected() {
        const uris = selectedUris(); if (!uris.length) return
        const r = selectedRow()
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
            else Kiki.Jobs.showToast({ text: Kiki.T.tr("toast.projectRoleFailed", { role: Kiki.T.tr("role." + role), error: (err && err.message) || Kiki.T.tr("error.unknown") }), undoable: false })
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
        Kiki.Daemon.request("Arrange", { layout: "project", root: projectRoot, windows: windows, leftWidth: Kiki.Settings.project.width || 320 }, (ok, err) => { if (ok && ok.missing.length) Kiki.Jobs.showToast({ text: Kiki.T.tr("toast.couldNotPlace", { names: ok.missing.join(", ") }), undoable: false }) })
    }
    // The tree took the keyboard when project mode began (`focus: win.projectMode`, and it forces
    // the focus as it appears); hiding it hands the focus to nobody, so every shortcut — Escape
    // included — was dead until something was clicked. Give the keymap its focus back by hand.
    function leaveProject() { projectMode = false; win.width = savedWidth; pane.open(projectRoot); keys.forceActiveFocus() }

    // Share (plan 18)
    property var sharePlugins: []
    function loadShare() { Kiki.Daemon.request("SharePlugins", {}, ok => { if (ok) sharePlugins = ok.plugins.filter(p => p.enabled !== false) }) }
    /// One entry per share plugin, with its own icon. A plugin that names no targets is sent to
    /// at once — Mail opens a composer with the files attached, and a form in front of that only
    /// asks for what the composer is about to ask for.
    /// `of` names what is to be sent when it is not the selection — the row a columns menu was
    /// raised over.
    function shareItems(of) {
        const uris = of || selectedUris()
        // Each way of sending is a row of the menu itself — "Send via Tailscale ▸" — rather than
        // all of them behind one "Share ▸": it is one level less to the device, and a plugin's
        // targets fit in the one submenu the menu has. They are asked for when the row is opened.
        // One that cannot work here — its program is not installed — stays in the menu, dimmed,
        // saying what is missing: it tells the user the thing exists and what it would take.
        const items = sharePlugins.map(p => {
            const it = { label: Kiki.T.tr("menu.sendVia", { name: p.name }), icon: p.icon || "share", enabled: uris.length > 0 && !p.unavailable, key: p.unavailable ? Kiki.T.tr("menu.notInstalled") : "" }
            if (p.unavailable) return it
            if (p.targets === "none") { it.action = () => { if (uris.length) shareNow(p, null, uris) }; return it }
            it.items = [{ id: "looking", label: Kiki.T.tr("menu.looking"), enabled: false, action: () => {} }]
            it.load = fill => shareTargetItems(p, uris, fill)
            return it
        })
        if (items.length) items[0].sep = true
        return items
    }
    function shareMenu() {
        if (!selectedUris().length) return
        const items = shareItems()
        menuUnder(toolbar.viewButton, items.length ? items : [{ id: "noSharePlugins", label: Kiki.T.tr("menu.noSharePlugins"), enabled: false, action: () => {} }], true)
    }
    /// Share without asking anything first. If the plugin needs more than the files — an SMTP
    /// account wants a recipient — it says so, and the sheet opens to collect it.
    function shareNow(p, target, uris) {
        Kiki.Daemon.request("Share", { plugin: p.id, uris: uris, target: target ? target.id : undefined, compose: {} }, (ok, err) => {
            if (err) shareSheet.open(p, target, uris)
        })
    }
    /// For scripts (`shell share <plugin>`): the targets as a menu of their own.
    function shareTargets(p, uris) { shareTargetItems(p, uris, items => menuUnder(toolbar.viewButton, items, true)) }
    /// A share plugin's targets as menu items: online ones first as the plugin sorted them,
    /// offline ones greyed, and the reason when there are none.
    function shareTargetItems(p, uris, fill) {
        Kiki.Daemon.request("ShareTargets", { plugin: p.id }, (ok, err) => {
            if (err) { fill([{ label: err.message, enabled: false, action: () => {} }]); return }
            const items = ok.targets.map(t => ({ label: t.name + (t.detail ? "  ·  " + t.detail : ""), key: t.online ? "" : Kiki.T.tr("menu.offline"), icon: t.icon || p.icon || "share", enabled: t.online, action: () => shareSheet.open(p, t, uris) }))
            fill(items.length ? items : [{ id: "nothingFound", label: Kiki.T.tr("menu.nothingFound"), enabled: false, action: () => {} }])
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
            { id: "openAiHere", label: Kiki.T.tr("menu.openAiHere"), key: keymap.chordFor("ai"), sep: true, enabled: local, action: () => win.openAiHere(dir, files) },
            { id: "openTerminalHere", label: Kiki.T.tr("menu.openTerminalHere"), enabled: local, action: () => win.openTerminalHere(dir) },
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
    // In columns the filter is the focused column's: the keyboard leaving that column takes the
    // filter with it (0.1.1, D2) — a column never shows fewer rows than it has without the bar.
    Connections {
        target: win.pane.view === "columns" ? win.currentView() : null
        ignoreUnknownSignals: true
        function onFocusColChanged() { if (win.filterOpen) win.closeFilter() }
    }
    // `RepoChanged` comes once per listing that read the repo — the other pane's, every column's —
    // and each used to reload the same repo state; the pane's own listing's word is enough.
    Connections { target: Kiki.Daemon; function onEvent(msg) { if (msg.event === "RepoChanged" && (!msg.lid || msg.lid === win.pane.listing.lid)) win.loadRepo(); if (msg.event === "OpenInChanged") win.loadOpenIn(); if (msg.event === "ShowChooser") portal.open(msg); if (msg.event === "ShowItems") win.showItems(msg) } }
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
    function setInspectorWidth(w, room) { inspectorW = Math.max(inspectorPanel.minWidth, Math.min(Math.floor(room * 0.7), Math.round(w))) }
    property string inspectedUri: ""
    property var inspectedRow: null
    /// The rows of a selection of more than one: what the info panel shows instead of the
    /// current row (0.1.1). Empty for one row or none.
    property var inspectedRows: []
    Connections {
        target: win.pane.selection
        function onChanged() {
            if (!win.pane) return
            const p = win.pane.selection.current; const r = p >= 0 ? win.pane.listing.row(p) : null
            win.inspectedRow = r; win.inspectedUri = r ? win.pane.childUri(r.name) : ""
            const ps = win.pane.selection.positions()
            win.inspectedRows = ps.length > 1 ? ps.map(i => win.pane.listing.row(i)).filter(x => x) : []
            win.quickLookFollow()
        }
    }
    // Quick Look (docs/0.2.0/05-quicklook.md): Space on a file opens it in a window of its own
    // beside this one, Space again closes it. While it is up it follows the selection — the
    // gallery's "step", not a second window — and its own j k and arrows ask this window to
    // move the selection. Columns keeps a selection of its own, so its highlight is watched too.
    function openQuickLook() {
        const r = win.selectedRow(), u = win.selectedUris()
        if (!r || r.isDir || !u.length) return false
        quickLookWin.show(r, u[0])
        return true
    }
    function toggleQuickLook() { if (quickLookWin.visible) quickLookWin.close(); else openQuickLook() }
    function quickLookFollow() {
        if (!quickLookWin.visible) return
        const r = win.selectedRow(), u = win.selectedUris()
        if (r && u.length && u[0] !== quickLookWin.uri) quickLookWin.show(r, u[0])
    }
    function quickLookState() {
        const on = quickLookWin.visible
        return { visible: on, uri: on ? quickLookWin.uri : "", kind: on ? quickLookWin.shown : "", face: on ? quickLookWin.face : "" }
    }
    Connections {
        target: win.pane.view === "columns" ? win.currentView() : null
        ignoreUnknownSignals: true
        function onInspectedUriChanged() { win.quickLookFollow() }
    }
    function submitChmod(uri, mode, recursive) { ops.chmod(uri, mode, recursive) }
    function submitChmodMany(mask, bits, recursive) { ops.chmodMany(win.selectedUris(), mask, bits, recursive) }

    // Operations (plan 04) live in Ops.qml, which knows nothing about windows and dialogs, so
    // the interaction tests can drive them; the window supplies the confirmation and wl-copy.
    Kiki.Ops {
        id: ops
        pane: win.pane
        onConfirmNeeded: (spec, reply) => confirm.ask(spec, reply)
        onFolderNeeded: (spec, reply) => portal.pick(
            { mode: "open", directory: true, title: spec.title, currentFolder: (spec.start || "").replace(/^file:\/\//, "") || win.home },
            uris => reply(uris && uris.length ? uris[0].replace(/\/+$/, "") : ""))
        // Which view will show a folder being made, so its new row can be named where the user is
        // looking. Columns answers for the column that folder is in; everywhere else nobody does,
        // and Ops falls back to the pane's own listing and list view's editor.
        onListingNeeded: (spec, reply) => { const c = win.columnsPane(); reply(c ? c.listingFor(spec.dest) : null) }
        onCopyText: text => Quickshell.execDetached(["wl-copy", text])
    }
    property alias clipboard: ops.clipboard
    function copySelection(cut) { ops.copySelection(cut, win.selectedUris()) }
    function paste() { ops.paste() }
    function trashSelection() { if (win.galleryPane()) win.galleryPane().keepPlace(); ops.trashSelection(win.selectedUris()) }
    /// Ctrl+Shift+N. The folder is made in the folder being worked in, which in columns is the key
    /// column's — not the one the pane is standing in, several columns back.
    function newFolder() { const c = win.columnsPane(); ops.newFolder(c ? c.newFolderUri() : "") }
    /// F2. Columns has an editor of its own, on the row the key column highlights: the pane's
    /// selection is not what that view shows, and switching to list view to rename would both
    /// leave the view and rename the wrong file.
    function renameSelected() { const c = win.columnsPane(); if (c) c.beginRename(); else ops.renameSelected() }
    /// Which row has the inline editor open, for scripts and tests; -1 when none.
    function renamingRow() { const c = win.columnsPane(); return c ? c.renamingIndex : pane.renamingIndex }
    function copyPath() { ops.copyPath(win.selectedUris()) }
    // View menu (plan 02): one toolbar button, the three views, then hidden files.
    // A menu hung under the toolbar item that opened it, in window coordinates.
    /// A menu hung under `item`. The toolbar's buttons stand at the right edge, so their menus
    /// hang with their right edge on the button's (owner, 2026-09-23); the path's hang from the
    /// left, where the crumb is. `place()` still keeps the box inside the window either way.
    function menuUnder(item, items, alignRight) {
        const p = item.mapToItem(menu.parent, alignRight ? item.width - menu.box.width : 0, item.height + 4)
        menu.open(items, Qt.point(p.x, p.y))
    }
    // Clicking the path offers the folders above this one, and the way into typing one.
    function pathMenu() {
        const crumb = activeCrumb()
        const items = crumb.ancestors().map(a => ({ label: a.label, action: () => win.pane.open(a.uri) }))
        items.push({ id: "typePath", label: Kiki.T.tr("menu.typePath"), key: "Ctrl+L", sep: items.length > 0, action: () => crumb.edit() })
        menuUnder(crumb, items)
    }
    /// The same menu for a pane's own header: the folders above, and typing a path.
    function paneHeaderPathMenu(target, crumb) {
        const items = crumb.ancestors().map(a => ({ label: a.label, action: () => target.open(a.uri) }))
        items.push({ id: "typePath", label: Kiki.T.tr("menu.typePath"), sep: items.length > 0, action: () => crumb.edit() })
        menuUnder(crumb, items)
    }
    /// The gear: settings, the keymap, and who made this.
    function gearItems() {
        return [
            { id: "settings", label: Kiki.T.tr("menu.settings"), key: keymap.chordFor("settings"), action: () => settingsWin.open("general") },
            { id: "shortcuts", label: Kiki.T.tr("menu.shortcuts"), key: keymap.chordFor("shortcuts"), action: () => keysWin.open() },
            { id: "about", label: Kiki.T.tr("menu.about"), sep: true, action: () => aboutDlg.open() },
        ]
    }
    function gearMenu() { menuUnder(toolbar.gearButton, gearItems(), true) }
    /// The toolbar folded: every button it hides, as one menu — the views as a submenu, the
    /// gear's rows at the bottom.
    function hamburgerItems() {
        const items = [
            { label: win.inspectorRequested ? Kiki.T.tr("menu.hideInfo") : Kiki.T.tr("menu.showInfo"), key: keymap.chordFor("inspector"), enabled: win.pane.view !== "columns" && win.inspectedUri !== "", action: () => win.inspectorRequested = !win.inspectorRequested },
        ]
        if (win.remoteOpen || win.split) items.push({ label: win.split ? Kiki.T.tr("menu.onePane") : Kiki.T.tr("menu.sideBySide"), key: keymap.chordFor("viewMirror"), action: () => win.toggleMirrorView() })
        items.push({ id: "view", label: Kiki.T.tr("menu.view"), sep: true, items: win.viewMenuItems() })
        items.push({ label: win.sidebarShown ? Kiki.T.tr("menu.hideFavorites") : Kiki.T.tr("menu.showFavorites"), key: keymap.chordFor("sidebar"), action: () => win.sidebarShown = !win.sidebarShown })
        const gear = win.gearItems(); gear[0].sep = true
        return items.concat(gear)
    }
    function hamburgerMenu(button) { menuUnder(button, hamburgerItems(), true) }
    /// The rows and their ticks are `viewmenu.js`'s (tested there); what each does is here.
    /// Ticked for the FOCUSED pane's view, side by side as well — each pane has its own. (The
    /// ticks used to be withheld when split: a leftover from when Mirror was itself a view and
    /// none of these was the current one.)
    function viewMenuItems() {
        const act = { gallery: () => win.enterGallery(), hidden: () => pane.setHidden(!pane.showHidden) }
        return ViewMenu.items(pane.view, pane.showHidden, Kiki.T.tr, pane.isLocal).map(it => Object.assign(it, { action: act[it.id] || (() => win.setView(it.id)) }))
    }
    function viewMenu() {
        const items = viewMenuItems()
        menuUnder(toolbar.viewButton, items, true)
    }
    // Sidebar keyboard focus (plan 23): Ctrl+B, then Up/Down/Enter, Esc back to the pane.
    property bool sidebarFocus: false
    function focusSidebar(on) { sidebarFocus = on; if (on && sidebarPanel.keyIndex < 0) sidebarPanel.keyIndex = 0; if (!on) sidebarPanel.keyIndex = -1 }
    // Open with (plans 02/03 and 14): one list holding the desktop entries for the file's MIME
    // type and kiki's own tools, so there is a single way to open something elsewhere.
    /// Open with is the desktop's applications. The terminal tools and agents of plan 14 are
    /// their own thing (`Alt+Enter`, the editor keys) and do not belong in a list of apps.
    function openWithItems() { return [{ id: "looking", label: Kiki.T.tr("menu.looking"), enabled: false, action: () => {} }] }
    function loadOpenWith(uris, apply) {
        apply(win.openWithItems())
        // A selection is offered what opens all of it, and the app is handed the lot.
        Kiki.Daemon.request("OpenWith", { uris: uris }, (ok, err) => {
            if (!ok) { apply([{ label: err ? err.message : "Nothing offered", enabled: false, action: () => {} }]); return }
            const apps = ok.apps.map(a => ({ label: a.name + (a.default ? "  ·  default" : ""), icon: "open",
                                             action: () => Kiki.Daemon.request("Launch", { app: a.id, uris: uris },
                                                 (r, e) => { if (e) Kiki.Jobs.showToast({ text: e.message, undoable: false }) }) }))
            apply(apps.length ? apps : [{ label: uris.length > 1 && !ok.mime ? "No application opens all of these" : "No application for this kind", enabled: false, action: () => {} }])
        })
    }
    function openWithMenu(pos) {
        const u = selectedUris(); if (!u.length) return
        win.loadOpenWith(u, list => {
            const items = list.length ? list : [{ id: "nothingToOpenWith", label: Kiki.T.tr("menu.nothingToOpenWith"), enabled: false, action: () => {} }]
            if (menu.visible) menu.items = items
            else if (pos) menu.open(items, pos)
            else menuUnder(toolbar.viewButton, items, true)
        })
    }
    // Trash view (plan 04): restore to the original path, delete for good, or empty everything.
    property var trashInfo: ({})
    function loadTrashInfo() { Kiki.Daemon.request("TrashInfo", {}, ok => { if (ok) { const m = {}; for (const it of ok.items) m[it.name] = it; win.trashInfo = m } }) }
    function trashNames() { return ops.selectedNames() }
    function restoreSelection() { ops.restoreSelection() }
    function deleteForever() { ops.deleteForever(win.selectedUris()) }
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
        win.loadOpenWith(uris, list => { win.openWithSub = list; if (menu.visible) menu.refill("openWith", list) })
        const folder = uri.replace(/\/[^/]*$/, "")
        return [
            { id: "open", label: Kiki.T.tr("menu.open"), key: "Enter", action: () => row && row.isDir ? win.pane.open(uri) : win.openExternal(uri) },
            { id: "openWith", label: Kiki.T.tr("menu.openWith"), items: win.openWithSub },
            { id: "getInfo", label: Kiki.T.tr("menu.getInfo"), key: "Ctrl+I", action: () => { win.inspectedUri = uri; win.inspectedRow = row; win.inspectorRequested = true } },
            { id: "copy", label: Kiki.T.tr("menu.copy"), key: "Super+C", sep: true, action: () => ops.copySelection(false, uris) },
            { id: "cut", label: Kiki.T.tr("menu.cut"), key: "Super+X", action: () => ops.copySelection(true, uris) },
            // The same rows, in the same order, as list view's menu (`contextItems`) — this one
            // had fallen behind it: no Paste, no New folder, no "Extract to…" and none of the
            // ways of sending. Paste goes into the folder the row is in; New folder goes where
            // Ctrl+Shift+N does, inside the row when the row is a folder.
            { id: "paste", label: Kiki.T.tr("menu.paste"), key: "Super+V", enabled: win.clipboard.uris.length > 0, action: () => ops.paste(folder) },
            // On the row the menu was raised over: the right click made that row the column's
            // highlighted one, as a right click does in a list.
            { id: "newFolder", label: Kiki.T.tr("menu.newFolder"), key: "Ctrl+Shift+N", sep: true, action: () => win.newFolder() },
            { id: "rename", label: Kiki.T.tr("menu.rename"), key: "F2", action: () => win.renameSelected() },
            { id: "compress", label: Kiki.T.tr("menu.compress"), action: () => compressDialog.open(uris, folder) },
            { id: "extractHere", label: Kiki.T.tr("menu.extractHere"), enabled: !!row && row.kind === "archive", action: () => Kiki.Jobs.submit({ op: "extract", archive: uri, dest: folder }) },
            { id: "extractTo", label: Kiki.T.tr("menu.extractTo"), enabled: !!row && row.kind === "archive", action: () => ops.extractTo(row.name, uri) },
            { id: "copyPath", label: Kiki.T.tr("menu.copyPath"), action: () => ops.copyPath(uris) },
            ...win.shareItems(uris),
            ...win.hereItems(row && row.isDir ? uri : folder, row && row.isDir ? [] : uris),
            { id: "trash", label: Kiki.T.tr("menu.trash"), key: "Del", danger: true, sep: true, action: () => ops.trashSelection(uris) },
        ]
    }
    /// The menu for the background of a folder — nothing under the pointer. The same list as a
    /// row's, so nothing moves about, with everything that needs a file already greyed out by
    /// `contextItems`; only the two actions that need somewhere to put things are re-aimed, since
    /// in columns view the folder clicked need not be the one the pane is on.
    function folderItems(folderUri) {
        pane.selection.clear()
        const items = win.contextItems(-1)
        // Named even when it IS the folder the pane is on: `New folder` untouched goes where the
        // KEY COLUMN is, which in columns is another folder entirely once one has been drilled
        // into — and the menu belongs to the column it was raised over, not to that one.
        if (folderUri) {
            for (const it of items) {
                if (it.id === "paste") it.action = () => ops.paste(folderUri)
                else if (it.id === "newFolder") it.action = () => ops.newFolder(folderUri)
            }
        }
        return items
    }
    function contextItems(index) {
        // A placeholder goes in at once so the submenu is never empty; the applications land a
        // moment later and replace it, even if the submenu is already showing.
        win.openWithSub = win.openWithItems()
        const chosen = win.selectedUris()
        if (chosen.length) win.loadOpenWith(chosen, list => { win.openWithSub = list; if (menu.visible) menu.refill("openWith", list) })
        const r = index >= 0 ? pane.listing.row(index) : null
        const sel = pane.selection.count() > 0
        if (pane.isTrash) {
            const info = r && win.trashInfo[r.name]
            return [
                { label: info ? Kiki.T.tr("menu.restoreTo", { path: Kiki.Format.display(info.path.replace(/\/[^/]*$/, "") || "/", win.home) }) : Kiki.T.tr("menu.restore"), key: "Enter", enabled: sel, action: () => win.restoreSelection() },
                { id: "copyPath", label: Kiki.T.tr("menu.copyPath"), enabled: sel && !!info, action: () => Quickshell.execDetached(["wl-copy", info.path]) },
                { id: "emptyTrash", label: Kiki.T.tr("menu.emptyTrash"), danger: true, sep: true, enabled: pane.listing.count > 0, action: () => win.emptyTrash() },
            ]
        }
        const items = [
            { id: "open", label: Kiki.T.tr("menu.open"), key: "Enter", enabled: sel, action: () => win.openSelected() },
            { id: "openWith", label: Kiki.T.tr("menu.openWith"), enabled: sel, items: win.openWithSub },
            { id: "getInfo", label: Kiki.T.tr("menu.getInfo"), key: "Ctrl+I", enabled: sel, action: () => win.inspectorRequested = true },
            { id: "copy", label: Kiki.T.tr("menu.copy"), key: "Super+C", sep: true, enabled: sel, action: () => win.copySelection(false) },
            { id: "cut", label: Kiki.T.tr("menu.cut"), key: "Super+X", enabled: sel, action: () => win.copySelection(true) },
            { id: "paste", label: Kiki.T.tr("menu.paste"), key: "Super+V", enabled: win.clipboard.uris.length > 0, action: () => win.paste() },
            { id: "newFolder", label: Kiki.T.tr("menu.newFolder"), key: "Ctrl+Shift+N", sep: true, action: () => win.newFolder() },
            { id: "rename", label: Kiki.T.tr("menu.rename"), key: "F2", enabled: sel && pane.selection.count() === 1, action: () => win.renameSelected() },
            { id: "compress", label: Kiki.T.tr("menu.compress"), enabled: sel, action: () => compressDialog.open(win.selectedUris(), pane.uri) },
            { id: "extractHere", label: Kiki.T.tr("menu.extractHere"), enabled: !!r && r.kind === "archive", action: () => ops.extractHere(r.name) },
            { id: "extractTo", label: Kiki.T.tr("menu.extractTo"), enabled: !!r && r.kind === "archive", action: () => ops.extractTo(r.name) },
            { id: "copyPath", label: Kiki.T.tr("menu.copyPath"), enabled: sel, action: () => win.copyPath() },
            ...win.shareItems(),
            ...win.hereItems(r && r.isDir && pane.selection.count() === 1 ? pane.childUri(r.name) : pane.uri, win.selectedUris().filter(u => !(r && r.isDir && pane.selection.count() === 1))),
            { id: "trash", label: Kiki.T.tr("menu.trash"), key: "Del", danger: true, sep: true, enabled: sel, action: () => win.trashSelection() },
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
    /// What the operations act on. Columns keeps a selection of its own — the highlighted row in
    /// the key column, in a folder the pane need not be standing in — so while it is showing, it
    /// is asked instead of the pane. Everything destructive comes through here.
    function selectedUris() { const c = win.columnsPane(); return c ? c.selectedUris() : ops.selectedUris() }
    /// The row that goes with it, for the callers that need to know what KIND of thing is chosen.
    /// Asked of the same place as the URIs: a URI from the columns paired with a row from the
    /// pane's own selection is two different files.
    function selectedRow() { const c = win.columnsPane(); return c ? c.selectedRow() : pane.listing.row(pane.selection.current) }
    /// The context menu for what is chosen now — the Menu key's list, and the one the IPC acts
    /// through. In columns the chosen row can live in a folder the pane is not standing in, so it
    /// is asked for by URI, exactly as the right click on that row is; with nothing highlighted
    /// there, the menu is the key column's folder's, as a right click on its empty space gives.
    function contextItemsNow() {
        const c = win.columnsPane()
        if (!c) return win.contextItems(pane.selection.current)
        const uris = c.selectedUris(), r = c.selectedRow()
        if (uris.length && r) return win.contextItemsForUri(uris[0], r)
        return win.folderItems(c.columns.length ? c.columns[c.focusCol].uri : pane.uri)
    }
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
        else if (!pane.isLocal) win.openQuickLook()          // a server's file: looked at, not opened
        else openExternal(pane.childUri(r.name))
    }
    /// A double-click on a local file: the daemon opens it with the default application for its
    /// type (0.2.0). It used to be `xdg-open` on the URI here. A file on a server goes to Quick
    /// Look instead (owner, 2026-09-25); asked anyway, the daemon says 1330.
    function openExternal(uri) { Kiki.Daemon.request("OpenDefault", { uri: uri }, (ok, err) => { if (err) Kiki.Jobs.showToast({ text: err.message, undoable: false }) }) }
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
        case "project": if (win.projectMode) win.leaveProject(); else { const u = win.selectedUris(), r = win.selectedRow(); win.enterProject(u.length && r && r.isDir ? u[0] : win.pane.uri) } return true
        case "terminal": if (pane.uri.indexOf("file://") !== 0) return false; win.openTerminalHere(pane.uri); return true
        case "eject": { const d = win.devices.find(d => pane.uri.startsWith(d.uri.replace(/\/$/, ""))); if (!d) return false; Kiki.Daemon.request("Eject", { uri: d.uri }); return true }
        }
        return false
    }
    // ------------------------------------------------ the keys that belong to the view
    // The arrows and the Vim letters mean the same thing: h j k l are Left Down Up Right (0.2.0,
    // owner: "make defaults for nav the vim ones — remove preference"). Bare letters are commands
    // here, so there is no type-ahead; `/` and `f` filter.
    /// `v`: extending, as if Shift were held — j k, the arrows, Home and End grow the selection
    /// from where v was pressed, until Esc, until a folder is entered or left, or until the
    /// selection is used (copied, cut, trashed).
    property bool visual: false
    readonly property string paneUri: pane.uri
    onPaneUriChanged: visual = false
    function keyDown(shift) { if (win.sidebarFocus) sidebarPanel.moveKey(1); else if (win.galleryPane()) win.galleryPane().step(1); else if (win.columnsPane()) win.columnsPane().moveKey(1); else win.moveSelection(win.rowStep, shift || win.visual) }
    function keyUp(shift) { if (win.sidebarFocus) sidebarPanel.moveKey(-1); else if (win.galleryPane()) win.galleryPane().step(-1); else if (win.columnsPane()) win.columnsPane().moveKey(-1); else win.moveSelection(-win.rowStep, shift || win.visual) }
    function keyLeft() { win.visual = false; if (win.galleryPane()) { if (!win.galleryPane().step(-1)) pane.up() } else if (win.columnsPane()) { if (!win.columnsPane().focusLeft()) pane.up() } else pane.up() }
    function keyRight() { win.visual = false; if (win.galleryPane()) win.galleryPane().step(1); else if (win.columnsPane()) win.columnsPane().focusRight(); else win.enterSelected() }
    function openMenuKey() { menu.open(win.contextItemsNow(), Qt.point(400, 200)) }
    /// `dd` trashes: the first d waits 600 ms for the second.
    property bool pendingD: false
    Timer { id: ddTimer; interval: 600; onTriggered: win.pendingD = false }
    /// The Vim letters, bare (no Ctrl, no Alt): whether `key` was one of them. In the gallery
    /// its own bare keys keep their meaning (f is the filmstrip there).
    function vimKey(key, shift) {
        if (key !== Qt.Key_D) pendingD = false
        switch (key) {
        case Qt.Key_J: keyDown(shift); return true
        case Qt.Key_K: keyUp(shift); return true
        case Qt.Key_H: keyLeft(); return true
        case Qt.Key_L: keyRight(); return true
        case Qt.Key_V: visual = !visual; if (visual && pane.selection.current >= 0 && !Object.keys(pane.selection.rows).length) pane.selection.set(pane.selection.current); return true
        case Qt.Key_Y: visual = false; return runAction("copy")
        case Qt.Key_X: visual = false; return runAction("cut")
        case Qt.Key_P: return runAction("paste")
        case Qt.Key_R: return runAction("rename")
        case Qt.Key_Z: return runAction(shift ? "redo" : "undo")
        case Qt.Key_E: editSelected(); return true
        case Qt.Key_I: inspectorRequested = !inspectorRequested; return true
        case Qt.Key_F: if (galleryPane()) galleryPane().filmstrip = !galleryPane().filmstrip; else openFilter(); return true
        case Qt.Key_Colon: return runAction("typePath")
        case Qt.Key_M: openMenuKey(); return true
        case Qt.Key_Period: return runAction("hidden")
        case Qt.Key_D: if (pendingD) { pendingD = false; visual = false; return runAction("trash") } pendingD = true; ddTimer.restart(); return true
        }
        return false
    }
    function moveSelection(delta, extend) {
        const n = pane.listing.count; if (!n) return
        const cur = pane.selection.current < 0 ? (delta > 0 ? -1 : n) : pane.selection.current
        const next = Math.max(0, Math.min(n - 1, cur + delta))
        if (extend) pane.selection.range(next); else pane.selection.set(next)
        const v = win.currentView(); if (v && v.ensureVisible) v.ensureVisible(next)
    }
    function selectAt(index, extend) {
        const n = pane.listing.count; if (!n) return
        const i = Math.max(0, Math.min(n - 1, index))
        if (extend) pane.selection.range(i); else pane.selection.set(i)
        const v = win.currentView(); if (v && v.ensureVisible) v.ensureVisible(i)
    }
    /// Jobs a person would call running: not the machinery (an undo's inverse ops, a preflight).
    property int runningShown: 0
    Connections { target: Kiki.Jobs; function onChanged() { win.runningShown = Kiki.Jobs.running().filter(j => !j.hidden).length } }
    /// What the bottom bar says about the folder. In the gallery one thing is on the stage at a
    /// time, so it is where you are among them — "7 of 31" — rather than how many are selected.
    function countText() {
        const n = pane.listing.count, more = pane.listing.done ? "" : " " + Kiki.T.tr("count.more")
        if (pane.filterText) return Kiki.T.tr("count.match", { n: n })
        if (pane.view === "gallery" && !win.split && pane.selection.current >= 0 && n > 0) return Kiki.T.tr("count.ofN", { i: pane.selection.current + 1, n: n }) + more
        return Kiki.T.tr("count.items", { n: n }) + more + (pane.selection.count() ? " · " + Kiki.T.tr("count.selected", { n: pane.selection.count() }) : "")
    }
    /// The focused pane's view item when it is the gallery, else null.
    /// The view item the focused pane is showing, whichever side it is on.
    function currentView() { return (win.pane === win.right && rightLoader.item) ? rightLoader.item : viewLoader.item }
    function galleryPane() {
        const v = (win.pane === win.right && rightLoader.item) ? rightLoader.item : viewLoader.item
        return v && v.step ? v : null
    }
    property string galleryFrom: "icon"
    /// Not on a server: the menu greys the gallery out there, and the key and the arrow into a
    /// picture follow the menu.
    function enterGallery() { if (!pane.isLocal) return; if (pane.view !== "gallery") { galleryFrom = pane.view; pane.view = "gallery" } }
    /// The focused pane's view item when it is the columns view, else null.
    function columnsPane() {
        const v = (win.pane === win.right && rightLoader.item) ? rightLoader.item : viewLoader.item
        return v && v.focusLeft ? v : null
    }
    /// The focused pane's view item when it is the list, else null.
    // By `columnWidths`, which only the list has: columns view resizes its columns too, and has a
    // `setColumnWidth` of its own.
    function listPane() { const v = currentView(); return v && v.columnWidths ? v : null }
    /// The list's column widths, by role, or nothing when the focused pane is not showing a list.
    /// The widths themselves are global (settings.toml), so both panes always agree on them.
    function listColumnWidths() { const l = listPane(); return l ? l.columnWidths() : ({}) }
    // Up/Down step one row: in the icon grid that is one row of tiles, elsewhere one entry.
    // The view of the pane that has the focus — as a property, so what hangs from it follows a
    // change of pane. The keys scrolled, and took their step from, the LEFT view whichever pane
    // they were typed into: side by side, the right pane's selection walked out of sight and
    // its list never followed (owner: "on the remote side, I can't scroll to top/bottom items").
    readonly property var activeView: (win.pane === win.right && rightLoader.item) ? rightLoader.item : viewLoader.item
    readonly property int rowStep: activeView && activeView.perRow ? activeView.perRow : 1
    readonly property int pageStep: activeView && activeView.pageSize ? activeView.pageSize : 20

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
        objectName: "shell-keys"
        anchors.fill: parent
        focus: !win.filterOpen && !searchOverlay.visible && !toolbar.breadcrumb.editing && !leftHeader.breadcrumb.editing && !rightHeader.breadcrumb.editing && !menu.visible && !settingsWin.visible
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
            // Whatever is left is contextual — the arrows and the Vim letters, Enter, Backspace —
            // and belongs to the view.
            const action = keymap.idFor(event.key, event.modifiers, event.text)
            if (action && win.runAction(action)) { event.accepted = true; return }
            // The letters are commands only bare: with Ctrl or Alt they are chords, and a chord
            // the table does not have means nothing.
            if (!ctrl && !alt && win.vimKey(event.key, shift)) { event.accepted = true; return }
            switch (event.key) {
            // Bare keys are free in the gallery: it has no list to move about in.
            case Qt.Key_1: if (win.galleryPane()) win.galleryPane().actual(); else return; break
            case Qt.Key_0: if (win.galleryPane()) win.galleryPane().fit(); else return; break
            case Qt.Key_Plus: case Qt.Key_Equal: if (win.galleryPane()) win.galleryPane().zoomBy(1.25); else return; break
            case Qt.Key_Minus: if (win.galleryPane()) win.galleryPane().zoomBy(0.8); else return; break
            // In the gallery Space steps, as it always has; elsewhere it is Quick Look, on and off.
            case Qt.Key_Space: if (win.galleryPane()) win.galleryPane().step(1); else if (quickLookWin.visible) quickLookWin.close(); else if (!win.openQuickLook()) return; break
            case Qt.Key_Down: win.keyDown(shift); break
            case Qt.Key_Up: if (alt) pane.up(); else win.keyUp(shift); break
            case Qt.Key_Home: win.selectAt(0, shift || win.visual); break
            case Qt.Key_End: win.selectAt(pane.listing.count - 1, shift || win.visual); break
            case Qt.Key_PageDown: win.moveSelection(win.pageStep, shift || win.visual); break
            case Qt.Key_PageUp: win.moveSelection(-win.pageStep, shift || win.visual); break
            case Qt.Key_Return: case Qt.Key_Enter: if (win.sidebarFocus) { sidebarPanel.activateKey(); win.focusSidebar(false); break } if (win.columnsPane()) { win.columnsPane().activateKey(); break } { const rr = pane.listing.row(pane.selection.current); if (rr && rr.isDir && !pane.isTrash) { win.openFolder(pane.childUri(rr.name)); break } } win.openSelected(); break
            case Qt.Key_Backspace: pane.up(); break
            // Left leaves a folder, Right enters one, whichever view is showing; with Alt they
            // walk the history. (In columns, Left first walks back through the columns the
            // inspector pushed off screen.)
            case Qt.Key_Left: if (alt) pane.back(); else win.keyLeft(); break
            case Qt.Key_Right: if (alt) pane.forward(); else win.keyRight(); break
            case Qt.Key_Menu: win.openMenuKey(); break
            // The mirror workspace answers Escape itself where it has something to stop — the
            // compare on its Preflight screen — and leaves it alone on its other screens.
            case Qt.Key_Escape: if (win.visual) win.visual = false; else if (activity.visible) activity.close(); else if (infoPopover.visible) win.inspectorRequested = false; else if (win.mirrorOpen && mirrorWs.escapeKey()) { /* the workspace stopped its compare */ } else if (win.sidebarFocus) win.focusSidebar(false); else if (win.galleryPane()) pane.view = win.galleryFrom; else pane.selection.clear(); break
            case Qt.Key_Tab: if (win.split) win.focusPane(win.otherPane()); else return; break
            default: return
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
        /// A drop, without a pointer: `uris` is one or more URIs separated by newlines — the very
        /// shape of a `text/uri-list`, and not JSON, because Quickshell's IPC eats square brackets
        /// out of an argument. `dest` is the folder it lands in — `trash:///` is the sidebar's
        /// Trash — and `modifiers` any of ctrl/shift/alt. It goes through the same
        /// `Pane.dropInto` a real drag does, with an event shaped as Qt shapes one (`fakeDrop`: the
        /// keys folded into `proposedAction`), so a flow drives the code a hand drives, less the
        /// press and the pointer. Answers what the drop decided.
        function drop(uris: string, dest: string, modifiers: string): string { return win.fakeDrop(win.pane, uris, dest, modifiers) }
        /// The same, into a pane named `left` or `right` rather than the focused one — the pane a
        /// drop lands in takes the focus, and this is how a flow sees that happen. Naming a pane
        /// is also how a flow drops on the trash VIEW's folder rather than on the sidebar's
        /// Trash: the same `trash:///`, two different targets, and this one is refused.
        function dropOn(side: string, uris: string, dest: string, modifiers: string): string {
            return win.fakeDrop(side === "right" ? win.right : win.left, uris, dest, modifiers, true)
        }
        /// The yes/no question, for scripts and tests: `yes` or `no` answers it, anything else
        /// leaves it up. Either way, answers what it was asking.
        function question(answer: string): string {
            const was = { open: confirm.visible, title: confirm.title, message: confirm.message, label: confirm.confirmLabel, danger: confirm.danger }
            if (confirm.visible && (answer === "yes" || answer === "no")) confirm.answer(answer === "yes")
            return JSON.stringify(was)
        }
        /// A rebindable action by its keymap id (`trash`, `deleteForever`, …): what its key does,
        /// for a flow that has no keyboard.
        function action(id: string): void { win.runAction(id) }
        function back(): void { win.pane.back() }
        function forward(): void { win.pane.forward() }
        function setView(v: string): void { win.pane.view = v }
        function search(text: string): void { if (text) win.openFilter(); const bar = win.pane === win.right ? rightFilter : leftFilter; bar.text = text; win.applyFilter(text) }
        function searchEverywhere(text: string): void { if (searchOverlay.visible && !text) searchOverlay.close(); else win.openSearch(text) }
        /// A selection of several, by name, comma-separated; the last named is the current row.
        function selectMany(names: string): void {
            const want = names.split(","), at = []
            for (let i = 0; i < win.pane.listing.count; i++) { const r = win.pane.listing.row(i); if (r && want.indexOf(r.name) >= 0) at.push(i) }
            if (at.length) win.pane.selection.setMany(at, at[at.length - 1])
        }
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
        /// Quick Look, for the harness: `open` is Space on the selected file, `close` is Space
        /// again (or Esc in the window), `toggle` is either; `step <n>` is j or k inside the
        /// window; `key <name>` is a key pressed in it — escape, space, j, k. Answers what the
        /// window shows, as `state` carries it.
        function quickLook(action: string): string {
            const a = (action || "").trim().split(/\s+/)
            if (a[0] === "open") win.openQuickLook()
            else if (a[0] === "close") quickLookWin.close()
            else if (a[0] === "toggle") win.toggleQuickLook()
            else if (a[0] === "step") quickLookWin.step(parseInt(a[1]) || 1)
            else if (a[0] === "key") { const k = { escape: Qt.Key_Escape, space: Qt.Key_Space, j: Qt.Key_J, k: Qt.Key_K, down: Qt.Key_Down, up: Qt.Key_Up, left: Qt.Key_Left, right: Qt.Key_Right }[a[1]]; if (k !== undefined) quickLookWin.handleKey(k, 0) }
            return JSON.stringify(win.quickLookState())
        }
        /// The mirror workspace (plan 08's four screens) without a pointer. One word and its
        /// arguments, space-separated, because Quickshell's IPC hands a function strings:
        ///   `open` / `download`   start a run from the local or the remote side
        ///   `set <option> <value>`  direction, detector, deletes, filters, window, windowValue,
        ///                           windowUnit, `offset auto` | `offset <hours>` — what the
        ///                           Configure screen's controls set
        ///   `preflight`           the Preflight button: scan, then Review
        ///   `tab <all|new|changed|equal|delete>`  a Review tab
        ///   `check <row> <on|off>`  a click on a row's box
        ///   `report`              what "Save report…" saves, into `report` below
        ///   `run`                 the Mirror button; `confirm yes|no` answers the large-delete
        ///                         question it may ask
        ///   `cancel`              the Cancel button: the compare on Preflight, the run on Running
        ///   `escape`              what the Escape key does — Cancel on Preflight, nothing else
        ///   `back` / `close`      Back, and the button that leaves
        ///   `rules`               read the filter rules, into `rules` below
        ///   `rules-edit`          the Edit rules… button
        ///   `rules-set <lines>`   the rules dialog's Done: one `<kind> <value>` per LINE, because
        ///                         IPC eats the quotes out of an argument; nothing at all is "no
        ///                         rules", which is not the same as the defaults
        ///   `rules-defaults`      Restore defaults, and Done
        ///   `rules-cancel`        the rules dialog's Cancel
        /// Each of them calls the function the click calls, so there is no second path to drift.
        /// Answers what the workspace is showing: screen, options, plan counts and rows, the
        /// question if it is up, the Done summary and the filter rules.
        function mirror(action: string): string {
            const a = (action || "").trim().split(/\s+/)
            switch (a[0]) {
            case "open": win.startMirror(true); break
            case "download": win.startMirror(false); break
            case "close": mirrorWs.leave(); break
            case "set": mirrorWs.setOption(a[1], a[2]); break
            case "preflight": mirrorWs.preflight(); break
            case "tab": mirrorWs.setTab(a[1] || "all"); break
            case "check": mirrorWs.toggleRow(parseInt(a[1]) || 0, a[2] !== "off"); break
            case "report": mirrorWs.fetchReport(); break
            // The button itself: the chooser comes up; `save <uri>` is what choosing that file does.
            case "saveReport": mirrorWs.saveReport(); break
            case "save": if (portal.visible) portal.finish([a[1]]); break
            case "run": mirrorWs.mirror(false); break
            case "confirm": mirrorWs.answerLargeDelete(a[1] === "yes"); break
            case "cancel": mirrorWs.stop(); break
            case "escape": mirrorWs.escapeKey(); break
            case "back": mirrorWs.back(); break
            // The filter rules: `rules` reads them, `rules-edit` is the Edit rules… button, and
            // `rules-set` is the dialog's Done. The rules come one per line rather than as JSON,
            // because IPC strips the quotes out of an argument — the same reason `drop` takes its
            // URIs newline-separated.
            case "rules": mirrorWs.loadRules(); break
            case "rules-edit": mirrorWs.editRules(); break
            case "rules-set": mirrorWs.setRules(action.replace(/^[ \t]*rules-set[ \t]*/, "")); break
            case "rules-defaults": mirrorWs.restoreRules(); break
            case "rules-cancel": mirrorWs.cancelRules(); break
            }
            return JSON.stringify(mirrorWs.info())
        }
        function mirrorScreen(): string { return win.mirrorOpen ? mirrorWs.screen : "" }
        function focusPane(side: string): void { win.focusPane(side === "right" ? win.right : win.left) }
        function transfer(kind: string): void { win.transfer(kind === "move") }
        function openLocation(name: string): void { const l = win.locations.find(x => x.name === name); if (l) win.openLocation(l) }
        /// The sidebar menu's Disconnect, by name; `locationDots` is which locations wear the green dot.
        function disconnectLocation(name: string): void { win.disconnectLocation(name) }
        function locationDots(): string { return JSON.stringify(win.locations.filter(l => l.connected).map(l => l.name)) }
        function state(): string {
            return JSON.stringify({ uri: win.pane.uri, view: win.pane.view, count: win.pane.listing.count, done: win.pane.listing.done, error: win.pane.listing.error, selection: win.selectedUris(), inspector: win.inspector, sidebar: win.sidebarShown, keyFocus: keys.activeFocus, filterOpen: win.filterOpen, searchOpen: searchOverlay.visible, settingsVisible: settingsWin.visible, menuVisible: menu.visible, clipboard: win.clipboard.uris, clipboardCut: win.clipboard.cut === true, renaming: win.renamingRow(),
                daemon: { ready: Kiki.Daemon.ready, connected: Kiki.Daemon.connected },
                dialogs: { confirm: confirm.visible, compress: compressDialog.visible, location: locationDialog.visible, integration: integrationDialog.visible, portal: portal.visible, share: shareSheet.visible }, split: win.split, infoPopover: infoPopover.visible, infoRows: win.inspectedRows.length, filter: win.pane.filterText, filterColumn: (win.pane.view === "columns" && win.currentView()) ? win.currentView().focusCol : -1, sort: [win.pane.sortRole, win.pane.sortOrder], toast: win.toast,
                listColumns: win.listColumnWidths(), quickLook: win.quickLookState() })
        }
        /// Side by side, for scripts and tests: `toggle`; `drag <px>` is what dragging the line
        /// between the panes to that x does, `end` lets go, `reset` is the double click.
        function sideBySide(action: string): string {
            if (action === "toggle") win.toggleMirrorView(true)
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
        /// A list column's width, for scripts and tests, following `columns info <px>`:
        /// `listColumn <role> <px>` is what dragging that column's edge to that width does — the
        /// same function, clamped the same way — `listColumn <role> reset` is the double click on
        /// it, and `listColumn` alone only reads. Answers the widths the list is drawing, name
        /// included; `shell state` carries them too.
        function listColumn(role: string, px: string): string {
            const l = win.listPane()
            if (l && role) {
                if (px === "reset") l.resetColumnWidth(role)
                else if (px !== "") { l.setColumnWidth(role, parseInt(px) || 0); l.endColumnResize() }
            }
            return JSON.stringify(win.listColumnWidths())
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
        function contextMenu(action: string): void { const it = win.contextItemsNow().find(i => i.id === action || i.label === action); if (it && it.enabled !== false && it.action) it.action() }
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
            const cols = win.columnsPane(); if (cols) cols.cancelRename()
            menu.close()
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
        id: content
        visible: !win.projectMode
        anchors.fill: parent
        // What the frosted layers (menus, the toast, the activity card, the chooser) blur behind
        // themselves: the content, and only the content — they are its siblings, never inside it.
        Component.onCompleted: Kiki.Theme.behind = content
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
            inspector: win.inspectorRequested
            inspectorAvailable: win.pane.view !== "columns" && win.inspectedUri !== ""
            onToggleInspector: win.inspectorRequested = !win.inspectorRequested
            remoteHost: win.split ? win.remoteHost() : ""
            remoteOpen: win.remoteOpen
            crumbUri: !win.split && win.activeView && win.activeView.shownUri ? win.activeView.shownUri : ""
            onDisconnect: win.disconnectRemote()
            locations: win.locations
            repo: win.repo
            onViewMenu: win.viewMenu()
            onPathMenu: win.pathMenu()
            onSettings: win.gearMenu()
            onHamburger: button => win.hamburgerMenu(button)
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
                onSaveWanted: (name, reply) => portal.pick({ mode: "save", title: Kiki.T.tr("dialog.saveReport"), currentFolder: win.home, currentName: name },
                                                           uris => reply(uris && uris.length ? uris[0] : ""))
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
                    width: Math.max(0, (win.split ? win.leftPaneWidth(parent.width) : parent.width) - (!win.split ? inspectorSlot.width : 0)); height: parent.height
                    UI.PaneHeader { objectName: "pane-header-left"; visible: win.split; width: parent.width; pane: win.left; view: viewLoader.item; home: win.home; id: leftHeader; onClicked: win.focusPane(win.left); onPathMenu: c => win.paneHeaderPathMenu(win.left, c) }
                    UI.FilterBar {
                        id: leftFilter
                        visible: win.filterOpen && win.pane === win.left
                        width: parent.width; pane: win.left; total: win.filterTotal
                        count: win.filterTarget().count; placeholder: win.filterPlaceholder()
                        onApply: text => win.applyFilter(text)
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
                        UI.PaneConnecting { anchors.fill: parent; z: 49; pane: win.left }
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
                    UI.PaneHeader { objectName: "pane-header-right"; width: parent.width; pane: win.right; view: rightLoader.item; home: win.home; id: rightHeader; onClicked: win.focusPane(win.right); onPathMenu: c => win.paneHeaderPathMenu(win.right, c) }
                    UI.FilterBar {
                        id: rightFilter
                        visible: win.filterOpen && win.pane === win.right
                        width: parent.width; pane: win.right; total: win.filterTotal
                        count: win.filterTarget().count; placeholder: win.filterPlaceholder()
                        onApply: text => win.applyFilter(text)
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
                        UI.PaneConnecting { anchors.fill: parent; z: 49; pane: win.right }
                        UI.PaneError { anchors.fill: parent; z: 50; pane: win.right; onRetry: win.right.listing.open(win.right.uri) }
                    }
                }
                // The panel slides in from the right and the view slides over to make room, rather
                // than both snapping (2026-09-23). The slot's width eases; the panel inside keeps its
                // full width, pinned to the slot's right edge, so it moves rather than squashes.
                Item {
                    id: inspectorSlot
                    // Not in side by side: there the info is a popover on the selected row (0.1.1).
                    readonly property bool shown: !win.split && win.pane.view !== "columns" && win.inspector
                    readonly property int full: Math.max(inspectorPanel.minWidth, Math.min(win.inspectorW, Math.floor(parent.width * 0.7)))
                    width: shown ? full : 0; height: parent.height
                    visible: width > 0; clip: true
                    Behavior on width { enabled: !inspectorPanel.resizing; NumberAnimation { duration: 160; easing.type: Easing.OutCubic } }
                UI.Inspector {
                    id: inspectorPanel
                    // Columns view supplies its own inspector column; icon and list show it with the selection.
                    anchors.right: parent.right
                    width: inspectorSlot.full; height: parent.height
                    uri: win.inspectedUri; row: win.inspectedRow; rows: win.inspectedRows; home: win.home
                    onClosed: win.inspectorRequested = false
                    onChmodMany: (mask, bits, recursive) => win.submitChmodMany(mask, bits, recursive)
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
        }
        UI.ShortcutBar {
            id: bar
            width: parent.width
            // Side by side, Ctrl+M is the thing the layout is for, so it leads the hints.
            // The chips are a preference, off by default (owner, 2026-09-25: "hide the shortcuts stuff
            // in the bottom bar"); messages roll into the bar either way.
            keys: Kiki.Settings.view.shortcutChips !== true ? [] : (win.split ? [{ key: "^M", label: Kiki.T.tr("chip.mirror") }] : []).concat([{ key: "h j k l", label: Kiki.T.tr("chip.move") },
                { key: "Enter", label: Kiki.T.tr("chip.open") }, { key: "←", label: Kiki.T.tr("chip.up") }, { key: "→", label: Kiki.T.tr("chip.into") },
                { key: "^I", label: Kiki.T.tr("chip.info") }, { key: "F2", label: Kiki.T.tr("chip.rename") }, { key: "Del", label: Kiki.T.tr("chip.trash") },
                { key: "❖C", label: Kiki.T.tr("chip.copy") }, { key: "❖V", label: Kiki.T.tr("chip.paste") }, { key: "/", label: Kiki.T.tr("chip.filter") },
                { key: "^?", label: Kiki.T.tr("chip.keys") }])
            statusInset: 38      // the orb stands at the right end
            status: (win.runningShown ? win.runningShown + " running · " : "") + win.countText()
            toast: Kiki.Jobs.toast
            onUndo: { Kiki.Jobs.undo(); Kiki.Jobs.dismissToast() }
            onDismiss: Kiki.Jobs.dismissToast()
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
        searchOpen: searchOverlay.visible
        onSearchRequested: win.toggleSearch()
        onEjectDevice: dev => Kiki.Daemon.request("Eject", { uri: dev.uri }, (ok, err) => { if (err) Kiki.Jobs.showToast({ text: Kiki.T.tr("toast.ejectFailed", { error: err.message }), undoable: false }) })
        onDeviceMenu: dev => menu.open([
            { id: "open", label: Kiki.T.tr("menu.open"), enabled: !dev.busy, action: () => win.pane.open(dev.uri) },
            { id: "eject", label: Kiki.T.tr("menu.eject"), key: "Ctrl+E", action: () => Kiki.Daemon.request("Eject", { uri: dev.uri }) },
            { label: dev.busy ? Kiki.T.tr("menu.inUseBy", { what: dev.busy }) : dev.kind.toUpperCase() + " · " + dev.vendor + " " + dev.model, enabled: false, sep: true, action: () => {} },
        ], Qt.point(40, 300))
        onOpen: uri => win.pane.open(uri)
        onAddLocation: locationDialog.open(null)
        onOpenLocation: loc => win.openLocation(loc)
        onDropOn: (uri, drop) => win.pane.dropInto(uri, drop)
        onDropOnTrash: uris => ops.trashSelection(uris)
        onFavoriteMenu: (index, pos) => menu.open([
            { id: "removeFromSidebar", label: Kiki.T.tr("menu.removeFromSidebar"), action: () => {
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
        onMountVolume: vol => Kiki.Daemon.request("Mount", { device: vol.device }, (ok, err) => { if (ok) win.pane.open(ok.uri); else Kiki.Jobs.showToast({ text: Kiki.T.tr("toast.mountFailed", { error: err ? err.message : "" }), undoable: false }) })
        onVolumeMenu: vol => menu.open([
            { label: vol.mounted === false ? Kiki.T.tr("menu.mount") : Kiki.T.tr("menu.open"), action: () => vol.mounted === false ? Kiki.Daemon.request("Mount", { device: vol.device }, ok => { if (ok) win.pane.open(ok.uri) }) : win.pane.open(vol.uri) },
            { id: "unmount", label: Kiki.T.tr("menu.unmount"), enabled: vol.mounted !== false && vol.uri !== "file:///", action: () => Kiki.Daemon.request("Unmount", { device: vol.device }, (ok, err) => { if (err) Kiki.Jobs.showToast({ text: Kiki.T.tr("toast.unmountFailed", { error: err.message }), undoable: false }) }) },
            { id: "eject", label: Kiki.T.tr("menu.eject"), enabled: !!vol.removable, action: () => Kiki.Daemon.request("Eject", { device: vol.device }, (ok, err) => { if (err) Kiki.Jobs.showToast({ text: Kiki.T.tr("toast.ejectFailed", { error: err.message }), undoable: false }) }) },
        ], Qt.point(40, 200))
        onEditLocation: loc => menu.open([{ id: "open", label: Kiki.T.tr("menu.open"), action: () => win.pane.open(loc.remoteUri) }, { id: "edit", label: Kiki.T.tr("menu.edit"), action: () => locationDialog.open(loc) }].concat(win.locationImageItems(loc)).concat([{ id: "connectionLog", label: Kiki.T.tr("menu.connectionLog"), sep: true, action: () => jobLog.openLocation(loc.name) }, { id: "disconnect", label: Kiki.T.tr("menu.disconnect"), action: () => win.disconnectLocation(loc.name) }, { id: "remove", label: Kiki.T.tr("menu.remove"), danger: true, sep: true, action: () => Kiki.Daemon.request("RemoveLocation", { name: loc.name }, () => win.loadSidebar()) }]), Qt.point(40, 200))
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
    UI.KeymapWindow { id: keysWin; parent: win.contentItem; keymap: keymap; onClosed: keys.forceActiveFocus() }
    UI.AboutDialog { id: aboutDlg; parent: win.contentItem; onClosed: keys.forceActiveFocus() }
    UI.ContextMenu { id: menu; parent: win.contentItem; onClosed: keys.forceActiveFocus() }
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
            { mode: "open", directory: true, title: Kiki.T.tr("dialog.chooseLocalFolder"), currentFolder: (start || "").replace(/^~/, win.home) || win.home },
            uris => { if (uris && uris.length) reply(decodeURIComponent(uris[0].replace(/^file:\/\//, "")).replace(/\/+$/, "") || "/"); locationDialog.forceActiveFocus() })
    }
    /// kiki's own chooser, asked for one picture; answers with a plain path.
    function pickLocationImage(start, reply) {
        const dir = start ? start.replace(/\/[^\/]*$/, "") : ""
        portal.pick({ mode: "open", title: Kiki.T.tr("dialog.chooseImage"), currentFolder: dir || win.home,
                      filters: [{ name: "Images", patterns: ["*.png", "*.jpg", "*.jpeg", "*.webp", "*.svg", "*.gif", "*.bmp"] }] },
            uris => { if (uris && uris.length) reply(decodeURIComponent(uris[0].replace(/^file:\/\//, ""))) })
    }
    /// The sidebar menu's part for a location's picture: set one, and take it off again.
    function locationImageItems(loc) {
        const set = image => Kiki.Daemon.request("SetLocationImage", { name: loc.name, image: image }, () => win.loadSidebar())
        const items = [{ label: loc.image ? Kiki.T.tr("menu.changeImage") : Kiki.T.tr("menu.setImage"), sep: true, action: () => win.pickLocationImage(loc.image || "", set) }]
        if (loc.image) items.push({ id: "removeImage", label: Kiki.T.tr("menu.removeImage"), action: () => set("") })
        return items
    }
    UI.PortalDialog { id: portal; objectName: "portal"; anchors.fill: parent; home: win.home; favorites: win.favorites; locations: win.locations }
    UI.SettingsWindow { id: settingsWin; parent: win.contentItem; onVisibleChanged: if (!visible) keys.forceActiveFocus() }
    UI.IntegrationDialog { id: integrationDialog; parent: win.contentItem }
    UI.ConfirmDialog { id: confirm; objectName: "confirm"; parent: win.contentItem; onVisibleChanged: if (!visible) keys.forceActiveFocus() }
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
    // The info panel in side by side (0.1.1): over the focused pane, pointing at its selected row.
    UI.InfoPopover {
        id: infoPopover
        anchors.fill: parent
        visible: win.split && win.inspector && win.pane.view !== "columns" && !win.mirrorOpen
        paneItem: win.pane === win.right ? rightCol : leftCol
        view: win.pane === win.right ? rightLoader.item : viewLoader.item
        rowIndex: win.pane.selection.current
        uri: win.inspectedUri; row: win.inspectedRow; rows: win.inspectedRows; home: win.home
        onClosed: win.inspectorRequested = false
        onChmod: (mode, recursive) => win.submitChmod(win.inspectedUri, mode, recursive)
        onChmodMany: (mask, bits, recursive) => win.submitChmodMany(mask, bits, recursive)
        onEdit: (u, line) => win.editAt(u, line)
        onOpen: u => win.openExternal(u)
    }
    UI.ActivityPopover {
        id: activity
        objectName: "activity"
        aimX: orb.x + orb.width / 2; aimY: orb.y
        onLogRequested: job => { activity.close(); jobLog.openJob(job) }
        onRevealRequested: uri => win.revealUri(uri)
    }
    UI.JobLogWindow { id: jobLog; objectName: "joblog"; onCopyText: text => Quickshell.execDetached(["wl-copy", text]) }
    // A second top-level window, declared here so it is this window's: Quickshell maps it when
    // `visible` goes true and unmaps it when it goes false, and maps it again on the next
    // (measured under 0.3.1 — a nested FloatingWindow, unlike the root, does come back).
    UI.QuickLookWindow {
        id: quickLookWin
        home: win.home; paneUri: win.pane.uri
        onStep: delta => win.moveSelection(delta, false)
        onOpenWith: win.openWithMenu()
        onDismissed: keys.forceActiveFocus()
    }
    /// Show a file where it is: its folder, with it selected.
    function revealUri(uri) {
        const parent = uri.replace(/\/[^/]*$/, "") || uri
        win.pane.open(parent)
        win.pane.selectAfterLoad = decodeURIComponent(uri.split("/").pop())
        win.selectCameFrom()
    }
    Component { id: columnsView; Views.ColumnsPane { pane: win.left; home: win.home; onActivate: uri => win.openExternal(uri); onEdit: (u, line) => win.editAt(u, line)
        onChmod: (uri, mode, recursive) => win.submitChmod(uri, mode, recursive)
        onChmodMany: (uris, mask, bits, recursive) => ops.chmodMany(uris, mask, bits, recursive)
        onContextMenu: (uri, row, pos) => { win.focusPane(pane); menu.open(win.contextItemsForUri(uri, row), pos) }
        onContextMenuFolder: (uri, pos) => { win.focusPane(pane); menu.open(win.folderItems(uri), pos) } } }
}
