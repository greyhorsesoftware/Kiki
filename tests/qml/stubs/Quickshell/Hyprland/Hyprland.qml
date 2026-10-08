pragma Singleton
import QtQuick

// Hyprland's IPC, which the chooser asks which monitor the person is looking at. Off Hyprland —
// a test, sway — the real one answers nothing after a warning, which is what this does.
QtObject {
    readonly property var focusedMonitor: null
}
