import QtQuick
import QtTest
import "../../qml/kiki" as Kiki
import KikiTest

// Every file operation of plan 04, as the menu items and the keymap call them: the assertion is
// the job that went out on the wire (plan 28, layer A). What the job then does to disk is the
// daemon's own tests; that the two meet is an end-to-end flow.
TestCase {
    id: tc
    name: "Ops"
    when: windowShown
    visible: true

    property var fake: null
    property var pane: null
    property var ops: null
    property var asked: null           // the last confirmation spec
    property var answer: null          // and the callback that answers it
    property string copied: ""         // what copyPath put on the clipboard

    Component { id: fakeC; FakeDaemon {} }
    Component { id: paneC; Kiki.Pane {} }
    Component { id: opsC; Kiki.Ops {} }

    function init() {
        Kiki.Settings.viewPrefs = ({})
        Wire.reset()
        asked = null; answer = null; copied = ""
        fake = fakeC.createObject(tc)
        fake.tree = {
            "file:///home/t": [fake.dir("Projects"), fake.file("a.txt"), fake.file("b.txt"), fake.file("box.zip", { kind: "archive" })],
            "trash:///": [fake.file("gone.txt")]
        }
        pane = paneC.createObject(tc)
        pane.listing.daemon = fake
        ops = opsC.createObject(tc, { pane: pane })
        ops.confirmNeeded.connect((spec, reply) => { tc.asked = spec; tc.answer = reply })
        ops.copyText.connect(text => tc.copied = text)
        pane.open("file:///home/t")
        wait(50)
        Wire.reset()
    }
    function cleanup() { ops.destroy(); pane.destroy(); fake.destroy() }

    function submitted() { const r = Wire.last("Submit"); return r ? r.op : null }

    // ---------------------------------------------------------------- clipboard

    function test_copy_then_paste_submits_a_copy_into_this_folder() {
        pane.selection.set(1)                      // a.txt
        ops.copySelection(false)
        compare(ops.clipboard.uris[0], "file:///home/t/a.txt")
        compare(ops.clipboard.cut, false)
        pane.open("file:///home/t/Projects")
        wait(50)
        ops.paste()
        compare(submitted().op, "copy")
        compare(submitted().dest, "file:///home/t/Projects")
        compare(submitted().items[0], "file:///home/t/a.txt")
        compare(ops.clipboard.uris.length, 1)      // a copy can be pasted again
    }

    function test_cut_then_paste_moves_and_empties_the_clipboard() {
        pane.selection.set(1)
        ops.copySelection(true)
        pane.open("file:///home/t/Projects")
        wait(50)
        ops.paste()
        compare(submitted().op, "move")
        compare(ops.clipboard.uris.length, 0)
    }

    function test_paste_with_nothing_copied_does_nothing() {
        ops.paste()
        compare(Wire.count("Submit"), 0)
    }

    function test_copy_path_hands_over_the_decoded_path() {
        pane.selection.set(1)
        ops.copyPath()
        compare(copied, "/home/t/a.txt")
    }

    function test_nothing_selected_means_nothing_to_copy() {
        pane.selection.clear()
        ops.copySelection(false)
        compare(ops.clipboard.uris.length, 0)
    }

    // ---------------------------------------------------------------- create and rename

    function test_new_folder_takes_the_first_free_name() {
        ops.newFolder()
        compare(submitted().op, "mkdir")
        compare(submitted().uri, "file:///home/t/New%20folder")
    }

    function test_new_folder_steps_past_a_name_already_there() {
        fake.tree["file:///home/t"] = fake.tree["file:///home/t"].concat([fake.dir("New folder")])
        pane.listing.refresh()
        wait(50)
        ops.newFolder()
        compare(submitted().uri, "file:///home/t/New%20folder%202")
    }

    // The folder only exists once the daemon says so, and the editor opens when the row arrives.
    function test_a_new_folder_lands_selected_and_being_renamed() {
        ops.newFolder()
        const req = Wire.last("Submit")
        fake.tree["file:///home/t"] = fake.tree["file:///home/t"].concat([fake.dir("New folder")])
        Wire.reply(req.id, { job: 1 })
        fake.emitEvent({ event: "Reset", lid: pane.listing.lid, n: 5 })
        wait(80)
        const at = pane.selection.current
        verify(at >= 0)
        compare(pane.listing.row(at).name, "New folder")
        compare(pane.renamingIndex, at)
    }

    // Which listing a new folder will show up in, and who names it, is the window's to answer:
    // columns is showing several folders at once and names the row in the column it lands in.
    // Ops asks rather than reaching into the pane — before this it made the folder in the folder
    // the PANE was on and switched the view to list to find an editor.
    function test_a_new_folder_is_named_where_the_view_answering_says() {
        pane.view = "columns"
        let named = -1
        const rows = [fake.dir("New folder"), fake.file("deep.txt")]
        const site = { count: () => rows.length, row: i => rows[i], rename: i => { named = i; return true } }
        const onAsk = (spec, reply) => { if (spec.dest === "file:///home/t/Projects") reply(site) }
        ops.listingNeeded.connect(onAsk)
        ops.newFolder("file:///home/t/Projects")
        compare(submitted().op, "mkdir")
        compare(submitted().uri, "file:///home/t/Projects/New%20folder%202",
                "the free name is found among THAT folder's rows, not the pane's")
        rows.push(fake.dir("New folder 2"))
        tryVerify(() => named === 2, 2000, "the row is named where it landed")
        compare(pane.renamingIndex, -1, "the pane's own editor stays out of it")
        compare(pane.view, "columns", "and the view is left as the user had it")
        ops.listingNeeded.disconnect(onAsk)
    }

    // With nobody showing the folder there is no row to name, and nothing is left spinning.
    function test_a_new_folder_made_out_of_sight_is_left_alone() {
        ops.newFolder("file:///home/t/Projects")
        compare(submitted().uri, "file:///home/t/Projects/New%20folder")
        compare(ops.renameSoon, "")
    }

    function test_rename_switches_to_the_view_that_has_an_editor() {
        pane.view = "icon"
        pane.selection.set(1)
        ops.renameSelected()
        compare(pane.view, "list")
        compare(pane.renamingIndex, 1)
    }

    function test_rename_with_no_selection_opens_no_editor() {
        pane.selection.clear()
        ops.renameSelected()
        compare(pane.renamingIndex, -1)
    }

    // ---------------------------------------------------------------- remove and restore

    function test_trash_submits_the_selection_without_asking() {
        pane.selection.set(1)
        pane.selection.toggle(2)
        ops.trashSelection()
        compare(asked, null)
        compare(submitted().op, "trash")
        compare(submitted().items.length, 2)
    }

    // A server has no trash: what is on one is deleted for good, and only once the user says so.
    // Del, the menu and a drop on the Trash all come through `trashSelection`.
    function test_del_on_a_server_asks_and_then_deletes() {
        fake.tree["sftp://lab/srv"] = [fake.file("r.txt")]
        pane.open("sftp://lab/srv")
        wait(50)
        pane.selection.set(0)
        ops.trashSelection()
        verify(asked !== null, "it asks first")
        compare(Wire.count("Submit"), 0, "nothing until the answer comes back")
        answer(true)
        compare(submitted().op, "delete")
        compare(submitted().items[0], "sftp://lab/srv/r.txt")
    }

    function test_remote_files_dropped_on_the_trash_are_deleted_after_asking() {
        ops.trashSelection(["sftp://lab/srv/a.txt"])
        verify(asked !== null, "it asks first")
        compare(asked.title, "Delete permanently?")
        verify(asked.message.indexOf("a.txt is on lab") === 0, asked.message)
        compare(Wire.count("Submit"), 0, "nothing until the answer comes back")
        answer(true)
        compare(submitted().op, "delete")
        compare(submitted().items.length, 1)
        compare(submitted().items[0], "sftp://lab/srv/a.txt")
    }

    function test_remote_files_dropped_on_the_trash_and_declined_are_left_alone() {
        ops.trashSelection(["sftp://lab/srv/a.txt", "sftp://lab/srv/b.txt"])
        verify(asked.message.indexOf("These 2 items are on lab") === 0, asked.message)
        answer(false)
        compare(Wire.count("Submit"), 0)
    }

    function test_a_mixed_drop_trashes_the_local_files_and_asks_about_the_rest() {
        ops.trashSelection(["file:///home/t/a.txt", "ftps://box/b.txt"])
        compare(submitted().op, "trash")
        compare(submitted().items.length, 1)
        compare(submitted().items[0], "file:///home/t/a.txt")
        verify(asked !== null)
        answer(true)
        compare(submitted().op, "delete")
        compare(submitted().items.length, 1)
        compare(submitted().items[0], "ftps://box/b.txt")
    }

    function test_delete_asks_before_it_deletes() {
        pane.selection.set(1)
        ops.deleteForever()
        verify(asked !== null)
        compare(asked.label, "Delete")
        compare(Wire.count("Submit"), 0)           // nothing until the answer comes back
        answer(true)
        compare(submitted().op, "delete")
    }

    function test_delete_declined_submits_nothing() {
        pane.selection.set(1)
        ops.deleteForever()
        answer(false)
        compare(Wire.count("Submit"), 0)
    }

    function test_in_the_trash_delete_goes_straight_through() {
        pane.open("trash:///")
        wait(50)
        pane.selection.set(0)
        ops.deleteForever()
        compare(asked, null)
        compare(submitted().op, "delete")
    }

    function test_empty_trash_asks_and_then_empties() {
        pane.open("trash:///")
        wait(50)
        ops.emptyTrash()
        compare(asked.label, "Empty Trash")
        answer(true)
        compare(submitted().op, "emptyTrash")
    }

    function test_restore_submits_names_not_uris() {
        pane.open("trash:///")
        wait(50)
        pane.selection.set(0)
        ops.restoreSelection()
        compare(submitted().op, "restore")
        compare(submitted().names[0], "gone.txt")
    }

    // ---------------------------------------------------------------- archives, modes, panes

    function test_extract_here_unpacks_into_this_folder() {
        ops.extractHere("box.zip")
        compare(submitted().op, "extract")
        compare(submitted().archive, "file:///home/t/box.zip")
        compare(submitted().dest, "file:///home/t")
    }

    // Extract to… asks where first, starting from this folder, and extracts into the answer. It
    // makes no folder of its own: what lands, and under which name, is the daemon's rule.
    function test_extract_to_asks_where_and_extracts_there() {
        let asked = null, answer = null
        const onAsk = (spec, reply) => { asked = spec; answer = reply }
        ops.folderNeeded.connect(onAsk)
        ops.extractTo("box.zip")
        ops.folderNeeded.disconnect(onAsk)
        verify(asked !== null, "a folder is asked for")
        compare(asked.start, "file:///home/t")
        compare(Wire.count("Submit"), 0, "nothing is submitted until there is an answer")
        answer("file:///home/t/Documents")
        compare(submitted().op, "extract")
        compare(submitted().archive, "file:///home/t/box.zip")
        compare(submitted().dest, "file:///home/t/Documents")
        compare(Wire.count("Submit"), 1, "and no mkdir beside it")
    }

    function test_extract_to_cancelled_does_nothing() {
        let answer = null
        const onAsk = (spec, reply) => { answer = reply }
        ops.folderNeeded.connect(onAsk)
        ops.extractTo("box.zip")
        ops.folderNeeded.disconnect(onAsk)
        answer("")
        compare(Wire.count("Submit"), 0)
    }

    function test_compress_carries_the_format() {
        ops.compress(["file:///home/t/a.txt"], "file:///home/t/out.tar.zst", "tar.zst")
        compare(submitted().op, "compress")
        compare(submitted().format, "tar.zst")
    }

    function test_chmod_carries_the_mode_and_the_recursive_flag() {
        ops.chmod("file:///home/t/a.txt", 0o644, true)
        compare(submitted().op, "chmod")
        compare(submitted().mode, 0o644)
        compare(submitted().recursive, true)
    }

    function test_transfer_moves_into_the_other_pane() {
        pane.selection.set(1)
        ops.transferTo("sftp://lab/srv", true)
        compare(submitted().op, "move")
        compare(submitted().dest, "sftp://lab/srv")
    }

    function test_transfer_with_no_destination_does_nothing() {
        pane.selection.set(1)
        ops.transferTo("", true)
        compare(Wire.count("Submit"), 0)
    }
}
