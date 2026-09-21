import QtQuick
import QtTest
import "../../qml/kiki" as Kiki
import "../../qml/kiki/ui" as UI
import KikiTest

// A pane on a server says it is connecting. Between asking for a folder and the first of it
// arriving there was nothing to see — it looked like an empty folder, or like a click that had
// not taken — and a sign-in over a slow line is exactly when somebody wonders.
TestCase {
    id: tc
    name: "PaneConnecting"
    when: windowShown
    visible: true
    width: 600; height: 400

    Kiki.Pane { id: pane }
    UI.PaneConnecting { id: panel; anchors.fill: parent; pane: pane; delay: 40 }

    function init() { Wire.reset(); pane.open("file:///home/t"); answer(0) }
    function answer(n) {
        Wire.reply(Wire.last("Open").id, { cached: false })
        Wire.reply(Wire.last("Window").id, { first: 0, n: n, done: true, rows: [] })
    }

    function test_a_server_that_takes_its_time_says_it_is_connecting() {
        pane.open("sftp://homelab/srv/site")
        verify(!panel.visible, "not at once: a server that answers quickly flashes nothing")
        tryVerify(() => panel.visible, 1000, "but it does once the wait has gone on")
        compare(findChild(panel, "pane-connecting-text").text, "Connecting to homelab…")
        verify(findChild(panel, "pane-connecting-spinner") !== null)
        answer(0)
        verify(!panel.visible, "and it goes when the folder arrives, empty or not")
    }

    function test_a_quick_answer_shows_nothing_at_all() {
        pane.open("sftp://homelab/srv/site")
        answer(3)
        wait(120)
        verify(!panel.visible)
    }

    function test_a_failure_hands_over_to_the_error() {
        pane.open("sftp://homelab/srv/site")
        tryVerify(() => panel.visible, 1000)
        const o = Wire.last("Open"), w = Wire.last("Window")
        Wire.fail(o.id, "Io", "connect failed: timed out")
        Wire.fail(w.id, "NotFound", "no listing " + o.lid)
        verify(!panel.visible, "what went wrong is said instead, by PaneError")
    }

    function test_a_folder_on_this_machine_never_says_it() {
        pane.open("file:///home/t/slow")
        wait(120)
        verify(!panel.visible, "connecting is a word for servers")
    }
}
