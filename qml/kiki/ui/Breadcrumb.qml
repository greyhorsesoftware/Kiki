import QtQuick
import Quickshell
import ".." as Kiki

// The pane URI as crumbs. Click a crumb to jump to it, click the path itself (or Ctrl+L) to
// type one, and right-click for the folders above this one.
Rectangle {
    id: bc
    property string uri: ""
    property string home: ""
    property bool editing: false
    property var repo: null
    signal navigate(string uri)
    signal pathMenu()
    height: 30; radius: 2
    color: editing ? Kiki.Theme.bgDark : "transparent"
    clip: true
    // What the crumbs may use: the field less the branch chip.
    readonly property int crumbSpace: width - 20 - (branchChip.visible ? branchChip.width + 10 : 0)

    function edit() { editing = true; input.text = bc.uri; input.forceActiveFocus(); input.selectAll() }
    /// The folders above this one, nearest first: what the path dropdown offers.
    function ancestors() {
        const parts = Kiki.Format.crumbs(uri, home)
        const out = []
        for (let i = parts.length - 2; i >= 0; i--) out.push({ label: parts[i], uri: crumbUri(i) })
        return out
    }
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
        id: crumbRow
        visible: !bc.editing
        anchors.verticalCenter: parent.verticalCenter; spacing: 6
        // A path too long for the field slides left so the folder you are in stays visible.
        x: Math.min(10, bc.crumbSpace - width)
        Repeater {
            model: Kiki.Format.crumbs(bc.uri, bc.home)
            delegate: Row {
                required property int index
                required property string modelData
                spacing: 6
                Icon { visible: index > 0; name: "chev-r"; size: 12; color: Kiki.Theme.gutter; anchors.verticalCenter: parent.verticalCenter }
                // Every crumb is a chip; home shows its icon instead of a "~".
                Rectangle {
                    id: crumbChip
                    readonly property bool isHome: index === 0 && modelData === "~"
                    readonly property bool last: index === Kiki.Format.crumbs(bc.uri, bc.home).length - 1
                    anchors.verticalCenter: parent.verticalCenter
                    width: isHome ? 28 : crumbText.implicitWidth + 20
                    height: 22; radius: 11
                    color: hit.containsMouse ? Kiki.Theme.accent : Kiki.Theme.surface
                    Icon { visible: crumbChip.isHome; anchors.centerIn: parent; name: "home"; size: 13; color: hit.containsMouse ? Kiki.Theme.bg : Kiki.Theme.chrome }
                    Text {
                        id: crumbText
                        visible: !crumbChip.isHome
                        anchors.centerIn: parent
                        text: modelData
                        color: hit.containsMouse ? Kiki.Theme.bg : (crumbChip.last ? Kiki.Theme.fg : Kiki.Theme.muted)
                        font.family: Kiki.Theme.mono; font.pixelSize: Kiki.Theme.fontSize; font.bold: crumbChip.last
                    }
                    MouseArea { id: hit; anchors.fill: parent; hoverEnabled: true; onClicked: bc.navigate(bc.crumbUri(index)) }
                }
            }
        }
    }
    // Branch chip (plan 15)
    Rectangle {
        id: branchChip
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
    // Sits behind the crumbs, so clicking a crumb still jumps to it. Clicking the rest of the
    // path turns it into a field you type in; the folders above are on the right button.
    MouseArea {
        anchors.fill: parent; visible: !bc.editing; z: -1
        acceptedButtons: Qt.LeftButton | Qt.RightButton
        onClicked: mouse => mouse.button === Qt.RightButton ? bc.pathMenu() : bc.edit()
    }
}
