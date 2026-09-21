import QtQuick
import QtTest
import "../../qml/kiki/ui" as UI

// The path as a field to type in. It went away on Escape, on Enter and when it lost the keyboard
// focus — but a click on the files takes the focus from nothing, so a click outside left it up.
// A press anywhere else is Escape now, and the press still lands on what it was aimed at.
TestCase {
    id: tc
    name: "BreadcrumbEdit"
    when: windowShown
    visible: true
    width: 700; height: 200

    UI.Breadcrumb { id: crumb; x: 10; y: 10; width: 300; home: "/home/t"; uri: "file:///home/t/Projects" }
    Rectangle {
        id: files; x: 0; y: 80; width: 700; height: 120; color: "transparent"
        property int clicks: 0
        MouseArea { anchors.fill: parent; onClicked: files.clicks += 1 }
    }
    SignalSpy { id: navigated; target: crumb; signalName: "navigate" }

    function init() { crumb.editing = false; files.clicks = 0; navigated.clear() }

    function test_a_click_outside_is_escape_and_still_a_click() {
        crumb.edit()
        verify(crumb.editing)
        mouseClick(files, 100, 60)
        verify(!crumb.editing, "the field stayed up after a click on the files")
        compare(files.clicks, 1, "and the click reached what it was aimed at")
        compare(navigated.count, 0, "nothing typed is acted on: Escape, not Enter")
    }
    function test_a_click_in_the_field_keeps_it() {
        crumb.edit()
        mouseClick(crumb, 150, crumb.height / 2)
        verify(crumb.editing)
    }
    function test_the_right_button_outside_counts_too() {
        crumb.edit()
        mouseClick(files, 100, 60, Qt.RightButton)
        verify(!crumb.editing)
    }
    function test_with_the_field_down_nothing_is_in_the_way() {
        verify(!findChild(tc, "breadcrumb-outside") || !findChild(tc, "breadcrumb-outside").visible)
        mouseClick(files, 100, 60)
        compare(files.clicks, 1)
    }
}
