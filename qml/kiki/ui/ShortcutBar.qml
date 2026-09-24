import QtQuick
import ".." as Kiki

// The bottom strip: context keys on the left, status on the right. A message for the user — the
// last destructive job with its Undo, an error — takes the keys' place for a while: the chips
// roll up out of the strip, the message rolls in from below, and when it is done (the toast's
// timer, its ×, Undo) they change places again (owner, 2026-09-24: "instead of toast, show the
// message on the bottom bar where the shortcuts are"). Until then it was a card floating over
// the view.
Rectangle {
    id: bar
    property var keys: []        // [{key, label}]
    property string status: ""
    /// The message up now: `{ text, undoable }`, or null. `Kiki.Jobs.toast`, handed in.
    property var toast: null
    signal undo()
    signal dismiss()
    /// Where the chips and the message are between their two places: 0 chips, 1 message.
    readonly property real rolled: toast ? 1 : 0
    property real roll: rolled
    Behavior on roll { NumberAnimation { duration: 180; easing.type: Easing.OutCubic } }
    // The message text is kept through the roll out, so the line does not blank as it goes.
    property string shownText: ""
    property bool shownUndoable: false
    onToastChanged: if (toast) { shownText = toast.text || ""; shownUndoable = toast.undoable === true }
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
        WheelHandler { enabled: bar.overflow > 0 && !bar.toast; orientation: Qt.Vertical; acceptedDevices: PointerDevice.Mouse | PointerDevice.TouchPad; onWheel: event => bar.wheeled(event, false) }
        WheelHandler { enabled: bar.overflow > 0 && !bar.toast; orientation: Qt.Horizontal; acceptedDevices: PointerDevice.Mouse | PointerDevice.TouchPad; onWheel: event => bar.wheeled(event, true) }
        Row {
            id: chipRow
            objectName: "shortcut-row"
            y: Math.round((parent.height - height) / 2) - bar.roll * bar.height; x: 14 - bar.scroll; spacing: 14
            Repeater { model: bar.keys; delegate: KeyChip { required property var modelData; key: modelData.key; label: modelData.label } }
        }
        // The message, in the chips' place.
        Row {
            id: toastRow
            objectName: "bar-toast"
            visible: bar.roll > 0
            y: Math.round((parent.height - height) / 2) + (1 - bar.roll) * bar.height; x: 14; spacing: 12
            Text { anchors.verticalCenter: parent.verticalCenter; text: bar.shownText; color: Kiki.Theme.fg; font.family: Kiki.Theme.mono; font.pixelSize: 12; elide: Text.ElideRight
                   width: Math.min(implicitWidth, chipView.width - 14 - (undoChip.visible ? undoChip.width + 12 : 0) - 12 - 12 - 12) }
            Rectangle {
                id: undoChip
                objectName: "toast-undo"
                visible: bar.shownUndoable
                height: 18; width: undoRow.width + 12; radius: 2; color: Kiki.Theme.surface; anchors.verticalCenter: parent.verticalCenter
                Row { id: undoRow; anchors.centerIn: parent; spacing: 6
                    Text { text: "Undo"; color: Kiki.Theme.accent; font.family: Kiki.Theme.mono; font.pixelSize: 11; font.bold: true }
                    Text { text: "Ctrl+Z"; color: Kiki.Theme.muted; font.family: Kiki.Theme.mono; font.pixelSize: 10; anchors.verticalCenter: parent.verticalCenter }
                }
                MouseArea { anchors.fill: parent; onClicked: bar.undo() }
            }
            Icon { objectName: "toast-close"; name: "x"; size: 10; color: Kiki.Theme.muted; anchors.verticalCenter: parent.verticalCenter; MouseArea { anchors.fill: parent; anchors.margins: -4; onClicked: bar.dismiss() } }
        }
    }
    Text { id: status; anchors.right: parent.right; anchors.rightMargin: bar.statusInset; anchors.verticalCenter: parent.verticalCenter; text: bar.status; color: Kiki.Theme.muted; font.family: Kiki.Theme.mono; font.pixelSize: 11 }
}
