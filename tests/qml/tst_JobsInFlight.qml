import QtQuick
import QtTest
import KikiTest
import "../../qml/kiki" as Kiki

// A file a transfer is writing right now is known by its URI, so a view can show it as not yet
// whole. The job says which item it is on and where it is writing; the URI is made the way a
// pane makes one for its rows, so the two match.
TestCase {
    name: "JobsInFlight"

    function init() { Kiki.Jobs.list = [] }

    function test_the_file_being_written_is_in_flight() {
        Kiki.Jobs.list = [
            { id: 1, state: "running", op: "copy", dest: "file:///home/t/Downloads", current: { name: "a b.txt", bytes: 10, size: 100 } },
            { id: 2, state: "done",    op: "copy", dest: "file:///home/t/Downloads", current: { name: "done.txt", bytes: 5, size: 5 } },
            { id: 3, state: "running", op: "copy", dest: "sftp://lab/up/", current: { name: "photo.jpg" } }
        ]
        verify(Kiki.Jobs.isInFlight("file:///home/t/Downloads/a%20b.txt"), "the one being written, spaces encoded as a pane would")
        verify(!Kiki.Jobs.isInFlight("file:///home/t/Downloads/done.txt"), "a finished job's file is whole")
        verify(Kiki.Jobs.isInFlight("sftp://lab/up/photo.jpg"), "an upload too, into the remote folder")
        verify(!Kiki.Jobs.isInFlight("file:///home/t/Downloads/other.txt"))
    }

    function test_a_job_with_nothing_current_marks_nothing() {
        Kiki.Jobs.list = [{ id: 4, state: "running", op: "copy", dest: "file:///home/t", current: null }, { id: 5, state: "queued", op: "copy", dest: "file:///home/t" }]
        compare(Object.keys(Kiki.Jobs.inFlight).length, 0)
    }
}
