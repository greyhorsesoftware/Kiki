import QtQuick
import ".." as Kiki

// A folder to drop on. While a drag is over it, it tells the drag what letting go would do — the
// cursor is drawn from that, "not allowed" included — and when the drag lets go, it does it. Every
// view's drop areas are these, so they all tell the same story.
//
// A target that would refuse still TAKES the drag, with the action set to "ignore": a DropArea
// that turns a drag away hears no more of it, keys going down included. So `containsDrag` is true
// over a refusal too; the outline a view draws around a welcoming folder binds to `welcoming`.
DropArea {
    id: t
    property var pane
    property string dest: ""
    property bool refusing: false
    /// Lit while a drag is over it and would be taken — and blinking while it is about to spring.
    readonly property bool welcoming: containsDrag && !refusing && (!flashing || flashOn)
    keys: ["text/uri-list"]

    // ---------------------------------------------------------------- springing (owner, 2026-09-28)
    // A drag held over a folder for a moment opens it — "tunnelling" — so a drop can go deeper
    // than the folder shown without letting go. The folder flashes first, so the opening is seen
    // coming; the pane remembers where the tunnel began (`Pane.tunnelInto`) and goes back there
    // if the drag ends in nothing or leaves the window. The folder already shown never springs:
    // there is nowhere to go.
    property bool springs: true
    readonly property bool canSpring: springs && !!pane && dest !== "" && dest.replace(/\/+$/, "") !== (pane.uri || "").replace(/\/+$/, "")
    property bool flashing: false
    property bool flashOn: true
    property int _flashes: 0
    Timer { id: hold; interval: 700; running: t.containsDrag && !t.refusing && t.canSpring && !t.flashing; onTriggered: t.spring() }
    Timer { id: flash; interval: 90; repeat: true; onTriggered: { t.flashOn = !t.flashOn; if (++t._flashes >= 4) { flash.stop(); t.flashing = false; t.flashOn = true; t.pane.springDest = t.dest } } }
    /// Flash, then ask for the open. A drag that moves on during the flash stops it (below), so
    /// the asking only ever follows a flash the drag stayed for. The opening itself is the view's
    /// SpringPad's to do: this row must not replace the rows while it holds the drag.
    function spring() { flashing = true; flashOn = false; _flashes = 0; flash.start() }
    onContainsDragChanged: if (!containsDrag && flashing) { flash.stop(); flashing = false; flashOn = true }
    // The window's badge is drawn from what `dragOver` has just resolved — `drag.action`, what
    // letting go HERE would do — not from the keys (`proposedAction`): a drag from this machine
    // onto a server is a copy with no key held, and the + was not up until Ctrl was pressed
    // (01-ui-cleanup.md, item 3).
    onEntered: drag => { t.refusing = !t.pane.dragOver(t.dest, drag); Kiki.DragTrack.moved(t.mapToItem(null, drag.x, drag.y), drag.action) }
    onPositionChanged: drag => { t.refusing = !t.pane.dragOver(t.dest, drag); Kiki.DragTrack.moved(t.mapToItem(null, drag.x, drag.y), drag.action) }
    onExited: { t.refusing = false; Kiki.DragTrack.left() }
    onDropped: drop => { t.refusing = false; t.pane.dropInto(t.dest, drop) }
}
