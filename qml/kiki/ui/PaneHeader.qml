import QtQuick
import ".." as Kiki

// Split mode: each pane's badge and path; the focused pane has the accent underline.
Rectangle {
    id: h
    property Kiki.Pane pane
    property string home: ""
    property var location: null      // the sidebar location when the pane is remote
    signal clicked()
    height: 34
    color: Kiki.Theme.bgDark
    Rectangle { anchors.bottom: parent.bottom; width: parent.width; height: 2; color: h.pane.focused ? Kiki.Theme.accent : Kiki.Theme.line }
    Row {
        anchors.fill: parent; anchors.leftMargin: 12; anchors.rightMargin: 12; spacing: 8
        Rectangle {
            anchors.verticalCenter: parent.verticalCenter; height: 20; width: badge.width + 14; radius: 2; color: "transparent"; border.width: 1; border.color: Kiki.Theme.gutter
            Row {
                id: badge; anchors.centerIn: parent; spacing: 6
                Icon { name: h.pane.uri.startsWith("file://") ? "hdd" : "server"; size: 12; color: h.pane.uri.startsWith("file://") ? Kiki.Theme.fgDim : Kiki.Theme.green; anchors.verticalCenter: parent.verticalCenter }
                Text { text: h.pane.uri.startsWith("file://") ? "local" : h.pane.uri.split("://")[1].split("/")[0]; color: h.pane.uri.startsWith("file://") ? Kiki.Theme.fgDim : Kiki.Theme.green; font.family: Kiki.Theme.mono; font.pixelSize: 11 }
            }
        }
        Text { anchors.verticalCenter: parent.verticalCenter; width: parent.width - 120; elide: Text.ElideMiddle; text: Kiki.Format.display(h.pane.uri, h.home); color: h.pane.focused ? Kiki.Theme.fg : Kiki.Theme.fgDim; font.family: Kiki.Theme.mono; font.pixelSize: Kiki.Theme.fontSize }
    }
    MouseArea { anchors.fill: parent; onClicked: h.clicked(); z: -1 }
}
