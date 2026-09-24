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

    // ---------------------------------------------------------------- the lasso
    // A press between the icons and a drag draws a band, and what the band touches is selected:
    // Ctrl (or Shift) adds to what was selected before, and touching one of those again takes
    // it out — the band toggles, as Ctrl+click does. Worked out from the grid's geometry, not
    // from the tiles on screen, so a band dragged past the edge — the view scrolls under it —
    // holds rows that have no tile any more.
    property bool lassoing: false
    property point _lassoFrom: Qt.point(0, 0)      // content coordinates: it scrolls with the files
    property point _lassoTo: Qt.point(0, 0)        // viewport coordinates: it stays under the pointer
    property var _lassoBase: []
    /// The band, in content coordinates.
    function lassoRect() {
        const tx = _lassoTo.x, ty = _lassoTo.y + grid.contentY
        return Qt.rect(Math.min(_lassoFrom.x, tx), Math.min(_lassoFrom.y, ty), Math.abs(tx - _lassoFrom.x), Math.abs(ty - _lassoFrom.y))
    }
    /// The positions whose tile the rectangle touches. A tile is its cell less the gutter round
    /// it: a band drawn down the gap between two columns selects neither.
    function tilesIn(r) {
        const inset = 10, n = root.pane.listing.count, per = grid.perRow, out = []
        const c0 = Math.max(0, Math.floor((r.x + inset) / cellW)), c1 = Math.min(per - 1, Math.floor((r.x + r.width - inset) / cellW))
        const r0 = Math.max(0, Math.floor((r.y + inset) / cellH)), r1 = Math.floor((r.y + r.height - inset) / cellH)
        for (let row = r0; row <= r1; row++) {
            for (let c = c0; c <= c1; c++) {
                const i = row * per + c
                if (i >= n) return out
                const x = c * cellW, y = row * cellH
                if (r.x < x + cellW - inset && r.x + r.width > x + inset && r.y < y + cellH - inset && r.y + r.height > y + inset) out.push(i)
            }
        }
        return out
    }
    function _lassoApply() {
        const hit = tilesIn(lassoRect()), base = _lassoBase
        const keep = base.filter(i => hit.indexOf(i) < 0)
        const add = hit.filter(i => base.indexOf(i) < 0)
        root.pane.selection.setMany(keep.concat(add), hit.length ? hit[hit.length - 1] : undefined)
    }
    MouseArea {
        id: lasso
        objectName: "icon-lasso"
        anchors.fill: grid; z: -1
        acceptedButtons: Qt.LeftButton | Qt.RightButton
        property bool _down: false
        onPressed: mouse => {
            if (mouse.button !== Qt.LeftButton) return
            _down = true
            root._lassoFrom = Qt.point(mouse.x, mouse.y + grid.contentY)
            root._lassoTo = Qt.point(mouse.x, mouse.y)
            root._lassoBase = (mouse.modifiers & (Qt.ControlModifier | Qt.ShiftModifier)) ? root.pane.selection.positions() : []
        }
        onPositionChanged: mouse => {
            if (!_down) return
            root._lassoTo = Qt.point(Math.max(0, Math.min(width, mouse.x)), mouse.y)
            if (!root.lassoing && Math.abs(mouse.x - root._lassoFrom.x) + Math.abs(mouse.y + grid.contentY - root._lassoFrom.y) < 6) return
            root.lassoing = true
            root._lassoApply()
        }
        onReleased: mouse => {
            if (mouse.button !== Qt.LeftButton) return
            // A click that went nowhere: between the icons, it lets go of the selection.
            if (_down && !root.lassoing && !root._lassoBase.length) root.pane.selection.clear()
            _down = false; root.lassoing = false
        }
        onCanceled: { _down = false; root.lassoing = false }
        // Right-clicking between the icons asks for the folder's menu, not nothing at all.
        onClicked: mouse => {
            if (mouse.button !== Qt.RightButton) return
            root.pane.selection.clear()
            root.contextMenu(-1, mapToItem(null, mouse.x, mouse.y))
        }
    }
    // Past the top or the bottom the view follows, faster the further out the pointer is.
    Timer {
        interval: 16; repeat: true
        running: root.lassoing && (root._lassoTo.y < 0 || root._lassoTo.y > grid.height)
        onTriggered: {
            const over = root._lassoTo.y < 0 ? root._lassoTo.y : root._lassoTo.y - grid.height
            const to = Math.max(0, Math.min(grid.contentY + Math.max(-40, Math.min(40, over / 3)), Math.max(0, grid.contentHeight - grid.height)))
            if (to !== grid.contentY) { grid.contentY = to; root._lassoApply() }
        }
    }
    Rectangle {
        objectName: "icon-lasso-band"
        visible: root.lassoing
        z: 5
        readonly property rect r: root.lassoing ? root.lassoRect() : Qt.rect(0, 0, 0, 0)
        x: grid.x + r.x; y: grid.y + Math.max(0, r.y - grid.contentY)
        width: r.width; height: Math.max(0, Math.min(grid.height, r.y + r.height - grid.contentY) - Math.max(0, r.y - grid.contentY))
        color: Qt.rgba(Kiki.Theme.accent.r, Kiki.Theme.accent.g, Kiki.Theme.accent.b, 0.12)
        border.width: 1; border.color: Kiki.Theme.accent
    }
    /// The delegate showing row `i`, for whoever aims at it (the info popover's pointer); null
    /// while it is pooled or scrolled away.
    function rowItem(i) { return grid.itemAtIndex(i) }
    GridView {
        id: grid
        UI.NaturalScroll { }
        anchors.fill: parent; anchors.margins: 18
        clip: true; reuseItems: true; cacheBuffer: root.cellH * 6
        // A drag with the mouse moves files or draws the lasso; the wheel and two fingers scroll.
        interactive: false
        cellWidth: root.cellW; cellHeight: root.cellH
        model: root.pane.listing.count
        readonly property int perRow: Math.max(1, Math.floor(width / cellWidth))
        function tellViewport() { if (root.pane) root.pane.listing.setViewport(Math.max(0, Math.floor(contentY / cellHeight) * perRow), (Math.ceil(height / cellHeight) + 1) * perRow) }
        onContentYChanged: tellViewport()
        onHeightChanged: tellViewport()
        // Handed another pane after it was built (the right-hand view is): tell that one too.
        Connections { target: root; function onPaneChanged() { grid.tellViewport() } }
        Connections { target: root.pane.listing; function onReset() { grid.forceLayout() } }
        // Drops on empty space land in the folder being shown (tiles sit above this and win).
        // Re-parented to the pane: declared here it would be a child of the grid's CONTENT, which
        // is only as tall as its rows of tiles — everything below the last row, and the margins,
        // took no drop at all, so a drag had to find a folder to land on.
        DropTarget { objectName: "icon-drop-background"; parent: root; anchors.fill: parent; z: -1; enabled: !root.pane.isTrash; pane: root.pane; dest: root.pane.uri }
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
                    // The focused pane's selection wears the accent; the other pane's, the grey
                    // the columns view gives its trail (0.1.1).
                    color: root.pane && !root.pane.focused ? Kiki.Theme.surface : Qt.rgba(Kiki.Theme.accent.r, Kiki.Theme.accent.g, Kiki.Theme.accent.b, 0.16)
                    border.width: 1; border.color: root.pane && !root.pane.focused ? Kiki.Theme.gutter : Kiki.Theme.accent
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
                    color: root.pane && !root.pane.focused ? Kiki.Theme.surface : Kiki.Theme.accent
                    width: Math.min(body.width, label.paintedWidth + 16)
                    height: label.paintedHeight + 6
                    x: Math.round((body.width - width) / 2); y: col.y + label.y - 3
                }
                Column {
                    id: col
                    objectName: "tile-body"
                    anchors.horizontalCenter: parent.horizontalCenter; y: 14; spacing: 14; width: parent.width - 12
                    // Ignored by git: the whole tile steps back, as the row's name does in the list.
                    opacity: Kiki.Format.gitDimmed(cell.row) && !cell.selected ? 0.45 : 1
                    Item {
                        id: iconBox
                        anchors.horizontalCenter: parent.horizontalCenter; width: Math.min(root.iconSize + 20, parent.width); height: root.iconSize + 4
                        UI.KindIcon { visible: !(cell.row && cell.row.thumb); anchors.centerIn: parent; kind: cell.row ? cell.row.kind : ""; size: root.iconSize; color: Kiki.Theme.kindColor(cell.row ? cell.row.kind : "file") }
                        Image { id: thumb; visible: cell.row && cell.row.thumb; anchors.fill: parent; source: cell.row && cell.row.thumb ? "file://" + cell.row.thumb : ""; sourceSize: Qt.size(Math.round(root.iconSize * 1.6), Math.round(root.iconSize * 1.6)); fillMode: Image.PreserveAspectFit; asynchronous: true; smooth: true }
                        // A repository root has no room for a branch on a tile, so the dot it
                        // already draws carries the whole of it: the aggregate's colour (plan 15).
                        Rectangle { objectName: "git-dot"; visible: !!Kiki.Format.gitDotMark(cell.row); anchors.right: parent.right; anchors.top: parent.top; width: 10; height: 10; radius: 5; color: Kiki.Format.gitColor(Kiki.Format.gitDotMark(cell.row)); border.width: 2; border.color: Kiki.Theme.bg }
                    }
                    Text { id: label; width: parent.width; horizontalAlignment: Text.AlignHCenter; wrapMode: Text.WrapAnywhere; maximumLineCount: 2; elide: Text.ElideRight; text: cell.row ? cell.row.name : ""; color: cell.selected ? Kiki.Theme.bg : Kiki.Theme.fgDim; font.family: Kiki.Theme.mono; font.pixelSize: 12 }
                }
                DropTarget {
                    anchors.fill: parent
                    enabled: !!cell.row && cell.row.isDir
                    pane: root.pane
                    dest: cell.row ? root.pane.childUri(cell.row.name) : ""
                    Rectangle { anchors.fill: parent; radius: 2; color: "transparent"; border.width: 1; border.color: Kiki.Theme.accent; visible: parent.welcoming }
                }
                Item {
                    id: cellDrag
                    Drag.dragType: Drag.Automatic
                    Drag.supportedActions: Qt.CopyAction | Qt.MoveAction
                    Drag.proposedAction: Qt.MoveAction
                }
                MouseArea {
                    objectName: "tile-hit"
                    // The picture and the name, not the whole cell: the space round them is
                    // where a lasso starts, and a press there must not pick the file up.
                    width: Math.min(parent.width, Math.max(cell.artWidth + 16, label.paintedWidth + 16))
                    x: Math.round((parent.width - width) / 2); y: col.y; height: col.height
                    acceptedButtons: Qt.LeftButton | Qt.RightButton
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
