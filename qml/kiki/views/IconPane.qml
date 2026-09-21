import QtQuick
import ".." as Kiki
import "../ui" as UI

// Icon grid: tiles as wide as they need to be, with a 96px icon at zoom 1 — the size Nautilus
// opens a folder at, which is what a picture folder wants before anyone reaches for the zoom.
Item {
    id: root
    property Kiki.Pane pane
    signal activate(int index)
    signal contextMenu(int index, point pos)
    readonly property real zoom: root.pane ? root.pane.iconZoom : 1
    readonly property int cellAvail: Math.max(1, width - 36)
    /// The icon at zoom 1; everything in a tile is measured from it.
    readonly property int iconSize: Math.round(96 * zoom)
    readonly property int cellTarget: iconSize + 48
    readonly property int cellCols: Math.max(1, Math.min(12, Math.round(cellAvail / cellTarget)))
    readonly property int cellW: Math.floor(cellAvail / cellCols)
    readonly property int cellH: iconSize + 72
    function setZoom(z) { if (root.pane) root.pane.iconZoom = Math.max(0.4, Math.min(2.5, z)) }

    function ensureVisible(i) { grid.positionViewAtIndex(i, GridView.Contain) }
    /// What scrolls, and the rows behind it — for the scroll probe.
    function scroller() { return { view: grid, cache: root.pane.listing } }
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

    // Right-clicking between the icons asks for the folder's menu, not nothing at all.
    MouseArea {
        anchors.fill: grid; z: -1
        acceptedButtons: Qt.RightButton
        onClicked: mouse => {
            root.pane.selection.clear()
            root.contextMenu(-1, mapToItem(null, mouse.x, mouse.y))
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
        // Drops on empty space land in the folder being shown (tiles sit above this and win).
        // Re-parented to the pane: declared here it would be a child of the grid's CONTENT, which
        // is only as tall as its rows of tiles — everything below the last row, and the margins,
        // took no drop at all, so a drag had to find a folder to land on.
        DropArea { objectName: "icon-drop-background"; parent: root; anchors.fill: parent; z: -1; keys: ["text/uri-list"]; enabled: !root.pane.isTrash; onDropped: drop => root.pane.dropInto(root.pane.uri, drop, mapToItem(null, drop.x, drop.y)) }
        delegate: Item {
            id: cell
            required property int index
            objectName: "tile-" + index
            property var row: root.pane.listing.row(index)
            property bool selected: root.pane.selection.has(index)
            /// What the tile draws: the thumbnail at its painted size, or the kind icon.
            readonly property real artWidth: (row && row.thumb && thumb.paintedWidth > 0) ? thumb.paintedWidth : root.iconSize
            readonly property real artHeight: (row && row.thumb && thumb.paintedHeight > 0) ? thumb.paintedHeight : root.iconSize
            width: grid.cellWidth; height: grid.cellHeight
            Connections { target: root.pane.listing; function onRowsUpdated(first, n) { if (cell.index >= first && cell.index < first + n) cell.row = root.pane.listing.row(cell.index) } function onReset() { cell.row = root.pane.listing.row(cell.index) } }
            Connections { target: root.pane.selection; function onChanged() { cell.selected = root.pane.selection.has(cell.index) } }
            onIndexChanged: { row = root.pane.listing.row(index); selected = root.pane.selection.has(index) }
            Item {
                id: body
                anchors.fill: parent; anchors.margins: 4
                // Two marks rather than one box around the pair: a rounded frame on the icon,
                // and a pill behind the name, so the name stays readable at any tile size.
                Rectangle {
                    visible: cell.selected
                    radius: 8
                    color: Qt.rgba(Kiki.Theme.accent.r, Kiki.Theme.accent.g, Kiki.Theme.accent.b, 0.16)
                    border.width: 1; border.color: Kiki.Theme.accent
                    // Around what is actually drawn — a picture is only as wide as it is painted,
                    // and the frame should sit the same distance from every edge of it.
                    readonly property int pad: 8
                    width: Math.round(cell.artWidth) + pad * 2
                    height: Math.round(cell.artHeight) + pad * 2
                    x: Math.round((body.width - width) / 2)
                    y: Math.round(col.y + iconBox.y + (iconBox.height - height) / 2)
                }
                Rectangle {
                    visible: cell.selected
                    radius: height / 2
                    color: Kiki.Theme.accent
                    width: Math.min(body.width, label.paintedWidth + 16)
                    height: label.paintedHeight + 6
                    x: Math.round((body.width - width) / 2); y: col.y + label.y - 3
                }
                Column {
                    id: col
                    anchors.horizontalCenter: parent.horizontalCenter; y: 14; spacing: 14; width: parent.width - 12
                    // Ignored by git: the whole tile steps back, as the row's name does in the list.
                    opacity: Kiki.Format.gitDimmed(cell.row) && !cell.selected ? 0.45 : 1
                    Item {
                        id: iconBox
                        anchors.horizontalCenter: parent.horizontalCenter; width: Math.min(root.iconSize + 20, parent.width); height: root.iconSize + 4
                        UI.KindIcon { visible: !(cell.row && cell.row.thumb); anchors.centerIn: parent; kind: cell.row ? cell.row.kind : ""; size: root.iconSize; color: Kiki.Theme.kindColor(cell.row ? cell.row.kind : "file") }
                        Image { id: thumb; visible: cell.row && cell.row.thumb; anchors.fill: parent; source: cell.row && cell.row.thumb ? "file://" + cell.row.thumb : ""; sourceSize: Qt.size(Math.round(root.iconSize * 1.6), Math.round(root.iconSize * 1.6)); fillMode: Image.PreserveAspectFit; asynchronous: true; smooth: true }
                        Rectangle { objectName: "git-dot"; visible: !!Kiki.Format.gitMark(cell.row); anchors.right: parent.right; anchors.top: parent.top; width: 10; height: 10; radius: 5; color: Kiki.Format.gitColor(cell.row ? cell.row.git : null); border.width: 2; border.color: Kiki.Theme.bg }
                    }
                    Text { id: label; width: parent.width; horizontalAlignment: Text.AlignHCenter; wrapMode: Text.WrapAnywhere; maximumLineCount: 2; elide: Text.ElideRight; text: cell.row ? cell.row.name : ""; color: cell.selected ? Kiki.Theme.bg : Kiki.Theme.fgDim; font.family: Kiki.Theme.mono; font.pixelSize: 12 }
                }
                DropArea {
                    anchors.fill: parent
                    enabled: cell.row && cell.row.isDir
                    keys: ["text/uri-list"]
                    onDropped: drop => root.pane.dropInto(root.pane.childUri(cell.row.name), drop, mapToItem(null, drop.x, drop.y))
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
                    drag.onActiveChanged: { if (drag.active) { if (!cell.selected) root.pane.selection.set(cell.index); cellDrag.Drag.mimeData = root.pane.dragMime(cell.index, pressedButtons & Qt.RightButton); cellDrag.Drag.active = true } else cellDrag.Drag.active = false }
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
