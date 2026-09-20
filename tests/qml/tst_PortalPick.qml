import QtQuick
import QtTest
import "../../qml/kiki" as Kiki
import "../../qml/kiki/ui" as UI
import KikiTest

// The chooser kiki gives other apps through the portal, asked by kiki itself: the answer comes
// back to the caller and nothing is sent to the daemon as a portal result.
TestCase {
    id: tc
    name: "PortalPick"
    when: windowShown
    visible: true
    width: 900; height: 600

    property var fake: null
    Component { id: fakeC; FakeDaemon {} }
    UI.PortalDialog { id: portal; anchors.fill: parent; home: "/home/t" }

    function init() {
        Wire.reset(); Wire.connectAll()
        fake = fakeC.createObject(tc)
        fake.tree = { "file:///home/t": [fake.dir("Sites")], "file:///home/t/Sites": [] }
        portal.pane.listing.daemon = fake
    }
    function cleanup() { portal.visible = false; portal._local = null; fake.destroy() }

    function test_choosing_a_folder_answers_the_caller_and_not_the_daemon() {
        let got = "unset"
        portal.pick({ mode: "open", directory: true, title: "Choose the local folder", currentFolder: "/home/t/Sites" }, uris => got = uris)
        verify(portal.visible)
        compare(portal.pane.uri, "file:///home/t/Sites")
        Wire.reset()
        portal.accept()
        verify(!portal.visible)
        compare(got.length, 1)
        compare(got[0], "file:///home/t/Sites")
        compare(Wire.count("ChooserResult"), 0)
    }

    function test_cancelling_answers_null() {
        let got = "unset"
        portal.pick({ mode: "open", directory: true, currentFolder: "/home/t" }, uris => got = uris)
        portal.finish(null)
        compare(got, null)
        compare(Wire.count("ChooserResult"), 0)
    }

    // A portal request after a local pick still goes to the daemon: the callback is one-shot.
    function test_the_portal_path_is_untouched() {
        portal.pick({ mode: "open", directory: true, currentFolder: "/home/t" }, () => {})
        portal.finish(null)
        Wire.reset()
        portal.open({ mode: "open", directory: true, currentFolder: "/home/t", token: "tok-1" })
        portal.accept()
        compare(Wire.count("ChooserResult"), 1)
        compare(Wire.last("ChooserResult").token, "tok-1")
    }

    // A location can only be answered with a URI the asking application cannot open.
    function test_the_chooser_offers_no_remote_locations() {
        const section = findChild(tc, "chooser-locations")
        verify(section !== null)
        verify(!section.visible)
    }
}
