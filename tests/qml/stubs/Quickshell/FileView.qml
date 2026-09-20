import QtQuick

// Inert FileView: never loads anything. It counts what it is asked, so a test can tell what a
// change to one file makes the theme read again.
QtObject {
    property string path: ""
    property bool watchChanges: false
    property bool printErrors: true
    property int reloads: 0
    signal loaded()
    signal fileChanged()
    function text() { return "" }
    function reload() { reloads++ }
}
