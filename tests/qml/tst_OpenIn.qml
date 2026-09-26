import QtQuick
import QtTest
import "../../qml/kiki" as Kiki
import KikiTest

// "Open in" (plan 14): the tools the daemon offers, which of them is the default, and what
// `Alt+Enter` sends. kiki knows nothing about the tools themselves — an entry is a command
// template in `open-in.toml`, and whether its binary is on PATH is the daemon's answer — so what
// the window owns is the list it keeps, the entry it picks, and what it opens on.
//
// The whole window is built here rather than a leaf of it: the list, the key and the toast are
// three different corners of `Shell.qml`, and this is what they add up to.
//
// Plan 14 also asks for an "Open in" toolbar dropdown and an "Open in ▸" context submenu. Neither
// is built — the context menu's "Open with" is the freedesktop application list, which is plan
// 02's, and `Alt+Shift+Enter` opens that. Both are new UI and the owner's to decide; what is here
// is what ships.
TestCase {
    id: tc
    name: "OpenIn"
    when: windowShown
    visible: true
    width: 200; height: 200

    property var fake: null
    Component { id: fakeC; FakeDaemon {} }
    Kiki.Shell { id: shell; width: 1200; height: 760 }
    Kiki.Keymap { id: keymap }

    /// What `OpenInList` answers with: Neovim is the editor entry, Helix's binary is not on PATH,
    /// and "Terminal here" needs no binary at all.
    readonly property var tools: [
        { id: "neovim", name: "Neovim", role: "editor", enabled: true, accepts: "file" },
        { id: "claude", name: "Claude Code", enabled: true, accepts: "both" },
        { id: "helix", name: "Helix", enabled: false, reason: "hx is not installed", accepts: "file" },
        { id: "terminal", name: "Terminal here", enabled: true, accepts: "folder" },
    ]

    function init() {
        Wire.reset(); Wire.connectAll()
        fake = fakeC.createObject(tc)
        fake.tree = { "file:///home/t": [fake.dir("Projects"), fake.file("a.txt"), fake.file("b.txt")] }
        shell.left.listing.daemon = fake
        shell.pane.open("file:///home/t")
        wait(50)
        shell.loadOpenIn()
        Wire.replyTo("OpenInList", { tools: tools })
        Kiki.Jobs.dismissToast()
        Wire.reset()
    }
    function cleanup() { shell.pane.selection.clear(); Kiki.Jobs.dismissToast(); fake.destroy() }

    function opened() { const r = Wire.last("OpenIn"); return r }
    function select(name) {
        for (let i = 0; i < shell.pane.listing.count; i++) if (shell.pane.listing.row(i).name === name) { shell.pane.selection.set(i); return }
        fail(name + " is not in the folder")
    }

    // ---------------------------------------------------------------- what is offered
    // A double-click on a file (0.2.0) asks the daemon for the default application — which
    // fetches a remote file first — rather than running xdg-open on the URI here; a folder is
    // entered, and asks for nothing.
    function test_a_double_click_on_a_file_asks_the_daemon_to_open_it_with_the_default_application() {
        select("a.txt")
        shell.openSelected()
        const r = Wire.last("OpenDefault")
        verify(r, "the daemon is asked")
        compare(r.uri, "file:///home/t/a.txt")
        compare(Wire.count("OpenIn"), 0, "not a tool")
        Wire.reset()
        select("Projects")
        shell.openSelected()
        compare(Wire.count("OpenDefault"), 0, "a folder is entered, not opened")
        compare(shell.pane.uri, "file:///home/t/Projects")
    }
    function test_an_entry_whose_binary_is_absent_is_not_offered() {
        compare(shell.openInTools.map(t => t.id), ["neovim", "claude", "terminal"])
        // And it cannot be reached by name either: asking for it opens nothing rather than
        // running a command whose program is not there.
        shell.openIn("helix")
        compare(Wire.count("OpenIn"), 0)
    }

    function test_the_list_is_read_again_when_the_files_change() {
        // The daemon watches both TOML files and says so; nothing is restarted for a new tool.
        Wire.reset()
        Wire.emitEvent({ event: "OpenInChanged" })
        verify(Wire.last("OpenInList") !== null)
        Wire.replyTo("OpenInList", { tools: [{ id: "helix", name: "Helix", enabled: true }] })
        compare(shell.openInTools.map(t => t.id), ["helix"])
    }

    // ---------------------------------------------------------------- which one is the default
    function test_the_default_is_the_first_entry_that_is_not_the_editor() {
        // `e` has the editor (plan 13); Alt+Enter is for the other kind of tool, so the editor
        // entry is stepped over even though it is first in the file.
        compare(shell.openInDefaultTool().id, "claude")
        select("a.txt")
        shell.openIn("")
        compare(opened().tool, "claude")
    }

    function test_with_only_an_editor_configured_that_is_the_default() {
        shell.loadOpenIn()
        Wire.replyTo("OpenInList", { tools: [{ id: "neovim", name: "Neovim", role: "editor", enabled: true }] })
        compare(shell.openInDefaultTool().id, "neovim")
    }

    function test_with_nothing_configured_nothing_happens() {
        shell.loadOpenIn()
        Wire.replyTo("OpenInList", { tools: [] })
        compare(shell.openInDefaultTool(), null)
        Wire.reset()
        shell.runAction("openDefault")
        compare(Wire.count("OpenIn"), 0, "and no toast about a tool that was never chosen")
        compare(Kiki.Jobs.toast, null)
    }

    // ---------------------------------------------------------------- Alt+Enter
    function test_alt_enter_is_the_key_that_asks_for_it() {
        // The window reads a keypress through `Keymap` and then runs the action by name; the two
        // halves are asserted here rather than by a keypress, since a window that is never shown
        // has nothing to give the keyboard focus to.
        compare(keymap.idFor(Qt.Key_Return, Qt.AltModifier, ""), "openDefault")
        compare(keymap.idFor(Qt.Key_Enter, Qt.AltModifier, ""), "openDefault")
        compare(keymap.chordFor("openDefault"), "Alt+Enter")
        select("a.txt")
        verify(shell.runAction("openDefault"))
        compare(opened().tool, "claude")
        compare(opened().uris, ["file:///home/t/a.txt"])
    }

    function test_it_opens_on_everything_that_is_selected() {
        select("a.txt")
        shell.pane.selection.toggle(2)                    // b.txt as well
        shell.runAction("openDefault")
        compare(opened().uris, ["file:///home/t/a.txt", "file:///home/t/b.txt"])
    }

    function test_with_nothing_selected_it_opens_on_the_folder_being_shown() {
        shell.pane.selection.clear()
        shell.runAction("openDefault")
        compare(opened().uris, ["file:///home/t"])
    }

    // ---------------------------------------------------------------- the request itself
    function test_the_tool_is_named_in_a_field_of_its_own() {
        // What this guards: the tool used to be sent as `id`, which is the field every request
        // carries the client's own number in. The number was replaced by "claude", the daemon
        // could not parse a request without one, and Alt+Enter did nothing at all — answered,
        // to nobody, with "Protocol: missing id".
        select("a.txt")
        shell.runAction("openDefault")
        const r = opened()
        compare(r.tool, "claude")
        compare(typeof r.id, "number", "the request keeps its own id, so the answer comes back")
        verify(r.id > 0)
    }

    // ---------------------------------------------------------------- when it will not start
    function test_a_tool_that_will_not_start_says_so() {
        select("a.txt")
        shell.runAction("openDefault")
        const r = opened()
        Wire.fail(r.id, "Invalid", "Claude Code is not installed")
        verify(Kiki.Jobs.toast !== null, "the answer reached the window")
        compare(Kiki.Jobs.toast.text, "Claude Code is not installed")
        compare(Kiki.Jobs.toast.undoable, false)
    }
}
