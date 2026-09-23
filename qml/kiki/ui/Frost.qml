import QtQuick
import QtQuick.Effects
import ".." as Kiki

// Frosted glass under something that floats over the window — a menu, the toast, the activity
// card, the chooser's scrim. What is behind shows through, blurred and tinted, so a floating
// thing reads as floating over THIS place rather than as a flat box that could be anywhere.
// Fill the box with it (`anchors.fill: parent`) and give the box no colour of its own.
//
// It reads `Kiki.Theme.behind` — the window's content, set by the shell — through a
// ShaderEffectSource clipped to its own rectangle. The content must not contain the frost
// itself (the shell keeps its floating layers as siblings of the content), and under a test,
// where no shell has set `behind`, it is the tint alone: the flat look it replaces.
Item {
    id: frost
    property real radius: 2
    /// The glass's colour and how much of it: the theme's dark ground at 72 %.
    property color tint: Kiki.Theme.bgDark
    property real tintOpacity: 0.76
    property real blur: 1.0
    readonly property Item behind: Kiki.Theme.behind
    readonly property bool live: !!behind && width > 0 && height > 0

    // Where this rectangle is in the content's coordinates, read again whenever anything that
    // can move it does — the box the frost fills is placed by assignment, not by anchors.
    property int _tick: 0
    function bump() { _tick++ }
    Connections { target: frost.parent; ignoreUnknownSignals: true; function onXChanged() { frost.bump() } function onYChanged() { frost.bump() } }
    Connections { target: frost.parent ? frost.parent.parent : null; ignoreUnknownSignals: true; function onXChanged() { frost.bump() } function onYChanged() { frost.bump() } }
    readonly property rect where: {
        _tick
        if (!live) return Qt.rect(0, 0, 0, 0)
        const p = frost.mapToItem(behind, 0, 0)
        return Qt.rect(p.x, p.y, width, height)
    }

    // The grab, as a layer with the blur as its effect. (A MultiEffect given a ShaderEffectSource
    // as its `source` drew nothing at all, under Quickshell and under a test alike; as a
    // `layer.effect` on an item that holds the grab, it draws.)
    Item {
        id: mask
        anchors.fill: parent; visible: false
        layer.enabled: true; layer.smooth: true
        Rectangle { anchors.fill: parent; radius: frost.radius; color: "black"; antialiasing: true }
    }
    Item {
        id: glass
        anchors.fill: parent
        visible: frost.live
        layer.enabled: frost.live
        layer.smooth: true
        layer.effect: MultiEffect {
            blurEnabled: true; blur: frost.blur; blurMax: 64; blurMultiplier: 2.0
            maskEnabled: true; maskSource: mask; maskThresholdMin: 0.5; maskSpreadAtMin: 1.0
        }
        ShaderEffectSource {
            anchors.fill: parent
            sourceItem: frost.live ? frost.behind : null
            sourceRect: frost.where
            // Grabbed at a sixth of its size: a blur is only as wide as its kernel, and 48 px
            // of kernel on a 400 px grab is a soft edge, not glass. A sixth and scaled back
            // up, the same kernel is six times the blur for a fraction of the work.
            textureSize: Qt.size(Math.max(1, Math.round(width / 6)), Math.max(1, Math.round(height / 6)))
            live: true; recursive: false; hideSource: false
            smooth: true
        }
    }
    Rectangle {
        anchors.fill: parent; radius: frost.radius
        color: frost.tint; opacity: frost.live ? frost.tintOpacity : 1
        antialiasing: true
    }
}
