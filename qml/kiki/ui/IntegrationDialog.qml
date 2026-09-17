import QtQuick
import ".." as Kiki

// First launch (plan 09): offer to make kiki the default for this user. Every item is a
// per-user, reversible change; Settings → Omarchy shows the same list with Remove.
Rectangle {
    id: dlg
    visible: false
    anchors.fill: parent; color: Qt.rgba(0, 0, 0, 0.55); z: 95
    property var status: ({})
    property var picked: ({ mime: true, dbus: true, hypr: true, portal: true })
    property var results: []
    property bool busy: false
    readonly property var items: [
        { id: "mime", label: "Open folders from other applications", detail: "inode/directory in ~/.config/mimeapps.list" },
        { id: "dbus", label: "\"Show in folder\" from browsers and chat apps", detail: "user D-Bus activation for org.freedesktop.FileManager1" },
        { id: "hypr", label: "Keys: Super+Shift+F opens kiki, Super+Alt+Shift+F opens the terminal's folder", detail: "a marked block in ~/.config/hypr/bindings.conf, rolled back if Hyprland rejects it" },
        { id: "portal", label: "Open and Save dialogs from other applications", detail: "FileChooser=kiki;gtk in ~/.config/xdg-desktop-portal/portals.conf" },
    ]
    function open() { results = []; Kiki.Daemon.request("Integration", {}, ok => { if (ok) status = ok; visible = true }) }
    function decide(apply) {
        Kiki.Settings.set("integration", "asked", true)
        if (!apply) { visible = false; return }
        const parts = items.map(i => i.id).filter(id => picked[id])
        busy = true
        Kiki.Daemon.request("Integrate", { parts: parts }, (ok, err) => {
            busy = false
            if (err) { results = [{ part: "all", ok: false, message: err.message }]; return }
            results = ok.results; status = ok.status
            if (results.every(r => r.ok)) visible = false
        })
    }
    MouseArea { anchors.fill: parent }
    Rectangle {
        anchors.centerIn: parent; width: 640; height: col.height + 48; color: Kiki.Theme.bg; border.width: 2; border.color: Kiki.Theme.accent
        Column {
            id: col; x: 24; y: 24; width: parent.width - 48; spacing: 12
            Text { text: "Make kiki your file manager?"; color: Kiki.Theme.fg; font.family: Kiki.Theme.mono; font.pixelSize: 16; font.bold: true }
            Text { width: parent.width; wrapMode: Text.WordWrap; text: "These are per-user settings and each one can be removed later from Settings → Omarchy."; color: Kiki.Theme.fgDim; font.family: Kiki.Theme.mono; font.pixelSize: 12 }
            Repeater {
                model: dlg.items
                delegate: Row {
                    required property var modelData
                    spacing: 10; width: col.width
                    Rectangle {
                        width: 16; height: 16; radius: 2; anchors.top: parent.top; anchors.topMargin: 2
                        color: dlg.picked[modelData.id] ? Kiki.Theme.accent : "transparent"; border.width: 1; border.color: dlg.picked[modelData.id] ? Kiki.Theme.accent : Kiki.Theme.gutter
                        Text { anchors.centerIn: parent; text: "✓"; visible: dlg.picked[modelData.id]; color: Kiki.Theme.bg; font.pixelSize: 11; font.bold: true }
                        MouseArea { anchors.fill: parent; onClicked: { const p = Object.assign({}, dlg.picked); p[modelData.id] = !p[modelData.id]; dlg.picked = p } }
                    }
                    Column {
                        width: parent.width - 26; spacing: 2
                        Text { width: parent.width; wrapMode: Text.WordWrap; text: modelData.label + (dlg.status[modelData.id] ? "  ·  already set" : ""); color: Kiki.Theme.fg; font.family: Kiki.Theme.mono; font.pixelSize: 12 }
                        Text { width: parent.width; wrapMode: Text.WordWrap; text: modelData.detail; color: Kiki.Theme.muted; font.family: Kiki.Theme.mono; font.pixelSize: 11 }
                        Text { visible: !!(dlg.results.find(r => r.part === modelData.id && !r.ok)); width: parent.width; wrapMode: Text.WordWrap; text: (dlg.results.find(r => r.part === modelData.id) || {}).message || ""; color: Kiki.Theme.red; font.family: Kiki.Theme.mono; font.pixelSize: 11 }
                    }
                }
            }
            Text { visible: !!dlg.status.hyprConfigErrors && dlg.status.hyprConfigErrors.length > 0; width: parent.width; wrapMode: Text.WordWrap; text: "Hyprland reports config errors already; the keybinding step will refuse until they are fixed:\n" + (dlg.status.hyprConfigErrors || []).join("\n"); color: Kiki.Theme.yellow; font.family: Kiki.Theme.mono; font.pixelSize: 11 }
            Row {
                spacing: 8; anchors.right: parent.right
                Button { text: "Not now"; onClicked: dlg.decide(false) }
                Button { text: dlg.busy ? "Applying…" : "Make kiki the default"; primary: true; enabled: !dlg.busy; onClicked: dlg.decide(true) }
            }
        }
    }
}
