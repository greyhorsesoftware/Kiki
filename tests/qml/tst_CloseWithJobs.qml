import QtQuick
import QtTest
import "../../qml/kiki" as Kiki
import KikiTest

// "If kiki is not running, stuff should not continue in the background" — so a window closed
// while jobs run asks first, and Yes stops them. The close that can be refused is the real
// window's `closing`; here the event is handed to `closeAsked` as Qt would hand it over. (The
// daemon's half, for a kiki that went without asking, is `kikid/tests/shell_gone.rs`.)
TestCase {
    id: tc
    name: "CloseWithJobs"
    when: windowShown
    visible: true
    width: 200; height: 200

    Kiki.Shell { id: shell; width: 1200; height: 760 }
    property int quits: 0

    function job(id, more) { return Object.assign({ id: id, op: "copy", state: "running", name: "site", count: 1, title: "Copy", hidden: false }, more || {}) }
    function dialog() { return findChild(shell.contentItem, "confirm") }
    function close() { const ev = { accepted: true }; shell.closeAsked(ev); return ev.accepted }

    function init() {
        Wire.reset(); Wire.connectAll()
        quits = 0
        shell._quit = () => { tc.quits += 1 }
        shell._quitting = false
        Kiki.Jobs.list = []
    }
    function cleanup() { if (dialog().visible) dialog().answer(false); Kiki.Jobs.list = [] }

    function test_the_window_is_found() {
        verify(shell._backing, "no window to hear `closing` from")
        compare(typeof shell._backing.closing, "function")
    }
    function test_nothing_running_closes_without_a_word() {
        Kiki.Jobs.list = [job(1, { state: "done" }), job(2, { state: "failed" })]
        verify(close())
        verify(!dialog().visible)
    }
    function test_machinery_alone_is_not_worth_a_question() {
        Kiki.Jobs.list = [job(1, { hidden: true, op: "mirrorScan" })]
        verify(close())
        verify(!dialog().visible)
    }
    function test_a_running_job_holds_the_close_and_asks() {
        Kiki.Jobs.list = [job(7)]
        verify(!close(), "the window closed over a running job")
        verify(dialog().visible)
        compare(dialog().title, "Quit kiki?")
        verify(dialog().message.indexOf("site") >= 0, dialog().message)
        compare(dialog().confirmLabel, "Quit")
    }
    function test_several_are_counted_and_the_queued_count_too() {
        Kiki.Jobs.list = [job(1), job(2, { state: "queued" }), job(3, { state: "done" })]
        verify(!close())
        verify(dialog().message.indexOf("2 jobs") === 0, dialog().message)
    }
    function test_no_keeps_kiki_and_the_jobs() {
        Kiki.Jobs.list = [job(7)]
        close()
        dialog().answer(false)
        compare(quits, 0)
        compare(Wire.count("Cancel"), 0)
        verify(!close(), "asked once, and the next close went through unasked")
    }
    function test_yes_stops_every_job_and_quits() {
        Kiki.Jobs.list = [job(7), job(8, { hidden: true }), job(9, { state: "done" })]
        close()
        dialog().answer(true)
        compare(Wire.requests("Cancel").map(r => r.job).sort(), [7, 8])
        compare(quits, 1)
        verify(close(), "the close that follows the quit was held up again")
    }
}
