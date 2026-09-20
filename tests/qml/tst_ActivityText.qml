import QtQuick
import QtTest
import "../../qml/kiki" as Kiki
import KikiTest

// What the activity view says about a job (plan 32): which jobs it shows at all, and the words
// for each stage of each kind. Pure functions over the job the daemon sends, so they are tested
// here without a pixel drawn.
TestCase {
    name: "ActivityText"

    function job(o) {
        return Object.assign({ id: 1, op: "copy", state: "running", done: 0, total: 0, bytes: 0, bytesTotal: 0, title: "Copy x", error: null,
                               name: "site", count: 1, isDir: true, direction: "upload", hidden: false, phase: "running", cancelling: false,
                               current: null, rate: 0, result: null, revealUri: null }, o)
    }
    function init() { Wire.reset(); Kiki.Jobs.list = []; Kiki.Jobs.seenFailure = 0 }

    function test_sizes_get_finer_as_they_get_bigger() {
        compare(Kiki.Format.transferSize(0), "0 B")
        compare(Kiki.Format.transferSize(1023), "1023 B")
        compare(Kiki.Format.transferSize(12 * 1024 + 600), "13 KB")
        compare(Kiki.Format.transferSize(42.7 * 1024 * 1024), "42.7 MB")
        compare(Kiki.Format.transferSize(1.1 * 1024 * 1024 * 1024), "1.10 GB")
    }

    function test_an_error_loses_the_names_of_what_passed_it_along() {
        compare(Kiki.Format.cleanError("some.package.SomeError: other.package.OtherError: Network is unreachable"), "Network is unreachable")
        compare(Kiki.Format.cleanError("Io: russh::Error: Connection reset by peer"), "Connection reset by peer")
        compare(Kiki.Format.cleanError("site/img/c.bin: Denied"), "site/img/c.bin: permission denied", "a file name is not a type name; a bare code becomes words")
        compare(Kiki.Format.cleanError("NotFound"), "not found")
        compare(Kiki.Format.cleanError("the server said: Denied by policy"), "the server said: Denied by policy", "only a code standing alone at the end")
        compare(Kiki.Format.cleanError("Remote host: no route"), "Remote host: no route")
        compare(Kiki.Format.cleanError(""), "Failed")
        compare(Kiki.Format.cleanError(null), "Failed")
    }

    function test_status_lines() {
        const J = Kiki.Jobs
        compare(J.statusLine(job({ state: "queued" })), "Waiting…")
        compare(J.statusLine(job({ phase: "preparing" })), "Preparing to transfer…")
        compare(J.barMode(job({ phase: "preparing" })), "busy")
        // One file: bytes and a rate, and a bar of bytes.
        const one = job({ total: 1, bytes: 1.2 * 1024 * 1024, bytesTotal: 42.7 * 1024 * 1024, rate: 350 * 1024 })
        compare(J.statusLine(one), "1.2 MB of 42.7 MB (350 KB/sec)")
        verify(Math.abs(J.fraction(one) - 1.2 / 42.7) < 0.001)
        // Many: files, and a bar of files — a bar of bytes stalls on the big one.
        const many = job({ total: 458, done: 12, bytes: 5, bytesTotal: 1000 })
        compare(J.statusLine(many), "12 of 458 transferred — 2% complete")
        verify(Math.abs(J.fraction(many) - 12 / 458) < 0.001)
        compare(J.statusLine(job({ op: "mirrorRun", total: 900, done: 120, bytes: 40.2 * 1024 * 1024, bytesTotal: 1.1 * 1024 * 1024 * 1024, rate: 2.1 * 1024 * 1024 })),
                "120 of 900 items · 40.2 MB of 1.10 GB · 2.1 MB/sec")
        compare(J.statusLine(job({ op: "delete", total: 458, done: 12 })), "Processing 12 of 458 items")
        // Cancelling: the header says it, the bar goes busy, the line clears.
        const c = job({ cancelling: true, total: 10, done: 3 })
        compare(J.headline(c), "Cancelling…")
        compare(J.barMode(c), "busy")
        compare(J.statusLine(c), "")
    }

    function test_the_file_in_hand_is_detail_only_when_there_are_several() {
        const J = Kiki.Jobs
        const cur = { name: "site/img/logo.bin", bytes: 12 * 1024, size: 254 * 1024 }
        verify(!J.hasDetail(job({ total: 1, current: cur })), "one file is its own detail")
        const many = job({ total: 9, done: 2, current: cur, rate: 45 * 1024 })
        verify(J.hasDetail(many))
        compare(J.detailLine(many), "12 KB of 254 KB (45 KB/sec)")
        verify(!J.hasDetail(job({ total: 9, current: cur, phase: "preparing" })), "nothing to open while it is still counting")
        // A mirror with several workers names the file but has no bar for it (size 0).
        const m = job({ op: "mirrorRun", total: 900, current: { name: "delete old/a.txt", bytes: 0, size: 0 } })
        verify(J.hasDetail(m))
        compare(J.detailLine(m), "")
    }

    function test_completion_lines() {
        const J = Kiki.Jobs
        compare(J.completion(job({ state: "done", direction: "download", done: 458 })), "Downloaded 458 items")
        compare(J.completion(job({ state: "done", direction: "upload", done: 1 })), "Uploaded 1 item")
        compare(J.completion(job({ state: "done", direction: "local", op: "move", done: 3 })), "Moved 3 items")
        compare(J.completion(job({ state: "done", op: "chmod", done: 3 })), "Changed permissions on 3 items")
        compare(J.completion(job({ state: "done", op: "mirrorRun", result: { copies: 412, deletes: 9, skipped: 3 } })), "412 copied, 9 deleted, 3 skipped")
        compare(J.completion(job({ state: "done", op: "mirrorRun", result: { copies: 2, deletes: 0, skipped: 0 } })), "2 copied, 0 deleted")
        compare(J.completion(job({ state: "cancelled" })), "Cancelled")
        compare(J.completion(job({ state: "cancelled", op: "mirrorRun" })), "Mirror cancelled")
        compare(J.completion(job({ state: "failed", error: "Io: site/a.txt: Denied" })), "site/a.txt: permission denied")
        compare(J.headline(job({ name: "a.txt", count: 3 })), "a.txt and 2 more")
    }

    function test_what_is_shown_and_what_the_orb_says() {
        const J = Kiki.Jobs
        J.list = [job({ id: 1, state: "done" }), job({ id: 2, hidden: true }), job({ id: 3, op: "delete", state: "done" }),
                  job({ id: 4, op: "delete", state: "failed", error: "Denied" }), job({ id: 5, op: "trash", state: "running" })]
        compare(J.shown().map(j => j.id), [1, 4, 5], "no machinery; a delete that went well is not history, one that did not is; in the order asked for")
        compare(J.orbState(), "failed", "a failure outranks what is running")
        compare(J.orbTip(), "1 failed · 1 running")
        J.markSeen()
        compare(J.orbState(), "running", "until it has been looked at")
        J.list = [job({ id: 1, state: "done" }), job({ id: 2, hidden: true })]
        compare(J.orbState(), "idle", "machinery does not light the orb")
        compare(J.orbTip(), "No activity")
        // A failure after the last look is news again.
        J.list = J.list.concat([job({ id: 9, state: "failed", error: "x" })])
        compare(J.orbState(), "failed")
    }

    function test_a_long_job_is_not_pushed_out_by_the_ones_after_it() {
        const J = Kiki.Jobs
        J._upsert(job({ id: 1, state: "running" }))
        for (let i = 2; i < 80; i++) J._upsert(job({ id: i, state: "done" }))
        verify(J.list.some(j => j.id === 1), "still running, still listed")
        compare(J.list.filter(j => j.state === "done").length, 50)
    }

    function test_clearing_is_the_daemons_and_its_answer_is_what_removes() {
        const J = Kiki.Jobs
        J.list = [job({ id: 1, state: "done" }), job({ id: 2, state: "running" })]
        J.clear()
        compare(Wire.count("ClearJobs"), 1)
        compare(J.list.length, 2, "nothing goes until the daemon says so")
        Wire.emitEvent({ event: "JobsCleared", jobs: [1] })
        compare(J.list.map(j => j.id), [2])
        J.dismiss(2)
        compare(Wire.last("DismissJob").job, 2)
    }
}
