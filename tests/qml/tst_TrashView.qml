import QtQuick
import QtTest
import "../../qml/kiki" as Kiki
import "../../qml/kiki/views" as Views
import KikiTest

// The trash is a view of what has been thrown away, not a folder to work in: nothing is put into
// it by hand. Plan 04's rule, asserted on all three ways in — the menu, the keys and a drop —
// because each used to be answered somewhere else and only the menu knew about it.
//
// Dropping ON the Trash in the sidebar is the opposite thing and still works: that is how files
// are thrown away.
TestCase {
    id: tc
    name: "TrashView"
    when: windowShown
    visible: true
    width: 200; height: 200

    property var fake: null
    Component { id: fakeC; FakeDaemon {} }
    Kiki.Shell { id: shell; width: 1200; height: 760 }
    Views.ListPane { id: list; width: 600; height: 300; pane: shell.pane }
    Views.IconPane { id: icons; width: 600; height: 300; pane: shell.pane; visible: false }

    function init() {
        Wire.reset(); Wire.connectAll()
        fake = fakeC.createObject(tc)
        fake.tree = {
            "file:///home/t": [fake.dir("Projects"), fake.file("a.txt")],
            "trash:///": [fake.dir("old"), fake.file("gone.txt")],
        }
        shell.left.listing.daemon = fake
        shell.pane.open("file:///home/t")
        wait(50)
        Wire.reset()
    }
    function cleanup() { shell.pane.selection.clear(); shell.pane.open("file:///home/t"); wait(50); fake.destroy() }

    function intoTheTrash() {
        shell.pane.open("trash:///")
        wait(50)
        Wire.replyTo("TrashInfo", { items: [{ name: "gone.txt", path: "/home/t/gone.txt", deleted: 1700000000000 }] })
        Wire.reset()
    }
    function labels() { return shell.contextItemsNow().map(i => i.label) }
    function submitted() { const r = Wire.last("Submit"); return r ? r.op : null }
    /// A drop as Qt delivers one (see `tst_DropAction`).
    function dropEvent(urls) {
        return { accepted: false, action: 0, hasUrls: true, hasText: false, urls: urls, proposedAction: Qt.MoveAction,
                 accept: function (a) { this.accepted = true; this.action = a } }
    }

    // ---------------------------------------------------------------- the menu
    function test_the_menu_in_the_trash_offers_only_what_can_be_done_there() {
        intoTheTrash()
        shell.pane.selection.set(1)
        // Restore says where to, out of the trash's own record of where the file came from —
        // which is a path, not a URI, and was being shown with its first two letters cut off.
        compare(labels(), ["Restore to /home/t", "Copy path", "Empty Trash"])
    }

    function test_and_the_same_menu_in_a_folder_has_both() {
        // The control: without this, the assertion above would pass on a menu that had lost
        // Paste and New folder everywhere.
        shell.pane.selection.set(1)
        verify(labels().indexOf("Paste") >= 0)
        verify(labels().indexOf("New folder") >= 0)
    }

    // ---------------------------------------------------------------- the keys
    function test_paste_does_nothing_in_the_trash() {
        shell.pane.selection.set(1)
        shell.runAction("copy")
        intoTheTrash()
        shell.runAction("paste")
        compare(Wire.count("Submit"), 0, "nothing is copied into the trash")
        // And what was on the clipboard is still there, to be pasted somewhere it can go.
        shell.pane.open("file:///home/t")
        wait(50)
        shell.runAction("paste")
        compare(submitted().op, "copy")
        compare(submitted().dest, "file:///home/t")
    }

    function test_a_new_folder_cannot_be_made_in_the_trash() {
        intoTheTrash()
        shell.runAction("newFolder")
        compare(Wire.count("Submit"), 0)
        compare(shell.pane.renamingIndex, -1, "and nothing is waiting to be renamed")
        // The control, in a folder that takes one.
        shell.pane.open("file:///home/t")
        wait(50)
        shell.runAction("newFolder")
        compare(submitted().op, "mkdir")
    }

    // ---------------------------------------------------------------- a drop
    function test_the_views_take_no_drop_while_the_trash_is_showing() {
        verify(findChild(list, "list-drop-background").enabled, "an ordinary folder takes one")
        intoTheTrash()
        verify(!findChild(list, "list-drop-background").enabled)
        verify(!findChild(icons, "icon-drop-background").enabled)
    }

    function test_a_folder_inside_the_trash_takes_no_drop_either() {
        // The background is not the only target: every folder row is one too, and a folder in
        // the trash is on its way out rather than somewhere to put things.
        intoTheTrash()
        const ev = dropEvent(["file:///home/t/a.txt"])
        shell.pane.dropInto("trash:///old", ev)
        compare(Wire.count("Submit"), 0)
        verify(ev.accepted, "the drop is answered rather than passed to whatever is behind it")
        compare(ev.action, Qt.IgnoreAction)
        compare(shell.pane.dropAction(["file:///home/t/a.txt"], "trash:///old", false), null)
        // While the drag is still in the air the cursor says so: "not allowed", now, rather than
        // nothing happening when the button comes up.
        const drag = dropEvent(["file:///home/t/a.txt"])
        compare(shell.pane.dragOver("trash:///old", drag), false)
        compare(drag.action, Qt.IgnoreAction)
    }

    // ---------------------------------------------------------------- and the other direction
    function test_a_drop_on_the_trash_in_the_sidebar_still_throws_things_away() {
        const answer = JSON.parse(shell.fakeDrop(shell.pane, "file:///home/t/a.txt\nfile:///home/t/Projects", "trash:///", ""))
        compare(answer.accepted, true)
        compare(answer.action, "move")
        compare(submitted().op, "trash")
        compare(submitted().items, ["file:///home/t/a.txt", "file:///home/t/Projects"])
    }

    function test_and_things_can_still_be_taken_out_of_the_trash() {
        intoTheTrash()
        shell.pane.selection.set(1)
        shell.runAction("trash")                    // Del in the trash is delete for good
        compare(submitted().op, "delete")
        Wire.reset()
        shell.restoreSelection()
        compare(submitted().op, "restore")
        compare(submitted().names, ["gone.txt"])
    }
}
