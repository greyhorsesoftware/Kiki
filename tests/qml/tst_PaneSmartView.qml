import QtQuick
import QtTest
import "../../qml/kiki" as Kiki
import KikiTest

// The view a folder opens in when nothing is remembered about it (plan 24's rule, plan 27's
// answer): a picture folder opens in **Gallery**. D18 — plan 24 still says Icon, and the code has
// always said Gallery; the code is right, and this is what pins it.
//
// Two ways to be a picture folder: by name, or by what is in it — at least twelve rows held and
// at least sixty per cent of the first two hundred of them images or video. Anything else opens
// in the default view, and a folder that is remembered opens the way it was left.
TestCase {
    id: tc
    name: "PaneSmartView"

    property var fake: null
    property var pane: null
    Component { id: fakeC; FakeDaemon {} }
    Component { id: paneC; Kiki.Pane {} }

    /// `n` rows of which `media` are pictures (or video, with `kind`), the rest plain text, plus
    /// `dirs` folders. Named so that name order is the order they are made in.
    function folder(n, media, kind, dirs) {
        let rows = []
        for (let i = 0; i < n; i++) {
            const tag = ("000" + i).slice(-3)
            rows.push(i < media ? fake.file("a" + tag + ".img", { kind: kind || "image" }) : fake.file("b" + tag + ".txt"))
        }
        for (let j = 0; j < (dirs || 0); j++) rows.push(fake.dir("d" + j))
        return rows
    }

    function init() {
        Wire.reset(); Wire.connectAll()
        Kiki.Settings.view = Object.assign({}, Kiki.Settings.view, { "default": "list", rememberPerFolder: true })
        Kiki.Settings.viewPrefs = ({})
        fake = fakeC.createObject(tc)
        fake.tree = ({})
        pane = paneC.createObject(tc); pane.listing.daemon = fake
        Wire.reset()
    }
    function cleanup() { pane.destroy(); fake.destroy() }

    /// Puts `rows` at `uri`, opens it, and answers with the view the pane chose.
    function opens(uri, rows) {
        const t = fake.tree; t[uri] = rows; fake.tree = t
        pane.open(uri)
        return pane.view
    }

    // ---------------------------------------------------------------- by name
    function test_a_folder_with_a_picture_name_opens_in_gallery() {
        // Every name on the list, whatever it is spelled like, and whether or not it holds
        // anything: the name is the whole of the rule.
        for (const name of pane.pictureNames) {
            compare(opens("file:///home/t/" + encodeURIComponent(name), []), "gallery", name)
            compare(opens("file:///home/t/x/" + encodeURIComponent(name.toUpperCase()), []), "gallery", name.toUpperCase())
        }
        // "camera roll" arrives percent-encoded and still counts, which is the one that would
        // quietly stop working if the name were read straight off the URI.
        verify(pane.pictureNames.indexOf("camera roll") >= 0)
        compare(opens("file:///home/t/y/camera%20roll", []), "gallery")
        // A name that is not on the list is not a picture folder.
        compare(opens("file:///home/t/Documents", []), "list")
    }

    // ---------------------------------------------------------------- by contents
    function test_a_folder_of_mostly_pictures_opens_in_gallery() {
        compare(opens("file:///home/t/snaps", folder(20, 15)), "gallery")
        compare(opens("file:///home/t/clips", folder(12, 12, "video")), "gallery", "video counts as much as stills")
    }

    function test_a_mixed_folder_opens_in_the_default_view() {
        compare(opens("file:///home/t/mixed", folder(20, 6)), "list")
        // Sixty per cent exactly is a picture folder; a shade under is not.
        compare(opens("file:///home/t/sixty", folder(20, 12)), "gallery")
        compare(opens("file:///home/t/under", folder(20, 11)), "list")
        // Folders count against it: twelve pictures among eight subfolders is not a gallery.
        compare(opens("file:///home/t/tree", folder(12, 12, "image", 8)), "gallery")
        compare(opens("file:///home/t/deep", folder(12, 12, "image", 9)), "list")
    }

    function test_a_handful_of_pictures_is_not_a_picture_folder() {
        // Under twelve rows held, nothing is concluded — a folder of three photographs is not
        // what Gallery is for, and the name rule is still there for one that is.
        compare(opens("file:///home/t/few", folder(11, 11)), "list")
        compare(opens("file:///home/t/twelve", folder(12, 12)), "gallery")
    }

    function test_only_the_first_two_hundred_rows_are_looked_at() {
        // 300 rows, the first 200 by name pictures: a picture folder.
        compare(opens("file:///home/t/front", folder(300, 200)), "gallery")
        // The same 300 rows the other way round: the pictures are past where the rule looks.
        let back = []
        for (let i = 0; i < 300; i++) {
            const tag = ("000" + i).slice(-3)
            back.push(i < 200 ? fake.file("a" + tag + ".txt") : fake.file("b" + tag + ".img", { kind: "image" }))
        }
        compare(opens("file:///home/t/back", back), "list")
    }

    // ---------------------------------------------------------------- memory wins
    function test_a_remembered_view_beats_the_contents() {
        Kiki.Settings.viewPrefs = ({ "file:///home/t/snaps": { view: "icon", sort: "name", order: "asc", hidden: false } })
        compare(opens("file:///home/t/snaps", folder(20, 15)), "icon")
        compare(pane.hasPref, true)
    }

    function test_choosing_a_view_in_a_picture_folder_is_what_it_opens_in_next_time() {
        compare(opens("file:///home/t/snaps", folder(20, 15)), "gallery")
        compare(Wire.count("SetViewPref"), 0, "the pane's own guess is not a choice to remember")
        pane.view = "list"                                   // the user says otherwise
        compare(Wire.count("SetViewPref"), 1)
        opens("file:///home/t/elsewhere", [])
        compare(opens("file:///home/t/snaps", folder(20, 15)), "list", "and it stays said")
    }

    // ---------------------------------------------------------------- and where it is off
    function test_side_by_side_makes_no_guess() {
        // `rememberViews` off is the half-window: a folder must not open in Gallery there, just
        // as a folder remembered as Gallery must not.
        pane.rememberViews = false
        pane.view = "list"
        compare(opens("file:///home/t/Pictures", folder(20, 15)), "list")
    }

    function test_the_default_view_is_what_an_ordinary_folder_opens_in() {
        Kiki.Settings.view = Object.assign({}, Kiki.Settings.view, { "default": "icon" })
        compare(opens("file:///home/t/mixed", folder(20, 6)), "icon")
        compare(opens("file:///home/t/snaps", folder(20, 15)), "gallery", "a picture folder still overrides it")
    }
}
