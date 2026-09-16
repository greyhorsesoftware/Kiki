import QtQuick
import ".." as Kiki

Rectangle {
    id: item
    property string icon: "folder"
    property color iconColor: Kiki.Theme.accent
    property string label: ""
    property bool active: false
    property string detail: ""
    signal clicked()
    signal rightClicked()
    height: 30; radius: 2
    anchors.left: parent ? parent.left : undefined; anchors.right: parent ? parent.right : undefined
    anchors.leftMargin: 8; anchors.rightMargin: 8
    color: active ? Kiki.Theme.surface : (hover.containsMouse ? Qt.rgba(1, 1, 1, 0.04) : "transparent")
    Row {
        anchors.verticalCenter: parent.verticalCenter; x: 8; spacing: 10
        Icon { name: item.icon; color: item.iconColor; anchors.verticalCenter: parent.verticalCenter }
        Text { text: item.label; color: item.active ? Kiki.Theme.fg : Kiki.Theme.fgDim; font.family: Kiki.Theme.mono; font.pixelSize: Kiki.Theme.fontSize; elide: Text.ElideRight; width: item.width - 60 }
    }
    MouseArea { id: hover; anchors.fill: parent; hoverEnabled: true; acceptedButtons: Qt.LeftButton | Qt.RightButton; onClicked: mouse => mouse.button === Qt.RightButton ? item.rightClicked() : item.clicked() }
}
