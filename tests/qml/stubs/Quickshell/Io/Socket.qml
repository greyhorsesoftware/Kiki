import QtQuick
import KikiTest

// Recording socket: never connects, but every frame the shell writes is kept by Wire so a test
// can assert on it, and Wire can write back through the parser.
QtObject {
    id: sock
    property string path: ""
    property bool connected: false
    property QtObject parser: null
    signal connectionStateChanged()
    Component.onCompleted: Wire.register(sock)
    function write(data) {
        const lines = String(data).split("\n")
        for (let i = 0; i < lines.length; i++) if (lines[i]) Wire.record(lines[i])
    }
    function flush() {}
}
