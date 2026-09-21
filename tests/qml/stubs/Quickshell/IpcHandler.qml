import QtQuick

// The IPC end that `qs ipc call` reaches. A test calls the functions on it directly — they are
// ordinary QML functions — so the stand-in only has to hold the target name.
QtObject {
    property string target: ""
}
