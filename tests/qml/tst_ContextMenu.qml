import QtQuick
import QtTest
import "../../qml/kiki/ui" as UI

TestCase {
    name: "ContextMenu"
    when: windowShown
    visible: true
    width: 600; height: 400
    property int fired: 0
    UI.ContextMenu { id: menu }

    function test_open_close_and_action() {
        fired = 0
        menu.open([{ label: "One", action: () => fired++ }, { label: "Two", key: "Ctrl+T", checked: true, action: () => fired += 10 }], Qt.point(10, 10))
        verify(menu.visible)
        compare(menu.items.length, 2)
        menu.close()
        verify(!menu.visible)
    }
    function test_clamps_to_parent() {
        menu.open([{ label: "A", action: () => {} }], Qt.point(590, 390))
        verify(menu.box.x + menu.box.width <= 600)
        verify(menu.box.y + menu.box.height <= 400)
        menu.close()
    }
    function test_escape_closes() {
        menu.open([{ label: "A", action: () => {} }], Qt.point(0, 0))
        keyClick(Qt.Key_Escape)
        verify(!menu.visible)
    }

    // The hover highlight is a pill inside the row — 4 px short of the box on either side, so it
    // never runs under the border — that fades in under the pointer (owner, 2026-09-23).
    function test_the_highlight_is_a_pill_inside_the_row() {
        menu.open([{ label: "One", action: () => {} }, { label: "Two", sep: true, action: () => {} }], Qt.point(10, 10))
        const row = findChild(menu, "menu-One")
        const pill = findChild(row, "menu-highlight")
        verify(pill, "no pill under the row")
        compare(pill.x, 4)
        compare(pill.width, row.width - 8)
        compare(pill.opacity, 0)
        mouseMove(row, row.width / 2, 13)
        tryCompare(pill, "opacity", 1)
        mouseMove(menu, 300, 300)
        tryCompare(pill, "opacity", 0)
        menu.close()
    }
}
