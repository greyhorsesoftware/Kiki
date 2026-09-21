import QtQuick
import ".." as Kiki
import "../ui" as UI

/// A repository-root row's branch, where the one-letter badge goes (plan 15): the breadcrumb's
/// branch chip scaled to a row. One element says both which branch the project is on and how it
/// stands — the colour is the aggregate's, in exactly the badge's colours — because a folder
/// like `~/Projects` is in no repository itself and its rows have nothing else to go on.
Item {
    id: cap
    objectName: "git-capsule"
    /// The row's `git`, as `Format.gitCapsule` hands it over; null draws nothing.
    property var mark: null
    /// A selected row draws it in the bar's colour, as the letter badge does.
    property bool onBar: false
    /// The most it may take. The file's name has priority, so the caller gives it a share of the
    /// name column and a long branch elides inside that.
    property int maxWidth: 120
    readonly property color tint: onBar ? Kiki.Theme.bg : Kiki.Format.gitColor(mark)

    visible: !!mark
    implicitWidth: pill.width
    implicitHeight: pill.height
    width: implicitWidth
    height: implicitHeight

    Rectangle {
        id: pill
        // Shorter than a row, so nothing about the row's height changes.
        width: Math.min(cap.maxWidth, glyph.width + label.width + 13)
        height: 16
        radius: 2
        // Over the accent bar the surface colour is invisible, so the pill is a breath of the
        // bar's own background instead, and the text takes the badge's selected colour.
        color: cap.onBar ? Qt.rgba(Kiki.Theme.bg.r, Kiki.Theme.bg.g, Kiki.Theme.bg.b, 0.18) : Kiki.Theme.surface
        clip: true
        Row {
            anchors.centerIn: parent
            spacing: 4
            UI.Icon { id: glyph; name: "mirror"; size: 9; color: cap.tint; anchors.verticalCenter: parent.verticalCenter }
            Text {
                id: label
                objectName: "git-capsule-text"
                anchors.verticalCenter: parent.verticalCenter
                width: Math.min(implicitWidth, cap.maxWidth - 17)
                elide: Text.ElideRight
                text: cap.mark ? cap.mark.branch : ""
                color: cap.tint
                font.family: Kiki.Theme.mono; font.pixelSize: 10
            }
        }
    }
}
