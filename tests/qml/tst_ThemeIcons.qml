import QtQuick
import QtTest
import "../../qml/kiki" as Kiki
import KikiTest

// Theme's icon cache (plan 28). The daemon answers what file an icon name maps to in the current
// theme; without a cache in front of it every rebuilt delegate waited a round trip and showed the
// wrong picture until the answer came — a visible flicker in a folder with many rows.
TestCase {
    id: tc
    name: "ThemeIcons"

    property var fake: null
    Component { id: fakeC; FakeDaemon {} }

    function init() {
        Wire.reset()
        fake = fakeC.createObject(tc)
        Kiki.Theme.daemon = fake
        // Naming a theme is also what empties the cache, so each test starts clean.
        Kiki.Theme.iconTheme = "Alpha"
        fake.reset()
    }
    function cleanup() {
        Kiki.Theme.daemon = null
        Kiki.Theme.iconTheme = ""
        fake.destroy()
    }

    // The first ask goes to the daemon and answers "" for now; the second is answered on the spot.
    function test_a_name_is_asked_for_once_then_answered_instantly() {
        let got = ""
        const first = Kiki.Theme.iconPath("folder", 16, p => got = p)
        compare(first, "")
        compare(fake.count("Icon"), 1)
        compare(fake.last("Icon").fields.theme, "Alpha")
        compare(fake.last("Icon").fields.size, 16)
        compare(got, "file:///icons/Alpha/16/folder.png")

        const second = Kiki.Theme.iconPath("folder", 16, () => {})
        compare(second, "file:///icons/Alpha/16/folder.png")
        compare(fake.count("Icon"), 1)       // no second round trip
    }

    // Size is part of the key: a 16 px folder and a 96 px folder are different files.
    function test_each_size_is_its_own_entry() {
        Kiki.Theme.iconPath("folder", 16, () => {})
        Kiki.Theme.iconPath("folder", 96, () => {})
        compare(fake.count("Icon"), 2)
        compare(Kiki.Theme.iconPath("folder", 96, () => {}), "file:///icons/Alpha/96/folder.png")
    }

    // Thirty rows appearing at once ask for the same handful of names. One request each.
    function test_callers_waiting_on_the_same_name_share_one_request() {
        fake.defer = true
        const answers = []
        for (let i = 0; i < 5; i++) compare(Kiki.Theme.iconPath("text-x-generic", 16, p => answers.push(p)), "")
        compare(fake.count("Icon"), 1)
        fake.flush()
        compare(answers.length, 5)
        for (const a of answers) compare(a, "file:///icons/Alpha/16/text-x-generic.png")
        fake.defer = false
        compare(Kiki.Theme.iconPath("text-x-generic", 16, () => {}), "file:///icons/Alpha/16/text-x-generic.png")
        compare(fake.count("Icon"), 1)
    }

    // A caller that throws (its delegate was destroyed while the request was in flight) must not
    // cost the callers queued behind it their answer: at first launch that left live rows on
    // kiki's own icon.
    function test_a_failing_caller_does_not_starve_the_others() {
        fake.defer = true
        const answers = []
        Kiki.Theme.iconPath("folder", 16, p => answers.push(p))
        Kiki.Theme.iconPath("folder", 16, () => { throw new TypeError("delegate is gone") })
        Kiki.Theme.iconPath("folder", 16, p => answers.push(p))
        ignoreWarning(/a waiting caller failed/)
        fake.flush()
        compare(answers.length, 2)
        for (const a of answers) compare(a, "file:///icons/Alpha/16/folder.png")
        fake.defer = false
    }

    // A new icon theme means every answer we hold is for the wrong theme.
    function test_a_theme_change_empties_the_cache() {
        Kiki.Theme.iconPath("folder", 16, () => {})
        compare(Kiki.Theme.iconPath("folder", 16, () => {}), "file:///icons/Alpha/16/folder.png")
        Kiki.Theme.iconTheme = "Beta"
        compare(Kiki.Theme.iconPath("folder", 16, () => {}), "")
        compare(fake.count("Icon"), 2)
        compare(fake.last("Icon").fields.theme, "Beta")
        compare(Kiki.Theme.iconPath("folder", 16, () => {}), "file:///icons/Beta/16/folder.png")
    }

    // An answer that arrives after the theme has moved on is not ours to cache or to show.
    function test_a_late_answer_for_a_theme_we_have_left_is_dropped() {
        fake.defer = true
        let got = "unset"
        Kiki.Theme.iconPath("folder", 16, p => got = p)
        Kiki.Theme.iconTheme = "Beta"
        fake.flush()
        compare(got, "unset")                                   // the callback never fired
        compare(Kiki.Theme.iconPath("folder", 16, () => {}), "") // and nothing was cached under Beta
    }

    // Without a theme there is nothing to resolve: the drawn icon is what shows.
    function test_no_icon_theme_means_no_request() {
        Kiki.Theme.iconTheme = ""
        fake.reset()
        compare(Kiki.Theme.iconPath("folder", 16, () => {}), "")
        compare(fake.count("Icon"), 0)
    }
}
