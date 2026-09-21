import QtQuick
import ".." as Kiki

// What a pane on a server shows between asking for a folder and the first of it arriving — the
// sign-in, the host that is slow to answer, the long listing. Without it that stretch looked
// like an empty folder, or like nothing having happened at all (owner, 2026-09-21: "so user
// knows something is happening"). Lay it over the pane's view, under PaneError: once there is
// an error, that is what is said instead.
Rectangle {
    id: pc
    property Kiki.Pane pane
    readonly property bool remote: !!pane && pane.uri !== "" && !/^(file|trash):/.test(pane.uri)
    /// Asked for, nothing here yet, nothing gone wrong yet.
    readonly property bool waiting: remote && !pane.listing.done && pane.listing.count === 0 && pane.listing.error === ""
    /// A server that answers at once should not flash a message nobody can read: it is shown
    /// only once the wait has gone on long enough to be wondered about.
    property int delay: 300
    property bool shown: false
    readonly property string host: pane ? Kiki.Format.authority(pane.uri) : ""
    objectName: "pane-connecting"
    visible: shown
    color: Kiki.Theme.bg
    onWaitingChanged: { if (waiting) wait.restart(); else { wait.stop(); shown = false } }
    Component.onCompleted: if (waiting) wait.restart()
    Timer { id: wait; interval: pc.delay; onTriggered: pc.shown = pc.waiting }
    MouseArea { anchors.fill: parent; acceptedButtons: Qt.AllButtons; onWheel: wheel => wheel.accepted = true }   // nothing underneath is there to be clicked
    Column {
        anchors.centerIn: parent; width: Math.min(parent.width - 48, 460); spacing: 12
        // A ring, and a bead going round it: drawn from what the theme has, since the icon set
        // has nothing that turns.
        Item {
            objectName: "pane-connecting-spinner"
            anchors.horizontalCenter: parent.horizontalCenter; width: 28; height: 28
            Rectangle { anchors.fill: parent; radius: width / 2; color: "transparent"; border.width: 2; border.color: Kiki.Theme.gutter }
            Item {
                anchors.fill: parent
                Rectangle { width: 6; height: 6; radius: 3; color: Kiki.Theme.accent; anchors.horizontalCenter: parent.horizontalCenter; y: -2 }
                RotationAnimator on rotation { from: 0; to: 360; duration: 900; loops: Animation.Infinite; running: pc.visible }
            }
        }
        Text {
            objectName: "pane-connecting-text"
            width: parent.width; horizontalAlignment: Text.AlignHCenter; elide: Text.ElideMiddle
            text: pc.host ? "Connecting to " + pc.host + "…" : "Connecting…"
            color: Kiki.Theme.fg; font.family: Kiki.Theme.mono; font.pixelSize: Kiki.Theme.fontSize + 1; font.bold: true
        }
    }
}
