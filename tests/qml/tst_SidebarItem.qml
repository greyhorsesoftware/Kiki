import QtQuick
import QtTest
import "../../qml/kiki/ui" as UI

TestCase {
    name: "SidebarItem"
    when: windowShown
    visible: true
    width: 224; height: 60
    Item { id: host; width: 224; height: 60
        UI.SidebarItem { id: item; label: "Downloads"; icon: "download" } }
    SignalSpy { id: clicks; target: item; signalName: "clicked" }
    SignalSpy { id: rights; target: item; signalName: "rightClicked" }

    function test_left_and_right_click() {
        clicks.clear(); rights.clear()
        mouseClick(item, 20, 15, Qt.LeftButton)
        mouseClick(item, 20, 15, Qt.RightButton)
        compare(clicks.count, 1)
        compare(rights.count, 1)
    }
    function test_keyed_highlight_draws_border() {
        item.keyed = false; compare(item.border.width, 0)
        item.keyed = true; compare(item.border.width, 1)
        item.keyed = false
    }
    function test_height_matches_plan_02() { compare(item.height, 30) }
    function test_a_dot_shows_only_when_given_a_colour() {
        const dot = findChild(item, "sidebar-dot")
        verify(!dot.visible)
        item.dot = "green"
        verify(dot.visible)
        item.dot = "transparent"
        verify(!dot.visible)
    }
}
