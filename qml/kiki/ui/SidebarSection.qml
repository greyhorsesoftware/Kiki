import QtQuick
import ".." as Kiki

Column {
    id: section
    property string title: ""
    property bool plus: false
    signal plusClicked()
    default property alias content: body.data
    // Items that sit over the header itself (a DropArea, say); putting them in `content` would
    // anchor them inside the item Column and stop it laying out.
    property alias headerData: headerItem.data
    anchors.left: parent ? parent.left : undefined; anchors.right: parent ? parent.right : undefined
    spacing: 1
    property bool compact: false
    readonly property int headerHeight: compact ? 10 : 28
    Item {
        id: headerItem
        height: section.headerHeight; width: parent.width
        Text { visible: !section.compact; x: 24; anchors.verticalCenter: parent.verticalCenter; text: section.title.toUpperCase(); color: Kiki.Theme.muted; font.family: Kiki.Theme.mono; font.pixelSize: 11; font.bold: true; font.letterSpacing: 1 }
        Rectangle {
            visible: section.plus && !section.compact; width: 24; height: 24; radius: 2; anchors.right: parent.right; anchors.rightMargin: 16; anchors.verticalCenter: parent.verticalCenter
            color: plusHover.containsMouse ? Kiki.Theme.surface : "transparent"
            Icon { anchors.centerIn: parent; name: "plus"; size: 14; color: Kiki.Theme.muted }
            MouseArea { id: plusHover; anchors.fill: parent; hoverEnabled: true; onClicked: section.plusClicked() }
        }
    }
    Column { id: body; width: parent.width; spacing: 1 }
}
