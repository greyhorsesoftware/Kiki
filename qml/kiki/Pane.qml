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
    function setHidden(show) { showHidden = show; listing.showHidden(show) }
    property var history: []
    property int historyIndex: -1
    property bool focused: false
    property int renamingIndex: -1
    signal renameRequested(string uri, string name)

    property Kiki.WindowCache listing: Kiki.WindowCache {}
    property Kiki.Selection selection: Kiki.Selection {}

    signal navigated(string uri)

    function open(target, push) {
        if (push === undefined) push = true
        if (push) {
            history = history.slice(0, historyIndex + 1).concat([target])
            historyIndex = history.length - 1
        }
        uri = target
        filterText = ""
        selection.clear()
        // Per-folder memory (plan 02): restore this folder's view and sort, else keep the current ones.
        const pref = Kiki.Settings.viewPref(target.replace(/\/+$/, "") || target)
        _applying = true
        if (pref) { if (pref.view && pref.view !== view) view = pref.view; sortRole = pref.sort || "name"; sortOrder = pref.order || "asc" }
        _applying = false
        listing.open(target)
        if (sortRole !== "name" || sortOrder !== "asc") listing.sort(sortRole, sortOrder)
        if (showHidden !== (Kiki.Settings.view.showHidden === true)) listing.showHidden(showHidden)
        navigated(target)
    }
    property bool _applying: false
    function _remember() { if (!_applying && uri) Kiki.Settings.setViewPref(uri.replace(/\/+$/, "") || uri, view, sortRole, sortOrder) }
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
    function childUri(name) { return uri.replace(/\/+$/, "") + "/" + encodeURIComponent(name).replace(/%2F/g, "/") }
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
