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
    width: 420; height: 900

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

    // The grid is the widest thing in the panel, and the panel's minimum is what holds it whole.
    function test_the_grid_fits_inside_the_panels_minimum_width() {
        const w = findChild(insp, "perm-world-1")            // the last box of the last column
        const right = w.mapToItem(insp, w.width, 0).x
        verify(right <= insp.minWidth - 16, "the grid's right edge at " + right + " is past the minimum's margin (" + (insp.minWidth - 16) + ")")
        verify(insp.minWidth >= 280, insp.minWidth)
    }
    // The panel reads the row and only the row: nothing is asked of the daemon for the fields.
    function test_the_fields_come_from_the_row_and_no_stat_is_asked() {
        Wire.reset()
        insp.uri = "sftp://lab/srv/site"
        insp.row = ({ name: "site", kind: "folder", isDir: true, meta: null })
        wait(20)
        compare(Wire.count("Stat"), 0, "a folder on a server was stat'd — which answers NotFound")
        insp.uri = "file:///home/t/a.txt"
        insp.row = ({ name: "a.txt", kind: "text", isDir: false, meta: { size: 10, mtime: 1700000000000, mode: 0o644, owner: "gideon", group: "users" } })
    }
}
