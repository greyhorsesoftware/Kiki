import QtQuick
import QtTest
import "../../qml/kiki/ui" as UI
import KikiTest

// The permissions grid (plan 03): the checkboxes are the mode, the octal and symbolic forms
// follow them, and Apply submits exactly what is shown — the mouse path to chmod.
TestCase {
    id: tc
    name: "InspectorPermissions"
    when: windowShown
    visible: true
    width: 420; height: 620

    UI.Inspector {
        id: insp
        anchors.fill: parent
        standalone: true
        uri: "file:///home/t/a.txt"
        row: ({ name: "a.txt", kind: "text", isDir: false, meta: { size: 10, mtime: 1700000000000, mode: 0o644, owner: "gideon", group: "users" } })
    }
    SignalSpy { id: chmods; target: insp; signalName: "chmod" }

    function init() {
        Wire.reset()
        insp.tab = "permissions"
        insp.editMode = insp.row.meta.mode & 0o777
        chmods.clear()
        wait(20)
    }

    function test_the_grid_starts_at_the_mode_it_was_given() {
        compare(insp.editMode, 0o644)
        compare(insp.octal(insp.editMode), "644")
        compare(insp.symbolic(0o644), "-rw-r--r--")
        compare(insp.dirty, false)
    }

    function test_ticking_owner_exec_sets_that_bit_only() {
        mouseClick(findChild(insp, "perm-owner-1"))
        compare(insp.editMode, 0o744)
        compare(insp.octal(insp.editMode), "744")
        compare(insp.dirty, true)
    }

    function test_clearing_world_read_clears_that_bit() {
        mouseClick(findChild(insp, "perm-world-4"))
        compare(insp.editMode, 0o640)
        compare(insp.symbolic(insp.editMode), "-rw-r-----")
    }

    function test_apply_submits_the_mode_on_show() {
        mouseClick(findChild(insp, "perm-owner-1"))
        mouseClick(findChild(insp, "perm-apply"))
        compare(chmods.count, 1)
        compare(chmods.signalArguments[0][0], 0o744)
        compare(chmods.signalArguments[0][1], false)
    }

    function test_apply_to_contained_items_rides_along() {
        mouseClick(findChild(insp, "perm-group-2"))
        mouseClick(findChild(insp, "perm-recursive"))
        mouseClick(findChild(insp, "perm-apply"))
        compare(chmods.signalArguments[0][1], true)
        // and again without it, to prove the tick is read and not assumed. A different bit, so
        // the mode still differs from the one on disk and Apply stays alive.
        mouseClick(findChild(insp, "perm-recursive"))
        mouseClick(findChild(insp, "perm-world-1"))
        mouseClick(findChild(insp, "perm-apply"))
        compare(chmods.count, 2)
        compare(chmods.signalArguments[1][1], false)
    }

    function test_revert_goes_back_to_the_mode_on_disk() {
        mouseClick(findChild(insp, "perm-owner-1"))
        compare(insp.dirty, true)
        mouseClick(findChild(insp, "perm-revert"))
        compare(insp.editMode, 0o644)
        compare(insp.dirty, false)
        compare(chmods.count, 0)
    }

    // Nothing to apply until something changes: an accidental click must not submit a job.
    function test_apply_is_dead_until_the_mode_changes() {
        mouseClick(findChild(insp, "perm-apply"))
        compare(chmods.count, 0)
    }
}
