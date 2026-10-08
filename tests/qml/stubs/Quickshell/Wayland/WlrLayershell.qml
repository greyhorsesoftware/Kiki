import QtQuick

// Quickshell's layer-shell window (`ui/ChooserWindow.qml`). A plain QtQuick Window stands in: a
// test has no compositor to put a layer surface on, and what the tests look at is the dialog
// inside, which is the same Item either way.
Window {
    property real implicitWidth: 0
    property real implicitHeight: 0
    property int layer: 0
    property int keyboardFocus: 0
    property string namespace: ""
    property int exclusiveZone: 0
    width: implicitWidth
    height: implicitHeight
}
