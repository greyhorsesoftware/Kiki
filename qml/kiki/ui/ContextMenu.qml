import QtQuick
import ".." as Kiki

// The right-click menu. Items are [{ label, key, action, danger, sep, enabled, checked }]; `checked`
// (true/false) draws a check column, undefined draws none; `sep` draws a divider above the item.
// An item carrying `items` opens them beside it (Send via Tailscale ▸ peers). One whose list has
// to be asked for carries `load: fill => …` as well: `items` is what shows while waiting, and
// `fill(list)` replaces it — once; the answer is kept on the item.
// While it is open the menu covers its parent, so a click anywhere outside the box closes it.
Item {
    id: menu
    property var items: []
    /// Whether an item is greyed out. Not `enabled === false`: a condition like `row && row.isDir`
    /// hands back null when there is no row, and null is not false — the item would come out
    /// looking live. Absent still means enabled.
    function off(it) { return it.enabled !== undefined && !it.enabled }
    property point at: Qt.point(0, 0)
    signal closed()
    // The menu box itself: the scrim fills the parent, so its geometry is the box's.
    readonly property alias box: box
    visible: false
    anchors.fill: parent
    z: 100

    /// Keep the box inside the window. Called again when its height settles: the items were only
    /// assigned a moment ago, so on the first pass the box is still the height of the last menu
    /// and a long one opened near the bottom would hang off it.
    function place() {
        box.x = Math.max(0, Math.min(menu.at.x, menu.width - box.width - 4))
        box.y = Math.max(0, Math.min(menu.at.y, menu.height - box.height - 4))
    }
    function open(itemList, pos) {
        items = itemList; at = pos; visible = true
        place()
        box.forceActiveFocus()
    }
    function close() { visible = false; subItems = []; subLabel = ""; closed() }
    /// Show `it`'s submenu, asking for its list the first time if it has to be fetched.
    function showSub(it, y) {
        subItems = it.items; subLabel = it.label; subY = y
        if (!it.load || it._asked) return
        it._asked = true
        it.load(list => {
            it.items = list
            if (menu.visible && menu.subLabel === it.label) menu.subItems = list
        })
    }
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
        color: "transparent"; border.width: 1; border.color: Kiki.Theme.gutter
        Frost { anchors.fill: parent; radius: parent.radius; z: -1 }
        onHeightChanged: if (menu.visible) menu.place()
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
                    Item {
                        y: parent.sep ? 5 : 0; width: parent.width; height: 26
                        // The highlight is a pill inside the row, not a bar across the box: it
                        // stops short of the border on both sides and fades in and out.
                        Rectangle {
                            objectName: "menu-highlight"
                            x: 4; y: 1; width: parent.width - 8; height: parent.height - 2; radius: 5
                            color: Kiki.Theme.surface
                            opacity: h.containsMouse && !menu.off(modelData) ? 1 : 0
                            Behavior on opacity { NumberAnimation { duration: 90 } }
                        }
                        Text { visible: modelData.checked !== undefined; x: 10; anchors.verticalCenter: parent.verticalCenter; text: modelData.checked ? "✓" : ""; color: Kiki.Theme.accent; font.family: Kiki.Theme.mono; font.pixelSize: Kiki.Theme.fontSize }
                        Icon { visible: modelData.icon !== undefined; x: 12; anchors.verticalCenter: parent.verticalCenter; name: modelData.icon || "file"; size: 14; color: menu.off(modelData) ? Kiki.Theme.gutter : Kiki.Theme.accent }
                        Text { x: modelData.icon !== undefined ? 34 : (modelData.checked !== undefined ? 26 : 12); anchors.verticalCenter: parent.verticalCenter; text: modelData.label; color: menu.off(modelData) ? Kiki.Theme.gutter : (modelData.danger ? Kiki.Theme.danger : Kiki.Theme.fg); font.family: Kiki.Theme.mono; font.pixelSize: Kiki.Theme.fontSize }
                        Text { visible: !parent.parent.hasSub; anchors.right: parent.right; anchors.rightMargin: 12; anchors.verticalCenter: parent.verticalCenter; text: modelData.key || ""; color: Kiki.Theme.muted; font.family: Kiki.Theme.mono; font.pixelSize: 11 }
                        Icon { visible: parent.parent.hasSub; anchors.right: parent.right; anchors.rightMargin: 10; anchors.verticalCenter: parent.verticalCenter; name: "chev-r"; size: 12; color: Kiki.Theme.muted }
                        MouseArea {
                            id: h; anchors.fill: parent; hoverEnabled: true
                            onEntered: {
                                // A greyed-out row has nothing to show: hovering "Share" with
                                // nothing selected must not fly a submenu out of it.
                                if (parent.parent.hasSub && !menu.off(modelData)) menu.showSub(modelData, parent.parent.y)
                                else { menu.subItems = []; menu.subLabel = "" }
                            }
                            onClicked: {
                                if (menu.off(modelData)) return
                                if (parent.parent.hasSub) { menu.showSub(modelData, parent.parent.y); return }
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
        color: "transparent"; border.width: 1; border.color: Kiki.Theme.gutter
        Frost { anchors.fill: parent; radius: parent.radius; z: -1 }
        x: Math.min(box.x + box.width - 2, menu.width - width - 4)
        y: Math.max(0, Math.min(box.y + menu.subY, menu.height - height - 4))
        MouseArea { anchors.fill: parent }
        Column {
            id: subCol; y: 4; width: parent.width
            Repeater {
                model: menu.subItems
                delegate: Rectangle {
                    id: subRow
                    required property var modelData
                    objectName: "menu-" + modelData.label
                    readonly property bool sep: modelData.sep === true
                    width: subCol.width; height: (sep ? 5 : 0) + 26
                    color: "transparent"
                    Rectangle {
                        objectName: "menu-highlight"
                        x: 4; y: (subRow.sep ? 5 : 0) + 1; width: parent.width - 8; height: 24; radius: 5
                        color: Kiki.Theme.surface
                        opacity: sh.containsMouse && !menu.off(modelData) ? 1 : 0
                        Behavior on opacity { NumberAnimation { duration: 90 } }
                    }
                    Text { visible: !!modelData.key; anchors.right: parent.right; anchors.rightMargin: 12; y: subRow.sep ? 5 : 0; height: 26; verticalAlignment: Text.AlignVCenter; text: modelData.key || ""; color: Kiki.Theme.muted; font.family: Kiki.Theme.mono; font.pixelSize: 11 }
                    Rectangle { visible: parent.sep; y: 2; width: parent.width; height: 1; color: Kiki.Theme.line }
                    Icon { visible: modelData.icon !== undefined; x: 12; y: (parent.sep ? 5 : 0) + 6; name: modelData.icon || "file"; size: 14; color: menu.off(modelData) ? Kiki.Theme.gutter : Kiki.Theme.accent }
                    Text { x: modelData.icon !== undefined ? 34 : 12; y: parent.sep ? 5 : 0; height: 26; verticalAlignment: Text.AlignVCenter; width: parent.width - (modelData.icon !== undefined ? 46 : 24); elide: Text.ElideRight; text: modelData.label; color: menu.off(modelData) ? Kiki.Theme.gutter : Kiki.Theme.fg; font.family: Kiki.Theme.mono; font.pixelSize: Kiki.Theme.fontSize }
                    MouseArea {
                        id: sh
                        anchors.fill: parent; hoverEnabled: true
                        onClicked: {
                            if (menu.off(modelData)) return
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
