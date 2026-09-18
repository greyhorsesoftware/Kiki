import QtQuick
import QtTest
import "../../qml/kiki/ui" as UI

TestCase {
    name: "ViewSwitcher"
    when: windowShown
    visible: true
    width: 200; height: 60
    UI.ViewSwitcher { id: sw; view: "list" }
    SignalSpy { id: menuSpy; target: sw; signalName: "menu" }

    function test_click_opens_menu() {
        menuSpy.clear()
        mouseClick(sw, sw.width / 2, sw.height / 2)
        compare(menuSpy.count, 1)
    }
    function test_view_property_round_trips() {
        for (const v of ["icon", "list", "columns", "mirror"]) { sw.view = v; compare(sw.view, v) }
    }
}
