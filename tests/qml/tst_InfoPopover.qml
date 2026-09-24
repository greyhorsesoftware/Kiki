import QtQuick
import QtTest
import "../../qml/kiki" as Kiki
import KikiTest

// The info panel in side by side (0.1.1): a callout on the selected row of the focused pane, not
// the docked panel — which, before this, was laid out past the window's right edge in split and
// opened invisibly. It follows the selection and the focus, and goes on Escape, Ctrl+I or a
// click anywhere else; the panes' widths do not move for it.
TestCase {
    id: tc
    name: "InfoPopover"
    when: windowShown
    visible: true
    width: 1200; height: 760

    property var fake: null
    Component { id: fakeC; FakeDaemon {} }
    Kiki.Shell { id: shell; width: 1200; height: 760; visible: true }

    function initTestCase() {
        fake = fakeC.createObject(tc)
        fake.tree = {
            "file:///home/t": [fake.file("a.txt"), fake.file("b.txt"), fake.file("c.txt")],
            "file:///home/u": [fake.file("x.txt"), fake.file("y.txt")],
            "file:///home/many": Array.from({ length: 40 }, (_, i) => fake.file("f" + String(i).padStart(2, "0") + ".txt")),
        }
        shell.left.listing.daemon = fake
        shell.right.listing.daemon = fake
    }
    function init() {
        shell.inspectorRequested = false
        shell.sideBySide = true
        shell.left.view = "list"; shell.right.view = "list"
        shell.left.open("file:///home/t"); shell.right.open("file:///home/u")
        wait(80)
        shell.focusPane(shell.left)
        shell.left.selection.set(0)
        wait(20)
    }
    function cleanup() { shell.inspectorRequested = false; shell.sideBySide = false; wait(20) }

    function card() { return findChild(shell, "info-popover") }
    /// Beside the pane, off its rows: to its right when there is room, else to its left.
    function beside(item, pane) {
        const a = item.mapToItem(null, 0, 0), p = pane.mapToItem(null, 0, 0)
        const right = a.x >= p.x + pane.width, left = a.x + item.width <= p.x
        return (right || left) && a.y >= p.y - 1
    }

    function test_ctrl_i_in_split_opens_a_card_over_the_focused_pane_and_moves_no_pane() {
        const left = findChild(shell, "pane-left"), right = findChild(shell, "pane-right")
        const wasLeft = left.width, wasRight = right.width
        shell.runAction("inspector")
        verify(shell.inspector)
        tryVerify(() => tc.card().visible, 2000)
        verify(beside(card(), left), "beside the left pane, off its rows")
        verify(!findChild(shell, "inspector-grip").visible || findChild(shell, "inspector-grip").parent !== card(), "no grip on the card")
        compare(left.width, wasLeft, "the panes do not move for it")
        compare(right.width, wasRight)
        const slot = findChild(shell, "pane-left")
        verify(slot !== null)
        verify(!findChild(card(), "inspector-count").visible, "one row: no count")
    }
    function test_the_pointer_sits_on_the_selected_row_and_follows_it() {
        shell.runAction("inspector")
        tryVerify(() => tc.card().visible, 2000)
        const pointer = findChild(shell, "info-popover-pointer")
        tryVerify(() => pointer.visible, 2000)
        const view = shell.currentView()
        const rowA = view.rowItem(0).mapToItem(null, 0, view.rowItem(0).height / 2).y
        let py = pointer.mapToItem(null, 7, 7).y            // its centre: rotation moves the corners, not that
        verify(Math.abs(py - rowA) < 4, "on a.txt: " + py + " vs " + rowA)
        shell.left.selection.set(2)
        const rowC = view.rowItem(2).mapToItem(null, 0, view.rowItem(2).height / 2).y
        tryVerify(() => Math.abs(findChild(shell, "info-popover-pointer").mapToItem(null, 7, 7).y - rowC) < 4, 2000)
        verify(beside(card(), findChild(shell, "pane-left")))
    }
    // Centred on the row (owner, 2026-09-24), where the pane's edges allow: a row mid-pane has
    // the card's middle on it; one at the top has the card held under the pane's top edge, the
    // pointer slid up to the row.
    function test_the_card_is_centred_on_the_row_unless_an_edge_holds_it() {
        shell.left.open("file:///home/many"); wait(80)
        shell.left.selection.set(12)
        shell.runAction("inspector")
        tryVerify(() => tc.card().visible, 2000)
        const view = shell.currentView()
        const rowY = view.rowItem(12).mapToItem(null, 0, view.rowItem(12).height / 2).y
        tryVerify(() => Math.abs(tc.card().mapToItem(null, 0, tc.card().height / 2).y - rowY) < 4, 2000, "centred on f12")
        shell.left.selection.set(0)
        const top = findChild(shell, "pane-left").mapToItem(null, 0, 0).y
        tryVerify(() => Math.abs(findChild(shell, "info-popover-pointer").mapToItem(null, 7, 7).y - view.rowItem(0).mapToItem(null, 0, view.rowItem(0).height / 2).y) < 4, 2000, "pointer on f00")
        verify(card().mapToItem(null, 0, 0).y >= top, "held under the pane's top")
        verify(card().mapToItem(null, 0, card().height / 2).y > view.rowItem(0).mapToItem(null, 0, 0).y + 40, "not centred: the top edge holds it")
    }
    function test_tab_to_the_other_pane_moves_the_card_there() {
        shell.runAction("inspector")
        tryVerify(() => tc.card().visible, 2000)
        shell.focusPane(shell.right)
        shell.right.selection.set(1)
        tryVerify(() => tc.beside(tc.card(), findChild(shell, "pane-right")), 2000)
        verify(findChild(card(), "inspector-title").children[0].text === "y.txt")
    }
    // Escape goes through the window's own key handler, which a key sent from the test's window
    // never reaches (the shell is a window of its own here); the side_by_side e2e flow presses a
    // real one. Ctrl+I's action and a click outside are what this covers.
    function test_ctrl_i_and_a_click_outside_close_it() {
        shell.runAction("inspector")
        tryVerify(() => tc.card().visible, 2000)
        shell.runAction("inspector"); tryVerify(() => !tc.card().visible, 1000, "Ctrl+I closes it")
        shell.runAction("inspector"); tryVerify(() => tc.card().visible, 2000, "up again")
        const host = findChild(shell, "info-popover-host"), left = findChild(shell, "pane-left")
        const at = left.mapToItem(host, left.width / 2, left.height - 20)       // the focused pane's empty bottom, away from the card
        mouseClick(host, at.x, at.y)
        tryVerify(() => !shell.inspectorRequested, 1000, "a click outside closes it")
    }
    function test_a_selection_of_many_puts_a_little_fan_in_the_header() {
        shell.left.selection.setMany([0, 1, 2], 2)
        shell.runAction("inspector")
        tryVerify(() => tc.card().visible, 2000)
        tryVerify(() => findChild(tc.card(), "inspector-mini-fan").visible, 2000)
        compare(findChild(card(), "inspector-title").children[0].text, "3 items")
    }
    function test_one_pane_again_is_the_docked_panel() {
        shell.runAction("inspector")
        tryVerify(() => tc.card().visible, 2000)
        shell.sideBySide = false
        tryVerify(() => !tc.card().visible, 1000)
        verify(shell.inspector, "still asked for")
        tryVerify(() => findChild(shell, "inspector-grip").visible, 2000, "the panel, with its grip")
    }
}
