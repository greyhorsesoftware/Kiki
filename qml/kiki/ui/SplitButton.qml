import QtQuick
import ".." as Kiki

// A labelled button whose arrow opens a list; the body runs the default.
Rectangle {
    id: sb
    property string label: "Open in"
    property string icon: "terminal"
    property bool enabled: true
    signal clicked()
    signal menu()
    height: 34; width: body.width + 26; radius: 2
    color: Kiki.Theme.bgDark; border.width: 1; border.color: Kiki.Theme.line
    opacity: enabled ? 1 : 0.5
    Row {
        id: body; anchors.verticalCenter: parent.verticalCenter; x: 8; spacing: 8
        Icon { name: sb.icon; color: Kiki.Theme.muted; anchors.verticalCenter: parent.verticalCenter }
        Text { text: sb.label; color: Kiki.Theme.fgDim; font.family: Kiki.Theme.mono; font.pixelSize: 12; anchors.verticalCenter: parent.verticalCenter }
        Rectangle { width: 1; height: 18; color: Kiki.Theme.line; anchors.verticalCenter: parent.verticalCenter }
        Icon { name: "chev-d"; size: 12; color: Kiki.Theme.muted; anchors.verticalCenter: parent.verticalCenter; MouseArea { anchors.fill: parent; anchors.margins: -8; enabled: sb.enabled; onClicked: sb.menu() } }
    }
    MouseArea { anchors.fill: parent; anchors.rightMargin: 26; enabled: sb.enabled; onClicked: sb.clicked() }
}
