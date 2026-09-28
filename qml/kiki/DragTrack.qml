pragma Singleton
import QtQuick

// Where a drag's pointer is over this window, in window coordinates, told by every drop area
// as the drag crosses it. The compositor draws a drag's icon once, at pick-up, and ignores a
// picture changed while the drag is held — so what has to change with the modifiers (the "+"
// of a copy) is drawn by the window itself, at this point (owner, 2026-09-28: "when I press the
// modifier keys, the drag image should show the different actions").
QtObject {
    id: track
    property point pointer: Qt.point(0, 0)
    property bool inside: false
    /// What the drop would do, as Qt proposes it for the modifiers held right now — Ctrl says
    /// copy, Shift says move. The window never sees the keys themselves during a drag (Qt runs
    /// the drag in a loop of its own), but every drop area is told this on every move.
    property int action: 0
    function moved(p, action) { pointer = p; inside = true; gone.stop(); if (action !== undefined) track.action = action }
    function left() { inside = false; gone.restart() }
    /// The drag has left the window, or ended: `inside` has stayed false long enough that this
    /// was not the gap between one row and the next. A tunnel closes on it.
    signal wentOut()
    property Timer gone: Timer { interval: 150; onTriggered: track.wentOut() }
}
