import QtQuick
import QtTest
import "../../qml/kiki" as Kiki
import KikiTest

// A keyboard transfer between the panes is seen to go (owner, 2026-09-28): a FileFan flies from
// the source pane to the other and fades, with the copy's "+"; a paste flies the other way.
TestCase {
    id: tc
    name: "FlyFiles"
    when: windowShown
    visible: true
    width: 1200; height: 760

    property var fake: null
    Component { id: fakeC; FakeDaemon {} }
    Kiki.Shell { id: shell; width: 1200; height: 760; visible: true }

    function initTestCase() {
        fake = fakeC.createObject(tc)
        fake.tree = { "file:///home/t": [fake.file("a.txt"), fake.file("b.txt")], "file:///home/u": [fake.file("x.txt")] }
        shell.left.listing.daemon = fake
        shell.right.listing.daemon = fake
    }
    function init() {
        Wire.reset()
        shell.sideBySide = true
        shell.left.view = "list"; shell.right.view = "list"
        shell.left.open("file:///home/t"); shell.right.open("file:///home/u")
        wait(80)
        shell.focusPane(shell.left)
        shell.left.selection.set(0)
        wait(20)
        shell.lastFly = null
    }
    function cleanup() { shell.sideBySide = false; wait(20) }
    function flyer() { return findChild(shell, "flyer") }

    function test_a_keyboard_copy_flies_from_this_pane_to_the_other_with_a_plus() {
        shell.transfer(false)
        verify(shell.lastFly !== null, "a flight was made")
        compare(shell.lastFly.from, "left"); compare(shell.lastFly.to, "right")
        compare(shell.lastFly.count, 1); compare(shell.lastFly.badge, "+")
        verify(flyer().visible, "in the air")
        compare(flyer().badge, "+")
        // And the copy itself was submitted, as before.
        const req = Wire.last("Submit"); verify(req !== null); compare(req.op.op, "copy")
        tryVerify(() => !flyer().visible, 3000, "landed and gone")
    }
    function test_a_keyboard_move_flies_without_a_badge() {
        shell.transfer(true)
        compare(shell.lastFly.badge, "")
        compare(Wire.last("Submit").op.op, "move")
    }
    function test_a_paste_flies_in_from_the_other_pane() {
        shell.copySelection(false)
        shell.focusPane(shell.right)
        shell.paste()
        verify(shell.lastFly !== null)
        compare(shell.lastFly.from, "left"); compare(shell.lastFly.to, "right")
        compare(shell.lastFly.badge, "+")
    }
    function test_nothing_flies_when_not_side_by_side() {
        shell.sideBySide = false; wait(20)
        shell.transfer(false)
        verify(shell.lastFly === null, "one pane: nowhere to fly to")
    }

    // The snap-back is the same flight, from the drop point to the pick-up point.
    function test_a_drop_on_nothing_flies_back_to_the_pick_up() {
        const ghost = findChild(shell, "drag-ghost"), flyer = findChild(shell, "flyer")
        ghost.prepareRows([{ kind: "file", thumb: "" }], 2, false, Qt.point(100, 100))
        Kiki.DragTrack.moved(Qt.point(500, 400), Qt.MoveAction)
        ghost.finished(Qt.IgnoreAction)
        verify(flyer.visible, "the fan is in flight")
        verify(shell.lastFly && shell.lastFly.snapBack, "and it is the snap-back")
        compare(shell.lastFly.count, 2)
    }

    // The "+" the window draws beside the pointer while a drag of ours would copy: a drag begun
    // on the ghost, a move reported with the copy action, and the badge is there; with a move
    // action, or no drag, it is not. (The compositor keeps the icon it was given at pick-up, and
    // never delivers the keys to the window during a drag, so this is the only way a "+" can
    // follow Ctrl.)
    function test_the_window_draws_a_plus_while_the_drop_would_copy() {
        const badge = findChild(shell, "drag-badge")
        verify(badge !== null, "the badge is in the window's item tree")
        const ghost = findChild(shell, "drag-ghost")
        verify(ghost !== null)
        const proxy = Qt.createQmlObject("import QtQuick; Item { Drag.active: true }", shell, "proxy")
        verify(!badge.visible)
        ghost.begin(proxy)
        Kiki.DragTrack.moved(Qt.point(300, 200), Qt.CopyAction)
        verify(badge.visible, "Ctrl: Qt proposes a copy, the badge shows")
        verify(badge.x < 300 && badge.y > 200, "just off the cursor's tip, below and to the left")
        Kiki.DragTrack.moved(Qt.point(310, 200), Qt.MoveAction)
        verify(!badge.visible, "a move: no badge")
        ghost.copyByDefault = true
        verify(badge.visible, "out of a server it is a copy whatever the keys say")
        ghost.copyByDefault = false
        Kiki.DragTrack.left()
        verify(!badge.visible, "gone with the pointer")
        ghost.end()
        proxy.destroy()
    }

}
