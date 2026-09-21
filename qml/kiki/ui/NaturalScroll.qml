import QtQuick

// Vertical wheel and two-finger scrolling move the content with the fingers, the way the
// sideways swipe in columns view does. Drop one inside any Flickable: `NaturalScroll { }`.
WheelHandler {
    property var view: {
        let p = parent
        while (p && p.contentY === undefined) p = p.parent
        return p
    }
    target: null
    orientation: Qt.Vertical
    acceptedModifiers: Qt.NoModifier
    acceptedDevices: PointerDevice.Mouse | PointerDevice.TouchPad
    /// What the wheel does when this view has nothing to scroll — everything in it fits. Columns
    /// view hands it to the strip, so the wheel over a short column walks the columns.
    property var whenItFits: null
    onWheel: event => {
        const dy = event.pixelDelta.y !== 0 ? event.pixelDelta.y : event.angleDelta.y / 2
        if (dy === 0 || !view) { event.accepted = false; return }
        if (whenItFits && view.contentHeight <= view.height) { whenItFits(dy); event.accepted = true; return }
        view.contentY = Math.max(0, Math.min(view.contentY + dy, Math.max(0, view.contentHeight - view.height)))
        event.accepted = true
    }
}
