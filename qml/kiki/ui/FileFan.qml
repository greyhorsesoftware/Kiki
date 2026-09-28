import QtQuick
import ".." as Kiki

// What is being carried, drawn the way a hand would hold it: one card for one file — its
// thumbnail if it has one, its kind's icon if not — and up to three cards fanned for several,
// with a pill saying how many when three is not all of them. The badge in the corner says what
// will happen to them: "+" for a copy (a download from a server, an upload to one, a Ctrl-drag),
// nothing for a move, "↗" for a link. Drawn once for a drag (DragGhost grabs it as the drag's
// image) and once for a keyboard transfer (the Shell flies it from one pane to the other) —
// the owner asked for both on 2026-09-28, having seen only the compositor's hand.
Item {
    id: fan
    /// The files, `{ kind, thumb }` each; only the first three are drawn.
    property var rows: []
    /// How many are carried altogether — the pill, when more than the cards show.
    property int count: rows.length
    /// "" for a move, "+" for a copy, "↗" for a link.
    property string badge: ""
    /// Whether the badge is drawn here. The drag ghost turns it off: its picture is handed to the
    /// compositor once, so the badge — which follows the modifiers — is drawn by the window.
    property bool badgeShown: true

    readonly property int cards: Math.min(3, Math.max(count, rows.length, 1))
    readonly property int cardW: 44
    readonly property int cardH: 54
    width: 96; height: 72

    Repeater {
        model: fan.cards
        // The first card on top, the rest fanned out behind it to the right.
        Rectangle {
            required property int index
            readonly property var row: fan.rows[index] || ({ kind: "file", thumb: "" })
            objectName: "fan-card"
            width: fan.cardW; height: fan.cardH; radius: 4
            x: 10 + index * 12; y: 8 + index * 2
            z: fan.cards - index
            rotation: (index - (fan.cards - 1) / 2) * 7
            // No box, either way (owner, 2026-09-28): a picture IS the card — the thumbnail
            // filling it, corners rounded — and a file without one is its kind's icon, large,
            // and nothing drawn behind it.
            color: "transparent"; border.width: 0
            RoundedImage { visible: !!row.thumb; anchors.fill: parent; radius: 4; source: row.thumb ? "file://" + row.thumb : "" }
            Item {
                visible: !row.thumb
                anchors.centerIn: parent; width: 44; height: 44
                KindIcon { visible: !row.thumb; anchors.centerIn: parent; kind: row.kind || "file"; size: 44; color: Kiki.Theme.kindColor(row.kind || "file") }
            }
        }
    }
    // The count, when the cards do not show it all.
    Rectangle {
        objectName: "fan-count"
        visible: fan.count > 3
        // Bottom right (owner, 2026-09-28); the badge keeps the other corner.
        anchors.bottom: parent.bottom; anchors.right: parent.right
        width: Math.max(20, countText.paintedWidth + 10); height: 18; radius: 9
        color: Kiki.Theme.accent
        Text { id: countText; anchors.centerIn: parent; text: fan.count; color: Kiki.Theme.bg; font.family: Kiki.Theme.mono; font.pixelSize: 11; font.bold: true }
    }
    // What will happen to them.
    Rectangle {
        objectName: "fan-badge"
        visible: fan.badge !== "" && fan.badgeShown
        anchors.bottom: parent.bottom; anchors.left: parent.left; anchors.leftMargin: 6
        width: 22; height: 22; radius: 11
        color: Kiki.Theme.accent; border.width: 2; border.color: Kiki.Theme.bg
        Text { anchors.centerIn: parent; text: fan.badge; color: Kiki.Theme.bg; font.family: Kiki.Theme.mono; font.pixelSize: 14; font.bold: true }
    }
}
