import QtQuick
import ".." as Kiki

// The bottom strip: context keys on the left, status on the right.
Rectangle {
    id: bar
    property var keys: []        // [{key, label}]
    property string status: ""
    /// Room kept clear at the right for what the window puts there: the activity orb.
    property int statusInset: 14
    height: Kiki.Theme.barHeight
    color: Kiki.Theme.bg
    Rectangle { anchors.top: parent.top; width: parent.width; height: 1; color: Kiki.Theme.line }
    // The chips take what the status text leaves. More of them than fit slide under the wheel,
    // or two fingers, to reach the rest — as the path does — rather than being cut off for
    // good: at rest the first chips show (the ones used most), and `scroll` is how far they have
    // been pulled left, 0 to `overflow`.
    readonly property real overflow: Math.max(0, chipRow.width + 28 - chipView.width)
    property real scroll: 0
    onKeysChanged: scroll = 0
    onOverflowChanged: scroll = Math.max(0, Math.min(scroll, overflow))
    function scrollBy(d) { scroll = Math.max(0, Math.min(overflow, scroll + d)) }
    // One axis per WheelHandler, so two. A mouse wheel is vertical only and there is nothing
    // vertical here, so its turn is taken as sideways.
    function wheeled(event, horizontal) {
        const px = horizontal ? event.pixelDelta.x : event.pixelDelta.y
        const d = px !== 0 ? px : (horizontal ? event.angleDelta.x : event.angleDelta.y) / 2
        if (d === 0) { event.accepted = false; return }
        bar.scrollBy(-d)              // the wheel down, or fingers leftwards, pull the chips left
        event.accepted = true
    }
    Item {
        id: chipView
        objectName: "shortcut-chips"
        anchors.left: parent.left; anchors.right: status.left; anchors.rightMargin: 12
        height: parent.height; clip: true
        WheelHandler { enabled: bar.overflow > 0; orientation: Qt.Vertical; acceptedDevices: PointerDevice.Mouse | PointerDevice.TouchPad; onWheel: event => bar.wheeled(event, false) }
        WheelHandler { enabled: bar.overflow > 0; orientation: Qt.Horizontal; acceptedDevices: PointerDevice.Mouse | PointerDevice.TouchPad; onWheel: event => bar.wheeled(event, true) }
        Row {
            id: chipRow
            objectName: "shortcut-row"
            anchors.verticalCenter: parent.verticalCenter; x: 14 - bar.scroll; spacing: 14
            Repeater { model: bar.keys; delegate: KeyChip { required property var modelData; key: modelData.key; label: modelData.label } }
        }
    }
    Text { id: status; anchors.right: parent.right; anchors.rightMargin: bar.statusInset; anchors.verticalCenter: parent.verticalCenter; text: bar.status; color: Kiki.Theme.muted; font.family: Kiki.Theme.mono; font.pixelSize: 11 }
}
