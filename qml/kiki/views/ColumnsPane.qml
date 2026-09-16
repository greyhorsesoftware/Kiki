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
    property int columnWidth: 220
    property var columns: []          // [{ uri, cache, selected }]

    Component.onCompleted: rebuild()
    Connections { target: root.pane; function onNavigated(uri) { root.rebuild() } }

    function rebuild() {
        for (const c of columns) if (c.cache !== root.pane.listing) c.cache.destroy()
        columns = [{ uri: root.pane.uri, cache: root.pane.listing, selected: -1 }]
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
        } else {
            root.fileSelected(cols[fromCol].uri.replace(/\/+$/, "") + "/" + encodeURIComponent(row.name))
        }
        columns = cols
        strip.contentX = Math.max(0, cols.length * columnWidth - strip.width)
    }
    Component { id: cacheComp; Kiki.WindowCache {} }

    Flickable {
        id: strip
        anchors.fill: parent; contentWidth: row.width; clip: true; flickableDirection: Flickable.HorizontalFlick
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
                        property int colIndex: index
                        anchors.fill: parent; anchors.rightMargin: 1; anchors.topMargin: 6
                        clip: true; reuseItems: true
                        model: modelData.cache.count
                        onContentYChanged: modelData.cache.setViewport(Math.max(0, Math.floor(contentY / Kiki.Theme.rowHeight)), Math.ceil(height / Kiki.Theme.rowHeight) + 1)
                        Connections { target: modelData.cache; function onReset() { list.forceLayout() } }
                        delegate: Rectangle {
                            id: cr
                            required property int index
                            property var r: modelData.cache.row(index)
                            property bool sel: index === modelData.selected
                            // The deepest column with a selection shows it in the accent; ancestors in grey.
                            property bool active: sel && list.colIndex === root.columns.length - 1
                            width: list.width; height: Kiki.Theme.rowHeight
                            color: sel ? (active ? Kiki.Theme.accent : Kiki.Theme.surface) : "transparent"
                            property color fg: active ? Kiki.Theme.bg : Kiki.Theme.fgDim
                            Connections { target: modelData.cache; function onRowsUpdated(first, n) { if (cr.index >= first && cr.index < first + n) cr.r = modelData.cache.row(cr.index) } function onReset() { cr.r = modelData.cache.row(cr.index) } }
                            Row {
                                anchors.fill: parent; anchors.leftMargin: 10; anchors.rightMargin: 10; spacing: 8
                                UI.Icon { anchors.verticalCenter: parent.verticalCenter; name: cr.r ? cr.r.kind : "file"; color: cr.active ? Kiki.Theme.bg : Kiki.Theme.kindColor(cr.r ? cr.r.kind : "file") }
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
