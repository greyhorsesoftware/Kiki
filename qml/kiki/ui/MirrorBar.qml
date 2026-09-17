import QtQuick
import ".." as Kiki

// Mirror view (plan 24): the strip above the two panes. Names both sides, offers Swap, shows
// when the pair was last mirrored, and holds the one action: "Mirror to <location>".
Rectangle {
    id: bar
    property Kiki.Pane leftPane
    property Kiki.Pane rightPane
    property string home: ""
    property var lastMirrored: null      // ms since epoch, or null
    signal swap()
    signal mirror(bool upload)
    signal options(point pos)
    readonly property string remoteName: { const u = rightPane && rightPane.uri.startsWith("file://") ? (leftPane ? leftPane.uri : "") : (rightPane ? rightPane.uri : ""); const m = u.match(/^[a-z]+:\/\/([^/]+)/); return m ? m[1] : "" }
    readonly property bool localLeft: leftPane && leftPane.uri.startsWith("file://")
    width: parent ? parent.width : 0; height: 44; color: Kiki.Theme.bg
    Rectangle { anchors.bottom: parent.bottom; width: parent.width; height: 1; color: Kiki.Theme.line }
    Row {
        anchors.fill: parent; anchors.leftMargin: 14; anchors.rightMargin: 14; spacing: 10
        Icon { anchors.verticalCenter: parent.verticalCenter; name: bar.localLeft ? "hdd" : "server"; size: 14; color: bar.localLeft ? Kiki.Theme.fgDim : Kiki.Theme.green }
        Text { anchors.verticalCenter: parent.verticalCenter; text: bar.leftPane ? Kiki.Format.display(bar.leftPane.uri, bar.home) : ""; color: Kiki.Theme.fgDim; font.family: Kiki.Theme.mono; font.pixelSize: 12; elide: Text.ElideMiddle; width: Math.min(implicitWidth, 260) }
        Rectangle {
            anchors.verticalCenter: parent.verticalCenter; width: 26; height: 26; radius: 2; color: swapHover.containsMouse ? Kiki.Theme.surface : "transparent"; border.width: 1; border.color: Kiki.Theme.gutter
            Icon { anchors.centerIn: parent; name: "mirror"; size: 12; color: Kiki.Theme.muted }
            MouseArea { id: swapHover; anchors.fill: parent; hoverEnabled: true; onClicked: bar.swap() }
        }
        Icon { anchors.verticalCenter: parent.verticalCenter; name: bar.localLeft ? "server" : "hdd"; size: 14; color: bar.localLeft ? Kiki.Theme.green : Kiki.Theme.fgDim }
        Text { anchors.verticalCenter: parent.verticalCenter; text: bar.rightPane ? Kiki.Format.display(bar.rightPane.uri, bar.home) : ""; color: Kiki.Theme.fgDim; font.family: Kiki.Theme.mono; font.pixelSize: 12; elide: Text.ElideMiddle; width: Math.min(implicitWidth, 260) }
        Item { width: parent.width - 14 * 2 - 26 - 10 * 7 - 14 * 2 - lhs.width - rhs.width - status.width - action.width; height: 1; property Item lhs: parent.children[1]; property Item rhs: parent.children[4] }
        Text { id: status; anchors.verticalCenter: parent.verticalCenter; text: bar.lastMirrored ? "last mirrored " + Kiki.Format.relative(bar.lastMirrored) : "not mirrored yet"; color: Kiki.Theme.muted; font.family: Kiki.Theme.mono; font.pixelSize: 11 }
        // The action: accent split button with the direction arrow, the label, the key chip and an options chevron.
        Rectangle {
            id: action
            anchors.verticalCenter: parent.verticalCenter; height: 32; radius: 2; color: Kiki.Theme.accent
            width: actionRow.width + 18
            Row {
                id: actionRow; anchors.verticalCenter: parent.verticalCenter; x: 12; spacing: 10
                Icon { anchors.verticalCenter: parent.verticalCenter; name: "arr-u"; size: 14; color: Kiki.Theme.bg }
                Text { anchors.verticalCenter: parent.verticalCenter; text: "Mirror to " + (bar.remoteName || "remote"); color: Kiki.Theme.bg; font.family: Kiki.Theme.mono; font.pixelSize: Kiki.Theme.fontSize; font.bold: true }
                Rectangle { anchors.verticalCenter: parent.verticalCenter; width: chip.width + 12; height: 18; radius: 2; color: "transparent"; border.width: 1; border.color: Qt.rgba(Kiki.Theme.bg.r, Kiki.Theme.bg.g, Kiki.Theme.bg.b, 0.35)
                    Text { id: chip; anchors.centerIn: parent; text: "⌃M"; color: Kiki.Theme.bg; font.family: Kiki.Theme.mono; font.pixelSize: 10 } }
                Rectangle { anchors.verticalCenter: parent.verticalCenter; width: 22; height: 22; color: "transparent"
                    Rectangle { x: -4; width: 1; height: parent.height; color: Qt.rgba(Kiki.Theme.bg.r, Kiki.Theme.bg.g, Kiki.Theme.bg.b, 0.35) }
                    Icon { anchors.centerIn: parent; name: "chev-d"; size: 10; color: Kiki.Theme.bg }
                    MouseArea { anchors.fill: parent; onClicked: bar.options(Qt.point(action.x + action.width - 232, bar.y + bar.height)) } }
            }
            MouseArea { anchors.fill: parent; anchors.rightMargin: 30; onClicked: bar.mirror(true) }
        }
    }
}
