import QtQuick
import ".." as Kiki

// The one thing that says what the background is doing (plan 32). It sits at the bottom right of
// the window and is always there — dim and still when nothing is happening — because a control
// that comes and goes is one nobody can learn. Green and breathing while jobs run; red with a
// mark when one has failed, until the popup has been opened: a failure nobody saw is still news.
Item {
    id: orb
    /// "idle" | "running" | "failed"
    property string state_: Kiki.Jobs.orbState()
    property string tip: Kiki.Jobs.orbTip()
    /// Lit while the popup it opens is up.
    property bool open: false
    signal clicked()

    Connections { target: Kiki.Jobs; function onChanged() { orb.state_ = Kiki.Jobs.orbState(); orb.tip = Kiki.Jobs.orbTip() } function onSeenFailureChanged() { orb.state_ = Kiki.Jobs.orbState(); orb.tip = Kiki.Jobs.orbTip() } }

    // A palette's "green" is a slot, not a promise: some themes put a red there.
    readonly property color go: Kiki.Theme.isReddish(Kiki.Theme.green) ? "#34b354" : Kiki.Theme.green
    width: 28; height: Kiki.Theme.barHeight

    Rectangle {
        objectName: "activity-orb-hover"
        anchors.centerIn: parent; width: 22; height: 22; radius: 11
        color: area.containsMouse || orb.open ? Kiki.Theme.surface : "transparent"
    }
    Rectangle {
        id: dot
        objectName: "activity-orb-dot"
        anchors.centerIn: parent
        width: orb.state_ === "failed" ? 14 : 10; height: width; radius: width / 2
        color: orb.state_ === "failed" ? Kiki.Theme.danger : (orb.state_ === "running" ? orb.go : Kiki.Theme.gutter)
        // Breathing, not blinking: about 45% to full and back over two and a half seconds.
        SequentialAnimation on opacity {
            id: breathe
            running: orb.state_ === "running"
            loops: Animation.Infinite
            NumberAnimation { from: 1; to: 0.45; duration: 1250; easing.type: Easing.InOutSine }
            NumberAnimation { from: 0.45; to: 1; duration: 1250; easing.type: Easing.InOutSine }
        }
        // Left part-way through a breath when the last job ends, it would stay dim for ever.
        readonly property bool breathing: breathe.running
        onBreathingChanged: if (!breathing) opacity = 1
        Text { visible: orb.state_ === "failed"; anchors.centerIn: parent; text: "!"; color: "white"; font.family: Kiki.Theme.mono; font.pixelSize: 11; font.bold: true }
    }
    Tip { visible: area.containsMouse && !orb.open; text: orb.tip; y: -height - 4 }
    MouseArea { id: area; anchors.fill: parent; hoverEnabled: true; cursorShape: Qt.PointingHandCursor; onClicked: orb.clicked() }
}
