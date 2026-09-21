import QtQuick

// Quickshell's window. A plain QtQuick Window is the same shape where it matters — a window is
// not an Item, so `Shell`'s `left` and `right` panes are properties rather than anchor lines —
// and its content item parents and lays out the shell's children as the real one does.
Window {
    // The real type sizes itself from these; nothing in a test looks at the screen.
    property real implicitWidth: 0
    property real implicitHeight: 0
    property int minimumSize: 0
    width: implicitWidth
    height: implicitHeight
}
