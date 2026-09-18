import QtQuick
import ".." as Kiki
import "../ui" as UI

// Miller columns. Each column is its own WindowCache; selecting a folder pushes
// the next column; selecting a file leaves room for the inspector (plan 03).
Item {
    id: root
    property Kiki.Pane pane
    signal activate(string uri)
    signal fileSelected(string uri)
    signal edit(string uri, int line)
    property int columnWidth: 220
    property string home: ""
    property var columns: []          // [{ uri, cache, selected }]
    property string inspectedUri: ""
    property var inspectedRow: null
    // Keyboard navigation: the column the arrows act on, and a selection whose row has not
    // arrived from the daemon yet (resolved by onRowsUpdated below).
    property int focusCol: 0
    property int pendingIndex: -1
    readonly property int inspectorWidth: Math.max(300, strip.width - columns.length * columnWidth)
    readonly property alias scrollX: strip.contentX
    readonly property int stripWidth: columns.length * columnWidth + (inspectedUri !== "" ? inspectorWidth : 0)

    Component.onCompleted: rebuild()
    Connections { target: root.pane; function onNavigated(uri) { root.rebuild() } }

    function rebuild() {
        for (const c of columns) if (c.cache !== root.pane.listing) c.cache.destroy()
        columns = [{ uri: root.pane.uri, cache: root.pane.listing, selected: -1 }]
        focusCol = 0; pendingIndex = -1
        inspectedUri = ""; inspectedRow = null
    }
    function push(fromCol, index, row) {
        const cols = columns.slice(0, fromCol + 1)
        cols[fromCol] = Object.assign({}, cols[fromCol], { selected: index })
        for (let i = fromCol + 1; i < columns.length; i++) columns[i].cache.destroy()
        if (row.isDir) {
            const uri = cols[fromCol].uri.replace(/\/+$/, "") + "/" + encodeURIComponent(row.name)
            const cache = cacheComp.createObject(root)
            cache.open(uri)
            cols.push({ uri: uri, cache: cache, selected: -1 })
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
        const left = focusCol * columnWidth
        const right = inspectedUri !== "" ? stripWidth : columns.length * columnWidth
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

    Flickable {
        id: strip
        UI.NaturalScroll { }
        anchors.fill: parent; contentWidth: root.stripWidth; clip: true; flickableDirection: Flickable.HorizontalFlick
        boundsBehavior: Flickable.StopAtBounds
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
            x: root.columns.length * root.columnWidth; width: root.inspectorWidth; height: strip.height
            uri: root.inspectedUri; row: root.inspectedRow; home: root.home
            onOpen: root.activate(root.inspectedUri)
            onEdit: (u, line) => root.edit(u, line)
        }
        Row {
            id: row
            Repeater {
                model: root.columns
                delegate: Item {
                    required property var modelData
                    required property int index
                    width: root.columnWidth; height: strip.height
                    Rectangle { anchors.right: parent.right; width: 1; height: parent.height; color: Kiki.Theme.line }
                    ListView {
                        id: list
                        UI.NaturalScroll { }
                        property int colIndex: index
                        anchors.fill: parent; anchors.rightMargin: 1; anchors.topMargin: 6
                        clip: true; reuseItems: true
                        model: modelData.cache.count
                        Component.onCompleted: if (modelData.selected >= 0) positionViewAtIndex(modelData.selected, ListView.Contain)
                        onContentYChanged: modelData.cache.setViewport(Math.max(0, Math.floor(contentY / Kiki.Theme.rowHeight)), Math.ceil(height / Kiki.Theme.rowHeight) + 1)
                        Connections { target: modelData.cache; function onReset() { list.forceLayout() } }
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
                            property var r: modelData.cache.row(index)
                            property bool sel: index === modelData.selected
                            // The focused column shows its selection in the accent; the others in grey.
                            property bool active: sel && list.colIndex === root.focusCol
                            width: list.width; height: Kiki.Theme.rowHeight
                            color: sel ? (active ? Kiki.Theme.accent : Kiki.Theme.surface) : "transparent"
                            property color fg: active ? Kiki.Theme.bg : Kiki.Theme.fgDim
                            Connections { target: modelData.cache; function onRowsUpdated(first, n) { if (cr.index >= first && cr.index < first + n) cr.r = modelData.cache.row(cr.index) } function onReset() { cr.r = modelData.cache.row(cr.index) } }
                            Row {
                                anchors.fill: parent; anchors.leftMargin: 10; anchors.rightMargin: 10; spacing: 8
                                Item {
                                    width: 16; height: 16; anchors.verticalCenter: parent.verticalCenter
                                    UI.Icon { visible: !(cr.r && cr.r.thumb); anchors.centerIn: parent; name: cr.r ? cr.r.kind : "file"; color: cr.active ? Kiki.Theme.bg : Kiki.Theme.kindColor(cr.r ? cr.r.kind : "file") }
                                    Image { visible: cr.r && cr.r.thumb; anchors.fill: parent; source: cr.r && cr.r.thumb ? "file://" + cr.r.thumb : ""; sourceSize: Qt.size(32, 32); fillMode: Image.PreserveAspectFit; asynchronous: true; smooth: true }
                                }
                                Text { anchors.verticalCenter: parent.verticalCenter; width: parent.width - 24 - (cr.r && cr.r.isDir ? 20 : 0); elide: Text.ElideRight; text: cr.r ? cr.r.name : ""; color: cr.fg; font.family: Kiki.Theme.mono; font.pixelSize: Kiki.Theme.fontSize }
                                UI.Icon { visible: cr.r && cr.r.isDir; anchors.verticalCenter: parent.verticalCenter; name: "chev-r"; size: 12; color: cr.active ? Kiki.Theme.bg : Kiki.Theme.gutter }
                            }
                            MouseArea {
                                anchors.fill: parent
                                onClicked: if (cr.r) root.push(list.colIndex, cr.index, cr.r)
                                onDoubleClicked: if (cr.r) root.activate(modelData.uri.replace(/\/+$/, "") + "/" + encodeURIComponent(cr.r.name))
                            }
                        }
                    }
                }
            }
        }
    }
}
