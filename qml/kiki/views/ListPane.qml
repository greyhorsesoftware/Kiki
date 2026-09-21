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
    // The chosen columns as one string. `columns` below is what the header and every row's cells
    // are built from, so it must change when the SET does and at no other time: reading
    // `Settings.view.columns` straight into it made a new array on every settings write — a
    // column resized, a slideshow delay picked — and each one threw away every cell delegate.
    readonly property string columnKey: (Kiki.Settings.view.columns || ["mtime", "size", "kind"]).join(",")
    readonly property var wantedColumns: columnKey.split(",").map(c => allColumns[c]).filter(c => c)
    // The name column never shrinks below this. The optional columns are squeezed towards their
    // own minimums first and drop off the right only when even those do not fit, so a narrower
    // pane — side by side, the inspector opening — narrows the columns rather than drawing them
    // on top of one another.
    readonly property int minNameWidth: 160
    /// The narrowest a column may be: its header label at 11px bold mono, the sort arrow beside it
    /// and a little air. Reckoned from the label rather than measured — the header is one font at
    /// one size, and a text measurement per column per frame does not earn its keep.
    function columnMin(c) { return Math.round(c.label.length * 7.5) + 30 }
    /// The width a column asks for: the drag in progress, else what was remembered (settings.toml
    /// `[view.listColumnWidths]`, one set of widths for every list view), else its default.
    function columnWanted(c) {
        const live = root.liveWidths[c.role], kept = (Kiki.Settings.view.listColumnWidths || ({}))[c.role]
        return Math.max(columnMin(c), Math.round(live || kept || c.w))
    }
    /// How many optional columns are shown: from the right, drop the ones whose minimum has no
    /// room left beside a readable name. An int, so `columns` is left alone while widths change.
    readonly property int keep: {
        let n = wantedColumns.length
        const room = root.width - 24 - minNameWidth
        while (n > 0 && wantedColumns.slice(0, n).reduce((a, c) => a + root.columnMin(c) + 12, 0) > room) n--
        return n
    }
    readonly property var columns: [{ role: "name", label: "Name" }].concat(wantedColumns.slice(0, keep))
    /// What each shown column is drawn at, by role: the width it asks for, squeezed from the right
    /// until the set fits beside a readable name. The squeeze belongs to the pane and not to the
    /// user, so it is never written back — widen the window and the remembered widths return.
    readonly property var colWidth: {
        const m = ({})
        let over = 24 + minNameWidth - root.width
        for (let i = 0; i < keep; i++) { const c = wantedColumns[i]; m[c.role] = root.columnWanted(c); over += m[c.role] + 12 }
        for (let i = keep - 1; i >= 0 && over > 0; i--) {
            const c = wantedColumns[i], give = Math.min(over, m[c.role] - root.columnMin(c))
            m[c.role] -= give; over -= give
        }
        return m
    }
    readonly property int valueWidth: columns.slice(1).reduce((a, c) => a + (root.colWidth[c.role] || c.w) + 12, 0)
    readonly property int nameWidth: Math.max(48, root.width - 24 - valueWidth)

    // ---------------------------------------------------------------- resizing a column
    /// What a drag is showing, by role, before it is remembered: the header and every row follow
    /// the pointer, and settings.toml is written once, when the button comes up — the way the info
    /// panel's edge and the line between two panes are remembered.
    property var liveWidths: ({})
    /// Widen or narrow a column. Clamped so its header stays readable and so it can never push the
    /// other columns, or the name, off the pane: what is left when the others keep their widths is
    /// all this one may take. The grip calls it; so does `shell listColumn`.
    function setColumnWidth(role, px) {
        const i = wantedColumns.findIndex(c => c.role === role)
        if (i < 0) return
        const c = wantedColumns[i]
        let w = Math.max(root.columnMin(c), Math.round(px))
        if (i < keep) {
            let others = 0
            for (let j = 0; j < keep; j++) if (j !== i) others += root.colWidth[wantedColumns[j].role] + 12
            w = Math.min(w, Math.max(root.columnMin(c), root.width - 24 - minNameWidth - others - 12))
        }
        const live = Object.assign({}, root.liveWidths); live[role] = w; root.liveWidths = live
    }
    /// The button is up, so the widths are worth remembering. A column that is switched off keeps
    /// the width it had: this writes over the remembered set, it does not replace it.
    function endColumnResize() {
        if (!Object.keys(root.liveWidths).length) return          // a press that never moved is not a resize
        Kiki.Settings.set("view", "listColumnWidths", Object.assign({}, Kiki.Settings.view.listColumnWidths || ({}), root.liveWidths))
        root.liveWidths = ({})
    }
    /// A double click on a grip puts that column back to the width it ships with.
    function resetColumnWidth(role) {
        Kiki.Settings.forget("view", "listColumnWidths", role)
        root.liveWidths = ({})
    }
    /// What the list is drawing, by role, for `shell listColumn` and `shell state`. Name is in it
    /// too, so a flow can watch it absorb what a resize gives up or takes.
    function columnWidths() { const m = Object.assign({}, root.colWidth); m.name = root.nameWidth; return m }

    function ensureVisible(i) { view.positionViewAtIndex(i, ListView.Contain) }
    /// What scrolls, and the rows behind it — for the scroll probe.
    function scroller() { return { view: view, cache: root.pane.listing } }
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
                    width: index === 0 ? root.nameWidth : (root.colWidth[modelData.role] || modelData.w)
                    height: root.headerHeight
                    Row {
                        anchors.verticalCenter: parent.verticalCenter; spacing: 6
                        layoutDirection: modelData.role === "size" ? Qt.RightToLeft : Qt.LeftToRight
                        anchors.right: modelData.role === "size" ? parent.right : undefined
                        Text { text: modelData.label.toUpperCase(); color: root.pane.sortRole === modelData.role ? Kiki.Theme.fgDim : Kiki.Theme.muted; font.family: Kiki.Theme.mono; font.pixelSize: 11; font.bold: true; font.letterSpacing: 0.6 }
                        UI.Icon { visible: root.pane.sortRole === modelData.role; name: root.pane.sortOrder === "asc" ? "sort-up" : "chev-d"; size: 12; color: Kiki.Theme.fgDim; anchors.verticalCenter: parent.verticalCenter }
                    }
                    MouseArea { anchors.fill: parent; onClicked: root.pane.setSort(modelData.role, root.pane.sortRole === modelData.role && root.pane.sortOrder === "asc" ? "desc" : "asc") }
                    // The edge a column begins at is its grip: drag it and that column follows the
                    // pointer while Name gives up or takes back the difference. It is this edge and
                    // not the far one because the value columns are laid out from the right — the
                    // far edge cannot move, only this one can. Nothing is drawn: the resize cursor
                    // on the way past is the affordance, as it is on the info panel's edge. It lies
                    // over the sort area and keeps the press, so a drag is never also a click.
                    MouseArea {
                        objectName: "col-grip-" + modelData.role
                        visible: index > 0
                        x: -4; width: 8; height: parent.height; z: 1
                        cursorShape: Qt.SplitHCursor
                        hoverEnabled: true
                        preventStealing: true
                        property real down: 0
                        onPressed: mouse => down = mouse.x
                        onPositionChanged: mouse => { if (pressed) root.setColumnWidth(modelData.role, root.colWidth[modelData.role] - (mouse.x - down)) }
                        onReleased: root.endColumnResize()
                        onDoubleClicked: root.resetColumnWidth(modelData.role)
                    }
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
        function tellViewport() { if (root.pane) root.pane.listing.setViewport(Math.max(0, Math.floor(contentY / Kiki.Theme.rowHeight)), Math.ceil(height / Kiki.Theme.rowHeight) + 1) }
        onContentYChanged: tellViewport()
        onHeightChanged: tellViewport()
        // The right-hand view is built for the left pane and handed its own a moment later: by
        // then its height has been set, so the right pane's listing was never told what was on
        // screen and went on fetching for the sixty rows it assumes.
        Connections { target: root; function onPaneChanged() { view.tellViewport() } }
        Connections { target: root.pane.listing; function onReset() { view.forceLayout() } }
        delegate: ListRow {
            required property int index
            pane: root.pane
            rowIndex: index
            width: view.width
            columns: root.columns; valueWidth: root.valueWidth; widths: root.colWidth
            onActivate: root.activate(index)
            onContextMenu: pos => root.contextMenu(index, pos)
        }
    }
    // A right-click on empty space is still a menu — the folder's, with everything that needs a
    // file greyed out. Rows sit above this and answer for themselves.
    MouseArea {
        anchors.fill: view; z: -1
        acceptedButtons: Qt.RightButton
        onClicked: mouse => {
            root.pane.selection.clear()
            root.contextMenu(-1, mapToItem(null, mouse.x, mouse.y))
        }
    }
    // Drops on empty space land in the folder being shown (rows sit above this and win).
    DropTarget {
        objectName: "list-drop-background"
        anchors.fill: view; z: -1
        enabled: !root.pane.isTrash
        pane: root.pane
        dest: root.pane.uri
    }
}
