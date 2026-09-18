import QtQuick
import QtTest
import "../../qml/kiki" as Kiki
import KikiTest

// Drag and drop policy (plan 28): what a row offers when it is dragged, and what a drop on a
// folder submits — move within a scheme, copy across one, Ctrl forces copy, self-drops refused.
//
// The drop is driven with a DragEvent-shaped object rather than a synthetic pointer: the panes
// use `Drag.Automatic`, which hands the drag to the compositor, so the pointer half of this
// belongs to the end-to-end layer. Everything the policy decides is here.
TestCase {
    id: tc
    name: "DragDrop"
    when: windowShown
    visible: true

    property var fake: null
    property var pane: null
    Component { id: fakeC; FakeDaemon {} }
    Component { id: paneC; Kiki.Pane {} }

    function init() {
        Kiki.Settings.viewPrefs = ({})
        Wire.reset()
        fake = fakeC.createObject(tc)
        fake.tree = { "file:///home/t": [fake.dir("Projects"), fake.file("a.txt"), fake.file("b.txt")] }
        pane = paneC.createObject(tc)
        pane.listing.daemon = fake
        pane.open("file:///home/t")
        wait(50)
    }
    function cleanup() { pane.destroy(); fake.destroy() }

    /// A DragEvent as QtQuick delivers it, reduced to what `dropInto` reads.
    function dropEvent(uris, modifiers) {
        return {
            hasUrls: false, urls: [],
            hasText: true, text: uris.join("\r\n") + "\r\n",
            modifiers: modifiers || 0,
            proposedAction: Qt.MoveAction,
            accepted: false,
            accept: function (action) { this.accepted = true; this.action = action }
        }
    }

    function test_a_dragged_row_offers_its_uri_as_a_uri_list() {
        pane.selection.set(1)
        const mime = pane.dragMime(1)
        compare(mime["text/uri-list"], "file:///home/t/a.txt\r\n")
    }

    function test_drop_on_a_folder_in_the_same_scheme_moves() {
        pane.dropInto("file:///home/t/Projects", dropEvent(["file:///home/t/a.txt"]))
        const req = Wire.last("Submit")
        verify(req !== null)
        compare(req.op.op, "move")
        compare(req.op.dest, "file:///home/t/Projects")
        compare(req.op.items[0], "file:///home/t/a.txt")
    }

    function test_ctrl_forces_a_copy() {
        pane.dropInto("file:///home/t/Projects", dropEvent(["file:///home/t/a.txt"], Qt.ControlModifier))
        compare(Wire.last("Submit").op.op, "copy")
    }

    function test_across_schemes_it_copies_rather_than_moves() {
        pane.dropInto("sftp://lab/srv", dropEvent(["file:///home/t/a.txt"]))
        compare(Wire.last("Submit").op.op, "copy")
    }

    function test_dropping_into_the_folder_it_came_from_does_nothing() {
        const e = dropEvent(["file:///home/t/a.txt"])
        pane.dropInto("file:///home/t", e)
        compare(e.accepted, false)
        compare(Wire.count("Submit"), 0)
    }

    function test_dropping_a_folder_onto_itself_does_nothing() {
        const e = dropEvent(["file:///home/t/Projects"])
        pane.dropInto("file:///home/t/Projects", e)
        compare(e.accepted, false)
        compare(Wire.count("Submit"), 0)
    }
}
