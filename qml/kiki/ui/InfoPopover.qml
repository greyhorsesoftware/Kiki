import QtQuick
import ".." as Kiki

// The info panel in side by side (0.1.1): a callout beside the selected row of the focused pane,
// its pointer on the row the way the activity popover's is on the orb — General and Permissions,
// no preview. It follows the selection, and the focus between the panes; it goes on Ctrl+I,
// Escape, or a click anywhere else. The inspector inside is the panel's own, in its compact shape.
Item {
    id: pop
    visible: false
    z: 60
    objectName: "info-popover-host"
    /// The focused pane's column (the header, the rows) — where the card sits and what it is
    /// clamped to — and its view, which knows where the selected row is drawn.
    property Item paneItem: null
    property var view: null
    property int rowIndex: -1
    property string uri: ""
    property var row: null
    property var rows: []
    property string home: ""
    signal closed()
    signal chmod(int mode, bool recursive)
    signal chmodMany(int mask, int bits, bool recursive)
    signal edit(string uri, int line)
    signal open(string uri)

    readonly property int cardWidth: 320
    /// Where the pane is, in this item's coordinates.
    property rect paneRect: Qt.rect(0, 0, width, height)
    /// The selected row's centre, in this item's coordinates; -1 when there is no row to aim at
    /// (nothing selected, an icon grid with the row pooled away, a row scrolled out of view).
    property real rowY: -1
    readonly property bool aimed: rowY >= paneRect.y && rowY <= paneRect.y + paneRect.height

    /// Asked again on a beat while up rather than bound: the row's item moves with a scroll,
    /// is pooled and recreated, and belongs to a view that says nothing when it does.
    function aim() {
        if (!paneItem) return
        const p = paneItem.mapToItem(pop, 0, 0)
        paneRect = Qt.rect(p.x, p.y, paneItem.width, paneItem.height)
        const it = view && view.rowItem && rowIndex >= 0 ? view.rowItem(rowIndex) : null
        if (!it) { rowY = -1; return }
        const c = it.mapToItem(pop, 0, it.height / 2)
        rowY = c.y
    }
    Timer { running: pop.visible; interval: 80; repeat: true; triggeredOnStart: true; onTriggered: pop.aim() }
    onVisibleChanged: if (visible) aim()
    onRowIndexChanged: if (visible) aim()

    // Anywhere else closes it.
    MouseArea { anchors.fill: parent; acceptedButtons: Qt.AllButtons; hoverEnabled: true; onPressed: pop.closed(); onWheel: wheel => wheel.accepted = true }

    Rectangle {
        id: card
        objectName: "info-popover"
        width: pop.cardWidth
        height: Math.min(insp.naturalHeight, Math.max(120, pop.height - 16))
        // Over the OTHER pane, its pointer facing the pane the row is in (owner, 2026-09-24):
        // the left pane's card sits to its right, pointer on the card's left edge; the right
        // pane's card sits to its left, pointer on the right edge. Which side is decided by
        // where the pane is, never by room — in a narrow window the card is clamped to the
        // window's edges and overlaps the divider a little, still pointing the right way. (It
        // used to pick the side with room, which in a narrow window put the left pane's card
        // over the left pane, pointing at the right one.) Centred on the row, the pointer at its
        // middle (owner, 2026-09-24); never above the pane's top, never below the window's
        // bottom — pushed off centre by either edge, the pointer slides along to stay on the row.
        readonly property bool onRight: pop.paneRect.x + pop.paneRect.width / 2 < pop.width / 2
        x: Math.max(8, Math.min(pop.width - width - 8, onRight ? pop.paneRect.x + pop.paneRect.width + 12 : pop.paneRect.x - 12 - width))
        y: {
            const top = pop.paneRect.y + 8, bottom = pop.height - height - 8
            const want = pop.aimed ? pop.rowY - height / 2 : top
            return Math.max(top, Math.min(bottom, want))
        }
        radius: 12; color: "transparent"; border.width: 1; border.color: Kiki.Theme.gutter
        // Near-opaque: this card is read, not seen through, and the rows under it are the other
        // pane's.
        Frost { anchors.fill: parent; radius: parent.radius; tintOpacity: 0.96; z: -2 }
        // Swallows what lands on the card, so only a click OUTSIDE closes.
        MouseArea { anchors.fill: parent; acceptedButtons: Qt.AllButtons; hoverEnabled: true }

        // The pointer, on the row's end: sliding along the edge that faces the pane to stay on
        // the row, never into a corner.
        Rectangle {
            objectName: "info-popover-pointer"
            visible: pop.aimed
            width: 14; height: 14; rotation: 45; color: Kiki.Theme.bgDark; border.width: 1; border.color: Kiki.Theme.gutter
            x: card.onRight ? -8 : card.width - 6
            y: Math.max(card.radius + 4, Math.min(card.height - card.radius - 18, pop.rowY - card.y - 7)); z: -1
        }
        // The pointer's inner half, covered by the card's own ground so its border does not cross the card.
        Rectangle { visible: pop.aimed; x: card.onRight ? 1 : card.width - 12; width: 11; y: 1; height: card.height - 2; color: Kiki.Theme.bgDark; opacity: 0.96; radius: 10; z: -1 }

        Inspector {
            id: insp
            anchors.fill: parent
            compact: true; standalone: true; closable: true
            color: "transparent"
            uri: pop.uri; row: pop.row; rows: pop.rows; home: pop.home
            onClosed: pop.closed()
            onChmod: (mode, recursive) => pop.chmod(mode, recursive)
            onChmodMany: (mask, bits, recursive) => pop.chmodMany(mask, bits, recursive)
            onEdit: (u, line) => pop.edit(u, line)
            onOpen: u => pop.open(u)
        }
    }
}
