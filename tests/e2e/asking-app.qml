import QtQuick
import Quickshell

// The application that asked for a file, for the chooser flow (docs/0.5.0/11-chooser-window.md).
// Not kiki: a plain window of its own, which the test compositor makes full screen like any
// other, so it covers the file manager's window completely. The chooser must be drawn over THIS
// — that is the whole of the owner's fault ("clicked save as and the panel showed up behind the
// window I was in") and the only way to tell a chooser that is above every window from one that
// is painted inside the file manager's.
//
// One flat colour and nothing else: the flow reads pixels, and a colour no theme would choose
// cannot be confused with the chooser's own.
ShellRoot {
    FloatingWindow {
        id: asker
        title: "the asking application"
        implicitWidth: 1280; implicitHeight: 720
        color: "#ff00ff"
        Rectangle { anchors.fill: parent; color: "#ff00ff" }
    }
}
