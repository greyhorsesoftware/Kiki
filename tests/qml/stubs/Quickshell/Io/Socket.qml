import QtQuick

// Inert Socket: never connects, so the Daemon singleton compiles but stays idle in tests.
QtObject {
    property string path: ""
    property bool connected: false
    property QtObject parser: null
    signal connectionStateChanged()
    function write(data) {}
    function flush() {}
}
