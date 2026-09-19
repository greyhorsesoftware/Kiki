import QtQuick
import QtTest
import "../../qml/kiki/ui" as UI

// Which pane has the focus, side by side: the one that was last pressed — white space included —
// and never merely the one under the pointer. Without taking the click from what is underneath.
TestCase {
    id: tc
    name: "PaneFocus"
    when: windowShown
    visible: true
    width: 600; height: 300

    property string focused: "left"
    property int rowClicks: 0

    Row {
        anchors.fill: parent
        Rectangle {
            id: left
            width: 300; height: 300; color: "#202020"
            UI.PaneFocus { anchors.fill: parent; z: 100; onWanted: tc.focused = "left" }
        }
        Rectangle {
            id: right
            width: 300; height: 300; color: "#303030"
            UI.PaneFocus { id: rightFocus; anchors.fill: parent; z: 100; onWanted: tc.focused = "right" }
            // A row, the way a listing has them: its own MouseArea, which takes the click.
            Rectangle {
                id: row
                width: parent.width; height: 28; color: "#404040"
                MouseArea { id: rowArea; anchors.fill: parent; hoverEnabled: true; onClicked: tc.rowClicks++ }
            }
        }
    }

    function init() { mouseMove(left, 10, 10); tc.focused = "left"; tc.rowClicks = 0; rightFocus.active = true }

    // The pointer passing over a pane is not a choice: reaching across one pane for the toolbar
    // must not change which pane the toolbar is about to act on.
    function test_the_pointer_passing_over_does_not_move_the_focus() {
        mouseMove(right, 150, 200)
        wait(60)
        compare(tc.focused, "left")
        mouseMove(left, 150, 200)
        mouseMove(right, 10, 10)
        wait(60)
        compare(tc.focused, "left")
    }

    function test_a_click_in_white_space_takes_the_focus() {
        mouseClick(right, 150, 200)               // white space, below the row
        compare(tc.focused, "right")
        mouseClick(left, 150, 200)
        compare(tc.focused, "left")
    }

    function test_a_click_on_a_row_focuses_the_pane_and_still_reaches_the_row() {
        mouseMove(right, 150, 10)
        tc.focused = "left"
        mouseClick(row, 150, 10)
        compare(tc.focused, "right")
        compare(tc.rowClicks, 1)
    }

    // Lying on top must not cost what is underneath its hover: row highlights, tooltips, the
    // divider's cursor all depend on it.
    function test_hover_still_reaches_what_is_underneath() {
        mouseMove(row, 150, 10)
        tryVerify(() => rowArea.containsMouse)
        mouseMove(right, 150, 200)
        tryVerify(() => !rowArea.containsMouse)
        mouseMove(row, 150, 12)
        tryVerify(() => rowArea.containsMouse)
    }

    function test_a_right_click_counts_too() {
        mouseMove(right, 150, 200)
        tc.focused = "left"
        mouseClick(right, 150, 200, Qt.RightButton)
        compare(tc.focused, "right")
    }

    // With one pane there is nothing to choose between.
    function test_inactive_it_asks_for_nothing() {
        rightFocus.active = false
        mouseMove(right, 150, 200)
        wait(50)
        mouseClick(right, 150, 200)
        compare(tc.focused, "left")
    }
}
