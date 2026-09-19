import QtQuick
import QtTest
import "../../qml/kiki/ui" as UI

// A path too long for its field: the folder you are in stays in view, and the wheel — or two
// fingers on a touchpad — slides the path back and forth to reach the rest of it.
TestCase {
    id: tc
    name: "BreadcrumbScroll"
    when: windowShown
    visible: true
    width: 700; height: 100

    readonly property string deep: "file:///home/t/one/two/three/four/five/six/seven/eight/nine/ten/eleven/twelve"
    UI.Breadcrumb { id: crumb; x: 10; y: 10; width: 300; home: "/home/t" }
    SignalSpy { id: navigated; target: crumb; signalName: "navigate" }

    // The crumbs are laid out after the URI is set, and the wheel is only heard once there is
    // something to scroll: wait for that rather than for a number of milliseconds.
    function init() { crumb.width = 300; crumb.uri = ""; crumb.uri = deep; tryVerify(() => crumb.overflow > 0); compare(crumb.scroll, 0); navigated.clear() }

    function row() { return findChild(crumb, "crumb-row") }
    function view() { return findChild(crumb, "crumb-view") }

    function test_at_rest_a_long_path_shows_its_end() {
        verify(crumb.overflow > 0)
        compare(crumb.scroll, 0)
        const r = row()
        verify(r.x < 0)                                              // the start is off to the left
        verify(Math.abs((r.x + r.width) - crumb.crumbSpace) <= 1)    // and the end is against the edge
    }

    function test_the_wheel_slides_it_back_to_its_start_and_no_further() {
        const r = row()
        mouseWheel(crumb, 150, 15, 0, 120)                           // one notch
        verify(crumb.scroll > 0)
        for (let i = 0; i < 60; i++) mouseWheel(crumb, 150, 15, 0, 120)
        compare(crumb.scroll, crumb.overflow)
        fuzzyCompare(r.x, 10, 0.01)                                             // first crumb where a short path puts it
        for (let i = 0; i < 80; i++) mouseWheel(crumb, 150, 15, 0, -120)
        compare(crumb.scroll, 0)                                     // and back to rest, not past it
    }

    function test_a_sideways_swipe_does_the_same() {
        mouseWheel(crumb, 150, 15, 240, 0)
        verify(crumb.scroll > 0)
    }

    function test_a_crumb_scrolled_into_view_can_be_clicked() {
        crumb.scrollBy(crumb.overflow)
        const r = row()
        const first = r.children[0]                                  // the home crumb
        const at = first.mapToItem(crumb, first.width / 2, first.height / 2)
        verify(at.x > 0 && at.x < crumb.width)
        mouseClick(crumb, at.x, at.y)
        compare(navigated.count, 1)
        compare(navigated.signalArguments[0][0], "file:///home/t")
    }

    function test_a_new_folder_starts_at_its_own_end() {
        crumb.scrollBy(crumb.overflow)
        crumb.uri = deep + "/thirteen"
        compare(crumb.scroll, 0)
    }

    function test_a_short_path_does_not_scroll() {
        crumb.uri = "file:///home/t/one"
        tryCompare(crumb, "overflow", 0)
        mouseWheel(crumb, 150, 15, 0, 120)
        compare(crumb.scroll, 0)
        compare(row().x, 10)
    }

    // Widening the field takes away the reason to be scrolled.
    function test_growing_the_field_clamps_the_scroll() {
        crumb.scrollBy(crumb.overflow)
        crumb.width = 2000
        tryCompare(crumb, "overflow", 0)
        compare(crumb.scroll, 0)
    }

    function test_the_crumbs_are_clipped_to_their_room() {
        verify(view().clip)
        verify(view().width <= crumb.width)
    }
}
