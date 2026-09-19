import QtQuick
import ".." as Kiki

// What a pane shows when its folder could not be listed — a server that does not answer, a name
// that does not resolve, a folder that has gone. Without it a failure looks like an empty folder.
// Lay it over the pane's view.
Rectangle {
    id: pe
    property Kiki.Pane pane
    readonly property string error: pane ? pane.listing.error : ""
    /// The daemon's words, less its prefixes; "gone" is the listing's own word for a folder that
    /// was deleted while it was open.
    readonly property string message: error === "gone" ? "This folder no longer exists." : error
    readonly property bool remote: !!pane && !/^(file|trash):/.test(pane.uri)
    signal retry()
    objectName: "pane-error"
    visible: error !== ""
    color: Kiki.Theme.bg
    MouseArea { anchors.fill: parent; acceptedButtons: Qt.AllButtons; onWheel: wheel => wheel.accepted = true }   // nothing underneath is there to be clicked
    Column {
        anchors.centerIn: parent; width: Math.min(parent.width - 48, 460); spacing: 12
        Icon { anchors.horizontalCenter: parent.horizontalCenter; name: "warn"; size: 28; color: Kiki.Theme.yellow }
        Text {
            width: parent.width; horizontalAlignment: Text.AlignHCenter
            text: pe.remote ? "Could not connect" : "Could not open this folder"
            color: Kiki.Theme.fg; font.family: Kiki.Theme.mono; font.pixelSize: Kiki.Theme.fontSize + 1; font.bold: true
        }
        Text {
            objectName: "pane-error-message"
            width: parent.width; horizontalAlignment: Text.AlignHCenter; wrapMode: Text.WordWrap
            text: pe.message; color: Kiki.Theme.fgDim; font.family: Kiki.Theme.mono; font.pixelSize: Kiki.Theme.fontSize
        }
        Button { objectName: "pane-error-retry"; anchors.horizontalCenter: parent.horizontalCenter; text: "Try again"; onClicked: pe.retry() }
    }
}
