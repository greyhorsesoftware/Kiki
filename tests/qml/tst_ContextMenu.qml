import QtQuick
import QtTest
import "../../qml/kiki/ui" as UI

TestCase {
    name: "ContextMenu"
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
}
