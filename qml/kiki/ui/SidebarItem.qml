import QtQuick
import ".." as Kiki

Rectangle {
    id: item
    property string icon: "folder"
    property color iconColor: item.active ? Kiki.Theme.accent : Kiki.Theme.chrome
    property string label: ""
    /// A picture to wear instead of the glyph (a location's own image): a path or a URL. One
    /// that cannot be loaded — moved, deleted — falls back to the glyph.
    property string image: ""
    readonly property string imageSource: image === "" ? "" : (/^[a-z]+:\/\//.test(image) ? image : "file://" + encodeURI(image))
    objectName: "sidebar-" + (label || icon)
    /// What the rail tooltip says; the label unless a shortcut is worth spelling out.
    property string tipText: label
    property bool active: false
    /// keyboard highlight (plan 23)
    property bool keyed: false
    property string detail: ""
    signal clicked()
    signal rightClicked()
    /// Set to accept dropped URIs; emits dropped(drop) with the DragEvent.
    property bool droppable: false
    /// A small ball at the icon's corner, transparent for none: green on a location kiki is
    /// connected to.
    property color dot: "transparent"
    readonly property bool hovered: hover.containsMouse
    /// Rail style: the icon alone, centred. Plain folders show their initial instead, since
    /// a column of identical folder glyphs tells you nothing.
    property bool compact: false
    signal dropped(var drop)
    /// How far the rail's icon grows under the pointer (Settings → General → "Rail icons grow").
    readonly property real railGrow: Kiki.Settings.view.railHover === false ? 1 : 1.35
    height: 30; radius: 9
    anchors.left: parent ? parent.left : undefined; anchors.right: parent ? parent.right : undefined
    anchors.leftMargin: compact ? 6 : 8; anchors.rightMargin: compact ? 6 : 8
    color: active ? Kiki.Theme.surface : (hover.containsMouse || keyed ? Qt.rgba(1, 1, 1, 0.04) : "transparent")
    border.width: keyed ? 1 : 0; border.color: Kiki.Theme.accent
    Row {
        anchors.verticalCenter: parent.verticalCenter
        x: item.compact ? Math.round((item.width - 16) / 2) : 8
        spacing: 10
        Item {
            objectName: "sidebar-glyph"
            width: glyph.width; height: glyph.height; anchors.verticalCenter: parent.verticalCenter
            // Rail style: the icon under the pointer comes forward, as there is no label to light.
            scale: item.compact && hover.containsMouse ? item.railGrow : 1
            Behavior on scale { NumberAnimation { duration: 110; easing.type: Easing.OutCubic } }
            Icon { id: glyph; name: item.icon; color: item.iconColor; visible: !picture.shown }
            RoundedImage {
                id: picture
                objectName: "sidebar-image"
                anchors.centerIn: parent; width: 20; height: 20; radius: 5
                source: item.imageSource; visible: shown
            }
            Rectangle {
                objectName: "sidebar-dot"
                visible: item.dot.a > 0
                anchors.right: parent.right; anchors.bottom: parent.bottom; anchors.rightMargin: -3; anchors.bottomMargin: -3
                width: 8; height: 8; radius: 4; color: item.dot
                border.width: 1.5; border.color: item.active ? Kiki.Theme.surface : Kiki.Theme.bg
            }
        }
        Text { visible: !item.compact; text: item.label; color: item.active ? Kiki.Theme.fg : Kiki.Theme.fgDim; font.family: Kiki.Theme.mono; font.pixelSize: Kiki.Theme.fontSize; elide: Text.ElideRight; width: item.width - 60 }
    }
    Tip { callout: true; visible: item.compact && hover.containsMouse && item.tipText !== ""; text: item.tipText }
    MouseArea { id: hover; anchors.fill: parent; hoverEnabled: true; acceptedButtons: Qt.LeftButton | Qt.RightButton; onClicked: mouse => mouse.button === Qt.RightButton ? item.rightClicked() : item.clicked() }
    DropArea {
        anchors.fill: parent; enabled: item.droppable; keys: ["text/uri-list"]
        onDropped: drop => item.dropped(drop)
        Rectangle { anchors.fill: parent; radius: 2; color: "transparent"; border.width: 1; border.color: Kiki.Theme.accent; visible: parent.containsDrag }
    }
}
