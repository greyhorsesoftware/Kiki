import QtQuick
import ".." as Kiki

// Where a drop lands while a spring replaces the rows. The row that sprang asks for the folder
// (`Pane.springDest`); this pad appears over the whole view, opens it at once, and stands in as
// the drag's target until the new rows are there — a drop meanwhile goes to the folder now
// shown. It stands down after a moment, hidden and never destroyed. (It first waited for the
// drag to ENTER it before opening, on the theory that destroying the row Qt pointed at as the
// target was what crashed Quickshell; the core dump said the drag's SOURCE was what died — see
// DragGhost — and waiting meant nothing opened until the pointer moved, 2026-09-28.)
DropArea {
    id: pad
    property var pane
    anchors.fill: parent
    z: 1000
    visible: !!pane && pane.springDest !== ""
    keys: ["text/uri-list"]
    onEntered: drag => { if (drag && drag.accept) drag.accept() }
    // The ask is the trigger, not the pad's visibility: a pad born with an ask already standing
    // sees no change of visibility, and must open all the same.
    Connections { target: pad.pane; function onSpringDestChanged() { if (pad.pane.springDest) pad.take(null) } }
    Component.onCompleted: if (pane && pane.springDest) take(null)
    onVisibleChanged: if (!visible) settle.stop()
    onPositionChanged: drag => Kiki.DragTrack.moved(pad.mapToItem(null, drag.x, drag.y), drag.proposedAction)
    // A drop while the pad is up goes to the folder now shown: it is what the drag is over.
    onDropped: drop => { pane.dropInto(pane.uri, drop); pane.springDest = "" }
    /// Open the asked-for folder now; the new rows take the drag over on its next move.
    function take(drag) {
        if (drag && drag.accept) drag.accept()
        const dest = pane.springDest
        if (!dest) return
        pane.tunnelInto(dest)
        settle.restart()
    }
    // Down again once the new rows have had a frame or two to appear; the drag's next move
    // finds them. Hidden, not destroyed.
    Timer { id: settle; interval: 250; onTriggered: pane.springDest = "" }
}
