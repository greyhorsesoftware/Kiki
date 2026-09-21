import QtQuick
import ".." as Kiki
import "../views" as Views

// The Open / Save dialog kiki provides to other apps through the portal (plan 09).
Rectangle {
    id: dlg
    visible: false
    anchors.fill: parent; color: Qt.rgba(0, 0, 0, 0.55); z: 95
    property var req: null            // the ShowChooser event
    property string home: ""
    property var favorites: []
    property var locations: []
    property int filterIndex: 0
    property Kiki.Pane pane: Kiki.Pane { view: "list" }

    function open(r) {
        req = r; filterIndex = 0; visible = true
        pane.open(r.currentFolder ? "file://" + encodeURI(r.currentFolder) : "file://" + home)
        nameInput.text = r.currentName || ""
        if (r.mode !== "open") nameInput.forceActiveFocus()
    }
    /// The same chooser, asked by kiki itself rather than by another app through the portal:
    /// `cb(uris)` gets the answer (null when cancelled) and nothing goes to the daemon.
    ///     portal.pick({ mode: "open", directory: true, title: "Local folder", currentFolder: "/home/me" }, uris => …)
    property var _local: null
    function pick(r, cb) { _local = cb; open(r) }
    function finish(uris) {
        visible = false
        if (_local) { const cb = _local; _local = null; cb(uris); return }
        Kiki.Daemon.request("ChooserResult", { token: req.token, uris: uris })
    }
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
            Rectangle {
                width: parent.width; height: 48; color: Kiki.Theme.bg
                Rectangle { anchors.bottom: parent.bottom; width: parent.width; height: 1; color: Kiki.Theme.line }
                Row {
                    anchors.fill: parent; anchors.leftMargin: 12; anchors.rightMargin: 12; spacing: 8
                    Text { anchors.verticalCenter: parent.verticalCenter; text: dlg.req ? (dlg.req.title || (dlg.req.mode === "open" ? "Open File" : "Save File")) : ""; color: Kiki.Theme.fg; font.family: Kiki.Theme.mono; font.pixelSize: Kiki.Theme.fontSize; font.bold: true }
                    Breadcrumb { anchors.verticalCenter: parent.verticalCenter; width: parent.width - 300; uri: dlg.pane.uri; home: dlg.home; onNavigate: uri => dlg.pane.open(uri) }
                    SearchBox { anchors.verticalCenter: parent.verticalCenter; width: 200; placeholder: "Search"; onChanged: text => dlg.pane.setFilter(text) }
                }
            }
            Row {
                width: parent.width; height: parent.height - 48 - 52
                Rectangle {
                    width: 180; height: parent.height; color: Kiki.Theme.bgDark
                    Rectangle { anchors.right: parent.right; width: 1; height: parent.height; color: Kiki.Theme.line }
                    Column {
                        anchors.fill: parent; anchors.topMargin: 8; spacing: 12
                        SidebarSection { title: "Favorites"; Repeater { model: dlg.favorites; delegate: SidebarItem { required property var modelData; icon: modelData.name === "Home" ? "home" : "folder"; label: modelData.name; active: dlg.pane.uri === modelData.uri; onClicked: dlg.pane.open(modelData.uri) } } }
                        // Not shown (plan 31, D13). Whoever asked for a file — another application
                        // through the portal, or kiki's own "local folder" and "extract to" — is
                        // going to open a path on this machine, and a location can only answer
                        // with an sftp:// URI it cannot read. Back when a pick is fetched to a
                        // local file first.
                        SidebarSection { objectName: "chooser-locations"; visible: false; title: "Locations"; Repeater { model: dlg.locations; delegate: SidebarItem { required property var modelData; icon: "server"; iconColor: Kiki.Theme.green; label: modelData.name + " · " + modelData.plugin; onClicked: dlg.pane.open(modelData.remoteUri) } } }
                    }
                }
                Views.ListPane { width: parent.width - 180; height: parent.height; pane: dlg.pane; onActivate: i => { const r = dlg.pane.listing.row(i); if (r && r.isDir) dlg.pane.open(dlg.pane.childUri(r.name)); else dlg.accept() } }
            }
            Rectangle {
                width: parent.width; height: 52; color: Kiki.Theme.bg
                Rectangle { anchors.top: parent.top; width: parent.width; height: 1; color: Kiki.Theme.line }
                Row {
                    id: chooserRight
                    anchors.right: parent.right; anchors.rightMargin: 14; height: parent.height; spacing: 8
                    Button { anchors.verticalCenter: parent.verticalCenter; text: "Cancel"; onClicked: dlg.finish(null) }
                    Button { anchors.verticalCenter: parent.verticalCenter; text: dlg.req && dlg.req.mode === "open" ? (dlg.req.directory ? "Choose" : "Open") : (dlg.req && dlg.req.mode === "saveFiles" ? "Save here" : "Save"); primary: true; onClicked: dlg.accept() }
                }
                Row {
                    anchors.left: parent.left; anchors.leftMargin: 14; height: parent.height; spacing: 8
                    readonly property int room: chooserRight.x - 14 - 8
                    Text { visible: dlg.req && dlg.req.mode === "saveFiles"; anchors.verticalCenter: parent.verticalCenter; text: (dlg.req ? (dlg.req.files || []).length : 0) + " files will be saved here"; color: Kiki.Theme.muted; font.family: Kiki.Theme.mono; font.pixelSize: 12 }
                    Rectangle {
                        visible: dlg.req && dlg.req.mode === "save"; anchors.verticalCenter: parent.verticalCenter; width: Math.max(120, Math.min(320, parent.room - 8)); height: 30; radius: 2; color: Kiki.Theme.bgDark; border.width: 1; border.color: nameInput.activeFocus ? Kiki.Theme.accent : Kiki.Theme.gutter
                        TextInput { id: nameInput; anchors.fill: parent; anchors.margins: 8; clip: true; verticalAlignment: TextInput.AlignVCenter; color: Kiki.Theme.fg; font.family: Kiki.Theme.mono; font.pixelSize: Kiki.Theme.fontSize; selectionColor: Kiki.Theme.accent; onAccepted: dlg.accept() }
                    }
                    Rectangle {
                        visible: dlg.req && dlg.req.filters && dlg.req.filters.length > 0; anchors.verticalCenter: parent.verticalCenter; height: 30; width: filterRow.width + 20; radius: 2; border.width: 1; border.color: Kiki.Theme.gutter; color: "transparent"
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
}
