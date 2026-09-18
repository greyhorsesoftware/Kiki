import QtQuick
import ".." as Kiki

// The right-click menu. Items are [{ label, key, action, danger, sep, enabled, checked }]; `checked`
// (true/false) draws a check column, undefined draws none; `sep` draws a divider above the item.
// While it is open the menu covers its parent, so a click anywhere outside the box closes it.
Item {
    id: menu
    property var items: []
    property point at: Qt.point(0, 0)
    signal closed()
    // The menu box itself: the scrim fills the parent, so its geometry is the box's.
    readonly property alias box: box
    visible: false
    anchors.fill: parent
    z: 100

    function open(itemList, pos) {
        items = itemList; at = pos; visible = true
        box.x = Math.max(0, Math.min(pos.x, menu.width - box.width - 4))
        box.y = Math.max(0, Math.min(pos.y, menu.height - box.height - 4))
        box.forceActiveFocus()
    }
    function close() { visible = false; subItems = []; subLabel = ""; closed() }
    /// Replace the items of a submenu that is already open (or of the row, for the next hover).
    function refill(label, items) {
        const all = menu.items.slice()
        for (const it of all) if (it.label === label) it.items = items
        menu.items = all
        if (menu.subLabel === label) menu.subItems = items
    }
    // One level of submenu: an item carrying `items` opens them beside it.
    property var subItems: []
    /// Which item's submenu is showing, so a list that arrives late can replace it in place.
    property string subLabel: ""
    property real subY: 0

    MouseArea { anchors.fill: parent; acceptedButtons: Qt.LeftButton | Qt.RightButton; onClicked: menu.close() }

    Rectangle {
        id: box
        width: 232; height: col.height + 8; radius: 2
        color: Kiki.Theme.bgDark; border.width: 1; border.color: Kiki.Theme.gutter
        focus: true
        Keys.onEscapePressed: menu.close()
        MouseArea { anchors.fill: parent }          // clicks in the box never reach the scrim

        Column {
            id: col; y: 4; width: parent.width
            Repeater {
                model: menu.items
                delegate: Item {
                    required property var modelData
                    objectName: "menu-" + modelData.label
                    // `sep` is absent on most items, and an absent value is not false: it has to be
                    // compared, or every row draws a divider.
                    readonly property bool sep: modelData.sep === true
                    readonly property bool hasSub: modelData.items !== undefined && modelData.items.length > 0
                    width: col.width; height: (sep ? 5 : 0) + 26
                    Rectangle { visible: parent.sep; y: 2; width: parent.width; height: 1; color: Kiki.Theme.line }
                    Rectangle {
                        y: parent.sep ? 5 : 0; width: parent.width; height: 26
                        color: h.containsMouse && modelData.enabled !== false ? Kiki.Theme.surface : "transparent"
                        Text { visible: modelData.checked !== undefined; x: 10; anchors.verticalCenter: parent.verticalCenter; text: modelData.checked ? "✓" : ""; color: Kiki.Theme.accent; font.family: Kiki.Theme.mono; font.pixelSize: Kiki.Theme.fontSize }
                        Icon { visible: modelData.icon !== undefined; x: 12; anchors.verticalCenter: parent.verticalCenter; name: modelData.icon || "file"; size: 14; color: modelData.enabled === false ? Kiki.Theme.gutter : Kiki.Theme.accent }
                        Text { x: modelData.icon !== undefined ? 34 : (modelData.checked !== undefined ? 26 : 12); anchors.verticalCenter: parent.verticalCenter; text: modelData.label; color: modelData.enabled === false ? Kiki.Theme.gutter : (modelData.danger ? Kiki.Theme.red : Kiki.Theme.fg); font.family: Kiki.Theme.mono; font.pixelSize: Kiki.Theme.fontSize }
                        Text { visible: !parent.parent.hasSub; anchors.right: parent.right; anchors.rightMargin: 12; anchors.verticalCenter: parent.verticalCenter; text: modelData.key || ""; color: Kiki.Theme.muted; font.family: Kiki.Theme.mono; font.pixelSize: 11 }
                        Icon { visible: parent.parent.hasSub; anchors.right: parent.right; anchors.rightMargin: 10; anchors.verticalCenter: parent.verticalCenter; name: "chev-r"; size: 12; color: Kiki.Theme.muted }
                        MouseArea {
                            id: h; anchors.fill: parent; hoverEnabled: true
                            onEntered: {
                                if (parent.parent.hasSub) { menu.subItems = modelData.items; menu.subLabel = modelData.label; menu.subY = parent.parent.y }
                                else { menu.subItems = []; menu.subLabel = "" }
                            }
                            onClicked: {
                                if (modelData.enabled === false) return
                                if (parent.parent.hasSub) { menu.subItems = modelData.items; menu.subLabel = modelData.label; menu.subY = parent.parent.y; return }
                                const act = modelData.action
                                menu.close()
                                if (act) act()
                            }
                        }
                    }
                }
            }
        }
    }

    Rectangle {
        id: subBox
        visible: menu.subItems.length > 0
        width: 232; height: subCol.height + 8; radius: 2
        color: Kiki.Theme.bgDark; border.width: 1; border.color: Kiki.Theme.gutter
        x: Math.min(box.x + box.width - 2, menu.width - width - 4)
        y: Math.max(0, Math.min(box.y + menu.subY, menu.height - height - 4))
        MouseArea { anchors.fill: parent }
        Column {
            id: subCol; y: 4; width: parent.width
            Repeater {
                model: menu.subItems
                delegate: Rectangle {
                    required property var modelData
                    objectName: "menu-" + modelData.label
                    readonly property bool sep: modelData.sep === true
                    width: subCol.width; height: (sep ? 5 : 0) + 26
                    color: sh.containsMouse && modelData.enabled !== false ? Kiki.Theme.surface : "transparent"
                    Rectangle { visible: parent.sep; y: 2; width: parent.width; height: 1; color: Kiki.Theme.line }
                    Icon { visible: modelData.icon !== undefined; x: 12; y: (parent.sep ? 5 : 0) + 6; name: modelData.icon || "file"; size: 14; color: modelData.enabled === false ? Kiki.Theme.gutter : Kiki.Theme.accent }
                    Text { x: modelData.icon !== undefined ? 34 : 12; y: parent.sep ? 5 : 0; height: 26; verticalAlignment: Text.AlignVCenter; width: parent.width - (modelData.icon !== undefined ? 46 : 24); elide: Text.ElideRight; text: modelData.label; color: modelData.enabled === false ? Kiki.Theme.gutter : Kiki.Theme.fg; font.family: Kiki.Theme.mono; font.pixelSize: Kiki.Theme.fontSize }
                    MouseArea {
                        id: sh
                        anchors.fill: parent; hoverEnabled: true
                        onClicked: {
                            if (modelData.enabled === false) return
                            const act = modelData.action
                            menu.close()
                            if (act) act()
                        }
                    }
                }
            }
        }
    }
}
