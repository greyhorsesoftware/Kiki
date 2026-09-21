import QtQuick
import QtTest
import "../../qml/kiki" as Kiki
import "../../qml/kiki/views" as Views
import "../qml" as Harness
import KikiTest

// Real drags, not drag-shaped objects. Under the `minimal` platform Qt performs an actual
// in-process drag — a QDrag with its own event loop, drag enter and move and drop delivered to the
// DropAreas, and the keys held folded into `proposedAction` by Qt itself — so what is tested here
// is what a hand gets, less only the compositor. (`offscreen`, which the rest of the QML suite
// uses, ends every drag the moment it starts; this directory is run separately, under `minimal`.)
//
// A drag runs its own event loop, so the test cannot step the pointer from its own body once the
// drag is in the air: a Timer does, from inside that loop, and writes down after every step what
// the folder under the pointer was showing — welcoming the drop, refusing it, or nothing.
TestCase {
    id: tc
    name: "RealDrag"
    when: windowShown
    visible: true
    width: 900; height: 400

    property var fake: null
    property var leftPane: null
    property var rightPane: null
    Component { id: fakeC; Harness.FakeDaemon {} }
    Component { id: paneC; Kiki.Pane {} }

    Item {
        anchors.fill: parent
        Views.ListPane { id: leftList; x: 0; y: 0; width: 440; height: parent.height; pane: tc.leftPane }
        Views.ListPane { id: rightList; x: 460; y: 0; width: 440; height: parent.height; pane: tc.rightPane }
    }

    property var steps: []
    property var shown: []                    // after each step: "left welcoming", "right refusing", ""
    Timer {
        id: stepper; interval: 40; repeat: true
        onTriggered: {
            const s = tc.steps.shift()
            if (!s) { stop(); return }
            if (s.key) keyClick(s.key)
            else if (s.release) mouseRelease(tc, s.x, s.y, Qt.LeftButton, s.mods || 0)
            else mouseMove(tc, s.x, s.y, -1, Qt.LeftButton, s.mods || 0)
            tc.shown.push(tc.showing())
        }
    }

    function init() {
        Kiki.Settings.viewPrefs = ({})
        Wire.reset()
        fake = fakeC.createObject(tc)
        fake.tree = {
            "file:///home/t": [fake.dir("Projects"), fake.file("a.txt"), fake.file("b.txt")],
            "file:///home/u": [fake.file("c.txt")],
            "sftp://lab/srv": [fake.file("far.txt")],
        }
        leftPane = paneC.createObject(tc); leftPane.listing.daemon = fake
        rightPane = paneC.createObject(tc); rightPane.listing.daemon = fake
        leftList.pane = leftPane; rightList.pane = rightPane
        leftPane.open("file:///home/t")
        rightPane.open("file:///home/u")
        wait(80)
        Wire.reset()
    }
    function cleanup() { leftList.pane = null; rightList.pane = null; leftPane.destroy(); rightPane.destroy(); fake.destroy() }

    /// What the two panes' folders are showing a drag right now.
    function showing() {
        const out = []
        for (const [name, list] of [["left", leftList], ["right", rightList]]) {
            const t = findChild(list, "list-drop-background")
            if (t.containsDrag) out.push(name + (t.refusing ? " refusing" : " welcoming"))
        }
        return out.join(", ")
    }
    /// A point inside `item`, in the test window's coordinates.
    function at(item, fx, fy) { return item.mapToItem(tc, item.width * fx, item.height * fy) }
    /// Press on `row`, pull it past the threshold, then play `path` from inside the drag's loop.
    function drag(row, path) {
        steps = path; shown = []
        const p = at(row, 0.5, 0.5)
        mousePress(tc, p.x, p.y, Qt.LeftButton)
        mouseMove(tc, p.x + 20, p.y, -1, Qt.LeftButton)
        mouseMove(tc, p.x + 40, p.y, -1, Qt.LeftButton)
        stepper.start()
        wait(path.length * 40 + 400)
    }
    /// Over the right pane's empty space, then let go there, holding `mods` throughout.
    function intoRight(mods) {
        const q = at(rightList, 0.5, 0.8)
        return [{ x: q.x, y: q.y, mods: mods }, { x: q.x + 4, y: q.y + 4, mods: mods }, { release: true, x: q.x + 4, y: q.y + 4, mods: mods }]
    }
    function submitted() { const r = Wire.last("Submit"); return r ? r.op : null }

    // ---------------------------------------------------------------- the keys, for real

    function test_no_key_within_one_place_moves() {
        drag(findChild(leftList, "row-1"), intoRight(0))                        // a.txt
        compare(shown[0], "right welcoming", "the folder took the drag while it was in the air")
        compare(submitted().op, "move")
        compare(submitted().items, ["file:///home/t/a.txt"])
        compare(submitted().dest, "file:///home/u")
    }

    function test_ctrl_copies() {
        drag(findChild(leftList, "row-1"), intoRight(Qt.ControlModifier))
        compare(submitted().op, "copy")
    }

    // Qt reports Shift exactly as it reports no key at all (Move), so within one place Shift moves.
    function test_shift_within_one_place_moves() {
        drag(findChild(leftList, "row-1"), intoRight(Qt.ShiftModifier))
        compare(submitted().op, "move")
    }

    // A source that offers no Link has Alt fall back to Copy, in Qt itself: so Alt copies.
    function test_alt_copies_as_ctrl_does() {
        drag(findChild(leftList, "row-1"), intoRight(Qt.AltModifier))
        compare(submitted().op, "copy")
    }

    // The keys are read at the drop, not captured at the start: Ctrl going down mid-drag copies.
    function test_a_key_pressed_mid_drag_decides_the_drop() {
        const q = at(rightList, 0.5, 0.8)
        drag(findChild(leftList, "row-1"), [
            { x: q.x, y: q.y },
            { x: q.x + 2, y: q.y, mods: Qt.ControlModifier },
            { x: q.x + 4, y: q.y, mods: Qt.ControlModifier },
            { release: true, x: q.x + 4, y: q.y, mods: Qt.ControlModifier },
        ])
        compare(submitted().op, "copy", "read at the drop, not captured at the start")
    }

    // ---------------------------------------------------------------- refused while dragging

    // Onto the folder it is already in: refused as it is dragged — the drag is told "ignore",
    // which is the "not allowed" cursor — not only when the button comes up.
    function test_a_refused_target_says_no_during_the_drag() {
        const home = at(leftList, 0.5, 0.85)                                   // a.txt's own folder
        drag(findChild(leftList, "row-1"), [
            { x: home.x, y: home.y }, { x: home.x + 4, y: home.y + 4 }, { release: true, x: home.x + 4, y: home.y + 4 }])
        compare(shown[0], "left refusing")
        compare(Wire.count("Submit"), 0)
    }

    // ---------------------------------------------------------------- across machines

    function test_across_machines_no_key_copies_and_so_does_shift() {
        rightPane.open("sftp://lab/srv"); wait(60); Wire.reset()
        drag(findChild(leftList, "row-1"), intoRight(0))
        compare(submitted().op, "copy")
        drag(findChild(leftList, "row-2"), intoRight(Qt.ShiftModifier))       // b.txt
        compare(submitted().op, "copy", "Shift reads as no key: it cannot force a move between machines")
    }

    // ---------------------------------------------------------------- focus and cancel

    function test_the_pane_a_drop_lands_in_says_so() {
        let took = 0
        const onTook = () => took++
        rightPane.received.connect(onTook)
        drag(findChild(leftList, "row-1"), intoRight(0))
        rightPane.received.disconnect(onTook)
        compare(took, 1, "the window gives this pane the focus when it hears it")
    }

    // Pressing one row of a selection to drag it must not collapse the selection to that row.
    function test_the_press_that_starts_a_drag_keeps_the_selection() {
        leftPane.selection.set(1)
        leftPane.selection.toggle(2)                                           // a.txt and b.txt
        drag(findChild(leftList, "row-1"), intoRight(0))
        compare(submitted().items, ["file:///home/t/a.txt", "file:///home/t/b.txt"], "both went")
    }

    function test_escape_cancels_and_changes_nothing() {
        const q = at(rightList, 0.5, 0.8)
        drag(findChild(leftList, "row-1"), [
            { x: q.x, y: q.y }, { key: Qt.Key_Escape }, { release: true, x: q.x, y: q.y }])
        compare(shown[0], "right welcoming")
        compare(Wire.count("Submit"), 0, "Escape let go of the drag: nothing was dropped")
    }
}
