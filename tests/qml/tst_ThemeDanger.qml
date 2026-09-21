import QtQuick
import QtTest
import "../../qml/kiki" as Kiki

// Danger is a meaning, not a palette slot: a theme whose "red" is green (hackerman) must not
// turn "Move to Trash" green.
TestCase {
    name: "ThemeDanger"
    property color was
    function init() { was = Kiki.Theme.red }
    function cleanup() { Kiki.Theme.red = was }

    function test_a_red_red_is_used_as_it_is() {
        Kiki.Theme.red = "#f7768e"
        verify(Qt.colorEqual(Kiki.Theme.danger, "#f7768e"))
        Kiki.Theme.red = "#e06c75"
        verify(Qt.colorEqual(Kiki.Theme.danger, "#e06c75"))
        Kiki.Theme.red = "#ff5f87"                       // towards pink: still says stop
        verify(Qt.colorEqual(Kiki.Theme.danger, "#ff5f87"))
    }

    function test_a_green_red_is_not() {
        Kiki.Theme.red = "#50f872"                       // hackerman
        verify(Qt.colorEqual(Kiki.Theme.danger, "#f7768e"))
        verify(Kiki.Theme.isReddish(Kiki.Theme.danger))
    }

    function test_nor_is_a_grey_one() {
        Kiki.Theme.red = "#8a8a8a"                       // a monochrome theme: no hue to speak of
        verify(Qt.colorEqual(Kiki.Theme.danger, "#f7768e"))
    }

    // The same rule the other way about: "changed" is the theme's yellow, unless that yellow is
    // red (matte-black) — a modified file must not wear the colour of a conflicted one.
    function test_changed_is_never_the_colour_of_danger() {
        const y = Kiki.Theme.yellow
        Kiki.Theme.yellow = "#e5c07b"
        verify(Qt.colorEqual(Kiki.Theme.changed, "#e5c07b"), "a yellow yellow is used as it is")
        Kiki.Theme.yellow = "#b91c1c"                    // matte-black
        verify(!Kiki.Theme.isReddish(Kiki.Theme.changed), "a red yellow is not")
        verify(!Qt.colorEqual(Kiki.Format.gitColor({ state: "modified" }), Kiki.Format.gitColor({ state: "conflicted" })))
        Kiki.Theme.yellow = y
    }
}
