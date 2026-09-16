import QtQuick
import ".." as Kiki
import "../ui" as UI

// Icon grid: 5–7 columns by width, 44px kind icons (thumbnails in plan 03).
Item {
    id: root
    property Kiki.Pane pane
    signal activate(int index)
    signal contextMenu(int index, point pos)
    readonly property int cellW: Math.floor((width - 36) / Math.max(5, Math.min(7, Math.floor((width - 36) / 130))))
    readonly property int cellH: 110

    function ensureVisible(i) { grid.positionViewAtIndex(i, GridView.Contain) }

    GridView {
        id: grid
        anchors.fill: parent; anchors.margins: 18
        clip: true; reuseItems: true; cacheBuffer: root.cellH * 6
        cellWidth: root.cellW; cellHeight: root.cellH
        model: root.pane.listing.count
        readonly property int perRow: Math.max(1, Math.floor(width / cellWidth))
        onContentYChanged: root.pane.listing.setViewport(Math.max(0, Math.floor(contentY / cellHeight) * perRow), (Math.ceil(height / cellHeight) + 1) * perRow)
        onHeightChanged: root.pane.listing.setViewport(Math.max(0, Math.floor(contentY / cellHeight) * perRow), (Math.ceil(height / cellHeight) + 1) * perRow)
        Connections { target: root.pane.listing; function onReset() { grid.forceLayout() } }
        DropArea { anchors.fill: parent; z: -1; keys: ["text/uri-list"]; enabled: !root.pane.isTrash; onDropped: drop => root.pane.dropInto(root.pane.uri, drop) }
        delegate: Item {
            id: cell
            required property int index
            property var row: root.pane.listing.row(index)
            property bool selected: root.pane.selection.has(index)
            width: grid.cellWidth; height: grid.cellHeight
            Connections { target: root.pane.listing; function onRowsUpdated(first, n) { if (cell.index >= first && cell.index < first + n) cell.row = root.pane.listing.row(cell.index) } function onReset() { cell.row = root.pane.listing.row(cell.index) } }
            Connections { target: root.pane.selection; function onChanged() { cell.selected = root.pane.selection.has(cell.index) } }
            onIndexChanged: { row = root.pane.listing.row(index); selected = root.pane.selection.has(index) }
            Rectangle {
                anchors.fill: parent; anchors.margins: 4; radius: 2
                color: cell.selected ? Qt.rgba(Kiki.Theme.accent.r, Kiki.Theme.accent.g, Kiki.Theme.accent.b, 0.16) : "transparent"
                border.width: 1; border.color: cell.selected ? Kiki.Theme.accent : "transparent"
                Column {
                    anchors.horizontalCenter: parent.horizontalCenter; y: 14; spacing: 8; width: parent.width - 12
                    Item {
                        anchors.horizontalCenter: parent.horizontalCenter; width: 64; height: 48
                        UI.Icon { visible: !(cell.row && cell.row.thumb); anchors.centerIn: parent; name: cell.row ? cell.row.kind : "file"; size: 44; strokeWidth: 1; color: Kiki.Theme.kindColor(cell.row ? cell.row.kind : "file") }
                        Image { visible: cell.row && cell.row.thumb; anchors.fill: parent; source: cell.row && cell.row.thumb ? "file://" + cell.row.thumb : ""; sourceSize: Qt.size(128, 128); fillMode: Image.PreserveAspectFit; asynchronous: true; smooth: true }
                        Rectangle { visible: cell.row && cell.row.git && cell.row.git.state !== "clean" && cell.row.git.state !== "ignored"; anchors.right: parent.right; anchors.top: parent.top; width: 10; height: 10; radius: 5; color: Kiki.Format.gitColor(cell.row ? cell.row.git : null); border.width: 2; border.color: Kiki.Theme.bg }
                    }
                    Text { width: parent.width; horizontalAlignment: Text.AlignHCenter; wrapMode: Text.WrapAnywhere; maximumLineCount: 2; elide: Text.ElideRight; text: cell.row ? cell.row.name : ""; color: cell.selected ? Kiki.Theme.fg : Kiki.Theme.fgDim; font.family: Kiki.Theme.mono; font.pixelSize: 12 }
                }
                DropArea {
                    anchors.fill: parent
                    enabled: cell.row && cell.row.isDir
                    keys: ["text/uri-list"]
                    onDropped: drop => root.pane.dropInto(root.pane.childUri(cell.row.name), drop)
                    Rectangle { anchors.fill: parent; radius: 2; color: "transparent"; border.width: 1; border.color: Kiki.Theme.accent; visible: parent.containsDrag }
                }
                Item {
                    id: cellDrag
                    Drag.dragType: Drag.Automatic
                    Drag.supportedActions: Qt.CopyAction | Qt.MoveAction
                    Drag.proposedAction: Qt.MoveAction
                }
                MouseArea {
                    anchors.fill: parent; acceptedButtons: Qt.LeftButton | Qt.RightButton
                    drag.target: cellDrag; drag.threshold: 8
                    drag.onActiveChanged: { if (drag.active) { if (!cell.selected) root.pane.selection.set(cell.index); cellDrag.Drag.mimeData = root.pane.dragMime(cell.index); cellDrag.Drag.active = true } else cellDrag.Drag.active = false }
                    onClicked: mouse => {
                        if (mouse.button === Qt.RightButton) { if (!cell.selected) root.pane.selection.set(cell.index); root.contextMenu(cell.index, Qt.point(mouse.x, mouse.y)); return }
                        if (mouse.modifiers & Qt.ShiftModifier) root.pane.selection.range(cell.index)
                        else if (mouse.modifiers & Qt.ControlModifier) root.pane.selection.toggle(cell.index)
                        else root.pane.selection.set(cell.index)
                    }
                    onDoubleClicked: root.activate(cell.index)
                }
            }
        }
    }
}
