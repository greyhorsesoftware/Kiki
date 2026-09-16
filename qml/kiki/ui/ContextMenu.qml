import QtQuick
import ".." as Kiki

// The right-click menu. Items are [{ label, key, action, danger, sep, enabled }].
Rectangle {
    id: menu
    property var items: []
    property point at: Qt.point(0, 0)
    signal closed()
    visible: false
    width: 232; height: col.height + 8; radius: 2; z: 100
    color: Kiki.Theme.bgDark; border.width: 1; border.color: Kiki.Theme.gutter

    function open(itemList, pos) {
        items = itemList; at = pos; visible = true
        x = Math.min(pos.x, (parent ? parent.width : 9999) - width - 4)
        y = Math.min(pos.y, (parent ? parent.height : 9999) - height - 4)
        forceActiveFocus()
    }
    function close() { visible = false; closed() }
    Keys.onEscapePressed: close()
    onActiveFocusChanged: if (!activeFocus) close()

    Column {
        id: col; y: 4; width: parent.width
        Repeater {
            model: menu.items
            delegate: Item {
                required property var modelData
                width: col.width; height: (modelData.sep ? 5 : 0) + 26
                Rectangle { visible: modelData.sep; y: 2; width: parent.width; height: 1; color: Kiki.Theme.line }
                Rectangle {
                    y: modelData.sep ? 5 : 0; width: parent.width; height: 26
                    color: h.containsMouse && modelData.enabled !== false ? Kiki.Theme.surface : "transparent"
                    Text { x: 12; anchors.verticalCenter: parent.verticalCenter; text: modelData.label; color: modelData.enabled === false ? Kiki.Theme.gutter : (modelData.danger ? Kiki.Theme.red : Kiki.Theme.fg); font.family: Kiki.Theme.mono; font.pixelSize: Kiki.Theme.fontSize }
                    Text { anchors.right: parent.right; anchors.rightMargin: 12; anchors.verticalCenter: parent.verticalCenter; text: modelData.key || ""; color: Kiki.Theme.muted; font.family: Kiki.Theme.mono; font.pixelSize: 11 }
                    MouseArea { id: h; anchors.fill: parent; hoverEnabled: true; onClicked: if (modelData.enabled !== false) { menu.close(); modelData.action() } }
                }
            }
        }
    }
}
