import QtQuick
import ".." as Kiki

// Share (plan 18): compose fields for the chosen plugin and target, then Send.
Rectangle {
    id: sheet
    visible: false
    anchors.fill: parent; color: Qt.rgba(0, 0, 0, 0.5); z: 92
    property var plugin: null
    property var target: null
    property var uris: []
    property var values: ({})
    property string status: ""
    function open(p, t, u) {
        plugin = p; target = t; uris = u; status = ""
        const v = {}; for (const f of (p.compose || [])) v[f.key] = f.default || ""
        values = v
        if (!(p.compose || []).length) { send(); return }
        visible = true
    }
    function send() {
        visible = false
        Kiki.Daemon.request("Share", { plugin: plugin.id, uris: uris, target: target ? target.id : null, compose: values }, (ok, err) => { if (err) Kiki.Jobs.showToast({ text: err.message, undoable: false }) })
    }
    MouseArea { anchors.fill: parent }
    Rectangle {
        anchors.centerIn: parent; width: 480; height: 120 + (sheet.plugin ? (sheet.plugin.compose || []).length * 66 : 0); color: Kiki.Theme.bg; border.width: 2; border.color: Kiki.Theme.accent
        Column {
            anchors.fill: parent; anchors.margins: 20; spacing: 12
            Text { text: Kiki.T.tr(sheet.target ? "share.titleTo" : "share.title", { n: sheet.uris.length, plugin: sheet.plugin ? sheet.plugin.name : "", target: sheet.target ? sheet.target.name : "" }); color: Kiki.Theme.fg; font.family: Kiki.Theme.mono; font.pixelSize: 15; font.bold: true }
            Repeater {
                model: sheet.plugin ? sheet.plugin.compose : []
                delegate: FormField { required property var modelData; field: modelData; value: sheet.values[modelData.key] || ""; onEdited: v => { const nv = Object.assign({}, sheet.values); nv[modelData.key] = v; sheet.values = nv } }
            }
            Row { spacing: 8; anchors.right: parent.right
                Button { text: Kiki.T.tr("common.cancel"); onClicked: sheet.visible = false }
                Button { text: Kiki.T.tr("share.send"); primary: true; onClicked: sheet.send() } }
        }
    }
    Keys.onEscapePressed: visible = false
}
