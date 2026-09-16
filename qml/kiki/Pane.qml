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
        listing.open(target)
        if (sortRole !== "name" || sortOrder !== "asc") listing.sort(sortRole, sortOrder)
        navigated(target)
    }
    function canBack() { return historyIndex > 0 }
    function canForward() { return historyIndex < history.length - 1 }
    function back() { if (canBack()) { historyIndex--; open(history[historyIndex], false) } }
    function forward() { if (canForward()) { historyIndex++; open(history[historyIndex], false) } }
    function up() { const p = parentOf(uri); if (p) open(p) }
    function setSort(role, order) { sortRole = role; sortOrder = order; listing.sort(role, order) }
    function setFilter(text) { filterText = text; listing.filter(text) }

    function parentOf(u) {
        const i = u.indexOf("://"); const head = u.slice(0, i + 3); let rest = u.slice(i + 3)
        const slash = rest.indexOf("/"); const auth = slash < 0 ? rest : rest.slice(0, slash); let path = slash < 0 ? "/" : rest.slice(slash)
        if (path === "/" || path === "") return null
        path = path.replace(/\/+$/, ""); const k = path.lastIndexOf("/")
        return head + auth + (k <= 0 ? "/" : path.slice(0, k))
    }
    function childUri(name) { return uri.replace(/\/+$/, "") + "/" + encodeURIComponent(name).replace(/%2F/g, "/") }
}
