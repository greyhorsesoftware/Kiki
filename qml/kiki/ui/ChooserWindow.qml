import QtQuick
import Quickshell
import Quickshell.Wayland
import Quickshell.Hyprland
import ".." as Kiki

// The file chooser's own window (docs/0.5.0/11-chooser-window.md). A layer surface on the
// overlay layer, not a toplevel: a chooser belongs to the application that asked for it, and on
// Wayland an ordinary window cannot put itself above that application — it was painted inside
// the file manager's window instead, which is behind the asker and on whatever workspace the
// file manager happens to be on (owner, 2026-10-04: "did a screenshot, clicked save as and the
// panel showed up behind the window I was in"). An overlay surface is above every window,
// fullscreen ones included, by the protocol rather than by a compositor rule.
//
// The surface exists only while a chooser is up: `visible` follows the dialog, so the exclusive
// keyboard focus — which a chooser must have to be typed into — is held then and at no other
// time. Unanchored, which the protocol centres on both axes, and `exclusiveZone: 0` so it
// reserves no room from anything else.
// `WlrLayershell` rather than a `PanelWindow` carrying the same thing as attached properties:
// the two are the same surface, and a window type reads plainly and can be stood in for by a
// plain Window where the leaf tests load the shell without a compositor under them.
WlrLayershell {
    id: chooserWin
    /// The dialog itself: the same `PortalDialog` that used to fill the window, with the same
    /// `open`, `pick`, `finish` and `visible` — this window only carries it.
    property alias dialog: dialog

    property string home: ""
    property var favorites: []
    property var locations: []
    /// The window's DragGhost, so a drag out of the chooser carries a picture like any other.
    property var ghost: null
    /// Where a listener's answer goes (ShellChooser sets its `chooserFinished`).
    property var chooser: ({ answered: function (token, uris) {} })
    /// Answered or cancelled: the focus goes back inside the file manager's window, which is an
    /// item change there and not a raise — the compositor hands the keyboard back to whoever had
    /// it before this surface, which for another application's dialog is that application.
    signal dismissed()

    layer: WlrLayer.Overlay
    keyboardFocus: WlrKeyboardFocus.Exclusive
    namespace: "kiki-chooser"
    exclusiveZone: 0
    color: "transparent"
    /// Whether a chooser is up — this window's own state, and the dialog's visibility follows
    /// from it rather than the other way about. An Item's `visible` is its parent's too, so a
    /// window bound to `dialog.visible` hid itself with the first answer and then held the
    /// dialog invisible for ever: the second chooser opened on to nothing (found 2026-10-04,
    /// three choosers in a row under sway).
    property bool up: false
    visible: up

    // As big as 860 × 560 and no bigger than the screen less a margin: the box it used to draw
    // inside the window is now the window, so the size that was the box's is this surface's.
    implicitWidth: Math.min(860, (screen ? screen.width : 1280) - 48)
    implicitHeight: Math.min(560, (screen ? screen.height : 720) - 48)

    /// The screen the person is looking at, read when a chooser opens rather than bound: a
    /// binding would move the dialog to another monitor mid-choice if the focus wandered.
    /// Hyprland is asked where the focus is; off Hyprland (sway under the tests, anything else)
    /// the module answers nothing and Quickshell's own default screen is used, which on one
    /// monitor is the only answer there is. On two monitors off Hyprland the chooser opens on
    /// the first.
    function onFocusedScreen() {
        const m = Hyprland.focusedMonitor
        if (!m || !m.name) return
        const s = Quickshell.screens.find(scr => scr.name === m.name)
        if (s) screen = s
    }
    /// A chooser another application asked for, through the portal. The surface comes up first:
    /// the dialog inside it cannot be visible before the window carrying it is.
    function show(r) { onFocusedScreen(); up = true; dialog.open(r) }
    /// The same chooser asked by kiki itself: `cb(uris)` gets the answer, nothing goes to the bus.
    function pick(r, cb) { onFocusedScreen(); up = true; dialog.pick(r, cb) }

    PortalDialog {
        id: dialog
        objectName: "portal"
        ownWindow: true
        anchors.fill: parent
        ghost: chooserWin.ghost
        home: chooserWin.home
        favorites: chooserWin.favorites
        locations: chooserWin.locations
        chooser: chooserWin.chooser
        onClosed: { chooserWin.up = false; chooserWin.dismissed() }
    }
}
