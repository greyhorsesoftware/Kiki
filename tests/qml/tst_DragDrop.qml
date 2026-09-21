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

    /// A DragEvent as QtQuick delivers it, reduced to what `dropInto` reads. `formats` carries
    /// the mime types, which is where a right-button drag's mark travels.
    function dropEvent(uris, modifiers, formats) {
        return {
            hasUrls: false, urls: [],
            hasText: true, text: uris.join("\r\n") + "\r\n",
            formats: formats || ["text/uri-list"],
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

    // ---------------------------------------------------------------- the drop that asks

    // Alt, or a drag the right button started, puts the choice to the user instead of taking it.
    // Nothing is submitted, and the drop is left UNACCEPTED: a source told its files were moved
    // may delete them, and the answer can still be Cancel.
    function test_alt_asks_instead_of_deciding() {
        let asked = null
        const onAsk = spec => asked = spec
        pane.askDrop.connect(onAsk)
        const e = dropEvent(["file:///home/t/a.txt"], Qt.AltModifier)
        pane.dropInto("file:///home/t/Projects", e)
        pane.askDrop.disconnect(onAsk)
        verify(asked !== null, "it asks")
        compare(asked.items, ["file:///home/t/a.txt"])
        compare(asked.dest, "file:///home/t/Projects")
        compare(asked.op, "move", "what it would have done unasked, for the menu to lead with")
        compare(e.accepted, false, "nothing is taken while the question stands")
        compare(Wire.count("Submit"), 0)
    }

    function test_a_right_button_drag_asks_by_the_mark_it_carries() {
        let asked = null
        const onAsk = spec => asked = spec
        pane.askDrop.connect(onAsk)
        pane.dropInto("file:///home/t/Projects", dropEvent(["file:///home/t/a.txt"], 0, ["text/uri-list", pane.askKey]))
        pane.askDrop.disconnect(onAsk)
        verify(asked !== null, "the mark in the payload is what asks: the drop end is never told the button")
        compare(Wire.count("Submit"), 0)
    }

    // A drop that cannot happen at all is refused before anything is asked about it.
    function test_a_refused_drop_asks_nothing_even_with_alt() {
        let asked = null
        const onAsk = spec => asked = spec
        pane.askDrop.connect(onAsk)
        pane.dropInto("file:///home/t", dropEvent(["file:///home/t/a.txt"], Qt.AltModifier))
        pane.askDrop.disconnect(onAsk)
        compare(asked, null)
        compare(Wire.count("Submit"), 0)
    }

    function test_a_dragged_row_can_carry_the_mark() {
        verify(pane.dragMime(1)[pane.askKey] === undefined, "the left button carries no mark")
        compare(pane.dragMime(1, true)[pane.askKey], "1")
        compare(pane.dragMime(1, true)["text/uri-list"], "file:///home/t/a.txt\r\n", "and the files just the same")
    }

    function test_dropping_a_folder_onto_itself_does_nothing() {
        const e = dropEvent(["file:///home/t/Projects"])
        pane.dropInto("file:///home/t/Projects", e)
        compare(e.accepted, false)
        compare(Wire.count("Submit"), 0)
    }
}
