import QtQuick
import ".." as Kiki
import "../ui" as UI

// Miller columns. Each column is its own WindowCache; selecting a folder pushes
// the next column; selecting a file leaves room for the inspector (plan 03).
Item {
    id: root
    property Kiki.Pane pane
    signal activate(string uri)
    /// A right click on a row, with the URI of that row — which may live in any column, not just
    /// the one the pane's listing is on.
    signal contextMenu(string uri, var row, point pos)
    /// A right click on a column's empty space: the menu for that folder, which need not be the
    /// folder the pane is on.
    signal contextMenuFolder(string uri, point pos)
    signal fileSelected(string uri)
    signal edit(string uri, int line)
    /// What a column is when the strip is full of them.
    property int columnWidthBase: 220
    /// The narrowest a column can be squeezed before names stop being readable; past that the
    /// strip scrolls instead.
    readonly property int columnWidthMin: 150
    /// The columns own the width the info panel does not take. One that has been dragged to a
    /// width keeps it (`handWidths`); the rest share what is left equally — a folder or two gets
    /// wide columns rather than a wide panel and a gap. This is that share.
    readonly property int columnWidth: {
        if (columns.length === 0) return columnWidthBase
        let room = strip.width - (inspectedUri !== "" ? inspectorWidth : 0), free = 0
        for (let i = 0; i < columns.length; i++) { const w = handWidth(i); if (w > 0) room -= w; else free++ }
        return free ? Math.max(columnWidthMin, Math.floor(room / free)) : columnWidthBase
    }
    // ---------------------------------------------------------------- resizing a column
    /// Widths set by hand, by the column's PLACE in the strip ("c0", "c1", …), not by its folder:
    /// the second column is as wide as it was left whatever is in it, as in Finder. Kept in
    /// settings.toml `[view.columnsWidths]`; `liveWidths` is a drag that has not ended yet, so
    /// the file is written once, when the button comes up.
    property var liveWidths: ({})
    readonly property int columnWidthMax: 900
    function handWidth(i) {
        const k = "c" + i, live = liveWidths[k], kept = (Kiki.Settings.view.columnsWidths || ({}))[k]
        const w = live !== undefined ? live : kept
        return w > 0 ? Math.max(columnWidthMin, Math.min(columnWidthMax, w)) : 0
    }
    function widthOf(i) { return handWidth(i) || columnWidth }
    /// Where each column starts, and — the last entry — where they end.
    readonly property var columnX: {
        const xs = [0]
        for (let i = 0; i < columns.length; i++) xs.push(xs[i] + widthOf(i))
        return xs
    }
    function setColumnWidth(i, px) {
        const live = Object.assign({}, liveWidths); live["c" + i] = Math.max(columnWidthMin, Math.min(columnWidthMax, Math.round(px))); liveWidths = live
    }
    function endColumnResize() {
        if (!Object.keys(liveWidths).length) return               // a press that never moved is not a resize
        Kiki.Settings.set("view", "columnsWidths", Object.assign({}, Kiki.Settings.view.columnsWidths || ({}), liveWidths))
        liveWidths = ({})
    }
    /// A double click on a column's edge: back to sharing the room with the others.
    function resetColumnWidth(i) {
        Kiki.Settings.forget("view", "columnsWidths", "c" + i)
        liveWidths = ({})
    }
    /// The wheel, turned sideways: Shift and the wheel anywhere, and the plain wheel over a column
    /// with nothing of its own to scroll. A mouse has no sideways wheel, and without this there
    /// was no way back to the first column but the keys.
    function sideways(d) { strip.contentX = Math.max(0, Math.min(strip.contentX + d, Math.max(0, stripWidth - strip.width))) }
    property string home: ""
    property var columns: []          // [{ uri, cache, selected }]
    property string inspectedUri: ""
    property var inspectedRow: null
    // Keyboard navigation: the column the arrows act on, and a selection whose row has not
    // arrived from the daemon yet (resolved by onRowsUpdated below).
    property int focusCol: 0
    property int pendingIndex: -1
    /// 0 until someone drags the info column's edge, and that width from then on.
    property int inspectorW: 0
    /// The info column is at its widest by default and drags narrower, never wider: the columns
    /// are what the view is for. Computed without reference to `columnWidth`, or the two would
    /// chase each other.
    readonly property int inspectorMax: Math.min(Kiki.Theme.inspectorWidth, Math.floor(strip.width * 0.45))
    readonly property int inspectorMin: 240
    readonly property int inspectorWidth: inspectorW > 0
        ? Math.max(inspectorMin, Math.min(inspectorW, inspectorMax))
        : inspectorMax
    readonly property alias scrollX: strip.contentX
    readonly property int stripWidth: columnX[columns.length] + (inspectedUri !== "" ? inspectorWidth : 0)

    /// One empty element per column, kept in step with `columns` by appending and removing at
    /// the end — the only kind of change that leaves the other columns' delegates alone.
    ListModel { id: slots }
    onColumnsChanged: {
        while (slots.count > columns.length) slots.remove(slots.count - 1)
        while (slots.count < columns.length) slots.append({})
        // Anything that moves the columns about — stepping into a folder, a file arriving in one
        // — takes the rename editor with it: it belongs to one row of one column, and that row
        // may not be where it was.
        cancelRename()
    }

    Component.onCompleted: rebuild()
    // The shell makes the view first and says whose it is second (`onLoaded: item.pane = …`). The
    // columns built in between are the DEFAULT pane's — so side by side the right pane's column
    // view showed the left pane's folder under the right pane's path. Build them again.
    onPaneChanged: rebuild()
    Connections { target: root.pane; function onNavigated(uri) { root.rebuild() } }

    function rebuild() {
        // Only what this view made. The first column is the pane's own listing, and when the
        // pane changes under us the OLD pane's listing is "not this pane's" too — comparing
        // against the current pane destroyed the other pane's listing out from under it.
        for (const c of columns) if (c.own) c.cache.destroy()
        columns = [{ uri: root.pane.uri, cache: root.pane.listing, selected: -1 }]
        focusCol = 0; pendingIndex = -1
        inspectedUri = ""; inspectedRow = null
    }
    /// Where the columns have got to: the deepest folder open, which is what the path over the
    /// view shows. The pane itself stays on the FIRST column's folder — drilling in opens columns,
    /// it does not navigate — so the path read `folderA` however far down C was.
    readonly property string shownUri: columns.length ? columns[columns.length - 1].uri : (pane ? pane.uri : "")
    /// A pill clicked in that path: if the folder is one of the columns, the columns come back to
    /// it — it becomes the last one, nothing in it chosen. False when it is not (a folder above
    /// the first column), and the pane opens it as it always did.
    function backTo(uri) {
        const want = (uri || "").replace(/\/+$/, "")
        const i = columns.findIndex(c => c.uri.replace(/\/+$/, "") === want)
        if (i < 0) return false
        for (let k = i + 1; k < columns.length; k++) if (columns[k].own) columns[k].cache.destroy()
        const cols = columns.slice(0, i + 1)
        cols[i] = Object.assign({}, cols[i], { selected: -1 })
        columns = cols
        focusCol = i; pendingIndex = -1
        inspectedUri = ""; inspectedRow = null
        ensureVisible(true)
        return true
    }
    /// Move the highlight within a column without opening anything.
    function markSelected(col, index) {
        const cols = columns.slice()
        cols[col] = Object.assign({}, cols[col], { selected: index })
        columns = cols
        focusCol = col
    }
    function push(fromCol, index, row) {
        const cols = columns.slice(0, fromCol + 1)
        cols[fromCol] = Object.assign({}, cols[fromCol], { selected: index })
        for (let i = fromCol + 1; i < columns.length; i++) columns[i].cache.destroy()
        if (row.isDir) {
            const uri = cols[fromCol].uri.replace(/\/+$/, "") + "/" + encodeURIComponent(row.name)
            // The pane's own daemon, which is the real one except under a test.
            const cache = cacheComp.createObject(root, { daemon: root.pane.listing.daemon })
            cache.open(uri)
            cols.push({ uri: uri, cache: cache, selected: -1, own: true })
            root.inspectedUri = ""; root.inspectedRow = null
        } else {
            root.inspectedUri = cols[fromCol].uri.replace(/\/+$/, "") + "/" + encodeURIComponent(row.name); root.inspectedRow = row
            root.fileSelected(root.inspectedUri)
        }
        columns = cols
        focusCol = fromCol
        pendingIndex = -1
        ensureVisible(false)
    }

    // Arrows. Up/Down move inside the focused column and the column to its right follows;
    // Left steps back to the parent column, Right walks into the selected folder.
    function moveKey(delta) {
        const c = columns[focusCol]; if (!c || !c.cache.count) return
        const n = c.cache.count
        const i = c.selected < 0 ? (delta > 0 ? 0 : n - 1) : Math.max(0, Math.min(n - 1, c.selected + delta))
        select(focusCol, i)
    }
    function select(col, index) {
        const c = columns[col]; if (!c) return
        const r = c.cache.row(index)
        if (r) { push(col, index, r); return }
        // The row is outside what the daemon has sent: show the selection and finish on arrival.
        const cols = columns.slice(0, col + 1)
        for (let i = col + 1; i < columns.length; i++) columns[i].cache.destroy()
        cols[col] = Object.assign({}, cols[col], { selected: index })
        inspectedUri = ""; inspectedRow = null
        columns = cols
        focusCol = col
        pendingIndex = index
        c.cache.setViewport(Math.max(0, index - 20), 41)
        ensureVisible(false)
    }
    /// What this view has chosen: the row highlighted in the key column — the one last clicked or
    /// moved in, which draws its highlight in the accent while the columns behind it, the trail
    /// drilled down, keep theirs in grey. It need not be in the folder the pane is standing in,
    /// so the pane's own selection cannot answer for it.
    function selectedRow() { const c = columns[focusCol]; return c && c.selected >= 0 ? c.cache.row(c.selected) : null }
    function selectedUris() {
        const r = selectedRow()
        return r ? [columns[focusCol].uri.replace(/\/+$/, "") + "/" + encodeURIComponent(r.name)] : []
    }
    /// The folder the key column is on — where the user is working, and so where a new folder
    /// goes. It need not be the folder the pane is standing in.
    function keyUri() { const c = columns[focusCol]; return c ? c.uri : "" }
    /// Where `Ctrl+Shift+N` makes its folder: **inside the folder highlighted in the key column**
    /// (owner, 2026-09-21) — you clicked Projects, the new folder is one of Projects' — and in the
    /// key column's own folder when what is highlighted is a file, or nothing is. The highlighted
    /// folder's column is opened if it is not showing (a row can be marked without being walked
    /// into), because that column is where the new row and its editor will be.
    function newFolderUri() {
        const c = columns[focusCol], r = selectedRow()
        if (!c || !r || !r.isDir) return keyUri()
        const uri = selectedUris()[0]
        if (columnOn(uri) < 0) push(focusCol, c.selected, r)
        return uri
    }
    /// Which column is showing `uri`, or -1; trailing slashes are not a difference.
    function columnOn(uri) {
        const u = (uri || "").replace(/\/+$/, "")
        for (let i = 0; i < columns.length; i++) if (columns[i].uri.replace(/\/+$/, "") === u) return i
        return -1
    }

    // ---------------------------------------------------------------- renaming a row in place
    /// The row whose inline editor is open, as a column and an index into it; -1 for neither when
    /// none is. Its own rather than the pane's `renamingIndex`, because the row need not be in the
    /// pane's listing at all — a column drilled into is a listing of its own.
    property int renamingCol: -1
    property int renamingIndex: -1
    /// F2: the editor over the row the key column highlights, in the column it lives in, so the
    /// view stays where it is. Nothing highlighted is nothing to rename.
    function beginRename() { const c = columns[focusCol]; if (c && c.selected >= 0) renameAt(focusCol, c.selected) }
    /// The editor over one row of one column, whichever way it was asked for — F2, the row menu's
    /// Rename, or a new folder landing. The row becomes the column's highlighted one first: the
    /// editor and the highlight are never on two different rows. Answers whether it opened.
    function renameAt(col, index) {
        const c = columns[col]
        if (!c || !c.cache || !c.cache.row(index)) return false
        // Marking replaces `columns`, which takes the editor off (see onColumnsChanged) — so it
        // goes first, and the editor is opened on what is by then the highlighted row.
        if (focusCol !== col || c.selected !== index) markSelected(col, index)
        const list = _lists[col]; if (list) list.positionViewAtIndex(index, ListView.Contain)
        renamingCol = col
        renamingIndex = index
        return true
    }
    function cancelRename() { renamingCol = -1; renamingIndex = -1 }
    /// What the shell answers `Ops.listingNeeded` with: the column showing `uri`, its own rows,
    /// and the editor over one of them — so a folder made here is named here, in the column the
    /// user is working in, rather than in a list view this one was thrown away for. Nothing when
    /// no column is on that folder.
    function listingFor(uri) {
        if (columnOn(uri) < 0) return null
        // Looked up again on every call, never held: the columns are rebuilt on every selection,
        // and the one this folder is in can have another index — or be gone — a moment later.
        const cache = () => { const i = root.columnOn(uri); return i < 0 ? null : root.columns[i].cache }
        return {
            count: () => { const c = cache(); return c ? c.count : 0 },
            row: i => { const c = cache(); return c ? c.row(i) : null },
            rename: i => root.renameAt(root.columnOn(uri), i),
        }
    }

    /// True when it moved; false at the leftmost column, so the caller can go up a directory.
    function focusLeft() {
        if (focusCol <= 0) return false
        focusCol--
        ensureVisible(true)
        return true
    }
    function focusRight() {
        const c = columns[focusCol]; if (!c) return
        if (c.selected < 0) { select(focusCol, 0); return }
        // The folder under the cursor may not have opened its column yet (its row can arrive
        // after the selection did), so open it here before stepping in.
        if (focusCol + 1 >= columns.length) {
            const r = c.cache.row(c.selected)
            if (!r || !r.isDir) return                      // a file has nothing to step into
            push(focusCol, c.selected, r)
        }
        if (focusCol + 1 >= columns.length) return
        focusCol++
        if (columns[focusCol].selected < 0) select(focusCol, 0)
        else ensureVisible(true)
    }
    function activateKey() {
        if (inspectedUri !== "") { root.activate(inspectedUri); return }
        const c = columns[focusCol]
        const r = c && c.selected >= 0 ? c.cache.row(c.selected) : null
        if (r) root.activate(c.uri.replace(/\/+$/, "") + "/" + encodeURIComponent(r.name))
    }
    /// Keeps the inspector whole when it is up; `preferFocus` pulls a column back into view
    /// instead, which is how Left walks back to the columns the inspector pushed off screen.
    function ensureVisible(preferFocus) {
        const left = columnX[focusCol] || 0
        const right = inspectedUri !== "" ? stripWidth : columnX[columns.length]
        let x = strip.contentX
        if (right - x > strip.width) x = right - strip.width
        if (left < x && (preferFocus || inspectedUri === "")) x = left
        strip.contentX = Math.max(0, Math.min(x, Math.max(0, stripWidth - strip.width)))
    }

    // A keyboard selection completes once the daemon sends the row it landed on.
    Connections {
        target: root.columns.length ? root.columns[Math.min(root.focusCol, root.columns.length - 1)].cache : null
        function onRowsUpdated(first, n) {
            if (root.pendingIndex < 0) return
            const c = root.columns[root.focusCol]
            const r = c ? c.cache.row(root.pendingIndex) : null
            if (r) { const i = root.pendingIndex; root.pendingIndex = -1; root.push(root.focusCol, i, r) }
        }
    }
    Component { id: cacheComp; Kiki.WindowCache {} }
    /// What scrolls — the focused column — and the rows behind it, for the scroll probe.
    property var _lists: ({})
    function scroller() { const c = columns[focusCol]; return { view: _lists[focusCol] || null, cache: c ? c.cache : null } }
    /// A file arriving or leaving in a column's folder moves its rows, not its selection.
    function spliceColumn(col, ops) {
        let sel = columns[col] ? columns[col].selected : -1
        if (sel < 0) return
        for (const op of ops) { if (sel < 0) break; sel = op.op === "remove" ? (sel === op.pos ? -1 : sel > op.pos ? sel - 1 : sel) : (sel >= op.pos ? sel + 1 : sel) }
        if (sel === columns[col].selected) return
        const cols = columns.slice()
        cols[col] = Object.assign({}, cols[col], { selected: sel })
        columns = cols
    }

    // To the right of the last column there is nothing but room: a drop there goes into the
    // deepest folder open, which is what the eye takes that room to belong to.
    DropTarget {
        objectName: "columns-drop-background"
        anchors.fill: parent; z: -1
        enabled: !root.pane.isTrash && root.columns.length > 0
        pane: root.pane
        dest: root.columns.length ? root.columns[root.columns.length - 1].uri : ""
    }
    Flickable {
        id: strip
        UI.NaturalScroll { whenItFits: d => root.sideways(d) }
        anchors.fill: parent; contentWidth: root.stripWidth; clip: true; flickableDirection: Flickable.HorizontalFlick
        boundsBehavior: Flickable.StopAtBounds
        WheelHandler {
            target: null
            acceptedModifiers: Qt.ShiftModifier
            acceptedDevices: PointerDevice.Mouse | PointerDevice.TouchPad
            onWheel: event => { const d = event.pixelDelta.y || event.pixelDelta.x || (event.angleDelta.y || event.angleDelta.x) / 2; root.sideways(d); event.accepted = true }
        }
        // A two-finger sideways swipe walks the columns. The per-column lists keep the vertical
        // axis, so only a horizontal delta is taken here.
        WheelHandler {
            target: null
            orientation: Qt.Horizontal
            acceptedDevices: PointerDevice.Mouse | PointerDevice.TouchPad
            onWheel: event => {
                const dx = event.pixelDelta.x !== 0 ? event.pixelDelta.x : event.angleDelta.x / 2
                if (dx === 0) { event.accepted = false; return }
                strip.contentX = Math.max(0, Math.min(strip.contentX + dx, Math.max(0, root.stripWidth - strip.width)))
                event.accepted = true
            }
        }
        UI.Inspector {
            id: inspectorCol
            visible: root.inspectedUri !== ""
            x: root.columnX[root.columns.length]; width: root.inspectorWidth; height: strip.height
            uri: root.inspectedUri; row: root.inspectedRow; home: root.home
            closable: false
            onEdit: (u, line) => root.edit(u, line)
            onOpen: u => root.activate(u)
            onResized: dx => root.inspectorW = root.inspectorWidth - dx
            onResizeEnded: Kiki.Settings.set("view", "inspectorWidth", root.inspectorWidth)
        }
        Row {
            id: row
            Repeater {
                // Not handed the array, and not its length either: `columns` is replaced on every
                // selection, and a Repeater given a new array — or a new NUMBER — throws away
                // every delegate and builds them again, so each click rebuilt every column and
                // every icon in them reloaded (a visible flash). `slots` only ever grows or
                // shrinks at its end, so a column lives as long as it has an index, and reads
                // its entry from `columns` here.
                model: slots
                delegate: Item {
                    id: colItem
                    required property int index
                    readonly property var modelData: root.columns[index] || ({ uri: "", cache: null, selected: -1 })
                    objectName: "column-" + index
                    width: root.widthOf(index); height: strip.height
                    Rectangle { anchors.right: parent.right; width: 1; height: parent.height; color: Kiki.Theme.line }
                    // The line between two columns is its grip: the system's resize cursor,
                    // nothing drawn, a double click to let the width go — as list view's are.
                    MouseArea {
                        objectName: "column-grip-" + colItem.index
                        anchors.right: parent.right; width: 7; height: parent.height; z: 10
                        cursorShape: Qt.SplitHCursor
                        preventStealing: true
                        property real fromX: 0
                        property int fromW: 0
                        onPressed: mouse => { fromX = mapToItem(root, mouse.x, 0).x; fromW = colItem.width }
                        onPositionChanged: mouse => { if (pressed) root.setColumnWidth(colItem.index, fromW + mapToItem(root, mouse.x, 0).x - fromX) }
                        onReleased: root.endColumnResize()
                        onDoubleClicked: root.resetColumnWidth(colItem.index)
                    }
                    // Behind the rows: a right click that lands on none of them asks about the
                    // column's folder instead of doing nothing.
                    MouseArea {
                        anchors.fill: parent; z: -1
                        acceptedButtons: Qt.RightButton
                        onClicked: mouse => root.contextMenuFolder(modelData.uri, mapToItem(null, mouse.x, mouse.y))
                    }
                    // And a drop that lands on none of them goes into the column's folder — which
                    // need not be the one the pane is on.
                    DropTarget {
                        objectName: "column-drop-" + colItem.index
                        anchors.fill: parent; z: -1
                        enabled: !root.pane.isTrash && modelData.uri !== ""
                        pane: root.pane
                        dest: modelData.uri
                        Rectangle { anchors.fill: parent; anchors.rightMargin: 1; color: "transparent"; border.width: 1; border.color: Kiki.Theme.accent; visible: parent.welcoming }
                    }
                    ListView {
                        id: list
                        UI.NaturalScroll { whenItFits: d => root.sideways(d) }
                        WheelHandler {
                            target: null
                            acceptedModifiers: Qt.ShiftModifier
                            acceptedDevices: PointerDevice.Mouse | PointerDevice.TouchPad
                            onWheel: event => { const d = event.pixelDelta.y || event.pixelDelta.x || (event.angleDelta.y || event.angleDelta.x) / 2; root.sideways(d); event.accepted = true }
                        }
                        property int colIndex: index
                        anchors.fill: parent; anchors.rightMargin: 1; anchors.topMargin: 6
                        clip: true; reuseItems: true
                        model: modelData.cache ? modelData.cache.count : 0
                        Component.onCompleted: { root._lists[colIndex] = list; if (modelData.selected >= 0) positionViewAtIndex(modelData.selected, ListView.Contain) }
                        Component.onDestruction: if (root._lists[colIndex] === list) delete root._lists[colIndex]
                        onContentYChanged: if (modelData.cache) modelData.cache.setViewport(Math.max(0, Math.floor(contentY / Kiki.Theme.rowHeight)), Math.ceil(height / Kiki.Theme.rowHeight) + 1)
                        Connections { target: modelData.cache; function onReset() { list.forceLayout() } function onSpliced(ops) { root.spliceColumn(list.colIndex, ops) } }
                        // A living column handed another folder starts at its top.
                        readonly property var shown: modelData.cache
                        onShownChanged: positionViewAtBeginning()
                        // A vertical list would otherwise swallow the sideways swipe before the
                        // strip sees it, so hand the horizontal part over here.
                        WheelHandler {
                            target: null
                            orientation: Qt.Horizontal
                            acceptedDevices: PointerDevice.Mouse | PointerDevice.TouchPad
                            onWheel: event => {
                                const dx = event.pixelDelta.x !== 0 ? event.pixelDelta.x : event.angleDelta.x / 2
                                if (dx === 0) { event.accepted = false; return }
                                strip.contentX = Math.max(0, Math.min(strip.contentX + dx, Math.max(0, root.stripWidth - strip.width)))
                                event.accepted = true
                            }
                        }
                        delegate: Rectangle {
                            id: cr
                            required property int index
                            objectName: "colrow-" + list.colIndex + "-" + index
                            // An arrow-function handler is plain JavaScript: the file's ids are
                            // not in its scope, so the pane is reached through a bound property.
                            readonly property var owner: root
                            property var r: cache ? cache.row(index) : null
                            // The column can be handed another folder while this row lives (the
                            // delegate now outlasts a selection), and a pooled row another index.
                            readonly property var cache: modelData.cache
                            onCacheChanged: r = cache ? cache.row(index) : null
                            onIndexChanged: r = cache ? cache.row(index) : null
                            property bool sel: index === modelData.selected
                            // The focused column shows its selection in the accent; the others in grey.
                            property bool active: sel && list.colIndex === root.focusCol
                            width: list.width; height: Kiki.Theme.rowHeight
                            color: "transparent"
                            property color fg: active ? Kiki.Theme.bg : Kiki.Theme.fgDim
                            readonly property var mark: Kiki.Format.gitMark(r)
                            Rectangle {
                                objectName: "rowmark"
                                anchors.fill: parent; anchors.leftMargin: 5; anchors.rightMargin: 5
                                radius: 6
                                color: cr.sel ? (cr.active ? Kiki.Theme.accent : Kiki.Theme.surface) : "transparent"
                            }
                            Connections { target: cr.cache; function onRowsUpdated(first, n) { if (cr.index >= first && cr.index < first + n) cr.r = cr.cache.row(cr.index) } function onReset() { cr.r = cr.cache.row(cr.index) } }
                            Row {
                                anchors.fill: parent; anchors.leftMargin: 10; anchors.rightMargin: 10; spacing: 8
                                Item {
                                    width: 16; height: 16; anchors.verticalCenter: parent.verticalCenter
                                    UI.KindIcon { visible: !(cr.r && cr.r.thumb); anchors.centerIn: parent; kind: cr.r ? cr.r.kind : ""; color: cr.active ? Kiki.Theme.bg : Kiki.Theme.kindColor(cr.r ? cr.r.kind : "file") }
                                    Image { visible: cr.r && cr.r.thumb; anchors.fill: parent; source: cr.r && cr.r.thumb ? "file://" + cr.r.thumb : ""; sourceSize: Qt.size(32, 32); fillMode: Image.PreserveAspectFit; asynchronous: true; smooth: true }
                                }
                                Text { anchors.verticalCenter: parent.verticalCenter; width: parent.width - 24 - (cr.r && cr.r.isDir ? 20 : 0) - (cr.mark ? 22 : 0) - (colCapsule.visible ? colCapsule.width + 8 : 0); elide: Text.ElideRight; text: cr.r ? cr.r.name : ""; color: Kiki.Format.gitDimmed(cr.r) && !cr.sel ? Kiki.Theme.muted : cr.fg; font.family: Kiki.Theme.mono; font.pixelSize: Kiki.Theme.fontSize }
                                // The same letter, in the same colours, as a list row (plan 15): every
                                // column is a listing like any other, and its rows carry `git`.
                                Text { objectName: "git-badge"; visible: !!cr.mark; anchors.verticalCenter: parent.verticalCenter; width: 14; horizontalAlignment: Text.AlignHCenter; text: Kiki.Format.gitBadge(cr.mark); color: cr.active ? Kiki.Theme.bg : Kiki.Format.gitColor(cr.mark); font.family: Kiki.Theme.mono; font.pixelSize: 11; font.bold: true }
                                // And the same capsule for a folder that is a repository of its own.
                                GitCapsule { id: colCapsule; anchors.verticalCenter: parent.verticalCenter; mark: Kiki.Format.gitCapsule(cr.r); onBar: cr.active; maxWidth: Math.round(parent.width * 0.4) }
                                UI.Icon { visible: cr.r && cr.r.isDir; anchors.verticalCenter: parent.verticalCenter; name: "chev-r"; size: 12; color: cr.active ? Kiki.Theme.bg : Kiki.Theme.gutter }
                            }
                            // Inline rename (F2): the editor list view uses, over this row's name
                            // — which may be in a folder the pane is not standing in, so the new
                            // name goes out against the column's own URI.
                            RenameEditor {
                                visible: list.colIndex === root.renamingCol && cr.index === root.renamingIndex
                                x: 30; y: 2; width: Math.max(40, parent.width - 36); height: parent.height - 4
                                name: cr.r ? cr.r.name : ""
                                onDismissed: root.cancelRename()
                                onRenamed: n => { if (cr.r) root.pane.renameRequested(modelData.uri.replace(/\/+$/, "") + "/" + encodeURIComponent(cr.r.name), n) }
                            }
                            // A folder row takes a drop; any row can be dragged, as itself.
                            DropTarget {
                                anchors.fill: parent
                                enabled: !!cr.r && cr.r.isDir && !root.pane.isTrash
                                pane: root.pane
                                dest: cr.r ? modelData.uri.replace(/\/+$/, "") + "/" + encodeURIComponent(cr.r.name) : ""
                                Rectangle { anchors.fill: parent; anchors.leftMargin: 5; anchors.rightMargin: 5; radius: 6; color: "transparent"; border.width: 1; border.color: Kiki.Theme.accent; visible: parent.welcoming }
                            }
                            Item {
                                id: colDrag
                                objectName: "coldrag"
                                Drag.dragType: Drag.Automatic
                                Drag.supportedActions: Qt.CopyAction | Qt.MoveAction
                                Drag.proposedAction: Qt.MoveAction
                            }
                            MouseArea {
                                anchors.fill: parent
                                acceptedButtons: Qt.LeftButton | Qt.RightButton
                                drag.target: colDrag; drag.threshold: 8
                                // A dragged row becomes the highlighted one, as it does in the
                                // other views: what is moving and what the column shows as chosen
                                // are the same row. Marked, not pushed — a drag opens nothing.
                                drag.onActiveChanged: {
                                    if (drag.active && cr.r) {
                                        cr.owner.markSelected(list.colIndex, cr.index)
                                        colDrag.Drag.mimeData = root.pane.uriListMime([modelData.uri.replace(/\/+$/, "") + "/" + encodeURIComponent(cr.r.name)])
                                        colDrag.Drag.active = true
                                    } else colDrag.Drag.active = false
                                }
                                onClicked: mouse => {
                                    if (!cr.r) return
                                    const uri = modelData.uri.replace(/\/+$/, "") + "/" + encodeURIComponent(cr.r.name)
                                    if (mouse.button === Qt.RightButton) {
                                        // Mark it without pushing: a right click on a folder asks
                                        // about the folder, it does not step into it.
                                        cr.owner.markSelected(list.colIndex, cr.index)
                                        cr.owner.contextMenu(uri, cr.r, cr.mapToItem(null, mouse.x, mouse.y))
                                        return
                                    }
                                    cr.owner.push(list.colIndex, cr.index, cr.r)
                                }
                                onDoubleClicked: if (cr.r) root.activate(modelData.uri.replace(/\/+$/, "") + "/" + encodeURIComponent(cr.r.name))
                            }
                        }
                    }
                }
            }
        }
    }
}
