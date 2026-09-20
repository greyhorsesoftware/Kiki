import QtQuick
import QtTest
import "../../qml/kiki/ui" as UI
import KikiTest

// The info panel's Git lines (plan 15): the row's state, and — asked of the daemon only for a file
// that is in a repository, since it costs a `git log` — the branch and the last commit.
TestCase {
    id: tc
    name: "InspectorGit"
    when: windowShown
    visible: true
    width: 420; height: 900

    UI.Inspector { id: insp; anchors.fill: parent; standalone: true }

    function init() { Wire.reset(); Wire.connectAll(); insp.uri = ""; insp.row = null }

    function show(name, git) {
        insp.row = ({ name: name, kind: "text", isDir: false, git: git, meta: { size: 10, mtime: 1700000000000, mode: 0o644 } })
        insp.uri = "file:///home/t/repo/" + name
    }

    function test_a_file_outside_a_repository_costs_no_git_request() {
        show("loose.txt", null)
        compare(Wire.count("GitStatus"), 0)
        verify(!findChild(insp, "insp-git").visible)
        verify(!findChild(insp, "insp-git-branch").visible)
    }

    function test_state_branch_and_last_commit() {
        show("main.rs", { state: "modified", staged: true })
        compare(findChild(insp, "insp-git").value, "modified · staged")
        compare(Wire.count("GitStatus"), 1)
        verify(!findChild(insp, "insp-git-branch").visible, "nothing invented before the answer")
        Wire.replyTo("GitStatus", { state: "modified", staged: true, branch: "main",
                                    last: { hash: "a1b2c3d4e5", short: "a1b2c3d", author: "Ada", time: Math.floor(Date.now() / 1000) - 7200, subject: "Fix the parser" } })
        compare(findChild(insp, "insp-git-branch").value, "main")
        const last = findChild(insp, "insp-git-last")
        verify(last.visible)
        verify(last.value.startsWith("a1b2c3d · Ada · "), last.value)
        compare(findChild(insp, "insp-git-subject").value, "Fix the parser")
    }

    function test_an_answer_for_the_file_before_is_not_shown_for_this_one() {
        show("old.rs", { state: "modified", staged: false })
        const asked = Wire.last("GitStatus")
        show("new.rs", { state: "untracked", staged: false })
        Wire.reply(asked.id, { state: "modified", staged: false, branch: "stale", last: null })
        verify(!findChild(insp, "insp-git-branch").visible, "the late answer belonged to old.rs")
    }

    function test_a_detached_head_and_an_uncommitted_file_show_what_there_is() {
        show("new.txt", { state: "untracked", staged: false })
        Wire.replyTo("GitStatus", { state: "untracked", staged: false, branch: null, last: null })
        compare(findChild(insp, "insp-git").value, "untracked")
        verify(!findChild(insp, "insp-git-branch").visible)
        verify(!findChild(insp, "insp-git-last").visible)
    }
}
