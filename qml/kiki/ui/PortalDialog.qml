import QtQuick
import ".." as Kiki
import "../views" as Views

// The Open / Save dialog kiki provides to other apps through the portal (plan 09).
Rectangle {
    id: dlg
    visible: false
    anchors.fill: parent; color: "transparent"; z: 95
    // The window behind, frosted and darkened: the chooser is over THIS folder, not over nothing.
    Frost { anchors.fill: parent; radius: 0; tint: "black"; tintOpacity: 0.45; blur: 0.8 }
    property var req: null            // the ShowChooser event
    property string home: ""
    property var favorites: []
    property var locations: []
    property int filterIndex: 0
    /// The window's DragGhost, handed on to this dialog's pane so a drag out of the chooser
    /// carries a picture like any other.
    property var ghost: null
    property Kiki.Pane pane: Kiki.Pane { view: "list"; ghost: dlg.ghost }
    /// The sidebar style the main window follows (Shell.qml): the rail unless set to traditional.
    readonly property bool railStyle: Kiki.Settings.view.sidebarStyle !== "traditional"
    /// The filter — `/`, as in the main window, and looking as it does there (owner, 2026-09-28):
    /// the same FilterBar across the top of the list, with its "n of m" count, its Escape, and
    /// its promotion to search everywhere. What is typed narrows the folder shown.
    property bool filterShown: false
    property int filterTotal: 0
    function openFilter() { filterTotal = pane.listing.count; filterShown = true; chooserFilter.focusInput() }
    function closeFilter() { filterShown = false; chooserFilter.clear(); pane.setFilter(""); dlg.forceActiveFocus() }

    // ---------------------------------------------------------------- search everywhere
    // The rail's Search entry is the main window's search-everywhere UI, not a box (owner,
    // 2026-09-28): the same overlay, over the index, with results of its own. A result that is
    // a file is the answer when the chooser is opening one; otherwise its folder is shown with
    // it selected, and a Save gets the name.
    property Kiki.WindowCache results: Kiki.WindowCache { padAhead: 100; padBehind: 50 }
    property string indexInfo: ""
    function runSearch(text, scope) {
        if (!text) return
        if (!results.lid) { results.lid = Kiki.Daemon.allocLid(); Kiki.Daemon.bind(results.lid, results) }
        Kiki.Daemon.request("Search", { lid: results.lid, scope: "everywhere", query: text, mode: "substring" }, (ok, err) => {
            if (err) { indexInfo = err.message; return }
            indexInfo = ok.indexAge === undefined ? Kiki.T.tr("search.noIndex") : Kiki.T.tr("search.indexAge", { n: Math.round(ok.indexAge / 60) })
        })
    }
    function toggleSearchEverywhere() { if (searchAll.visible) searchAll.close(); else searchAll.open("") }
    function fromSearch(uri, pick) {
        const folder = uri.endsWith("/") ? uri : (uri.replace(/\/[^/]*$/, "") || uri)
        const name = uri.endsWith("/") ? "" : decodeURIComponent(uri.split("/").pop())
        searchAll.close()
        if (pick && name && req && req.mode === "open" && !req.directory) { finish([uri]); return }
        pane.open(folder)
        if (name) { pane.selectAfterLoad = name; if (req && req.mode !== "open") nameInput.text = name }
    }

    // ---------------------------------------------------------------- keys
    // The main window's keys, the ones that mean something in a chooser (owner, 2026-09-28): a
    // dot for hidden files, a slash for the filter, j/k and the arrows to move, Enter to choose,
    // Backspace for the folder above. The window's own key handler stands down while a chooser
    // is up (Shell.qml), so without these it had Escape and nothing else. A name being typed
    // keeps its keys: the field takes them first.
    function moveSelection(delta, extend) {
        const n = pane.listing.count; if (!n) return
        const cur = pane.selection.current < 0 ? (delta > 0 ? -1 : n) : pane.selection.current
        const next = Math.max(0, Math.min(n - 1, cur + delta))
        if (extend) pane.selection.range(next); else pane.selection.set(next)
        if (chooserList.ensureVisible) chooserList.ensureVisible(next)
    }
    Keys.onPressed: event => {
        if (searchAll.visible) return
        const shift = event.modifiers & Qt.ShiftModifier
        switch (event.key) {
        case Qt.Key_Period: pane.setHidden(!pane.showHidden); break
        case Qt.Key_Slash: openFilter(); break
        case Qt.Key_J: case Qt.Key_Down: moveSelection(1, shift); break
        case Qt.Key_K: case Qt.Key_Up: moveSelection(-1, shift); break
        case Qt.Key_Backspace: pane.up(); break
        case Qt.Key_Return: case Qt.Key_Enter: accept(); break
        default: return
        }
        event.accepted = true
    }

    function open(r) {
        req = r; filterIndex = 0; visible = true; filterShown = false
        pane.open(r.currentFolder ? "file://" + encodeURI(r.currentFolder) : "file://" + home)
        nameInput.text = r.currentName || ""
        // Focus comes here with the dialog, in every mode. The window's own keys stand down
        // while a chooser is up (Shell.qml), so a dialog that took no focus in `open` mode left
        // Escape with nobody to hear it: a person given a kiki chooser by a browser's "Open file"
        // pressed Escape and nothing happened (found by the e2e flow, 2026-09-26).
        if (r.mode !== "open") nameInput.forceActiveFocus()
        else dlg.forceActiveFocus()
    }
    /// The same chooser, asked by kiki itself rather than by another app through the portal:
    /// `cb(uris)` gets the answer (null when cancelled) and nothing goes to the daemon.
    ///     portal.pick({ mode: "open", directory: true, title: "Local folder", currentFolder: "/home/me" }, uris => …)
    property var _local: null
    function pick(r, cb) { _local = cb; open(r) }
    function finish(uris) {
        visible = false
        if (_local) { const cb = _local; _local = null; cb(uris); return }
        // Collected from the window by token: the listener that asked is started by the bus and
        // has no connection to the daemon (docs/0.3.0/01-daemon-on-demand.md, decision 4).
        chooser.answered(req.token, uris)
    }
    /// Where a listener's answer goes: the Shell sets this to its `chooserFinished`.
    property var chooser: ({ answered: function (token, uris) {} })
    function accept() {
        if (req.mode === "saveFiles") { finish((req.files || []).map(n => pane.childUri(n))); return }
        if (req.mode === "open") {
            if (req.directory) { finish([pane.uri]); return }
            const sel = pane.selection.positions().map(p => pane.listing.row(p)).filter(r => r && !r.isDir).map(r => pane.childUri(r.name))
            if (sel.length) finish(req.multiple ? sel : [sel[0]])
        } else {
            const name = nameInput.text.trim(); if (!name) return
            finish([pane.childUri(name)])
        }
    }
    function matchesFilter(name) {
        if (!req || !req.filters || !req.filters.length) return true
        const f = req.filters[filterIndex]; if (!f) return true
        return f.patterns.some(p => new RegExp("^" + p.replace(/[.+^${}()|[\]\\]/g, "\\$&").replace(/\*/g, ".*").replace(/\?/g, ".") + "$", "i").test(name))
    }
    Connections { target: dlg.pane; function onNavigated(uri) { dlg.pane.setFilter("") } }
    MouseArea { anchors.fill: parent }
    Rectangle {
        // As big as 860 × 560, and no bigger than the window less a margin: a fixed box was cut
        // off in a window shorter than it, its buttons out of reach.
        anchors.centerIn: parent; width: Math.min(860, dlg.width - 24); height: Math.min(560, dlg.height - 24); color: Kiki.Theme.bg; border.width: 2; border.color: Kiki.Theme.accent
        Column {
            anchors.fill: parent
            // One header row (owner, 2026-09-28): the title — Open File, Save As, whatever the asking
            // application called it — at 17 px, the path to its right, and the search box on the
            // same row while the rail's Search entry is lit. A title on a row of its own was tried
            // and looked wrong.
            Rectangle {
                width: parent.width; height: 48; color: Kiki.Theme.bg
                Rectangle { anchors.bottom: parent.bottom; width: parent.width; height: 1; color: Kiki.Theme.line }
                Row {
                    anchors.fill: parent; anchors.leftMargin: 16; anchors.rightMargin: 16; spacing: 12
                    Text { id: chooserTitle; anchors.verticalCenter: parent.verticalCenter; text: dlg.req ? (dlg.req.title || (dlg.req.mode === "open" ? "Open File" : "Save File")) : ""; color: Kiki.Theme.fg; font.family: Kiki.Theme.mono; font.pixelSize: 17; font.bold: true }
                    Breadcrumb { anchors.verticalCenter: parent.verticalCenter; width: parent.width - chooserTitle.width - 12; uri: dlg.pane.uri; home: dlg.home; onNavigate: uri => dlg.pane.open(uri) }
                }
            }
            FilterBar {
                id: chooserFilter; objectName: "chooser-filter"
                visible: dlg.filterShown
                width: parent.width; pane: dlg.pane; total: dlg.filterTotal
                onApply: text => dlg.pane.setFilter(text)
                onPromote: text => { dlg.closeFilter(); searchAll.open(text) }
                onClosed: dlg.closeFilter()
            }
            Item {
                width: parent.width; height: parent.height - 48 - (chooserFilter.visible ? chooserFilter.height : 0) - 52
                Views.ListPane { id: chooserList; x: rail.width; width: parent.width - rail.width; height: parent.height; pane: dlg.pane; onActivate: i => { const r = dlg.pane.listing.row(i); if (r && r.isDir) dlg.pane.open(dlg.pane.childUri(r.name)); else dlg.accept() } }
                // The side part is the main window's sidebar as the person has it set — the rail
                // or the traditional one (Settings → View), and it does not widen or hide of its
                // own accord: a chooser looks like the app it belongs to (owner, 2026-09-28).
                // Favorites only — no search (nowhere for one to go) and no locations: whoever
                // asked for a file is going to open a path on this machine, and a location can
                // only answer with an sftp:// URI it cannot read (plan 31, D13).
                Sidebar {
                    id: rail; objectName: "chooser-rail"
                    x: 0; height: parent.height
                    compact: dlg.railStyle
                    width: dlg.railStyle ? 44 : Math.min(Kiki.Theme.sidebarWidth, Math.floor(dlg.width * 0.32))
                    showSearch: true; showLocations: false
                    searchOpen: searchAll.visible
                    onSearchRequested: dlg.toggleSearchEverywhere()
                    favorites: dlg.favorites; locations: []
                    currentUri: dlg.pane.uri
                    onOpen: uri => dlg.pane.open(uri)
                }
            }
            Rectangle {
                width: parent.width; height: 52; color: Kiki.Theme.bg
                Rectangle { anchors.top: parent.top; width: parent.width; height: 1; color: Kiki.Theme.line }
                Row {
                    id: chooserRight
                    anchors.right: parent.right; anchors.rightMargin: 14; height: parent.height; spacing: 8
                    Button { anchors.verticalCenter: parent.verticalCenter; text: Kiki.T.tr("common.cancel"); onClicked: dlg.finish(null) }
                    Button { anchors.verticalCenter: parent.verticalCenter; text: dlg.req && dlg.req.mode === "open" ? (dlg.req.directory ? "Choose" : "Open") : (dlg.req && dlg.req.mode === "saveFiles" ? "Save here" : "Save"); primary: true; onClicked: dlg.accept() }
                }
                Row {
                    anchors.left: parent.left; anchors.leftMargin: 14; height: parent.height; spacing: 8
                    readonly property int room: chooserRight.x - 14 - 8
                    Text { visible: dlg.req && dlg.req.mode === "saveFiles"; anchors.verticalCenter: parent.verticalCenter; text: Kiki.T.tr("portal.filesSaved", { n: dlg.req ? (dlg.req.files || []).length : 0 }); color: Kiki.Theme.muted; font.family: Kiki.Theme.mono; font.pixelSize: 12 }
                    Rectangle {
                        visible: dlg.req && dlg.req.mode === "save"; anchors.verticalCenter: parent.verticalCenter; width: Math.max(120, Math.min(320, parent.room - 8)); height: 30; radius: 2; color: Kiki.Theme.bgDark; border.width: 1; border.color: nameInput.activeFocus ? Kiki.Theme.accent : Kiki.Theme.gutter
                        TextInput { id: nameInput; anchors.fill: parent; anchors.margins: 8; clip: true; verticalAlignment: TextInput.AlignVCenter; color: Kiki.Theme.fg; font.family: Kiki.Theme.mono; font.pixelSize: Kiki.Theme.fontSize; selectionColor: Kiki.Theme.accent; onAccepted: dlg.accept() }
                    }
                    Rectangle {
                        visible: !!(dlg.req && dlg.req.filters && dlg.req.filters.length > 0); anchors.verticalCenter: parent.verticalCenter; height: 30; width: filterRow.width + 20; radius: 2; border.width: 1; border.color: Kiki.Theme.gutter; color: "transparent"
                        Row { id: filterRow; anchors.centerIn: parent; spacing: 6
                            Text { text: dlg.req && dlg.req.filters && dlg.req.filters[dlg.filterIndex] ? dlg.req.filters[dlg.filterIndex].name : ""; color: Kiki.Theme.fgDim; font.family: Kiki.Theme.mono; font.pixelSize: Kiki.Theme.fontSize }
                            Icon { name: "chev-d"; size: 12; color: Kiki.Theme.muted; anchors.verticalCenter: parent.verticalCenter } }
                        MouseArea { anchors.fill: parent; onClicked: dlg.filterIndex = (dlg.filterIndex + 1) % dlg.req.filters.length }
                    }
                }
            }
        }
    }
    Keys.onEscapePressed: dlg.finish(null)
    Views.SearchOverlay {
        id: searchAll; objectName: "chooser-search-all"
        anchors.fill: parent; z: 20
        results: dlg.results; locations: []; home: dlg.home; indexInfo: dlg.indexInfo
        onSearch: (text, scope) => dlg.runSearch(text, scope)
        onOpenUri: uri => dlg.fromSearch(uri, true)
        onRevealUri: uri => dlg.fromSearch(uri, false)
        onClosed: dlg.forceActiveFocus()
    }
}
