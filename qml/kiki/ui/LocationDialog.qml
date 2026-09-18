import QtQuick
import Quickshell
import ".." as Kiki

// Add or edit a location. One tab per installed plugin; fields from its Describe form.
Rectangle {
    id: dlg
    visible: false
    anchors.fill: parent; color: Qt.rgba(0, 0, 0, 0.5); z: 90
    property var plugins: []          // Describe results
    property int tab: 0
    property var values: ({})         // key -> string
    property var errors: ({})         // key -> message
    property string status: ""
    property bool busy: false
    property string editingName: ""   // non-empty when editing
    signal saved()

    function open(existing) {
        errors = ({}); status = ""; busy = false
        Kiki.Daemon.request("Plugins", {}, ok => {
            if (!ok) return
            plugins = ok.plugins
            if (existing) {
                editingName = existing.name
                tab = Math.max(0, plugins.findIndex(p => p.scheme === existing.plugin))
                const v = Object.assign({}, existing.config || {})
                v.name = existing.name
                v.remotePath = uriPath(existing.remoteUri); v.localPath = existing.localUri ? decodeURIComponent(existing.localUri.replace(/^file:\/\//, "")) : ""
                values = v
            } else {
                editingName = ""; tab = 0; values = defaults(plugins[0])
            }
            visible = true
        })
    }
    function defaults(p) { const v = {}; for (const f of (p ? p.form : [])) v[f.key] = (p.defaults && p.defaults[f.key]) || f.default || ""; return v }
    function uriPath(u) { if (!u) return "/"; const i = u.indexOf("://"); const rest = u.slice(i + 3); const s = rest.indexOf("/"); return s < 0 ? "/" : decodeURIComponent(rest.slice(s)) }
    function current() { return plugins[tab] }
    // A `browse` field asks its plugin for choices given what is filled in so far.
    function browseField(key) {
        const p = current(); if (!p) return
        status = "Looking…"
        Kiki.Daemon.request("PluginBrowse", { plugin: p.scheme, field: key, config: values, secrets: values }, (ok, err) => {
            status = err ? err.message : ""
            if (!ok) return
            const items = (ok.options || []).map(o => ({ label: o.label || o.value, action: () => { const nv = Object.assign({}, dlg.values); nv[key] = o.value; dlg.values = nv } }))
            if (!items.length) items.push({ label: "Nothing found", enabled: false, action: () => {} })
            browseMenu.open(items, Qt.point(80, 200))
        })
    }
    function build() {
        const p = current(); const config = {}; const secrets = {}
        for (const f of p.form) {
            if (["name", "remotePath", "localPath"].includes(f.key)) continue
            if ((p.secretFields || []).includes(f.key)) { if (values[f.key]) secrets[f.key] = values[f.key] } else config[f.key] = values[f.key] || ""
        }
        const name = (values.name || "").trim()
        const location = { name: name, plugin: p.scheme, remoteUri: p.scheme + "://" + name + (values.remotePath || "/"), localUri: values.localPath ? "file://" + encodeURI(values.localPath.replace(/^~/, Quickshell.env("HOME"))) : "", config: config }
        return { location: location, secrets: secrets }
    }
    /// Move to another kind, keeping it on screen. Locked while editing a saved location.
    function selectKind(i) {
        if (editingName || !plugins.length) return
        const n = Math.max(0, Math.min(plugins.length - 1, i))
        if (n === tab) return
        tab = n; values = defaults(plugins[n]); errors = ({})
        kinds.positionViewAtIndex(n, ListView.Contain)
    }
    /// The icon set has no per-protocol glyphs, so map the scheme onto the closest one.
    function kindIcon(scheme) {
        switch (scheme) {
        case "sftp": case "ftps": return "server"
        case "smb": case "dav": case "afp": return "cloud"
        case "mtp": case "ptp": case "afc": return "usb"
        }
        return "hdd"
    }
    function validate() {
        const e = {}
        if (!(values.name || "").trim()) e.name = "name is required"
        for (const f of current().form) if (f.required && !(values[f.key] || "").trim() && !e[f.key]) e[f.key] = f.label + " is required"
        errors = e
        return Object.keys(e).length === 0
    }
    function connect() {
        if (!validate()) return
        busy = true; status = "Connecting…"
        const req = build()
        Kiki.Daemon.request(editingName ? "UpdateLocation" : "AddLocation", req, (ok, err) => {
            busy = false
            if (err) { status = ""; if (err.field) { const e = {}; e[err.field] = err.message; errors = e } else status = err.message; return }
            visible = false; dlg.saved()
        })
    }

    MouseArea { anchors.fill: parent }
    Rectangle {
        anchors.centerIn: parent
        width: Math.min(520, parent.width - 32); height: Math.min(630, parent.height - 32)
        color: Kiki.Theme.bg; border.width: 2; border.color: Kiki.Theme.accent
        ToggleButton {
            anchors.right: parent.right; anchors.top: parent.top; anchors.margins: 4
            z: 2; icon: "x"; tip: "Close (Esc)"
            onClicked: dlg.visible = false
        }
        Column {
            anchors.fill: parent; anchors.margins: 24; anchors.topMargin: 20; spacing: 20
            Row {
                id: header
                width: parent.width
                Text { text: dlg.editingName ? "Edit location" : "Add location"; color: Kiki.Theme.fg; font.family: Kiki.Theme.mono; font.pixelSize: 15; font.bold: true }
            }
            // One tile per kind, scrolling sideways: the names are too long to share the width.
            ListView {
                id: kinds
                width: parent.width; height: 78
                orientation: ListView.Horizontal; spacing: 8; clip: true
                model: dlg.plugins
                delegate: Rectangle {
                    required property var modelData
                    required property int index
                    width: 92; height: 72; radius: 14
                    color: dlg.tab === index ? Kiki.Theme.surface : Kiki.Theme.bgDark
                    border.width: 1; border.color: dlg.tab === index ? Kiki.Theme.accent : Kiki.Theme.line
                    opacity: dlg.editingName && dlg.tab !== index ? 0.4 : 1
                    Column {
                        anchors.centerIn: parent; spacing: 6; width: parent.width - 12
                        Icon {
                            anchors.horizontalCenter: parent.horizontalCenter
                            name: dlg.kindIcon(modelData.scheme); size: 22
                            color: dlg.tab === index ? Kiki.Theme.accent : Kiki.Theme.fgDim
                        }
                        Text {
                            width: parent.width; horizontalAlignment: Text.AlignHCenter
                            wrapMode: Text.WordWrap; maximumLineCount: 2; elide: Text.ElideRight
                            text: modelData.displayName
                            color: dlg.tab === index ? Kiki.Theme.fg : Kiki.Theme.muted
                            font.family: Kiki.Theme.mono; font.pixelSize: 11
                        }
                    }
                    MouseArea {
                        anchors.fill: parent; enabled: !dlg.editingName
                        onClicked: { dlg.tab = index; dlg.values = dlg.defaults(modelData); dlg.errors = ({}); kinds.positionViewAtIndex(index, ListView.Contain) }
                    }
                }
                WheelHandler {
                    target: null
                    orientation: Qt.Horizontal
                    acceptedDevices: PointerDevice.Mouse | PointerDevice.TouchPad
                    onWheel: event => {
                        const dx = event.pixelDelta.x !== 0 ? event.pixelDelta.x : event.angleDelta.x / 2
                        if (dx === 0) { event.accepted = false; return }
                        kinds.contentX = Math.max(0, Math.min(kinds.contentX + dx, Math.max(0, kinds.contentWidth - kinds.width)))
                        event.accepted = true
                    }
                }
            }
            Flickable {
                NaturalScroll { }
                width: parent.width; height: Math.max(0, parent.height - header.height - kinds.height - footer.height - 3 * parent.spacing)
                clip: true; contentHeight: fields.height
                Column {
                    id: fields; width: parent.width; spacing: 14
                    Repeater {
                        model: dlg.current() ? dlg.current().form : []
                        delegate: FormField {
                            required property var modelData
                            field: modelData
                            value: dlg.values[modelData.key] || ""
                            error: dlg.errors[modelData.key] || ""
                            onEdited: v => { const nv = Object.assign({}, dlg.values); nv[modelData.key] = v; dlg.values = nv }
                            onBrowse: dlg.browseField(modelData.key)
                        }
                    }
                }
            }
            Row {
                id: footer
                width: parent.width; spacing: 8
                Text { width: parent.width - 220; anchors.verticalCenter: parent.verticalCenter; text: dlg.status || "Secrets are kept in the Omarchy keyring."; color: dlg.status && !dlg.busy ? Kiki.Theme.red : Kiki.Theme.muted; font.family: Kiki.Theme.mono; font.pixelSize: 11; wrapMode: Text.WordWrap }
                Button { text: "Cancel"; onClicked: dlg.visible = false }
                Button { text: dlg.busy ? "Connecting…" : "Connect"; primary: true; enabled: !dlg.busy; onClicked: dlg.connect() }
            }
        }
    }
    focus: visible
    activeFocusOnTab: true
    onVisibleChanged: if (visible) forceActiveFocus()
    // Tab walks on from the kind strip into the fields and then the buttons.
    Keys.onReturnPressed: dlg.connect()
    Keys.onEnterPressed: dlg.connect()
    // A text field takes Left/Right first, so these only fire when the strip has the keyboard.
    Keys.onLeftPressed: dlg.selectKind(dlg.tab - 1)
    Keys.onRightPressed: dlg.selectKind(dlg.tab + 1)
    Keys.onEscapePressed: visible = false
    ContextMenu { id: browseMenu; parent: dlg }
}
