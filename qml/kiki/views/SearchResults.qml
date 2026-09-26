import QtQuick
import ".." as Kiki
import "../ui" as UI

// Everywhere / location results: two-line rows with the match highlighted and the parent folder.
Item {
    id: root
    property Kiki.WindowCache results
    property string query: ""
    property string scopeLabel: ""
    property string home: ""
    property string indexInfo: ""
    property int current: -1
    signal open(string uri)
    signal reveal(string uri)

    function highlight(name) {
        const q = root.query.toLowerCase(), i = name.toLowerCase().indexOf(q)
        if (i < 0 || !q) return name
        const esc = s => s.replace(/&/g, "&amp;").replace(/</g, "&lt;")
        return esc(name.slice(0, i)) + "<b style='color:" + Kiki.Theme.accent + "'>" + esc(name.slice(i, i + q.length)) + "</b>" + esc(name.slice(i + q.length))
    }
    function move(d) { current = Math.max(0, Math.min(results.count - 1, current + d)); list.positionViewAtIndex(current, ListView.Contain) }
    function activate() { const r = results.row(current); if (r) root.open(r.uri) }

    Rectangle {
        id: head; width: parent.width; height: 34; color: Kiki.Theme.bg
        Rectangle { anchors.bottom: parent.bottom; width: parent.width; height: 1; color: Kiki.Theme.line }
        Row {
            anchors.verticalCenter: parent.verticalCenter; x: 16; spacing: 6
            Text { text: results.count; color: Kiki.Theme.fg; font.family: Kiki.Theme.mono; font.pixelSize: 11 }
            Text { text: Kiki.T.tr("search.resultsFor"); color: Kiki.Theme.muted; font.family: Kiki.Theme.mono; font.pixelSize: 11 }
            Text { text: root.query; color: Kiki.Theme.fg; font.family: Kiki.Theme.mono; font.pixelSize: 11 }
            Text { text: Kiki.T.tr("search.in"); color: Kiki.Theme.muted; font.family: Kiki.Theme.mono; font.pixelSize: 11 }
            Text { text: root.scopeLabel; color: Kiki.Theme.fg; font.family: Kiki.Theme.mono; font.pixelSize: 11 }
        }
        Text { anchors.right: parent.right; anchors.rightMargin: 16; anchors.verticalCenter: parent.verticalCenter; text: root.indexInfo; color: Kiki.Theme.muted; font.family: Kiki.Theme.mono; font.pixelSize: 11 }
    }
    ListView {
        id: list
        UI.NaturalScroll { }
        anchors.top: head.bottom; width: parent.width; height: parent.height - head.height
        clip: true; reuseItems: true; model: root.results.count
        onContentYChanged: root.results.setViewport(Math.max(0, Math.floor(contentY / 44)), Math.ceil(height / 44) + 1)
        Connections { target: root.results; function onReset() { list.forceLayout() } }
        delegate: Rectangle {
            id: row
            required property int index
            property var r: root.results.row(index)
            property bool sel: index === root.current
            width: list.width; height: 44
            color: sel ? Kiki.Theme.accent : "transparent"
            Connections { target: root.results; function onRowsUpdated(first, n) { if (row.index >= first && row.index < first + n) row.r = root.results.row(row.index) } function onReset() { row.r = root.results.row(row.index) } }
            Row {
                anchors.fill: parent; anchors.leftMargin: 16; anchors.rightMargin: 16; spacing: 10
                UI.Icon { anchors.verticalCenter: parent.verticalCenter; name: row.r ? row.r.kind : "file"; size: 20; strokeWidth: 1.25; color: row.sel ? Kiki.Theme.bg : Kiki.Theme.kindColor(row.r ? row.r.kind : "file") }
                Column {
                    anchors.verticalCenter: parent.verticalCenter; width: parent.width - 30 - 240; spacing: 1
                    Text { width: parent.width; elide: Text.ElideRight; textFormat: Text.RichText; text: row.r ? (row.sel ? row.r.name : root.highlight(row.r.name)) : ""; color: row.sel ? Kiki.Theme.bg : Kiki.Theme.fg; font.family: Kiki.Theme.mono; font.pixelSize: Kiki.Theme.fontSize }
                    Text { width: parent.width; elide: Text.ElideMiddle; text: row.r ? Kiki.Format.display(row.r.parent, root.home) : ""; color: row.sel ? Kiki.Theme.bg : Kiki.Theme.muted; font.family: Kiki.Theme.mono; font.pixelSize: 11 }
                }
                Text { width: 150; anchors.verticalCenter: parent.verticalCenter; text: row.r && row.r.meta ? Kiki.Format.date(row.r.meta.mtime) : ""; color: row.sel ? Kiki.Theme.bg : Kiki.Theme.muted; font.family: Kiki.Theme.mono; font.pixelSize: 12 }
                Text { width: 70; anchors.verticalCenter: parent.verticalCenter; horizontalAlignment: Text.AlignRight; text: row.r && row.r.meta && !row.r.isDir ? Kiki.Format.bytes(row.r.meta.size) : ""; color: row.sel ? Kiki.Theme.bg : Kiki.Theme.muted; font.family: Kiki.Theme.mono; font.pixelSize: 12 }
            }
            MouseArea { anchors.fill: parent; onClicked: root.current = row.index; onDoubleClicked: if (row.r) root.open(row.r.uri) }
        }
    }
}
