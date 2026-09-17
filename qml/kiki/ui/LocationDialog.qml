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
        anchors.centerIn: parent; width: 520; height: 630; color: Kiki.Theme.bg; border.width: 2; border.color: Kiki.Theme.accent
        Column {
            anchors.fill: parent; anchors.margins: 24; anchors.topMargin: 20; spacing: 20
            Row {
                width: parent.width
                Text { text: dlg.editingName ? "Edit location" : "Add location"; color: Kiki.Theme.fg; font.family: Kiki.Theme.mono; font.pixelSize: 15; font.bold: true }
                Item { width: parent.width - 200; height: 1 }
                Icon { name: "x"; size: 14; color: Kiki.Theme.muted; MouseArea { anchors.fill: parent; onClicked: dlg.visible = false } }
            }
            // Protocol tabs from the plugin registry
            Rectangle {
                width: parent.width; height: 34; radius: 2; color: Kiki.Theme.bgDark; border.width: 1; border.color: Kiki.Theme.line
                Row {
                    anchors.fill: parent; anchors.margins: 2; spacing: 2
                    Repeater {
                        model: dlg.plugins
                        delegate: Rectangle {
                            required property var modelData
                            required property int index
                            width: (parent.width - 2 * (dlg.plugins.length - 1)) / Math.max(1, dlg.plugins.length); height: 28; radius: 2
                            color: dlg.tab === index ? Kiki.Theme.surface : "transparent"
                            Text { anchors.centerIn: parent; text: modelData.displayName; color: dlg.tab === index ? Kiki.Theme.fg : Kiki.Theme.muted; font.family: Kiki.Theme.mono; font.pixelSize: Kiki.Theme.fontSize; font.bold: dlg.tab === index }
                            MouseArea { anchors.fill: parent; enabled: !dlg.editingName; onClicked: { dlg.tab = index; dlg.values = dlg.defaults(modelData); dlg.errors = ({}) } }
                        }
                    }
                }
            }
            Flickable {
                width: parent.width; height: parent.height - 34 - 20 - 20 - 30 - 20 - 20; clip: true; contentHeight: fields.height
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
                width: parent.width; spacing: 8
                Text { width: parent.width - 220; anchors.verticalCenter: parent.verticalCenter; text: dlg.status || "Secrets are kept in the Omarchy keyring."; color: dlg.status && !dlg.busy ? Kiki.Theme.red : Kiki.Theme.muted; font.family: Kiki.Theme.mono; font.pixelSize: 11; wrapMode: Text.WordWrap }
                Button { text: "Cancel"; onClicked: dlg.visible = false }
                Button { text: dlg.busy ? "Connecting…" : "Connect"; primary: true; enabled: !dlg.busy; onClicked: dlg.connect() }
            }
        }
    }
    Keys.onEscapePressed: visible = false
    ContextMenu { id: browseMenu; parent: dlg }
}
