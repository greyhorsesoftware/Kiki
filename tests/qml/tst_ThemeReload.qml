import QtQuick
import QtTest
import "../../qml/kiki" as Kiki

// How kiki notices an Omarchy theme switch (plan 30, W4). Omarchy deletes `current/theme` and moves
// a new folder into its place, so a watch on any file inside is dead after the first switch; it
// then writes `theme.name` in place. That file is the one thing watched, and its changing is what
// makes the rest be read again. There used to be a timer re-reading four files every two seconds.
TestCase {
    name: "ThemeReload"

    function reads() {
        const t = Kiki.Theme
        return [t.omarchy.reloads, t.iconsFile.reloads, t.legacy.reloads, t.olderLegacy.reloads]
    }

    function test_only_the_theme_name_is_watched() {
        const t = Kiki.Theme
        verify(t.themeName.watchChanges)
        for (const f of [t.omarchy, t.iconsFile, t.legacy, t.olderLegacy, t.gtk3, t.gtk4])
            verify(!f.watchChanges, f.path + " is inside a folder Omarchy replaces: a watch there dies")
    }

    function test_a_switch_reads_everything_again_and_a_second_one_does_too() {
        const before = reads()
        Kiki.Theme.themeName.fileChanged()
        compare(reads(), before.map(n => n + 1))
        Kiki.Theme.themeName.fileChanged()
        compare(reads(), before.map(n => n + 2))
    }

    function test_nothing_is_read_while_nothing_changes() {
        const before = reads()
        wait(2300)       // longer than the old timer's interval
        compare(reads(), before)
        verify(Kiki.Theme.poll === undefined, "the polling timer is gone")
    }

    // The files that need not exist — the older palette, GTK's settings — are read without a
    // warning in the log on every start.
    function test_optional_files_are_read_quietly() {
        const t = Kiki.Theme
        for (const f of [t.legacy, t.olderLegacy, t.gtk3, t.gtk4, t.iconsFile, t.omarchy, t.themeName]) verify(!f.printErrors, f.path)
    }
}
