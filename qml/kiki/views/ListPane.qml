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
    // Optional columns come from Settings (General → Columns); Name is always first.
    readonly property var allColumns: ({ mtime: { role: "mtime", label: "Modified", w: 160 }, size: { role: "size", label: "Size", w: 80 }, kind: { role: "kind", label: "Kind", w: 120 }, atime: { role: "atime", label: "Accessed", w: 150 } })
    readonly property var wantedColumns: (Kiki.Settings.view.columns || ["mtime", "size", "kind"]).map(c => allColumns[c]).filter(c => c)
    // The name column never shrinks below this; optional columns drop off the right until it fits,
    // so a narrow pane drops columns instead of drawing them on top of one another.
    readonly property int minNameWidth: 160
    readonly property var columns: {
        let keep = wantedColumns.length
        while (keep > 0 && root.width - 24 - wantedColumns.slice(0, keep).reduce((a, c) => a + c.w + 12, 0) < minNameWidth) keep--
        return [{ role: "name", label: "Name" }].concat(wantedColumns.slice(0, keep))
    }
    readonly property int valueWidth: columns.slice(1).reduce((a, c) => a + c.w + 12, 0)
    readonly property int nameWidth: Math.max(48, root.width - 24 - valueWidth)

    function ensureVisible(i) { view.positionViewAtIndex(i, ListView.Contain) }
    /// The delegate showing row `i`, asked of the view rather than found by name: delegates are
    /// pooled and a recycled one can still answer to the name it had in the last folder.
    function rowItem(i) { return view.itemAtIndex(i) }
    readonly property int perRow: 1
    readonly property int pageSize: Math.max(1, Math.floor(view.height / Kiki.Theme.rowHeight))

    Rectangle {
        id: header; width: parent.width; height: root.headerHeight; color: Kiki.Theme.bg; clip: true
        Rectangle { anchors.bottom: parent.bottom; width: parent.width; height: 1; color: Kiki.Theme.line }
        Row {
            anchors.fill: parent; anchors.leftMargin: 12; anchors.rightMargin: 12; spacing: 12
            Repeater {
                model: root.columns
                delegate: Item {
                    required property var modelData
                    required property int index
                    objectName: "header-" + modelData.role
                    width: modelData.w || root.nameWidth
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
        UI.NaturalScroll { }
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
            columns: root.columns; valueWidth: root.valueWidth
            onActivate: root.activate(index)
            onContextMenu: pos => root.contextMenu(index, pos)
        }
    }
    // Drops on empty space land in the folder being shown (rows sit above this and win).
    DropArea {
        anchors.fill: view; z: -1
        keys: ["text/uri-list"]
        enabled: !root.pane.isTrash
        onDropped: drop => root.pane.dropInto(root.pane.uri, drop)
    }
}
