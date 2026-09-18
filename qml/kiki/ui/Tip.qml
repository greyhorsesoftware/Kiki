import QtQuick
import QtQuick.Controls as QC
import ".." as Kiki

// A tooltip in kiki's colours. `callout` is the sidebar's variant: a point on the left aimed at
// the icon, then a translucent pill, sitting to the right instead of below.
QC.ToolTip {
    id: tip
    property bool callout: false
    delay: 300
    leftPadding: callout ? 18 : 10; rightPadding: callout ? 14 : 10
    topPadding: 6; bottomPadding: 6
    x: !parent ? 0 : (callout ? parent.width + 4 : Math.round((parent.width - width) / 2))
    y: !parent ? 0 : (callout ? Math.round((parent.height - height) / 2) : parent.height + 4)

    readonly property color fill: callout
        ? Qt.rgba(Kiki.Theme.surface.r, Kiki.Theme.surface.g, Kiki.Theme.surface.b, 0.82)
        : Kiki.Theme.bgDark

    contentItem: Text {
        text: tip.text
        color: Kiki.Theme.fg
        font.family: Kiki.Theme.mono; font.pixelSize: 11
    }
    background: Item {
        // A square turned 45 degrees, half tucked behind the pill, makes the point.
        Rectangle {
            visible: tip.callout
            width: 10; height: 10; rotation: 45; radius: 1
            x: 4; y: Math.round((parent.height - height) / 2)
            color: tip.fill
        }
        Rectangle {
            x: tip.callout ? 8 : 0
            width: Math.max(0, parent.width - x); height: parent.height
            radius: tip.callout ? height / 2 : 3
            color: tip.fill
            border.width: tip.callout ? 0 : 1; border.color: Kiki.Theme.gutter
        }
    }
}
