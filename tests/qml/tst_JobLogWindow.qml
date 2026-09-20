import QtQuick
import QtTest
import "../../qml/kiki" as Kiki
import "../../qml/kiki/ui" as UI
import KikiTest

// What the Log button opens (plan 32): read from the daemon from where the last read ended, shown
// with kiki's own lines forward and the libraries' behind, filterable, copyable, and following the
// tail until somebody scrolls away.
TestCase {
    id: tc
    name: "JobLogWindow"
    when: windowShown
    visible: true
    width: 1000; height: 700

    UI.JobLogWindow { id: log }
    SignalSpy { id: copied; target: log; signalName: "copyText" }

    readonly property var job: ({ id: 7, op: "copy", state: "failed", name: "site", count: 1, title: "Copy site", cancelling: false })
    function line(t, level, source, text) { return { t: 1000000 + t, level: level, source: source, text: text } }
    function init() { Wire.reset(); Wire.connectAll(); copied.clear(); log.visible = false }

    function test_it_asks_for_the_jobs_log_from_the_start_and_then_from_where_it_stopped() {
        log.openJob(job)
        verify(log.visible)
        compare(log.heading, "site — log")
        let asked = Wire.last("JobLog")
        compare(asked.job, 7); compare(asked.from, 0)
        Wire.reply(asked.id, { lines: [line(0, "info", "kiki", "Copy site — started"), line(120, "debug", "sftp kiki", "write /srv/a.kiki-part (17 bytes)")], next: 2, dropped: 0 })
        compare(log.lines.length, 2)
        log.fetch()
        asked = Wire.last("JobLog")
        compare(asked.from, 2, "reading on, not again")
        Wire.reply(asked.id, { lines: [line(300, "error", "kiki", "failed: a: Denied")], next: 3, dropped: 0 })
        compare(log.lines.length, 3)
        compare(log.stamp(log.lines[1].t), "+0.12s", "times are from the job's first line")
    }

    function test_a_location_has_a_log_too() {
        log.openLocation("homelab")
        compare(log.heading, "homelab — connection log")
        compare(Wire.last("LocationLog").location, "homelab")
        compare(Wire.count("JobLog"), 0)
    }

    function test_filter_copy_and_what_was_dropped() {
        log.openJob(job)
        Wire.replyTo("JobLog", { lines: [line(0, "info", "kiki", "started"), line(50, "debug", "ftps suppaftp::sync_ftp", "Put file /a.kiki-part"), line(90, "warn", "ftps kiki", "Write /a: Denied")], next: 40, dropped: 37 })
        verify(findChild(log, "joblog-dropped").visible)
        compare(findChild(log, "joblog-dropped").text, "… 37 earlier lines dropped")
        compare(log.shownLines.length, 3)
        log.filter = "suppaftp"
        compare(log.shownLines.length, 1, "the source is searched as well as the text")
        log.filter = "DENIED"
        compare(log.shownLines.length, 1, "whatever the case")
        log.filter = ""
        const copy = findChild(log, "joblog-copy")
        mouseClick(copy, copy.width / 2, copy.height / 2)
        compare(copied.count, 1)
        const text = copied.signalArguments[0][0]
        verify(text.startsWith("… 37 earlier lines dropped\n"), text)
        verify(text.indexOf("+0.05s  debug  ftps suppaftp::sync_ftp  Put file /a.kiki-part") >= 0, text)
    }

    function test_escape_clears_the_filter_first_and_then_closes() {
        log.openJob(job)
        const f = findChild(log, "joblog-filter")
        verify(f.activeFocus)
        keyClick(Qt.Key_X)
        compare(log.filter, "x")
        keyClick(Qt.Key_Escape)
        compare(log.filter, ""); verify(log.visible)
        keyClick(Qt.Key_Escape)
        verify(!log.visible)
    }

    function test_nothing_is_asked_once_it_is_closed() {
        log.openJob(job)
        log.close()
        Wire.reset()
        Kiki.Jobs.changed()
        compare(Wire.count("JobLog"), 0)
    }
}
