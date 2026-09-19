import QtQuick
import QtTest
import "../../qml/kiki" as Kiki
import "../../qml/kiki/ui" as UI
import KikiTest

// A folder that could not be listed says so. It used to look like an empty folder: no view read
// the listing's error, and the reason was overwritten by the "no listing" that the Window request
// sent along with the failed Open comes back with.
TestCase {
    id: tc
    name: "PaneError"
    when: windowShown
    visible: true
    width: 600; height: 400

    Kiki.Pane { id: pane }
    UI.PaneError { id: panel; anchors.fill: parent; pane: pane }
    SignalSpy { id: retried; target: panel; signalName: "retry" }

    function init() { Wire.reset(); retried.clear() }

    function failOpen(message) {
        pane.listing.open("sftp://nas/")
        const o = Wire.last("Open"), w = Wire.last("Window")
        verify(o && w)
        Wire.fail(o.id, "Io", message)
        Wire.fail(w.id, "NotFound", "no listing " + o.lid)
    }
    function test_the_reason_survives_and_is_shown() {
        failOpen("connect failed: failed to lookup address information")
        compare(pane.listing.error, "connect failed: failed to lookup address information")
        verify(pane.listing.done, "nothing more is coming")
        verify(panel.visible)
        compare(findChild(panel, "pane-error-message").text, pane.listing.error)
    }
    function test_try_again_asks_and_a_listing_that_opens_clears_it() {
        failOpen("connect failed: timed out")
        mouseClick(findChild(panel, "pane-error-retry"))
        compare(retried.count, 1)
        pane.listing.open("sftp://nas/")
        Wire.reply(Wire.last("Open").id, { cached: false })
        Wire.reply(Wire.last("Window").id, { first: 0, n: 0, done: true, rows: [] })
        compare(pane.listing.error, "")
        verify(!panel.visible, "an empty folder is just empty")
    }
}
