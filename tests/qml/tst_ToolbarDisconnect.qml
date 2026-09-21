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
}
