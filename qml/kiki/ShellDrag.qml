import QtQuick
import "." as Kiki
import "ui" as UI

// A drag and what is seen of it, split out of Shell.qml (docs/0.5.0/04-shell-split.md): the
// ghost that carries every drag of ours and its image, the "+" badge drawn beside the
// compositor's icon, the fan a keyboard transfer flies between the panes (and a refused drop
// flies back), the drop area under everything that tells `DragTrack` where the pointer is, the
// tunnels closed when a drag leaves — and the drop the IPC makes without a pointer. Each
// visible piece keeps `win.contentItem` as its parent, as it had in Shell.qml: declared under
// the window object itself it would be neither drawn nor hit.
Item {
    id: shellDrag
    visible: false
    required property var win

    /// The drag's image, the panes' `ghost`.
    property alias ghost: dragGhost

    /// The drop the IPC makes, shaped as Qt shapes a real one — which has no `modifiers` at all:
    /// the keys held arrive folded into `proposedAction`, by the table measured in
    /// `Pane.wantsCopy` (no key and Shift → Move, the source's proposal; Ctrl and Alt → Copy).
    /// What the drop decided comes back. `ontoPane` names a pane as the target, which is what
    /// tells the two trashes apart: `trash:///` on its own is the Trash in the SIDEBAR, which
    /// trashes what is dropped on it, while the same URI aimed at a pane is the trash VIEW's own
    /// folder — and nothing goes into that.
    function fakeDrop(target, uris, dest, modifiers, ontoPane) {
        const list = (uris || "").split(/[\r\n]+/).filter(u => u && !u.startsWith("#"))
        if (!ontoPane && dest.startsWith("trash:")) {
            if (list.length) win.ops.trashSelection(list)
            return JSON.stringify({ accepted: list.length > 0, action: list.length ? "move" : "none", items: list.length })
        }
        const keys = (modifiers || "").toLowerCase().split(/[+,\s]+/)
        const copy = keys.indexOf("ctrl") >= 0 || keys.indexOf("control") >= 0 || keys.indexOf("alt") >= 0
        let took = Qt.IgnoreAction
        const ev = {
            accepted: false,
            hasUrls: list.length > 0,
            urls: list.map(u => ({ toString: () => u })),
            hasText: false, text: "",
            proposedAction: copy ? Qt.CopyAction : Qt.MoveAction,
            accept: a => { ev.accepted = true; took = a === undefined ? ev.proposedAction : a }
        }
        target.dropInto(dest, ev)
        return JSON.stringify({ accepted: ev.accepted && took !== Qt.IgnoreAction,
                                action: took === Qt.CopyAction ? "copy" : took === Qt.MoveAction ? "move" : "none",
                                items: list.length, focused: win.pane === win.right ? "right" : "left" })
    }

    /// The rows a pane's selection would carry, for a FileFan: `{ kind, thumb }`, three at most.
    function carriedRows(p) {
        return p.selection.positions().slice(0, 3).map(i => { const r = p.listing.row(i); return r ? { kind: r.kind, thumb: r.thumb || "" } : { kind: "file", thumb: "" } })
    }
    /// The fan a keyboard transfer flies from one pane to the other (owner, 2026-09-28: a copy
    /// from a server to this machine should be seen to go). A mouse drag has the DragGhost; this
    /// is for the keys, where nothing moved on screen. `from`/`to` are panes; the fan starts
    /// at the source pane's centre and lands at the target's, then fades. One flight at a time:
    /// a second while one is in the air restarts it.
    property var lastFly: null
    function flyFiles(from, to, rows, count, badge) {
        const fromCol = from === win.right ? win.rightCol : win.leftCol, toCol = to === win.right ? win.rightCol : win.leftCol
        const a = fromCol.mapToItem(win.contentItem, fromCol.width / 2, fromCol.height / 2)
        const b = toCol.mapToItem(win.contentItem, toCol.width / 2, toCol.height / 2)
        lastFly = { from: from === win.right ? "right" : "left", to: to === win.right ? "right" : "left", count: count, badge: badge }
        flyPoints(a, b, rows, count, badge)
    }
    /// The fan from one point to another, in window coordinates: what a keyboard transfer does
    /// between the panes, and what a drag dropped on nothing does back to where it began.
    function flyPoints(a, b, rows, count, badge) {
        flyer.rows = rows; flyer.count = count; flyer.badge = badge
        flight.stop()
        flyer.x = a.x - flyer.width / 2; flyer.y = a.y - flyer.height / 2; flyer.opacity = 1; flyer.visible = true
        flight.toX = b.x - flyer.width / 2; flight.toY = b.y - flyer.height / 2
        flight.start()
    }

    // The drag's image, off screen (a grab needs an item that renders), reached through the panes.
    // A drag gone out of the window, or ended in nothing, closes every tunnel it opened.
    Connections { target: Kiki.DragTrack; function onWentOut() { win.left.tunnelBack(); win.right.tunnelBack() } }
    UI.DragGhost {
        id: dragGhost; parent: win.contentItem; objectName: "drag-ghost"
        // A drop on nothing — refused, or cancelled with Escape — flies the picture back to where
        // the drag was picked up, so it looks like what it is: nothing happened.
        onSnapBack: (from, to, rows, count) => { lastFly = { snapBack: true, count: count }; flyPoints(from, to, rows, count, "") }
    }
    // Empty space tells DragTrack where the pointer is too; the panes' drop targets sit above.
    DropArea {
        // In the window's item tree, as the ghost is: declared here it would be a child of the
        // window object and never drawn nor hit (found 2026-09-28: the badge below never showed).
        parent: win.contentItem
        anchors.fill: parent; z: -1
        onEntered: drag => Kiki.DragTrack.moved(mapToItem(null, drag.x, drag.y), drag.proposedAction)
        onPositionChanged: drag => Kiki.DragTrack.moved(mapToItem(null, drag.x, drag.y), drag.proposedAction)
        onExited: Kiki.DragTrack.left()
    }
    // The badge of a drag of ours, drawn by the window beside the compositor's icon and following
    // the modifiers live — the compositor draws its icon once and will not change it.
    Rectangle {
        objectName: "drag-badge"
        parent: win.contentItem
        // "+" when the drop would copy: Qt proposes a copy for Ctrl, and a drag out of a server
        // is a download whatever the keys say.
        readonly property bool copying: Kiki.DragTrack.action === Qt.CopyAction || dragGhost.copyByDefault
        visible: dragGhost.dragging && Kiki.DragTrack.inside && copying
        // Just off the cursor's tip, below and to the left (owner, 2026-09-28): the image hangs
        // below-right of the cursor (DragGhost.hotSpot), so this corner is free of it.
        x: Kiki.DragTrack.pointer.x - 30; y: Kiki.DragTrack.pointer.y + 10
        z: 950; width: 22; height: 22; radius: 11
        color: Kiki.Theme.accent; border.width: 2; border.color: Kiki.Theme.bg
        Text { anchors.centerIn: parent; text: "+"; color: Kiki.Theme.bg; font.family: Kiki.Theme.mono; font.pixelSize: 15; font.bold: true }
    }
    // The fan a keyboard transfer flies between the panes (`flyFiles`).
    UI.FileFan {
        id: flyer; objectName: "flyer"
        parent: win.contentItem; z: 900; visible: false
        ParallelAnimation {
            id: flight
            property real toX: 0
            property real toY: 0
            NumberAnimation { target: flyer; property: "x"; to: flight.toX; duration: 350; easing.type: Easing.InOutQuad }
            NumberAnimation { target: flyer; property: "y"; to: flight.toY; duration: 350; easing.type: Easing.InOutQuad }
            SequentialAnimation {
                PauseAnimation { duration: 250 }
                NumberAnimation { target: flyer; property: "opacity"; to: 0; duration: 150 }
            }
            onFinished: flyer.visible = false
        }
    }
}
