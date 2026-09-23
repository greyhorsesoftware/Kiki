import QtQuick
import QtTest
import "../../qml/kiki" as Kiki
import KikiTest

// Side by side with a server in one pane, the toolbar has a Disconnect beside Mirror. It lets go
// of that server and takes the pane off it — to the folder the location is paired with, or home —
// because a pane left standing on a server that has gone can only say so.
TestCase {
    id: tc
    name: "ToolbarDisconnect"
    when: windowShown
    visible: true
    width: 200; height: 200

    Kiki.Shell { id: shell; width: 1200; height: 760 }
    function button() { return findChild(shell.contentItem, "toolbar-disconnect") }

    function init() {
        Wire.reset(); Wire.connectAll()
        shell.locations = [{ name: "lab", plugin: "sftp", remoteUri: "sftp://lab/srv", localUri: "file:///home/t/site" }, { name: "bare", plugin: "sftp", remoteUri: "sftp://bare/", localUri: "" }]
        // The window is never shown here, so its content is 0 wide and the toolbar would fold.
        findChild(shell.contentItem, "toolbar").width = 1200
        shell.sideBySide = true
        shell.left.open("file:///home/t")
        shell.right.open("file:///home/t/other")
        wait(30)
    }
    function cleanup() { shell.left.open("file:///home/t"); shell.right.open("file:///home/t"); shell.sideBySide = false; wait(20) }

    function test_two_local_panes_have_nothing_to_disconnect() {
        verify(!button().visible)
    }
    function test_a_server_on_either_side_brings_it_and_names_the_server() {
        shell.right.open("sftp://lab/srv/www")
        tryVerify(() => button().visible)
        compare(button().tip, "Disconnect from lab")
        shell.right.open("file:///home/t/other"); shell.left.open("sftp://lab/srv")
        tryVerify(() => button().visible)
        // It stands beside Mirror, not on it.
        const m = findChild(shell.contentItem, "toolbar-mirror")
        verify(button().x >= m.x + m.width, "on top of the Mirror button")
    }
    function test_it_lets_go_of_the_server_and_takes_the_pane_to_its_local_folder() {
        shell.right.open("sftp://lab/srv/www")
        tryVerify(() => button().visible)
        Wire.reset()
        button().clicked()                 // the shell's window is not shown in a test: no pointer reaches it
        compare(Wire.last("Disconnect").name, "lab")
        // Off the server before it goes — whichever of the two pane objects it is by now: going
        // back to one pane may have changed them over.
        verify([shell.left.uri, shell.right.uri].indexOf("file:///home/t/site") >= 0, shell.left.uri + " | " + shell.right.uri)
        verify(!shell.isRemote(shell.left.uri) && !shell.isRemote(shell.right.uri), "a pane was left standing on the server")
        tryVerify(() => !button().visible)
        verify(!shell.split, "side by side was that server and its folder: one pane again")
        verify(shell.pane.uri.indexOf("file://") === 0)
    }
    function test_a_location_with_no_local_folder_goes_home() {
        shell.left.open("sftp://bare/x")
        tryVerify(() => button().visible)
        button().clicked()                 // the shell's window is not shown in a test: no pointer reaches it
        compare(Wire.last("Disconnect").name, "bare")
        compare(shell.left.uri, "file://" + shell.home)
    }

    // ---------------------------------------------------------------- side by side is for a server
    // (owner, 2026-09-21) Off, the window shows the server alone — whichever pane had the focus;
    // on again, both, each on its own side. With no server open the key does nothing.
    function test_one_pane_again_is_the_servers_pane_whichever_had_the_focus() {
        shell.right.open("sftp://lab/srv/www")
        shell.focusPane(shell.left)
        shell.toggleMirrorView()
        verify(!shell.split)
        compare(shell.pane.uri, "sftp://lab/srv/www")
        verify(shell.remoteOpen)
        shell.toggleMirrorView()
        verify(shell.split)
        compare(shell.left.uri, "file:///home/t")
        compare(shell.right.uri, "sftp://lab/srv/www")
    }
    function test_with_no_server_open_the_key_does_nothing() {
        shell.toggleMirrorView()                      // leaves: two local panes (a script's doing)
        verify(!shell.split)
        verify(!shell.remoteOpen)
        shell.toggleMirrorView()
        verify(!shell.split, "side by side came up over two folders on this machine")
        shell.toggleMirrorView(true)                  // the IPC may still ask for it
        verify(shell.split)
    }
    function test_from_one_pane_on_a_server_it_lays_out_as_a_location_does() {
        shell.toggleMirrorView()
        shell.pane.open("sftp://lab/srv/www")
        wait(20)
        if (shell.split) shell.leaveMirror()          // opening a location may have paired it already
        compare(shell.pane.uri, "sftp://lab/srv/www")
        shell.toggleMirrorView()
        verify(shell.split)
        compare(shell.right.uri, "sftp://lab/srv/www", "the server goes to the right")
        compare(shell.left.uri, "file:///home/t/site", "and the folder it is paired with to the left")
    }

    function test_off_it_stays_off_while_folders_in_the_server_are_opened() {
        shell.right.open("sftp://lab/srv/www")
        shell.toggleMirrorView()
        verify(!shell.split)
        compare(shell.pane.uri, "sftp://lab/srv/www")
        shell.pane.open("sftp://lab/srv/www/img")           // a double click on a folder
        wait(20)
        verify(!shell.split, "a step inside the server brought side by side back")
        compare(shell.pane.uri, "sftp://lab/srv/www/img")
        // Away to this machine and back to the server: that is arriving, and it pairs again.
        shell.pane.open("file:///home/t")
        shell.pane.open("sftp://lab/srv")
        wait(20)
        verify(shell.split)
    }

    // ---------------------------------------------------------------- the info button
    // Beside Search: opens and closes the info panel, and is lit while it is up — the same
    // toggle as Ctrl+I and the menu's Get info (owner, 2026-09-22).
    function test_the_info_button_opens_and_closes_the_panel_and_lights_with_it() {
        const b = findChild(shell.contentItem, "toolbar-info")
        verify(b.visible)
        shell.inspectedUri = "file:///home/t/a.txt"          // something selected
        shell.inspectorRequested = false
        verify(!b.active)
        b.clicked()
        verify(shell.inspectorRequested)
        verify(b.active, "lit while the panel is up")
        b.clicked()
        verify(!shell.inspectorRequested)
        verify(!b.active)
        // Opened another way — the key — the button follows.
        shell.inspectorRequested = true
        verify(b.active)
        shell.inspectorRequested = false
    }

    // The hamburger's menu carries every button it hides: search, info, side by side when it is
    // offered, the views as a submenu, favorites, and the gear's rows.
    function test_the_hamburger_menu_carries_every_folded_button() {
        shell.sideBySide = false
        shell.inspectorRequested = false
        let labels = shell.hamburgerItems().map(i => i.label)
        compare(labels.filter(l => l.indexOf("favorites") < 0), ["Search everywhere…", "Show info", "View", "Settings…", "Keyboard shortcuts…", "About kiki…"])
        verify(labels.indexOf("Search everywhere…") === 0)
        verify(labels.indexOf("Side by Side") < 0, "offered with no server open")
        const view = shell.hamburgerItems().find(i => i.label === "View")
        verify(view.items.length >= 4, "the views are a submenu: " + view.items.map(i => i.label))
        verify(labels.indexOf("Settings…") > labels.indexOf("View") && labels.indexOf("About kiki…") === labels.length - 1)
        shell.left.open("sftp://lab/srv"); wait(20)
        if (shell.split) shell.leaveMirror()
        labels = shell.hamburgerItems().map(i => i.label)
        verify(labels.indexOf("Side by Side") >= 0, "a server open: side by side is in the menu")
        shell.hamburgerItems().find(i => i.label === "Show info").action()
        verify(shell.inspectorRequested)
        compare(shell.hamburgerItems().find(i => i.label === "Hide info").label, "Hide info")
        shell.inspectorRequested = false
    }

    // Columns view has an info column of its own and the window's panel never shows there: the
    // button dims and takes no click; the folded menu's row is off too.
    // …and with nothing selected in list, icon or gallery view there is nothing to show.
    function test_with_nothing_selected_the_info_button_dims() {
        const b = findChild(shell.contentItem, "toolbar-info")
        shell.pane.view = "list"
        shell.inspectedUri = ""
        tryVerify(() => !b.enabled)
        compare(b.tip, "", "a dimmed button says nothing")
        verify(!shell.hamburgerItems().find(i => i.label.indexOf("info") >= 0).enabled)
        shell.inspectedUri = "file:///home/t/a.txt"
        tryVerify(() => b.enabled)
        shell.inspectedUri = ""
    }
    function test_in_columns_view_the_info_button_dims() {
        const b = findChild(shell.contentItem, "toolbar-info")
        shell.inspectedUri = "file:///home/t/a.txt"
        shell.pane.view = "columns"
        tryVerify(() => !b.enabled)
        verify(b.opacity < 0.5)
        verify(!shell.hamburgerItems().find(i => i.label.indexOf("info") >= 0).enabled)
        shell.pane.view = "list"
        tryVerify(() => b.enabled)
        compare(b.opacity, 1)
        shell.inspectedUri = ""
    }

    // The toolbar's menus hang with their right edge on their button's; the path's from the left.
    function test_toolbar_menus_hang_right_aligned_to_their_button() {
        const bar = findChild(shell.contentItem, "toolbar"), m = shell.contentItem.children.find(c => c.box !== undefined)
        // The window is never shown here, so the box is clamped into a 0 × 0 window: what is
        // checked is the point asked for (`at`), which is where the box goes when there is room.
        shell.gearMenu()
        const g = bar.gearButton.mapToItem(shell.contentItem, bar.gearButton.width, 0)
        verify(Math.abs((m.at.x + m.box.width) - g.x) <= 1, "gear menu right edge " + (m.at.x + m.box.width) + " vs button " + g.x)
        m.close()
        shell.viewMenu()
        const v = bar.viewButton.mapToItem(shell.contentItem, bar.viewButton.width, 0)
        verify(Math.abs((m.at.x + m.box.width) - v.x) <= 1, "view menu right edge " + (m.at.x + m.box.width) + " vs button " + v.x)
        m.close()
        shell.pathMenu()
        const c = shell.activeCrumb().mapToItem(shell.contentItem, 0, 0)     // side by side, a pane header's
        verify(Math.abs(m.at.x - c.x) <= 1, "the path's menu hangs from its left: " + m.at.x + " vs " + c.x)
        m.close()
    }
}
