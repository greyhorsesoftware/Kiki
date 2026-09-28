import QtQuick
import QtTest
import "../../qml/kiki" as Kiki
import "../../qml/kiki/ui" as UI
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

    /// A DragEvent as QtQuick delivers it, reduced to what `dropInto` reads. It has no
    /// `modifiers`: the keys held arrive folded into `proposedAction` (Move with no key or
    /// Shift, Copy under Ctrl).
    function dropEvent(uris, proposed) {
        return {
            hasUrls: false, urls: [],
            hasText: true, text: uris.join("\r\n") + "\r\n",
            proposedAction: proposed || Qt.MoveAction,
            accepted: false,
            accept: function (action) { this.accepted = true; this.action = action }
        }
    }

    Component { id: ghostC; UI.DragGhost {} }
    Component { id: proxyC; Item { Drag.dragType: Drag.Automatic } }

    // The drag's image (owner, 2026-09-28: "just see a hand now"): filled at the press from
    // what the row would drag, grabbed, and handed to the proxy's Drag.imageSource.
    function test_a_press_fills_the_ghost_and_the_drag_gets_its_image() {
        const ghost = ghostC.createObject(tc)
        pane.ghost = ghost
        pane.selection.set(1); pane.selection.toggle(2)
        ghost.prepare(pane, 1, false)
        compare(ghost.fan.rows.length, 2, "the selection, since the pressed row is in it")
        compare(ghost.fan.count, 2)
        compare(ghost.fan.badge, "", "a move within this machine: no badge")
        tryVerify(() => ghost.ready, 3000, "the grab lands")
        verify(String(ghost.url).length > 0)
        const proxy = proxyC.createObject(tc)
        proxy.Drag.imageSource = ghost.imageFor()
        compare(String(proxy.Drag.imageSource), String(ghost.url), "the drag carries the picture")
        // A row outside the selection drags alone.
        ghost.prepare(pane, 0, false)
        compare(ghost.fan.rows.length, 1)
        // Ctrl at the press: a copy, and the fan says so.
        ghost.prepare(pane, 0, true)
        compare(ghost.fan.badge, "+")
        proxy.destroy(); ghost.destroy()
    }

    // A drag out of a server is a download: a copy, badged as one, whatever the keys.
    function test_a_drag_out_of_a_server_is_badged_as_a_copy() {
        const ghost = ghostC.createObject(tc)
        fake.tree["stub://lab/"] = [fake.file("remote.txt")]
        pane.ghost = ghost
        pane.open("stub://lab/")
        wait(50)
        ghost.prepare(pane, 0, false)
        compare(ghost.fan.badge, "+")
        ghost.destroy()
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
        pane.dropInto("file:///home/t/Projects", dropEvent(["file:///home/t/a.txt"], Qt.CopyAction))
        compare(Wire.last("Submit").op.op, "copy")
    }

    function test_across_schemes_it_copies_rather_than_moves() {
        pane.dropInto("sftp://lab/srv", dropEvent(["file:///home/t/a.txt"]))
        compare(Wire.last("Submit").op.op, "copy")
    }

    function test_dropping_into_the_folder_it_came_from_does_nothing() {
        const e = dropEvent(["file:///home/t/a.txt"])
        pane.dropInto("file:///home/t", e)
        compare(e.action, Qt.IgnoreAction, "sent back where it came from")
        compare(Wire.count("Submit"), 0)
    }

    function test_dropping_a_folder_onto_itself_does_nothing() {
        const e = dropEvent(["file:///home/t/Projects"])
        pane.dropInto("file:///home/t/Projects", e)
        compare(e.action, Qt.IgnoreAction)
        compare(Wire.count("Submit"), 0)
    }

    // While a drag is held, Ctrl and Shift change the picture: "+" for a copy, nothing for a
    // move — and the proxy already carrying the drag is handed the new image.
    function test_modifiers_rebadge_the_picture_mid_drag() {
        const ghost = ghostC.createObject(tc)
        pane.ghost = ghost
        ghost.prepareRows([{ kind: "file", thumb: "" }], 1, false)
        tryVerify(() => ghost.ready, 3000)
        const first = String(ghost.url)
        const proxy = Qt.createQmlObject("import QtQuick; Item { Drag.active: true }", tc, "proxy")
        ghost.begin(proxy)
        verify(ghost.dragging)
        // The picture is not re-made mid-drag: the compositor keeps the icon it was given, so
        // the badge is the window's to draw, from the action Qt proposes for the keys held.
        verify(!ghost.fan.badgeShown, "the badge is not in the picture")
        Kiki.DragTrack.moved(Qt.point(10, 10), Qt.CopyAction)
        compare(Kiki.DragTrack.action, Qt.CopyAction, "Ctrl: Qt proposes a copy")
        Kiki.DragTrack.moved(Qt.point(12, 10), Qt.MoveAction)
        compare(Kiki.DragTrack.action, Qt.MoveAction, "Shift, or nothing: a move")
        compare(String(ghost.url), first, "and the picture stands")
        ghost.end()
        verify(!ghost.dragging)
        proxy.destroy(); pane.ghost = null; ghost.destroy()
    }


    // One file with a thumbnail is carried as that file: nothing rendered, ready at once.
    function test_a_single_thumbnail_is_carried_as_itself() {
        const ghost = ghostC.createObject(tc)
        ghost.prepareRows([{ kind: "image", thumb: "/tmp/t/abc.png" }], 1, false)
        verify(ghost.ready, "no grab to wait for")
        verify(ghost.direct)
        compare(String(ghost.url), "file:///tmp/t/abc.png")
        ghost.prepareRows([{ kind: "image", thumb: "/tmp/t/abc.png" }, { kind: "file", thumb: "" }], 2, false)
        verify(!ghost.direct, "two files are a fan, rendered")
        ghost.destroy()
    }

    // A drop that comes to nothing flies the picture back to where the drag was picked up; a drop
    // that did something does not.
    function test_a_drop_on_nothing_snaps_the_picture_back() {
        const ghost = ghostC.createObject(tc)
        const spy = Qt.createQmlObject("import QtTest; SignalSpy {}", tc, "spy")
        spy.target = ghost; spy.signalName = "snapBack"
        ghost.prepareRows([{ kind: "file", thumb: "" }], 1, false, Qt.point(40, 50))
        compare(ghost.origin, Qt.point(40, 50), "where the press was")
        Kiki.DragTrack.moved(Qt.point(300, 320), Qt.MoveAction)
        ghost.finished(Qt.IgnoreAction)
        compare(spy.count, 1, "nothing happened: the picture goes back")
        compare(spy.signalArguments[0][0], Qt.point(300, 320), "from where it was let go")
        compare(spy.signalArguments[0][1], Qt.point(40, 50), "to where it began")
        ghost.prepareRows([{ kind: "file", thumb: "" }], 1, false, Qt.point(40, 50))
        ghost.finished(Qt.CopyAction)
        compare(spy.count, 1, "a drop that copied flies nothing back")
        spy.destroy(); ghost.destroy()
    }

    // The window knows where a drag's pointer is, from every drop area it crosses.
    function test_the_window_tracks_the_drags_pointer() {
        Kiki.DragTrack.moved(Qt.point(120, 80))
        verify(Kiki.DragTrack.inside)
        compare(Kiki.DragTrack.pointer, Qt.point(120, 80))
        Kiki.DragTrack.left()
        verify(!Kiki.DragTrack.inside)
    }

}
