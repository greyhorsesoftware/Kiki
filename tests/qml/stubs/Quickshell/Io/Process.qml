import QtQuick
import KikiTest

// A process that is never run: it records the command it was asked for, so a test can say what
// the window tried to start, and `finish(code)` plays the exit back.
QtObject {
    id: proc
    property var command: []
    property bool running: false
    signal exited(int code, int status)
    onRunningChanged: if (running) Wire.started(proc.command)
    /// What a real Process does when the program ends: the test calls it in the program's place.
    function finish(code) { running = false; proc.exited(code, 0) }
}
