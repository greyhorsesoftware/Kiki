import QtQuick

// What makes a pane the focused one, side by side: a press inside it — on a file, on its header,
// or on white space. Nothing else: the pointer merely passing over a pane does not move the focus
// (tried, 2026-09-19: focus following the mouse meant reaching across one pane to the toolbar
// changed which pane the toolbar was about to act on). Lay it OVER the pane, topmost:
//
//     UI.PaneFocus { width: …; height: …; onWanted: win.focusPane(win.left) }
//
// It has to be on top. A handler on the pane's own parent never sees a press that a row's
// MouseArea claims (tried: TapHandler and PointHandler on the parent both stay idle, and a click
// on a file left the focus in the other pane). On top, it is asked first — and because it is a
// PointHandler, which only ever takes a PASSIVE grab, it then lets everything through: the rows,
// the rubber band, the wheel, hover and the context menu underneath all work as before.
Item {
    id: pf
    signal wanted()
    /// Off with one pane: there is nothing to choose between.
    property bool active: true
    PointHandler {
        enabled: pf.active
        acceptedButtons: Qt.LeftButton | Qt.RightButton | Qt.MiddleButton
        onActiveChanged: if (active) pf.wanted()
    }
}
