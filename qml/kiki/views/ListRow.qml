import QtQuick
import ".." as Kiki
import "../ui" as UI

Rectangle {
    id: r
    property Kiki.Pane pane
    property int rowIndex: -1
    objectName: "row-" + rowIndex
    property var row: pane ? pane.listing.row(rowIndex) : null
    property bool selected: pane ? pane.selection.has(rowIndex) : false
    signal activate()
    signal contextMenu(point pos)
    property var columns: [{ role: "name" }, { role: "mtime", w: 160 }, { role: "size", w: 80 }, { role: "kind", w: 120 }]
    // What each value column is drawn at, by role — the header's, so a cell follows a grip being
    // dragged. A property of its own and not a field of `columns`, because `columns` is a model:
    // changing it rebuilds every cell, and a resize must only rebind their widths.
    property var widths: ({})
    property int valueWidth: 160 + 80 + 120 + 36
    // Matches ListPane: whatever the value columns leave, never less than a sliver.
    readonly property int nameWidth: Math.max(48, r.width - 24 - r.valueWidth)
    // A folder that is a repository of its own (plan 15); at most a share of the name column,
    // because the file's name comes first and a branch can be called anything.
    readonly property var capsule: Kiki.Format.gitCapsule(row)
    readonly property int capsuleMax: Math.round(r.nameWidth * 0.4)
    // Accessed: kiki's own opens when that source is chosen and known, else the filesystem atime.
    readonly property bool kikiSource: Kiki.Settings.view.heatSource === "kiki"
    function heatTime() { if (!row) return 0; if (kikiSource && row.opened) return row.opened; return row.meta ? row.meta.atime : 0 }
    function heatFallback() { return kikiSource && row && !row.opened && row.meta && row.meta.atime > 0 }
    function cell(role) {
        if (!row) return ""
        switch (role) {
        case "mtime": return row.meta ? Kiki.Format.modified(row.meta.mtime) : "…"
        case "size": return row.meta ? (row.isDir ? "—" : Kiki.Format.bytes(row.meta.size)) : ""
        case "kind": return Kiki.Format.kindLabel(row.kind)
        case "atime": return heatTime() > 0 ? Kiki.Format.relative(heatTime()) : (row.meta ? "—" : "…")
        }
        return ""
    }
    height: Kiki.Theme.rowHeight
    clip: true
    color: "transparent"
    Connections { target: r.pane ? r.pane.listing : null; function onRowsUpdated(first, n) { if (r.rowIndex >= first && r.rowIndex < first + n) r.row = r.pane.listing.row(r.rowIndex) } function onReset() { r.row = r.pane.listing.row(r.rowIndex) } }
    Connections { target: r.pane ? r.pane.selection : null; function onChanged() { r.selected = r.pane.selection.has(r.rowIndex) } }
    onRowIndexChanged: { row = pane.listing.row(rowIndex); selected = pane.selection.has(rowIndex) }

    property color fg: selected ? Kiki.Theme.bg : Kiki.Theme.fgDim
    property color dim: selected ? Kiki.Theme.bg : Kiki.Theme.muted
    Rectangle {
        anchors.fill: parent; anchors.leftMargin: 6; anchors.rightMargin: 6
        radius: 6
        color: r.selected ? Kiki.Theme.accent : (hover.containsMouse ? Qt.rgba(1, 1, 1, 0.03) : "transparent")
    }
    Row {
        anchors.fill: parent; anchors.leftMargin: 12; anchors.rightMargin: 12; spacing: 12
        Row {
            width: r.nameWidth; height: parent.height; spacing: 8
            Item {
                width: 16; height: 16; anchors.verticalCenter: parent.verticalCenter
                UI.KindIcon { visible: !(r.row && r.row.thumb); kind: r.row ? r.row.kind : ""; color: r.selected ? Kiki.Theme.bg : Kiki.Theme.kindColor(r.row ? r.row.kind : "file") }
                Image { visible: r.row && r.row.thumb; anchors.fill: parent; source: r.row && r.row.thumb ? "file://" + r.row.thumb : ""; sourceSize: Qt.size(32, 32); fillMode: Image.PreserveAspectFit; asynchronous: true; smooth: true }
            }
            Text { objectName: "row-name"; anchors.verticalCenter: parent.verticalCenter; width: Math.max(0, parent.width - 24 - (Kiki.Format.gitMark(r.row) ? 20 : 0) - (capsule.visible ? capsule.width + 8 : 0)); elide: Text.ElideRight; text: r.row ? r.row.name : ""; color: r.row ? (Kiki.Format.gitDimmed(r.row) && !r.selected ? Kiki.Theme.muted : r.fg) : Kiki.Theme.gutter; font.family: Kiki.Theme.mono; font.pixelSize: Kiki.Theme.fontSize }
            Text { objectName: "git-badge"; visible: !!Kiki.Format.gitMark(r.row); anchors.verticalCenter: parent.verticalCenter; width: 14; horizontalAlignment: Text.AlignHCenter; text: Kiki.Format.gitBadge(Kiki.Format.gitMark(r.row)); color: r.selected ? Kiki.Theme.bg : Kiki.Format.gitColor(Kiki.Format.gitMark(r.row)); font.family: Kiki.Theme.mono; font.pixelSize: 11; font.bold: true }
            // The same place, for a folder that is a repository: the branch, coloured by how it stands.
            GitCapsule { id: capsule; anchors.verticalCenter: parent.verticalCenter; mark: r.capsule; onBar: r.selected; maxWidth: r.capsuleMax }
        }
        Repeater {
            model: r.columns.slice(1)
            delegate: Item {
                required property var modelData
                objectName: "cell-" + modelData.role
                width: r.widths[modelData.role] || modelData.w; height: r.height
                // Accessed: a heat swatch behind the text, bright for files touched recently.
                Rectangle { visible: modelData.role === "atime" && !r.selected && r.heatTime() > 0; anchors.fill: parent; anchors.topMargin: 3; anchors.bottomMargin: 3; anchors.rightMargin: 6; radius: 2
                    // hollow when the value is the filesystem fallback under the "kiki opens" source
                    color: r.heatFallback() ? "transparent" : Kiki.Format.heat(r.heatTime(), Kiki.Theme.accent); border.width: r.heatFallback() ? 1 : 0; border.color: Kiki.Format.heat(r.heatTime(), Kiki.Theme.accent) }
                Text { anchors.fill: parent; anchors.leftMargin: modelData.role === "atime" ? 6 : 0; verticalAlignment: Text.AlignVCenter; horizontalAlignment: modelData.role === "size" ? Text.AlignRight : Text.AlignLeft; text: r.cell(modelData.role); color: r.dim; font.family: Kiki.Theme.mono; font.pixelSize: 12; elide: Text.ElideRight }
            }
        }
    }
    // Inline rename (F2): a text input over the name column.
    RenameEditor {
        visible: r.pane && r.pane.renamingIndex === r.rowIndex
        x: 40; y: 2; width: Math.max(40, r.nameWidth - 24); height: parent.height - 4
        name: r.row ? r.row.name : ""
        onDismissed: if (r.pane.renamingIndex === r.rowIndex) r.pane.renamingIndex = -1
        onRenamed: n => { if (r.row) r.pane.renameRequested(r.pane.childUri(r.row.name), n) }
    }
    // Drop target: a folder row accepts files (move within the scheme, copy across, Ctrl copies).
    DropTarget {
        anchors.fill: parent
        enabled: !!r.row && r.row.isDir
        pane: r.pane
        dest: r.row && r.pane ? r.pane.childUri(r.row.name) : ""
        Rectangle { anchors.fill: parent; color: "transparent"; border.width: 1; border.color: Kiki.Theme.accent; visible: parent.welcoming }
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
            if (mouse.button === Qt.RightButton) { if (!r.selected) r.pane.selection.set(r.rowIndex); r.contextMenu(r.mapToItem(null, mouse.x, mouse.y)); return }
            if (mouse.modifiers & Qt.ShiftModifier) r.pane.selection.range(r.rowIndex)
            else if (mouse.modifiers & Qt.ControlModifier) r.pane.selection.toggle(r.rowIndex)
            else r.pane.selection.set(r.rowIndex)
        }
        onDoubleClicked: r.activate()
    }
}
