import QtQuick
import QtTest
import "../../qml/kiki" as Kiki
import KikiTest

// The recording socket (plan 28). Anything the shell singletons send reaches Wire, and Wire can
// answer it or push an event back — this is how a test asserts the request a file operation made
// when the operation goes through Jobs rather than through an injected daemon.
TestCase {
    id: tc
    name: "Wire"
    when: windowShown
    visible: true

    function init() { Wire.reset() }

    function test_a_submitted_operation_is_recorded_with_its_op() {
        Kiki.Jobs.submit({ op: "rename", uri: "file:///home/t/a.txt", name: "b.txt" })
        const req = Wire.last("Submit")
        verify(req !== null)
        compare(req.op.op, "rename")
        compare(req.op.uri, "file:///home/t/a.txt")
        compare(req.op.name, "b.txt")
        compare(Wire.count("Submit"), 1)
    }

    function test_a_reply_reaches_the_callback_that_asked() {
        let got = null
        Kiki.Jobs.submit({ op: "trash", uris: ["file:///home/t/a.txt"] }, (ok, err) => got = ok)
        const req = Wire.last("Submit")
        Wire.reply(req.id, { job: 7 })
        compare(got.job, 7)
    }

    function test_an_error_reply_reaches_the_callback() {
        let err = null
        Kiki.Jobs.submit({ op: "trash", uris: [] }, (ok, e) => err = e)
        Wire.fail(Wire.last("Submit").id, "Denied", "no")
        compare(err.code, "Denied")
    }

    // Events carry no id: they are dispatched by the singleton's event signal.
    function test_a_toast_event_reaches_jobs() {
        Kiki.Jobs.dismissToast()
        Wire.emitEvent({ event: "Toast", job: 3, text: "Moved 2 items", undoable: true })
        verify(Kiki.Jobs.toast !== null)
        compare(Kiki.Jobs.toast.text, "Moved 2 items")
        compare(Kiki.Jobs.toast.undoable, true)
        Kiki.Jobs.dismissToast()
    }

    function test_undo_and_redo_go_out_as_their_own_requests() {
        Kiki.Jobs.undo()
        Kiki.Jobs.redo()
        compare(Wire.count("Undo"), 1)
        compare(Wire.count("Redo"), 1)
    }
}
