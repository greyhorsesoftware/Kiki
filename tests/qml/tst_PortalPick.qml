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

    // The box fits the window it is in. It was 860 × 560 whatever the window, and in one shorter
    // than that (the owner's, 625 px with the title bar) its buttons were below the edge.
    function buttons() {
        const out = []
        function walk(it) { for (const c of it.children) { if (c.text !== undefined && c.clicked !== undefined && c.primary !== undefined) out.push(c); walk(c) } }
        walk(portal); return out.filter(b => b.visible)
    }
    function test_it_fits_a_short_window_data() { return [{ tag: "roomy", w: 1200, h: 760 }, { tag: "the owner's", w: 1176, h: 590 }, { tag: "small", w: 700, h: 420 }] }
    function test_it_fits_a_short_window(data) {
        portal.anchors.fill = undefined; portal.width = data.w; portal.height = data.h
        portal.pick({ mode: "save", title: "Save the mirror report", currentFolder: "/home/t", currentName: "kiki-mirror-ghs.txt" }, () => {})
        wait(20)
        const labels = buttons().map(b => b.text)
        verify(labels.indexOf("Save") >= 0 && labels.indexOf("Cancel") >= 0, labels.join(","))
        for (const b of buttons()) {
            const p = b.mapToItem(portal, 0, 0)
            verify(p.y >= 0 && p.y + b.height <= portal.height, b.text + " at y " + p.y + "–" + (p.y + b.height) + " in " + portal.height)
            verify(p.x >= 0 && p.x + b.width <= portal.width, b.text + " at x " + p.x + "–" + (p.x + b.width) + " in " + portal.width)
        }
        portal.finish(null)
        portal.anchors.fill = tc
    }
}
