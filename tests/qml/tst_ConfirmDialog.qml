import QtQuick
import QtTest
import "../../qml/kiki/ui" as UI

TestCase {
    name: "ConfirmDialog"
    when: windowShown
    visible: true
    width: 800; height: 600
    Item { id: host; width: 800; height: 600; UI.ConfirmDialog { id: dlg } }
    property var answers: []

    function test_enter_confirms_and_escape_cancels() {
        answers = []
        dlg.ask({ title: "Delete?", message: "x", label: "Delete" }, yes => answers.push(yes))
        verify(dlg.visible)
        compare(dlg.confirmLabel, "Delete")
        keyClick(Qt.Key_Return)
        verify(!dlg.visible)
        dlg.ask({ title: "Again?", message: "y" }, yes => answers.push(yes))
        keyClick(Qt.Key_Escape)
        compare(JSON.stringify(answers), "[true,false]")
    }
    function test_backdrop_click_cancels() {
        answers = []
        dlg.ask({ title: "T", message: "m" }, yes => answers.push(yes))
        mouseClick(dlg, 5, 5)
        compare(JSON.stringify(answers), "[false]")
        verify(!dlg.visible)
    }
}
