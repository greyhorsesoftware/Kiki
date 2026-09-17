import QtQuick
import QtTest
import "../../qml/kiki" as Kiki

// Navigation logic only: history, parent and child URIs, trash detection. The listing talks to
// the stubbed daemon and never answers, which is fine for these.
TestCase {
    name: "Pane"
    Kiki.Pane { id: pane }

    function test_child_and_parent_uris() {
        pane.uri = "file:///home/david"
        compare(pane.childUri("a b.txt"), "file:///home/david/a%20b.txt")
        compare(pane.parentOf("file:///home/david/Projects"), "file:///home/david")
        compare(pane.parentOf("file:///home"), "file:///")
        compare(pane.parentOf("file:///"), null)
        compare(pane.parentOf("sftp://homelab/srv/kiki"), "sftp://homelab/srv")
        compare(pane.parentOf("sftp://homelab/"), null)
    }
    function test_history_back_forward() {
        pane.open("file:///a"); pane.open("file:///b"); pane.open("file:///c")
        compare(pane.uri, "file:///c")
        verify(pane.canBack()); verify(!pane.canForward())
        pane.back(); compare(pane.uri, "file:///b")
        pane.back(); compare(pane.uri, "file:///a")
        verify(!pane.canBack())
        pane.forward(); compare(pane.uri, "file:///b")
        pane.open("file:///d")   // branching drops the old forward entry
        verify(!pane.canForward())
        compare(pane.history.length, 3)
    }
    function test_trash_flag_and_hidden_toggle() {
        pane.open("trash:///")
        verify(pane.isTrash)
        pane.open("file:///tmp")
        verify(!pane.isTrash)
        pane.setHidden(true); verify(pane.showHidden)
        pane.setHidden(false); verify(!pane.showHidden)
    }
}
