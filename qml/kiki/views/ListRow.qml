import QtQuick
import ".." as Kiki
import "../ui" as UI

Rectangle {
    id: r
    property Kiki.Pane pane
    property int rowIndex: -1
    property var row: pane ? pane.listing.row(rowIndex) : null
    property bool selected: pane ? pane.selection.has(rowIndex) : false
    signal activate()
    signal contextMenu(point pos)
    height: Kiki.Theme.rowHeight
    color: selected ? Kiki.Theme.accent : (hover.containsMouse ? Qt.rgba(1, 1, 1, 0.03) : "transparent")
    Connections { target: r.pane ? r.pane.listing : null; function onRowsUpdated(first, n) { if (r.rowIndex >= first && r.rowIndex < first + n) r.row = r.pane.listing.row(r.rowIndex) } function onReset() { r.row = r.pane.listing.row(r.rowIndex) } }
    Connections { target: r.pane ? r.pane.selection : null; function onChanged() { r.selected = r.pane.selection.has(r.rowIndex) } }
    onRowIndexChanged: { row = pane.listing.row(rowIndex); selected = pane.selection.has(rowIndex) }

    property color fg: selected ? Kiki.Theme.bg : Kiki.Theme.fgDim
    property color dim: selected ? Kiki.Theme.bg : Kiki.Theme.muted
    Row {
        anchors.fill: parent; anchors.leftMargin: 12; anchors.rightMargin: 12; spacing: 12
        Row {
            width: r.width - 24 - 36 - 160 - 80 - 120; height: parent.height; spacing: 8
            UI.Icon { anchors.verticalCenter: parent.verticalCenter; name: r.row ? r.row.kind : "file"; color: r.selected ? Kiki.Theme.bg : Kiki.Theme.kindColor(r.row ? r.row.kind : "file") }
            Text { anchors.verticalCenter: parent.verticalCenter; width: parent.width - 24; elide: Text.ElideRight; text: r.row ? r.row.name : ""; color: r.row ? r.fg : Kiki.Theme.gutter; font.family: Kiki.Theme.mono; font.pixelSize: Kiki.Theme.fontSize }
        }
        Text { width: 160; anchors.verticalCenter: parent.verticalCenter; text: r.row && r.row.meta ? Kiki.Format.date(r.row.meta.mtime) : (r.row ? "…" : ""); color: r.dim; font.family: Kiki.Theme.mono; font.pixelSize: 12 }
        Text { width: 80; anchors.verticalCenter: parent.verticalCenter; horizontalAlignment: Text.AlignRight; text: r.row && r.row.meta ? (r.row.isDir ? "—" : Kiki.Format.bytes(r.row.meta.size)) : ""; color: r.dim; font.family: Kiki.Theme.mono; font.pixelSize: 12 }
        Text { width: 120; anchors.verticalCenter: parent.verticalCenter; text: r.row ? Kiki.Format.kindLabel(r.row.kind) : ""; color: r.dim; font.family: Kiki.Theme.mono; font.pixelSize: 12 }
    }
    MouseArea {
        id: hover; anchors.fill: parent; hoverEnabled: true; acceptedButtons: Qt.LeftButton | Qt.RightButton
        onClicked: mouse => {
            if (mouse.button === Qt.RightButton) { if (!r.selected) r.pane.selection.set(r.rowIndex); r.contextMenu(Qt.point(mouse.x, mouse.y)); return }
            if (mouse.modifiers & Qt.ShiftModifier) r.pane.selection.range(r.rowIndex)
            else if (mouse.modifiers & Qt.ControlModifier) r.pane.selection.toggle(r.rowIndex)
            else r.pane.selection.set(r.rowIndex)
        }
        onDoubleClicked: r.activate()
    }
}
