import QtQuick
import QtTest
import "../../qml/kiki" as Kiki

TestCase {
    name: "Selection"
    when: windowShown
    visible: true
    Kiki.Selection { id: sel }
    function test_set_toggle_range() {
        sel.set(3); compare(sel.count(), 1); verify(sel.has(3))
        sel.toggle(5); compare(sel.count(), 2)
        sel.toggle(3); compare(sel.count(), 1); verify(!sel.has(3))
        sel.set(2); sel.range(6); compare(sel.positions(), [2, 3, 4, 5, 6])
        sel.clear(); compare(sel.count(), 0); compare(sel.current, -1)
    }

    // A file arriving or leaving above the selection (WindowCache.spliced) moves positions, and
    // what was selected has to stay selected — by being moved, not by being looked up again.
    function test_splice_moves_the_selection_with_its_rows() {
        sel.set(5); sel.toggle(8); sel.toggle(2)
        let told = 0
        const on = () => told++
        sel.changed.connect(on)
        sel.splice([{ op: "remove", pos: 3 }, { op: "insert", pos: 0 }, { op: "insert", pos: 100 }])
        sel.changed.disconnect(on)
        compare(sel.positions(), [3, 5, 8])      // 2→2→3, 5→4→5, 8→7→8
        compare(sel.anchor, 5)
        compare(sel.current, 3)
        compare(told, 1)
    }

    function test_splice_drops_a_selected_row_that_was_removed() {
        sel.set(4); sel.toggle(6)
        sel.splice([{ op: "remove", pos: 6 }])
        compare(sel.positions(), [4])
        compare(sel.current, -1)
        compare(sel.anchor, 4)
    }
}
