import QtQuick
import QtTest
import "../../qml/kiki" as Kiki
import "../../qml/kiki/views" as Views
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

    // The badge follows what the drop WOULD do, not the keys (01-ui-cleanup.md, item 3): what
    // DropTarget hands DragTrack is `drag.action` as `dragOver` has just set it.
    function test_over_a_server_the_plus_is_up_without_a_key() {
        const e = dropEvent(["file:///home/t/a.txt"])
        verify(pane.dragOver("sftp://lab/srv", e), "taken")
        compare(e.action, Qt.CopyAction, "a copy, no key held")
        Kiki.DragTrack.moved(Qt.point(10, 10), e.action)
        compare(Kiki.DragTrack.action, Qt.CopyAction, "and the window badges it")
        const local = dropEvent(["file:///home/t/a.txt"])
        pane.dragOver("file:///home/t/Projects", local)
        compare(local.action, Qt.MoveAction, "within this machine, no key: a move, no +")
        Kiki.DragTrack.moved(Qt.point(10, 10), local.action)
        compare(Kiki.DragTrack.action, Qt.MoveAction)
    }

    // A different mountpoint is a different place: the home and a stick are two devices, and a
    // drag between them is a copy with a + — where a move would have been a copy and a delete.
    function test_across_mountpoints_it_copies_and_within_one_it_moves() {
        const ghost = ghostC.createObject(tc)
        pane.ghost = ghost
        fake.devices = { "file:///home/t": 11, "file:///mnt/stick": 22, "file:///home/t/Projects": 11 }
        fake.tree["file:///mnt/stick"] = [fake.dir("photos")]
        pane.open("file:///home/t"); wait(50)
        compare(pane.listing.device, 11, "the folder's device came with the open")
        ghost.prepare(pane, 1, false)
        compare(ghost.sourceDevice, 11)
        // The target pane stands on the stick.
        const target = paneC.createObject(tc)
        target.listing.daemon = fake; target.ghost = ghost
        target.open("file:///mnt/stick"); wait(50)
        compare(target.listing.device, 22)
        const e = dropEvent(["file:///home/t/a.txt"])
        verify(target.dragOver("file:///mnt/stick/photos", e))
        compare(e.action, Qt.CopyAction, "another device: a copy")
        // A fresh event for the drop: `dragOver` marks its event accepted, as a DropArea's is.
        target.dropInto("file:///mnt/stick/photos", dropEvent(["file:///home/t/a.txt"]))
        compare(Wire.last("Submit").op.op, "copy")
        // The same device: a move, as ever.
        target.open("file:///home/t/Projects"); wait(50)
        const same = dropEvent(["file:///home/t/a.txt"])
        target.dragOver("file:///home/t/Projects", same)
        compare(same.action, Qt.MoveAction)
        // No source device (a drag from another application): one machine, a move.
        ghost.end()
        const foreign = dropEvent(["file:///home/t/a.txt"])
        target.open("file:///mnt/stick"); wait(50)
        target.dragOver("file:///mnt/stick/photos", foreign)
        compare(foreign.action, Qt.MoveAction)
        target.destroy(); ghost.destroy()
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


    // A copy asked for (Ctrl) of a file into its own folder is a duplicate — "name (2).ext",
    // with no collision prompt — where a move into its own folder is still nothing.
    function test_a_copy_into_its_own_folder_is_a_duplicate() {
        const here = "file:///home/t", file = "file:///home/t/a.txt"
        compare(pane.dropAction([file], here, false), null, "a move into its own folder is nothing")
        const r = pane.dropAction([file], here, true)
        verify(r !== null, "a copy asked for goes through")
        compare(r.op, "copy")
        compare(r.items, [file])
        compare(r.policy, "keepBoth", "named by the daemon, without asking")
        // Elsewhere, no policy is needed: the folder is not the file's own.
        const away = pane.dropAction([file], "file:///home/t/Projects", true)
        compare(away.policy, undefined)
    }


    // The tunnel: springing into a folder remembers where the drag began; a drop ends it where
    // it is; going out closes it all the way back.
    function test_the_tunnel_remembers_where_it_began_and_goes_back() {
        pane.open("file:///home/t"); wait(50)
        compare(pane.tunnelStart, "")
        pane.tunnelInto("file:///home/t/Projects")
        compare(pane.tunnelStart, "file:///home/t", "where the tunnel began")
        compare(pane.uri, "file:///home/t/Projects")
        pane.tunnelInto("file:///home/t/Projects/deeper")
        compare(pane.tunnelStart, "file:///home/t", "deeper is still the same tunnel")
        pane.springDest = "file:///home/t/Projects/deeper/still"     // asked for, not yet taken
        pane.tunnelBack()
        compare(pane.uri, "file:///home/t", "all the way back in one step")
        compare(pane.tunnelStart, "")
        compare(pane.springDest, "", "and a spring still standing is withdrawn with it")
        pane.tunnelInto("file:///home/t/Projects")
        pane.tunnelDone()
        compare(pane.uri, "file:///home/t/Projects", "a drop ends the tunnel where it is")
        compare(pane.tunnelStart, "")
        pane.tunnelBack()
        compare(pane.uri, "file:///home/t/Projects", "and there is nothing to go back to")
    }

    // A drag held over a folder flashes it and opens it; one that moves on before the flash is
    // done opens nothing.
    Component { id: targetC; Views.DropTarget {} }
    Component { id: padC; Views.SpringPad {} }
    function test_a_drag_held_over_a_folder_springs_it_open() {
        pane.open("file:///home/t"); wait(50)
        const t = targetC.createObject(tc, { pane: pane, dest: "file:///home/t/Projects", width: 100, height: 20 })
        verify(t.canSpring, "another folder than the one shown")
        t.spring()
        verify(t.flashing, "the flash is the warning")
        // The row only ASKS: the view's pad takes the drag over and does the opening, so the row
        // is never destroyed while Qt still points at it as the drag's target.
        tryCompare(pane, "springDest", "file:///home/t/Projects", 2000)
        compare(pane.uri, "file:///home/t", "nothing opened by the row itself")
        const pad = padC.createObject(tc, { pane: pane, width: 100, height: 100 })
        verify(pad.visible, "the pad is up while a spring is asked for")
        compare(pane.uri, "file:///home/t/Projects", "and opened it the moment it appeared — no move needed")
        compare(pane.tunnelStart, "file:///home/t")
        tryCompare(pane, "springDest", "", 2000)
        verify(!pad.visible, "and stood down once the rows were there")
        pad.destroy()
        // The pane's own folder never springs.
        const own = targetC.createObject(tc, { pane: pane, dest: "file:///home/t/Projects/", width: 100, height: 20 })
        verify(!own.canSpring)
        own.destroy(); t.destroy(); pane.tunnelBack()
    }

    // The tracker says when a drag has gone out: `inside` false for longer than a step between rows.
    function test_the_tracker_says_when_a_drag_has_gone_out() {
        const spy = Qt.createQmlObject("import QtTest; SignalSpy {}", tc, "spy")
        spy.target = Kiki.DragTrack; spy.signalName = "wentOut"
        Kiki.DragTrack.moved(Qt.point(10, 10), Qt.MoveAction)
        Kiki.DragTrack.left(); wait(40); Kiki.DragTrack.moved(Qt.point(12, 30), Qt.MoveAction)
        wait(200)
        compare(spy.count, 0, "a step between rows is not going out")
        Kiki.DragTrack.left()
        tryCompare(spy, "count", 1, 1000)
        spy.destroy()
    }


    // The drag is carried by the ghost's own source, not the view's proxy: the proxy sits in a
    // row that a spring may replace mid-drag, and the source has to outlive the rows.
    function test_the_drag_is_started_on_the_ghosts_own_source() {
        const ghost = ghostC.createObject(tc)
        pane.ghost = ghost
        const proxy = proxyC.createObject(tc)
        proxy.Drag.mimeData = pane.uriListMime(["file:///home/t/a.txt"])
        proxy.Drag.proposedAction = Qt.CopyAction
        ghost.prepareRows([{ kind: "file", thumb: "" }], 1, false)
        ghost.startDrag(proxy, () => true)
        verify(ghost.dragging)
        verify(ghost.proxy !== proxy, "not the view's proxy")
        compare(ghost.proxy.Drag.mimeData["text/uri-list"], "file:///home/t/a.txt\r\n", "carrying what the view gave it")
        compare(ghost.proxy.Drag.proposedAction, Qt.CopyAction)
        verify(!proxy.Drag.active, "the view's proxy drags nothing itself")
        ghost.end(); proxy.destroy(); pane.ghost = null; ghost.destroy()
    }

}
