import QtQuick

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
    readonly property bool welcoming: containsDrag && !refusing
    keys: ["text/uri-list"]
    onEntered: drag => t.refusing = !t.pane.dragOver(t.dest, drag)
    onPositionChanged: drag => t.refusing = !t.pane.dragOver(t.dest, drag)
    onExited: t.refusing = false
    onDropped: drop => { t.refusing = false; t.pane.dropInto(t.dest, drop) }
}
