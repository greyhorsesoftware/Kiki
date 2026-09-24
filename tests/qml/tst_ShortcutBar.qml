import QtQuick
import QtTest
import "../../qml/kiki/ui" as UI

// The shortcut bar's chips slide under the wheel when there are more than fit, as the path
// does (owner, 2026-09-22: "needs to support scrolling too vs being cut off"). At rest the first
// chips show; the wheel pulls the rest in; nothing is heard when they all fit.
TestCase {
    id: tc
    name: "ShortcutBar"
    when: windowShown
    visible: true
    width: 700; height: 60

    UI.ShortcutBar { id: bar; x: 0; y: 10; width: 360; status: "5 of 8" }
    readonly property var many: [{ key: "Enter", label: "open" }, { key: "←", label: "up" }, { key: "→", label: "into" }, { key: "^I", label: "info" }, { key: "F2", label: "rename" },
                                 { key: "Del", label: "trash" }, { key: "⌘C", label: "copy" }, { key: "⌘V", label: "paste" }, { key: "/", label: "filter" }, { key: "^?", label: "keys" }]
    // A fresh array each time: the same one again is no change to `keys`, and no reset.
    function init() { bar.width = 360; bar.keys = many.slice(); tryVerify(() => bar.overflow > 0); compare(bar.scroll, 0) }

    // A message takes the keys' place (owner, 2026-09-24): the chips roll up out of the strip,
    // the message rolls in from below with its Undo, and when it goes they change back.
    function test_a_message_rolls_the_chips_up_and_takes_their_place() {
        const chips = findChild(bar, "shortcut-row"), msg = findChild(bar, "bar-toast")
        const rest = chips.y
        verify(!msg.visible, "no message: the chips")
        bar.toast = { text: "Moved 3 items to Trash", undoable: true }
        tryCompare(bar, "roll", 1, 1000)
        compare(chips.y, rest - bar.height, "the chips rolled up out of the strip")
        verify(msg.visible && msg.y >= 0 && msg.y + msg.height <= bar.height, "the message in the strip: " + msg.y)
        verify(findChild(bar, "toast-undo").visible, "with its Undo")
        compare(findChild(msg, "toast-undo").visible, true)
        bar.toast = null
        tryCompare(bar, "roll", 0, 1000)
        compare(chips.y, rest, "and back in place")
        verify(!msg.visible)
    }
    function test_undo_and_the_cross_are_the_bars_word() {
        let undone = 0, dismissed = 0
        bar.undo.connect(() => undone++); bar.dismiss.connect(() => dismissed++)
        bar.toast = { text: "Deleted a.txt", undoable: true }
        tryCompare(bar, "roll", 1, 1000)
        mouseClick(findChild(bar, "toast-undo"))
        compare(undone, 1)
        mouseClick(findChild(bar, "toast-close"))
        compare(dismissed, 1)
        bar.toast = { text: "Nothing to undo", undoable: false }
        tryCompare(bar, "roll", 1, 1000)
        verify(!findChild(bar, "toast-undo").visible, "an error has no Undo")
        bar.toast = null; tryCompare(bar, "roll", 0, 1000)
    }
    function test_at_rest_the_first_chips_show_and_the_last_is_past_the_edge() {
        const row = findChild(bar, "shortcut-row"), view = findChild(bar, "shortcut-chips")
        compare(row.x, 14)
        verify(row.x + row.width > view.width, "everything fits — nothing to scroll: " + row.width + " in " + view.width)
    }
    function test_the_wheel_pulls_the_rest_in_and_stops_at_the_end() {
        const view = findChild(bar, "shortcut-chips"), row = findChild(bar, "shortcut-row")
        mouseWheel(view, 100, view.height / 2, 0, -120)
        verify(bar.scroll > 0, "a notch did not move the chips")
        for (let i = 0; i < 40; i++) mouseWheel(view, 100, view.height / 2, 0, -120)
        compare(bar.scroll, bar.overflow, "past the end")
        verify(row.x + row.width <= view.width - 12 + 0.5, "the last chip is in view: ends at " + (row.x + row.width) + " of " + view.width)
        for (let i = 0; i < 40; i++) mouseWheel(view, 100, view.height / 2, 0, 120)
        compare(bar.scroll, 0, "and back to the start, no further")
    }
    function test_a_sideways_wheel_works_too() {
        const view = findChild(bar, "shortcut-chips")
        mouseWheel(view, 100, view.height / 2, -120, 0)
        verify(bar.scroll > 0)
    }
    function test_new_keys_start_at_the_beginning() {
        bar.scrollBy(1000)
        verify(bar.scroll > 0)
        bar.keys = many.slice(0, 9)
        compare(bar.scroll, 0)
    }
    function test_when_everything_fits_the_wheel_is_not_heard() {
        bar.width = 1400
        tryCompare(bar, "overflow", 0)
        const view = findChild(bar, "shortcut-chips")
        mouseWheel(view, 100, view.height / 2, 0, -120)
        compare(bar.scroll, 0)
    }
}
