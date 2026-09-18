import QtQuick
import ".." as Kiki

// Narrows the folder you are looking at: same view, same sort, fewer rows. Enter hands the
// text to the global search, which is the other half of plan 12.
Rectangle {
    id: bar
    property Kiki.Pane pane
    property int total: 0            // rows before filtering, for the "n of m" count
    property alias text: input.text
    signal promote(string text)
    signal closed()

    height: 34
    color: Kiki.Theme.bg
    Rectangle { anchors.bottom: parent.bottom; width: parent.width; height: 1; color: Kiki.Theme.line }

    function focusInput() { input.forceActiveFocus(); input.selectAll() }
    function clear() { input.text = "" }

    Icon {
        id: glass
        x: 12; anchors.verticalCenter: parent.verticalCenter
        name: "search"; size: 13; color: input.activeFocus ? Kiki.Theme.accent : Kiki.Theme.muted
    }
    TextInput {
        id: input
        x: 32; width: Math.max(0, count.x - x - 12); height: parent.height
        verticalAlignment: TextInput.AlignVCenter; clip: true
        color: Kiki.Theme.fg; font.family: Kiki.Theme.mono; font.pixelSize: Kiki.Theme.fontSize
        selectionColor: Kiki.Theme.accent
        onTextChanged: debounce.restart()
        onAccepted: bar.promote(text)
        // Esc clears first, then closes, so it never loses the filter and the bar in one press.
        Keys.onEscapePressed: { if (text.length) text = ""; else bar.closed() }
        Text {
            visible: !input.text.length
            anchors.verticalCenter: parent.verticalCenter
            text: "Filter this folder"; color: Kiki.Theme.muted; font: input.font
        }
    }
    Text {
        id: count
        anchors.right: closeBtn.left; anchors.rightMargin: 8; anchors.verticalCenter: parent.verticalCenter
        text: bar.pane && bar.pane.filterText ? bar.pane.listing.count + " of " + bar.total : ""
        color: Kiki.Theme.muted; font.family: Kiki.Theme.mono; font.pixelSize: 11
    }
    ToggleButton {
        id: closeBtn
        anchors.right: parent.right; anchors.rightMargin: 4; anchors.verticalCenter: parent.verticalCenter
        icon: "x"; tip: "Close filter (Esc)"
        onClicked: bar.closed()
    }
    Timer { id: debounce; interval: Kiki.Settings.timers.searchDebounceMs; onTriggered: if (bar.pane) bar.pane.setFilter(input.text) }
}
