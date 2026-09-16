import QtQuick
import QtTest
import "../../qml/kiki" as Kiki

TestCase {
    name: "Selection"
    Kiki.Selection { id: sel }
    function test_set_toggle_range() {
        sel.set(3); compare(sel.count(), 1); verify(sel.has(3))
        sel.toggle(5); compare(sel.count(), 2)
        sel.toggle(3); compare(sel.count(), 1); verify(!sel.has(3))
        sel.set(2); sel.range(6); compare(sel.positions(), [2, 3, 4, 5, 6])
        sel.clear(); compare(sel.count(), 0); compare(sel.current, -1)
    }
}
