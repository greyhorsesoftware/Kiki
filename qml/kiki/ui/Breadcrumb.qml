import QtQuick
import Quickshell
import ".." as Kiki

// The pane URI as crumbs; click a crumb to jump; Ctrl+L edits the full URI.
Rectangle {
    id: bc
    property string uri: ""
    property string home: ""
    property bool editing: false
    property var repo: null
    signal navigate(string uri)
    height: 30; radius: 2
    color: Kiki.Theme.bgDark; border.width: 1; border.color: editing ? Kiki.Theme.accent : Kiki.Theme.line

    function edit() { editing = true; input.text = bc.uri; input.forceActiveFocus(); input.selectAll() }
    function crumbUri(index) {
        const parts = Kiki.Format.crumbs(uri, home)
        if (uri.startsWith("file://")) {
            let path = parts[0] === "/" ? "" : (parts[0] === "~" ? home : "")
            const start = (parts[0] === "/" || parts[0] === "~") ? 1 : 0
            for (let i = start; i <= index; i++) path += "/" + parts[i]
            if (index === 0 && parts[0] === "/") path = "/"
            return "file://" + encodeURI(path || "/")
        }
        const i = uri.indexOf("://"); const auth = parts[0]
        let path = ""; for (let k = 1; k <= index; k++) path += "/" + parts[k]
        return uri.slice(0, i + 3) + auth + (path || "/")
    }

    Row {
        visible: !bc.editing
        anchors.verticalCenter: parent.verticalCenter; x: 10; spacing: 6
        Repeater {
            model: Kiki.Format.crumbs(bc.uri, bc.home)
            delegate: Row {
                required property int index
                required property string modelData
                spacing: 6
                Icon { visible: index > 0; name: "chev-r"; size: 12; color: Kiki.Theme.gutter; anchors.verticalCenter: parent.verticalCenter }
                Text {
                    text: modelData
                    property bool last: index === Kiki.Format.crumbs(bc.uri, bc.home).length - 1
                    color: last ? Kiki.Theme.fg : Kiki.Theme.muted
                    font.family: Kiki.Theme.mono; font.pixelSize: Kiki.Theme.fontSize; font.bold: last
                    MouseArea { anchors.fill: parent; onClicked: bc.navigate(bc.crumbUri(index)) }
                }
            }
        }
    }
    // Branch chip (plan 15)
    Rectangle {
        visible: !bc.editing && bc.repo && bc.repo.branch
        anchors.right: parent.right; anchors.rightMargin: 6; anchors.verticalCenter: parent.verticalCenter
        height: 20; width: chip.width + 14; radius: 2; color: Kiki.Theme.surface
        Row { id: chip; anchors.centerIn: parent; spacing: 5
            Icon { name: "mirror"; size: 10; color: bc.repo && bc.repo.dirty ? Kiki.Theme.yellow : Kiki.Theme.muted; anchors.verticalCenter: parent.verticalCenter }
            Text { text: bc.repo ? (bc.repo.detached ? bc.repo.branch : bc.repo.branch) + (bc.repo.ahead ? " ↑" + bc.repo.ahead : "") + (bc.repo.behind ? " ↓" + bc.repo.behind : "") : ""; color: Kiki.Theme.fgDim; font.family: Kiki.Theme.mono; font.pixelSize: 11 } }
        MouseArea { anchors.fill: parent; onClicked: Quickshell.execDetached(["wl-copy", bc.repo.branch]) }
    }
    TextInput {
        id: input
        visible: bc.editing
        anchors.fill: parent; anchors.leftMargin: 10; anchors.rightMargin: 10; verticalAlignment: TextInput.AlignVCenter
        color: Kiki.Theme.fg; font.family: Kiki.Theme.mono; font.pixelSize: Kiki.Theme.fontSize; selectionColor: Kiki.Theme.accent; clip: true
        onAccepted: { bc.editing = false; bc.navigate(text.startsWith("/") || text.startsWith("~") ? "file://" + encodeURI(text.replace(/^~/, bc.home)) : text) }
        Keys.onEscapePressed: bc.editing = false
        onActiveFocusChanged: if (!activeFocus) bc.editing = false
    }
    MouseArea { anchors.fill: parent; visible: !bc.editing; onDoubleClicked: bc.edit(); z: -1 }
}
