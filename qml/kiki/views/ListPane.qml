import QtQuick
import ".." as Kiki
import "../ui" as UI

// List view: sortable header, one row per entry, rows read from the window cache.
Item {
    id: root
    property Kiki.Pane pane
    signal activate(int index)
    signal contextMenu(int index, point pos)
    property int headerHeight: 30
    readonly property var columns: [{ role: "name", label: "Name" }, { role: "mtime", label: "Modified", w: 160 }, { role: "size", label: "Size", w: 80 }, { role: "kind", label: "Kind", w: 120 }]

    function ensureVisible(i) { view.positionViewAtIndex(i, ListView.Contain) }

    Rectangle {
        id: header; width: parent.width; height: root.headerHeight; color: Kiki.Theme.bg
        Rectangle { anchors.bottom: parent.bottom; width: parent.width; height: 1; color: Kiki.Theme.line }
        Row {
            anchors.fill: parent; anchors.leftMargin: 12; anchors.rightMargin: 12; spacing: 12
            Repeater {
                model: root.columns
                delegate: Item {
                    required property var modelData
                    required property int index
                    width: modelData.w || (root.width - 24 - 36 - 160 - 80 - 120)
                    height: root.headerHeight
                    Row {
                        anchors.verticalCenter: parent.verticalCenter; spacing: 6
                        layoutDirection: modelData.role === "size" ? Qt.RightToLeft : Qt.LeftToRight
                        anchors.right: modelData.role === "size" ? parent.right : undefined
                        Text { text: modelData.label.toUpperCase(); color: root.pane.sortRole === modelData.role ? Kiki.Theme.fgDim : Kiki.Theme.muted; font.family: Kiki.Theme.mono; font.pixelSize: 11; font.bold: true; font.letterSpacing: 0.6 }
                        UI.Icon { visible: root.pane.sortRole === modelData.role; name: root.pane.sortOrder === "asc" ? "sort-up" : "chev-d"; size: 12; color: Kiki.Theme.fgDim; anchors.verticalCenter: parent.verticalCenter }
                    }
                    MouseArea { anchors.fill: parent; onClicked: root.pane.setSort(modelData.role, root.pane.sortRole === modelData.role && root.pane.sortOrder === "asc" ? "desc" : "asc") }
                }
            }
        }
    }

    ListView {
        id: view
        anchors.top: header.bottom; width: parent.width; height: parent.height - header.height
        clip: true; reuseItems: true; cacheBuffer: Kiki.Theme.rowHeight * 40
        model: root.pane.listing.count
        onContentYChanged: root.pane.listing.setViewport(Math.max(0, Math.floor(contentY / Kiki.Theme.rowHeight)), Math.ceil(height / Kiki.Theme.rowHeight) + 1)
        onHeightChanged: root.pane.listing.setViewport(Math.max(0, Math.floor(contentY / Kiki.Theme.rowHeight)), Math.ceil(height / Kiki.Theme.rowHeight) + 1)
        Connections { target: root.pane.listing; function onReset() { view.forceLayout() } }
        delegate: ListRow {
            required property int index
            pane: root.pane
            rowIndex: index
            width: view.width
            onActivate: root.activate(index)
            onContextMenu: pos => root.contextMenu(index, pos)
        }
    }
}
