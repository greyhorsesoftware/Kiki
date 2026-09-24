import QtQuick
import ".." as Kiki

// A selection of many, as a picture: the items' icons fanned out like a hand of cards, each on a
// small card of the darker ground, the current row's on top — the info panel's preview for N
// files (0.1.1), and, at header size, the popover's icon. No box or border around it: the cards
// are the picture. One card per item up to `most`, the distinct kinds dealt first: three folders
// are three folder cards, not one (it used to deal one card per kind, so a selection of one kind
// showed no fan at all — owner, 2026-09-24).
Item {
    id: fan
    /// The rows of the selection; the first is drawn on top.
    property var rows: []
    /// The card, in pixels; the icon and the offsets scale with it.
    property real cardWidth: 64
    readonly property real cardHeight: cardWidth * 1.25
    /// How many cards at most; more than that, the last carries "+n".
    property int most: 5
    readonly property var kinds: {
        const out = []
        for (const r of rows || []) { const k = r && r.kind ? r.kind : "file"; if (out.indexOf(k) < 0) out.push(k) }
        return out
    }
    /// The kinds on the cards: every distinct kind, then the rest of the rows' kinds, `most` at most.
    readonly property var cards: {
        const out = kinds.slice(0, most)
        for (const r of rows || []) { if (out.length >= Math.min(most, rows.length)) break; out.push(r && r.kind ? r.kind : "file") }
        return out
    }
    readonly property int shown: cards.length
    implicitWidth: cardWidth + (shown - 1) * cardWidth * 0.7
    implicitHeight: cardHeight * 1.15

    Repeater {
        model: fan.shown
        delegate: Rectangle {
            required property int index
            // Spread from the middle: -16°, 0°, +16° for three; wider fans for more.
            readonly property real t: fan.shown > 1 ? index / (fan.shown - 1) - 0.5 : 0
            readonly property string kind: fan.cards[index]
            width: fan.cardWidth; height: fan.cardHeight; radius: fan.cardWidth / 8
            color: Kiki.Theme.surface
            x: (fan.width - width) / 2 + t * fan.cardWidth * 1.4 * (fan.shown - 1) / 2
            y: (fan.height - height) / 2 + Math.abs(t) * fan.cardHeight * 0.2
            rotation: t * 32
            // The card in the middle is on top; the current row's kind is drawn first, so first.
            z: fan.shown - Math.abs(index - (fan.shown - 1) / 2) * 2
            layer.enabled: true
            Rectangle { anchors.fill: parent; anchors.margins: -1; radius: parent.radius + 1; z: -1; color: "transparent"; border.width: 1; border.color: Qt.rgba(0, 0, 0, 0.35) }
            Icon { anchors.centerIn: parent; name: parent.kind; size: fan.cardWidth * 0.56; strokeWidth: 1; color: Kiki.Theme.kindColor(parent.kind) }
        }
    }
    // More kinds than cards: "+n", centred at the fan's foot (owner, 2026-09-24), over the cards.
    Text {
        objectName: "fan-more"
        visible: (rows || []).length > fan.most
        anchors.horizontalCenter: parent.horizontalCenter; anchors.bottom: parent.bottom
        z: fan.shown + 1
        text: "+" + (rows.length - fan.most); color: Kiki.Theme.fg; font.family: Kiki.Theme.mono; font.pixelSize: Math.max(9, fan.cardWidth / 5); font.bold: true
    }
}
