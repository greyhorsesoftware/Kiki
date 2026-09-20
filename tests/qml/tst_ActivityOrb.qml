import QtQuick
import QtTest
import "../../qml/kiki" as Kiki
import "../../qml/kiki/ui" as UI
import KikiTest

// The orb and the popup it opens (plan 32, from plan 29 I). The words are tst_ActivityText's;
// this is the thing on screen: three states, the popup's open and close rules, one row per job.
TestCase {
    id: tc
    name: "ActivityOrb"
    when: windowShown
    visible: true
    width: 700; height: 500

    Item {
        id: stage
        anchors.fill: parent
        UI.ActivityOrb { id: orb; anchors.right: parent.right; anchors.bottom: parent.bottom; anchors.rightMargin: 6; z: 61; open: pop.visible; onClicked: pop.toggle() }
        UI.ActivityPopover { id: pop; aimX: orb.x + orb.width / 2; aimY: orb.y }
    }
    SignalSpy { id: logSpy; target: pop; signalName: "logRequested" }
    SignalSpy { id: revealSpy; target: pop; signalName: "revealRequested" }

    function job(o) {
        return Object.assign({ id: 1, op: "copy", state: "running", done: 3, total: 9, bytes: 10, bytesTotal: 100, title: "Copy x", error: null,
                               name: "site", count: 1, isDir: true, direction: "upload", hidden: false, phase: "running", cancelling: false,
                               current: { name: "site/a.bin", bytes: 5, size: 50 }, rate: 2048, result: null, revealUri: null }, o)
    }
    function set(list) { Kiki.Jobs.list = list; Kiki.Jobs.changed() }
    function init() { Wire.reset(); Wire.connectAll(); Kiki.Jobs.seenFailure = 0; set([]); pop.visible = false; pop._closedAt = 0; pop.opened = ({}); logSpy.clear(); revealSpy.clear() }
    // Rows and columns are laid out on the next frame: give them one before reading positions.
    function clickOrb() { mouseClick(orb, orb.width / 2, orb.height / 2); wait(40) }
    function dot() { return findChild(orb, "activity-orb-dot") }

    function test_it_is_always_there_and_says_which_of_three_things_is_so() {
        verify(orb.visible)
        compare(orb.state_, "idle")
        compare(orb.tip, "No activity")
        compare(dot().opacity, 1)
        set([job({})])
        compare(orb.state_, "running")
        compare(orb.tip, "1 running")
        tryVerify(() => dot().opacity < 0.95, 2000, "it breathes")
        set([job({}), job({ id: 2, state: "failed", error: "Denied" })])
        compare(orb.state_, "failed", "failed outranks running")
        set([job({ id: 1, state: "done" })])
        compare(orb.state_, "running" === "x" ? "" : "failed" === "x" ? "" : "idle")
        tryCompare(dot(), "opacity", 1, 1000, "and is not left dim when the last job ends mid-breath")
    }

    function test_a_failure_stays_red_until_the_popup_has_been_opened() {
        set([job({ id: 2, state: "failed", error: "Denied" })])
        compare(orb.state_, "failed")
        clickOrb()
        verify(pop.visible)
        compare(orb.state_, "idle", "seen")
        // A new failure while it is open is seen as it arrives.
        set(Kiki.Jobs.list.concat([job({ id: 3, state: "failed", error: "x" })]))
        compare(orb.state_, "idle")
    }

    function test_a_click_opens_it_and_a_click_closes_it() {
        clickOrb(); verify(pop.visible)
        clickOrb(); verify(!pop.visible)
    }

    function test_a_click_outside_closes_it_and_a_click_on_it_does_not() {
        set([job({})])
        clickOrb()
        const card = findChild(pop, "activity-card")
        mouseClick(card, card.width / 2, 20)
        verify(pop.visible, "a click on the card is not outside")
        mouseClick(stage, 20, 20)
        verify(!pop.visible)
    }

    // The outside-click that closes it and the click on the orb are one gesture when the orb is
    // what was clicked: it must end closed, not closed-and-reopened.
    function test_the_orb_clicked_just_after_a_close_counts_as_that_close() {
        clickOrb(); verify(pop.visible)
        pop.close()
        pop.toggle()
        verify(!pop.visible, "within 250 ms of the close")
        wait(300)
        pop.toggle()
        verify(pop.visible, "after that it is a click like any other")
    }

    function test_every_job_has_a_row_newest_first_and_machinery_has_none() {
        set([job({ id: 1, state: "done", direction: "upload", done: 9 }), job({ id: 2, hidden: true }), job({ id: 3, state: "queued" }), job({ id: 4 })])
        clickOrb()
        verify(findChild(pop, "activity-entry-1") !== null)
        verify(findChild(pop, "activity-entry-2") === null, "an undo's inverse op is not activity")
        verify(findChild(pop, "activity-entry-3") !== null && findChild(pop, "activity-entry-4") !== null)
        verify(findChild(pop, "activity-entry-4").y < findChild(pop, "activity-entry-1").y, "newest at the top")
        verify(!findChild(pop, "activity-empty").visible)
        compare(findChild(findChild(pop, "activity-entry-1"), "activity-completion").text, "Uploaded 9 items")
        compare(findChild(findChild(pop, "activity-entry-3"), "activity-status").text, "Waiting…")
    }

    function test_a_running_transfer_opens_to_show_the_file_in_hand() {
        set([job({ id: 4 })])
        clickOrb()
        const e = findChild(pop, "activity-entry-4")
        const detail = findChild(e, "activity-detail"), tri = findChild(e, "activity-disclosure")
        verify(tri.visible && !detail.visible)
        const before = e.height
        mouseClick(tri, tri.width / 2, tri.height / 2)
        verify(detail.visible)
        tryVerify(() => e.height > before, 1000, "the row grows to hold it")
        // Still counting: nothing to open yet.
        set([job({ id: 4, phase: "preparing", total: 0, bytes: 0, bytesTotal: 0, current: null })])
        verify(!findChild(findChild(pop, "activity-entry-4"), "activity-disclosure").visible)
    }

    function test_the_buttons_are_the_ones_that_make_sense() {
        set([job({ id: 1 }), job({ id: 2, state: "done", revealUri: "file:///home/t/Downloads/site" }), job({ id: 3, state: "failed", error: "Denied" })])
        clickOrb()
        const b = (id, name) => findChild(findChild(pop, "activity-entry-" + id), name)
        verify(b(1, "activity-cancel").visible && !b(1, "activity-reveal").visible && !b(1, "activity-dismiss").visible)
        verify(b(2, "activity-reveal").visible && !b(2, "activity-cancel").visible)
        verify(b(3, "activity-dismiss").visible && !b(3, "activity-reveal").visible)
        for (const id of [1, 2, 3]) verify(b(id, "activity-log").visible, "every entry has its log")

        mouseClick(b(1, "activity-cancel"), 16, 16)
        compare(Wire.last("Cancel").job, 1)
        mouseClick(b(3, "activity-dismiss"), 16, 16)
        compare(Wire.last("DismissJob").job, 3)
        mouseClick(b(3, "activity-log"), 16, 16)
        compare(logSpy.count, 1)
        compare(logSpy.signalArguments[0][0].id, 3)
        mouseClick(b(2, "activity-reveal"), 16, 16)
        compare(revealSpy.signalArguments[0][0], "file:///home/t/Downloads/site")
        verify(!pop.visible, "revealing is leaving")
    }

    function test_clear_is_offered_when_there_is_something_to_clear_and_asks_the_daemon() {
        set([job({ id: 1 })])
        clickOrb()
        verify(!findChild(pop, "activity-clear").visible, "nothing finished")
        set([job({ id: 1 }), job({ id: 2, state: "done" })])
        const clear = findChild(pop, "activity-clear")
        verify(clear.visible)
        mouseClick(clear, clear.width / 2, clear.height / 2)
        compare(Wire.count("ClearJobs"), 1)
        Wire.emitEvent({ event: "JobsCleared", jobs: [2] })
        verify(findChild(pop, "activity-entry-2") === null)
        verify(findChild(pop, "activity-entry-1") !== null, "what is running stays")
    }

    function test_nothing_to_show_says_so() {
        clickOrb()
        verify(findChild(pop, "activity-empty").visible)
    }
}
