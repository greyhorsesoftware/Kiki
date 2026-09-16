import QtQuick

// Inert FileView: never loads anything.
QtObject {
    property string path: ""
    property bool watchChanges: false
    signal loaded()
    signal fileChanged()
    function text() { return "" }
    function reload() {}
}
