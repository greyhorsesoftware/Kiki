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
    property var columns: [{ role: "name" }, { role: "mtime", w: 160 }, { role: "size", w: 80 }, { role: "kind", w: 120 }]
    property int valueWidth: 160 + 80 + 120 + 36
    function cell(role) {
        if (!row) return ""
        switch (role) {
        case "mtime": return row.meta ? Kiki.Format.date(row.meta.mtime) : "…"
        case "size": return row.meta ? (row.isDir ? "—" : Kiki.Format.bytes(row.meta.size)) : ""
        case "kind": return Kiki.Format.kindLabel(row.kind)
        case "atime": return row.meta ? Kiki.Format.relative(row.meta.atime) : "…"
        }
        return ""
    }
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
            width: r.width - 24 - r.valueWidth; height: parent.height; spacing: 8
            Item {
                width: 16; height: 16; anchors.verticalCenter: parent.verticalCenter
                UI.Icon { visible: !(r.row && r.row.thumb); name: r.row ? r.row.kind : "file"; color: r.selected ? Kiki.Theme.bg : Kiki.Theme.kindColor(r.row ? r.row.kind : "file") }
                Image { visible: r.row && r.row.thumb; anchors.fill: parent; source: r.row && r.row.thumb ? "file://" + r.row.thumb : ""; sourceSize: Qt.size(32, 32); fillMode: Image.PreserveAspectFit; asynchronous: true; smooth: true }
            }
            Text { anchors.verticalCenter: parent.verticalCenter; width: parent.width - 24 - (r.row && r.row.git && r.row.git.state !== "clean" ? 20 : 0); elide: Text.ElideRight; text: r.row ? r.row.name : ""; color: r.row ? (r.row.git && r.row.git.state === "ignored" && !r.selected ? Kiki.Theme.muted : r.fg) : Kiki.Theme.gutter; font.family: Kiki.Theme.mono; font.pixelSize: Kiki.Theme.fontSize }
            Text { visible: r.row && r.row.git && r.row.git.state !== "clean" && r.row.git.state !== "ignored"; anchors.verticalCenter: parent.verticalCenter; width: 14; horizontalAlignment: Text.AlignHCenter; text: Kiki.Format.gitBadge(r.row ? r.row.git : null); color: r.selected ? Kiki.Theme.bg : Kiki.Format.gitColor(r.row ? r.row.git : null); font.family: Kiki.Theme.mono; font.pixelSize: 11; font.bold: true }
        }
        Repeater {
            model: r.columns.slice(1)
            delegate: Item {
                required property var modelData
                width: modelData.w; height: r.height
                // Accessed: a heat swatch behind the text, bright for files touched recently.
                Rectangle { visible: modelData.role === "atime" && !r.selected && r.row && r.row.meta && r.row.meta.atime > 0; anchors.fill: parent; anchors.topMargin: 3; anchors.bottomMargin: 3; anchors.rightMargin: 6; radius: 2; color: r.row && r.row.meta ? Kiki.Format.heat(r.row.meta.atime, Kiki.Theme.accent) : "transparent" }
                Text { anchors.fill: parent; anchors.leftMargin: modelData.role === "atime" ? 6 : 0; verticalAlignment: Text.AlignVCenter; horizontalAlignment: modelData.role === "size" ? Text.AlignRight : Text.AlignLeft; text: r.cell(modelData.role); color: r.dim; font.family: Kiki.Theme.mono; font.pixelSize: 12; elide: Text.ElideRight }
            }
        }
    }
    // Inline rename (F2): a text input over the name column.
    Rectangle {
        visible: r.pane && r.pane.renamingIndex === r.rowIndex
        x: 40; y: 2; width: r.width - 24 - r.valueWidth - 24; height: parent.height - 4; radius: 2
        color: Kiki.Theme.bgDark; border.width: 1; border.color: Kiki.Theme.accent; z: 2
        onVisibleChanged: if (visible) { edit.text = r.row ? r.row.name : ""; edit.forceActiveFocus(); const dot = edit.text.lastIndexOf("."); edit.select(0, dot > 0 ? dot : edit.text.length) }
        TextInput {
            id: edit; anchors.fill: parent; anchors.leftMargin: 6; anchors.rightMargin: 6; verticalAlignment: TextInput.AlignVCenter
            color: Kiki.Theme.fg; font.family: Kiki.Theme.mono; font.pixelSize: Kiki.Theme.fontSize; selectionColor: Kiki.Theme.accent; clip: true
            onAccepted: { const name = text; r.pane.renamingIndex = -1; if (r.row && name && name !== r.row.name) r.pane.renameRequested(r.pane.childUri(r.row.name), name) }
            Keys.onEscapePressed: r.pane.renamingIndex = -1
            onActiveFocusChanged: if (!activeFocus && r.pane.renamingIndex === r.rowIndex) r.pane.renamingIndex = -1
        }
    }
    // Drop target: a folder row accepts files (move within the scheme, copy across, Ctrl copies).
    DropArea {
        anchors.fill: parent
        enabled: r.row && r.row.isDir
        keys: ["text/uri-list"]
        onDropped: drop => r.pane.dropInto(r.pane.childUri(r.row.name), drop)
        Rectangle { anchors.fill: parent; color: "transparent"; border.width: 1; border.color: Kiki.Theme.accent; visible: parent.containsDrag }
    }
    // Drag source: an invisible proxy carries the selection as text/uri-list.
    Item {
        id: dragProxy
        Drag.dragType: Drag.Automatic
        Drag.supportedActions: Qt.CopyAction | Qt.MoveAction
        Drag.proposedAction: Qt.MoveAction
        Drag.mimeData: r.pane ? r.pane.dragMime(r.rowIndex) : ({})
    }
    MouseArea {
        id: hover; anchors.fill: parent; hoverEnabled: true; acceptedButtons: Qt.LeftButton | Qt.RightButton
        drag.target: dragProxy; drag.threshold: 8
        drag.onActiveChanged: { if (drag.active) { if (!r.selected) r.pane.selection.set(r.rowIndex); dragProxy.Drag.mimeData = r.pane.dragMime(r.rowIndex); dragProxy.Drag.active = true } else dragProxy.Drag.active = false }
        onClicked: mouse => {
            if (mouse.button === Qt.RightButton) { if (!r.selected) r.pane.selection.set(r.rowIndex); r.contextMenu(Qt.point(mouse.x, mouse.y)); return }
            if (mouse.modifiers & Qt.ShiftModifier) r.pane.selection.range(r.rowIndex)
            else if (mouse.modifiers & Qt.ControlModifier) r.pane.selection.toggle(r.rowIndex)
            else r.pane.selection.set(r.rowIndex)
        }
        onDoubleClicked: r.activate()
    }
}
