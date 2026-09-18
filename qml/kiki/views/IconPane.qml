import QtQuick
import ".." as Kiki
import "../ui" as UI

// Icon grid: up to 7 columns, as many as fit at ~92px each, 44px kind icons.
Item {
    id: root
    property Kiki.Pane pane
    signal activate(int index)
    signal contextMenu(int index, point pos)
    readonly property real zoom: root.pane ? root.pane.iconZoom : 1
    readonly property int cellAvail: Math.max(1, width - 36)
    readonly property int cellTarget: Math.round(44 * zoom) + 48
    readonly property int cellCols: Math.max(1, Math.min(12, Math.round(cellAvail / cellTarget)))
    readonly property int cellW: Math.floor(cellAvail / cellCols)
    readonly property int cellH: Math.round(44 * zoom) + 66
    function setZoom(z) { if (root.pane) root.pane.iconZoom = Math.max(0.6, Math.min(3, z)) }

    function ensureVisible(i) { grid.positionViewAtIndex(i, GridView.Contain) }
    readonly property int perRow: grid.perRow
    readonly property int pageSize: Math.max(1, Math.floor(grid.height / cellH)) * grid.perRow

    // Spreading two fingers grows the icons; Ctrl and the wheel does the same for a mouse.
    PinchHandler {
        target: null
        property real startZoom: 1
        onActiveChanged: if (active) startZoom = root.zoom
        onActiveScaleChanged: root.setZoom(startZoom * activeScale)
    }
    WheelHandler {
        target: null
        acceptedModifiers: Qt.ControlModifier
        onWheel: event => {
            const dy = event.pixelDelta.y !== 0 ? event.pixelDelta.y : event.angleDelta.y / 8
            if (dy === 0) { event.accepted = false; return }
            root.setZoom(root.zoom * (1 + dy / 300))
            event.accepted = true
        }
    }

    GridView {
        id: grid
        UI.NaturalScroll { }
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
            Item {
                id: body
                anchors.fill: parent; anchors.margins: 4
                // The selection hugs the icon and the name rather than filling the whole cell.
                Rectangle {
                    visible: cell.selected
                    radius: 3
                    color: Qt.rgba(Kiki.Theme.accent.r, Kiki.Theme.accent.g, Kiki.Theme.accent.b, 0.16)
                    border.width: 1; border.color: Kiki.Theme.accent
                    width: Math.min(body.width, Math.max(iconBox.width, label.paintedWidth) + 16)
                    height: col.height + 12
                    x: Math.round((body.width - width) / 2); y: col.y - 6
                }
                Column {
                    id: col
                    anchors.horizontalCenter: parent.horizontalCenter; y: 14; spacing: 8; width: parent.width - 12
                    Item {
                        id: iconBox
                        anchors.horizontalCenter: parent.horizontalCenter; width: Math.min(Math.round(44 * root.zoom) + 20, parent.width); height: Math.round(44 * root.zoom) + 4
                        UI.Icon { visible: !(cell.row && cell.row.thumb); anchors.centerIn: parent; name: cell.row ? cell.row.kind : "file"; size: Math.round(44 * root.zoom); strokeWidth: 1; color: Kiki.Theme.kindColor(cell.row ? cell.row.kind : "file") }
                        Image { visible: cell.row && cell.row.thumb; anchors.fill: parent; source: cell.row && cell.row.thumb ? "file://" + cell.row.thumb : ""; sourceSize: Qt.size(Math.round(128 * root.zoom), Math.round(128 * root.zoom)); fillMode: Image.PreserveAspectFit; asynchronous: true; smooth: true }
                        Rectangle { visible: cell.row && cell.row.git && cell.row.git.state !== "clean" && cell.row.git.state !== "ignored"; anchors.right: parent.right; anchors.top: parent.top; width: 10; height: 10; radius: 5; color: Kiki.Format.gitColor(cell.row ? cell.row.git : null); border.width: 2; border.color: Kiki.Theme.bg }
                    }
                    Text { id: label; width: parent.width; horizontalAlignment: Text.AlignHCenter; wrapMode: Text.WrapAnywhere; maximumLineCount: 2; elide: Text.ElideRight; text: cell.row ? cell.row.name : ""; color: cell.selected ? Kiki.Theme.fg : Kiki.Theme.fgDim; font.family: Kiki.Theme.mono; font.pixelSize: 12 }
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
                        if (mouse.button === Qt.RightButton) { if (!cell.selected) root.pane.selection.set(cell.index); root.contextMenu(cell.index, cell.mapToItem(null, mouse.x, mouse.y)); return }
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
