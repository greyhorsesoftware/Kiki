import QtQuick
import ".." as Kiki

Rectangle {
    id: item
    property string icon: "folder"
    property color iconColor: item.active ? Kiki.Theme.accent : Kiki.Theme.chrome
    property string label: ""
    objectName: "sidebar-" + (label || icon)
    /// What the rail tooltip says; the label unless a shortcut is worth spelling out.
    property string tipText: label
    property bool active: false
    /// keyboard highlight (plan 23)
    property bool keyed: false
    property string detail: ""
    signal clicked()
    signal rightClicked()
    /// Set to accept dropped URIs; emits dropped(drop) with the DragEvent.
    property bool droppable: false
    readonly property bool hovered: hover.containsMouse
    /// Rail style: the icon alone, centred. Plain folders show their initial instead, since
    /// a column of identical folder glyphs tells you nothing.
    property bool compact: false
    signal dropped(var drop)
    height: 30; radius: 9
    anchors.left: parent ? parent.left : undefined; anchors.right: parent ? parent.right : undefined
    anchors.leftMargin: compact ? 6 : 8; anchors.rightMargin: compact ? 6 : 8
    color: active ? Kiki.Theme.surface : (hover.containsMouse || keyed ? Qt.rgba(1, 1, 1, 0.04) : "transparent")
    border.width: keyed ? 1 : 0; border.color: Kiki.Theme.accent
    Row {
        anchors.verticalCenter: parent.verticalCenter
        x: item.compact ? Math.round((item.width - 16) / 2) : 8
        spacing: 10
        Icon { name: item.icon; color: item.iconColor; anchors.verticalCenter: parent.verticalCenter }
        Text { visible: !item.compact; text: item.label; color: item.active ? Kiki.Theme.fg : Kiki.Theme.fgDim; font.family: Kiki.Theme.mono; font.pixelSize: Kiki.Theme.fontSize; elide: Text.ElideRight; width: item.width - 60 }
    }
    Tip { callout: true; visible: item.compact && hover.containsMouse && item.tipText !== ""; text: item.tipText }
    MouseArea { id: hover; anchors.fill: parent; hoverEnabled: true; acceptedButtons: Qt.LeftButton | Qt.RightButton; onClicked: mouse => mouse.button === Qt.RightButton ? item.rightClicked() : item.clicked() }
    DropArea {
        anchors.fill: parent; enabled: item.droppable; keys: ["text/uri-list"]
        onDropped: drop => item.dropped(drop)
        Rectangle { anchors.fill: parent; radius: 2; color: "transparent"; border.width: 1; border.color: Kiki.Theme.accent; visible: parent.containsDrag }
    }
}
