import QtQuick
import QtTest
import "../../qml/kiki" as Kiki
import "../../qml/kiki/views" as Views
import KikiTest

// A column is a listing like any other, so a folder that is a repository of its own wears the
// branch capsule there too (plan 15) — the same element as a list row, given a share of a
// narrower row. Its own file rather than `tst_ColumnsPane`, which is about the strip: this is
// about one row's mark, as `tst_GitBadges` is for the list.
TestCase {
    id: tc
    name: "ColumnsCapsule"
    when: windowShown
    visible: true
    width: 900; height: 400

    property var fake: null
    property var pane: null
    Component { id: fakeC; FakeDaemon {} }
    Component { id: paneC; Kiki.Pane {} }

    Item {
        anchors.fill: parent
        Views.ColumnsPane { id: cols; anchors.fill: parent; pane: tc.pane }
    }

    function init() {
        Kiki.Settings.viewPrefs = ({})
        Kiki.Settings.git = ({ enabled: true, showIgnored: "dim", folders: "aggregate" })
        Wire.reset()
        fake = fakeC.createObject(tc)
        fake.tree = { "file:///home/t": [
            fake.dir("proj", { git: { state: "modified", staged: false, root: true, branch: "main", detached: false } }),
            fake.dir("sub", { git: { state: "modified", staged: false } }),
            fake.file("a.txt"),
        ] }
        pane = paneC.createObject(tc)
        pane.listing.daemon = fake
        cols.pane = pane
        pane.open("file:///home/t")
        cols.rebuild()
        wait(50)
    }
    function cleanup() { cols.columns = []; cols.pane = null; pane.destroy(); fake.destroy() }

    function test_a_project_row_in_a_column_names_its_branch() {
        const row = findChild(cols, "colrow-0-0")
        verify(row !== null, "the project's row")
        const cap = findChild(row, "git-capsule")
        verify(cap.visible, "the capsule is drawn in a column too")
        compare(findChild(row, "git-capsule-text").text, "main")
        compare(findChild(row, "git-capsule-text").color.toString(), Kiki.Theme.yellow.toString(), "coloured by the project's state")
        verify(cap.width <= cap.maxWidth && cap.width > 20, "it fits its share: " + cap.width + " of " + cap.maxWidth)
        compare(row.height, Kiki.Theme.rowHeight, "and the row is the height it always was")
    }

    function test_a_folder_inside_a_repository_keeps_its_letter() {
        const row = findChild(cols, "colrow-0-1")
        verify(!findChild(row, "git-capsule").visible, "not a repository itself, so no capsule")
        compare(findChild(row, "git-badge").text, "M", "the aggregate letter, as before")
    }
}
