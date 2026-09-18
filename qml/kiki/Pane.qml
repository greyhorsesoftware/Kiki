import QtQuick
import "." as Kiki

// A pane is a view of one URI: its listing, history, view mode, sort and selection.
QtObject {
    id: pane
    property string uri: ""
    property string view: "list"          // icon | list | columns
    property string sortRole: "name"
    property string sortOrder: "asc"
    property string filterText: ""
    // Dot-files: starts from the setting, toggled per pane with Ctrl+H or the view menu.
    property bool showHidden: Kiki.Settings.view.showHidden === true
    function setHidden(show) { showHidden = show; listing.showHidden(show); _remember() }
    property var history: []
    property int historyIndex: -1
    property bool focused: false
    // Icon view zoom: pinch on a trackpad, or Ctrl and the wheel.
    property real iconZoom: 1
    property int renamingIndex: -1
    signal renameRequested(string uri, string name)

    property Kiki.WindowCache listing: Kiki.WindowCache {}
    property Kiki.Selection selection: Kiki.Selection {}

    signal navigated(string uri)

    /// Coming back out of a folder, the folder we left is the one worth selecting. The shell
    /// looks up its row once the listing has finished.
    property var visited: ({})        // folder uri -> the row that was selected there
    property string selectAfterLoad: ""
    /// Set by the keyboard paths into a folder: land on its first row so the arrows carry on.
    property bool selectFirstAfterLoad: false
    function open(target, push) {
        if (push === undefined) push = true
        const leaving = trimUri(uri)
        const dest = trimUri(target)
        // Remember what was selected here, so coming back lands on it again.
        const cur = selection.current
        const curRow = cur >= 0 ? listing.row(cur) : null
        if (leaving && curRow) { const v = Object.assign({}, visited); v[leaving] = curRow.name; visited = v }
        const child = leaving && parentOf(leaving) === dest ? decodeURIComponent(leaving.split("/").pop()) : ""
        selectAfterLoad = child || visited[dest] || ""
        selectFirstAfterLoad = false
        if (push) {
            history = history.slice(0, historyIndex + 1).concat([target])
            historyIndex = history.length - 1
        }
        uri = target
        filterText = ""
        selection.clear()
        // Per-folder memory (plan 02): restore this folder's view and sort, else keep the current ones.
        const pref = Kiki.Settings.viewPref(target.replace(/\/+$/, "") || target)
        hasPref = !!pref
        _applying = true
        if (pref) { if (pref.view && pref.view !== view) view = pref.view; sortRole = pref.sort || "name"; sortOrder = pref.order || "asc"; showHidden = pref.hidden !== undefined ? pref.hidden : (Kiki.Settings.view.showHidden === true) }
        else {
            showHidden = Kiki.Settings.view.showHidden === true
            const d = Kiki.Settings.view["default"] || "list"
            if (view !== d && view !== "mirror") view = d
        }
        _applying = false
        listing.open(target)
        // Always state the order: the daemon caches listings, so this folder may still carry the
        // order some earlier pane asked for. The daemon ignores a sort that is already in force.
        listing.sort(sortRole, sortOrder)
        if (showHidden !== (Kiki.Settings.view.showHidden === true)) listing.showHidden(showHidden)
        navigated(target)
    }
    property bool _applying: false
    property bool hasPref: false
    // Smart default (plan 24): with no memory for this folder, a picture folder opens in icon view.
    property string _smartChecked: ""
    readonly property var pictureNames: ["pictures", "photos", "dcim", "screenshots", "wallpapers", "camera", "camera roll"]
    function _smart() {
        if (_smartChecked === uri || hasPref || view === "mirror") return
        _smartChecked = uri
        const name = decodeURIComponent(uri.replace(/\/+$/, "").split("/").pop() || "").toLowerCase()
        let pick = pictureNames.indexOf(name) >= 0 ? "gallery" : ""
        if (!pick) {
            const n = Math.min(listing.count, 200); let held = 0, media = 0
            for (let i = 0; i < n; i++) { const r = listing.row(i); if (!r) continue; held++; if (r.kind === "image" || r.kind === "video") media++ }
            if (held >= 12 && media / held >= 0.6) pick = "gallery"
        }
        if (pick && pick !== view) { _applying = true; view = pick; _applying = false }
    }
    property Connections doneWatch: Connections { target: listing; function onDoneChanged() { if (listing.done) _smart() } }

    // Keeping the selection across a refresh. A job finishing, or anything else touching the
    // folder, makes the daemon re-send the listing, which clears the selection — so the file you
    // were about to rename is suddenly nothing. Remember the names and put them back.
    property var _keepNames: []
    property string _keepUri: ""
    property bool _restoring: false
    property bool _restorePending: false
    function selectedNames() {
        return selection.positions().map(p => { const r = listing.row(p); return r ? r.name : null }).filter(n => n)
    }
    function _restoreSelection() {
        if (!_restorePending) return
        if (listing.uri !== _keepUri || !_keepNames.length) { _restorePending = false; return }
        const want = _keepNames
        let found = []
        for (let i = 0; i < listing.count; i++) { const r = listing.row(i); if (r && want.indexOf(r.name) >= 0) found.push(i) }
        if (!found.length) return                     // rows not back yet; try again on the next batch
        _restoring = true
        selection.set(found[0])
        for (let j = 1; j < found.length; j++) selection.rows[found[j]] = true
        selection.changed()
        _restoring = false
        _restorePending = false
    }
    property Connections selWatch: Connections {
        target: pane.selection
        function onChanged() { if (!pane._restoring) { pane._keepNames = pane.selectedNames(); pane._keepUri = pane.listing.uri } }
    }
    property Connections keepWatch: Connections {
        target: pane.listing
        // A reset from the watcher stays in the same folder; one from navigating does not, and
        // there the selection rules of `open` apply instead.
        function onReset() { pane._restorePending = pane.listing.uri === pane._keepUri && pane._keepNames.length > 0 }
        function onRowsUpdated(first, n) { pane._restoreSelection() }
    }
    function _remember() { if (!_applying && uri) Kiki.Settings.setViewPref(uri.replace(/\/+$/, "") || uri, view, sortRole, sortOrder, showHidden) }
    onViewChanged: _remember()
    function canBack() { return historyIndex > 0 }
    function canForward() { return historyIndex < history.length - 1 }
    function back() { if (canBack()) { historyIndex--; open(history[historyIndex], false) } }
    function forward() { if (canForward()) { historyIndex++; open(history[historyIndex], false) } }
    function up() { const p = parentOf(uri); if (p) open(p) }
    function setSort(role, order) { sortRole = role; sortOrder = order; listing.sort(role, order); _remember() }
    function setFilter(text) { filterText = text; listing.filter(text) }

    function parentOf(u) {
        const i = u.indexOf("://"); const head = u.slice(0, i + 3); let rest = u.slice(i + 3)
        const slash = rest.indexOf("/"); const auth = slash < 0 ? rest : rest.slice(0, slash); let path = slash < 0 ? "/" : rest.slice(slash)
        if (path === "/" || path === "") return null
        path = path.replace(/\/+$/, ""); const k = path.lastIndexOf("/")
        return head + auth + (k <= 0 ? "/" : path.slice(0, k))
    }
    function childUri(name) {
        const base = uri.endsWith("/") ? uri : uri + "/"
        return base + encodeURIComponent(name).replace(/%2F/g, "/")
    }
    /// Drop trailing slashes but keep the authority's own, so "file:///" survives.
    function trimUri(u) {
        const m = u.match(/^([a-z0-9+.-]+:\/\/[^/]*)(\/.*)?$/i)
        if (!m) return u.replace(/\/+$/, "")
        const p = (m[2] || "/").replace(/\/+$/, "")
        return m[1] + (p || "/")
    }
    readonly property bool isTrash: uri.startsWith("trash://")

    // Drag and drop (plan 02). The dragged payload is text/uri-list so drops also work from and
    // into other Wayland apps.
    function dragUris(index) {
        const rows = selection.has(index) ? selection.positions() : [index]
        return rows.map(i => { const r = listing.row(i); return r ? childUri(r.name) : null }).filter(u => u)
    }
    function dragMime(index) { return { "text/uri-list": dragUris(index).join("\r\n") + "\r\n" } }
    // Drop `drop` (a DragEvent) into `dest`: move within one scheme, copy across, Ctrl forces copy.
    function dropInto(dest, drop) {
        const urls = drop.hasUrls ? drop.urls.map(u => u.toString()) : (drop.hasText ? drop.text.split(/\r?\n/).filter(l => l && !l.startsWith("#")) : [])
        const items = urls.filter(u => u && parentOf(u) !== dest.replace(/\/+$/, "") && parentOf(u) + "/" !== dest && u.replace(/\/+$/, "") !== dest.replace(/\/+$/, ""))
        if (!items.length) { drop.accepted = false; return }
        const sameScheme = items.every(u => u.split("://")[0] === dest.split("://")[0])
        const copy = (drop.modifiers & Qt.ControlModifier) || drop.proposedAction === Qt.CopyAction && !sameScheme || !sameScheme
        drop.accept(copy ? Qt.CopyAction : Qt.MoveAction)
        Kiki.Jobs.submit({ op: copy ? "copy" : "move", items: items, dest: dest })
    }
}
