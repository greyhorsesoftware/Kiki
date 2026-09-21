import QtQuick
import QtTest
import "../../qml/kiki" as Kiki
import "../../qml/kiki/views" as Views
import KikiTest

// Git status as the views draw it (plan 15): a one-letter badge after the name in list view, a
// coloured dot on the icon tile, the same letters and the same colours in both, a folder wearing
// what its subtree adds up to, and ignored rows stepped back rather than marked.
//
// The letters and the colours are plan 15's UI table. The colours come out of the theme — a badge
// is part of the window — so the table is asserted against the palette, not against six hex
// strings that would go on passing after the theme had moved underneath them.
TestCase {
    id: tc
    name: "GitBadges"
    when: windowShown
    visible: true
    width: 700; height: 400

    property var fake: null
    property var pane: null
    property color wasYellow

    Component { id: fakeC; FakeDaemon {} }
    Component { id: paneC; Kiki.Pane {} }

    Item {
        anchors.fill: parent
        Views.ListPane { id: list; anchors.fill: parent; pane: tc.pane }
        Views.IconPane { id: icons; anchors.fill: parent; pane: tc.pane; visible: false }
    }

    /// One row in each state plan 15 names, and a folder carrying its subtree's.
    function init() {
        Kiki.Settings.viewPrefs = ({})
        Kiki.Settings.git = ({ enabled: true, showIgnored: "dim", folders: "aggregate" })
        wasYellow = Kiki.Theme.yellow
        Wire.reset()
        fake = fakeC.createObject(tc)
        fake.tree = { "file:///home/t/repo": [
            fake.dir("src", { git: { state: "modified", staged: false } }),
            fake.file("add.txt", { git: { state: "added", staged: true } }),
            fake.file("conf.txt", { git: { state: "conflicted", staged: false } }),
            fake.file("del.txt", { git: { state: "deleted", staged: false } }),
            fake.file("ign.txt", { git: { state: "ignored", staged: false } }),
            fake.file("mod.txt", { git: { state: "modified", staged: false } }),
            fake.file("new.txt", { git: { state: "untracked", staged: false } }),
            fake.file("ok.txt", { git: { state: "clean", staged: false } }),
            fake.file("ren.txt", { git: { state: "renamed", staged: false } }),
            fake.file("plain.txt"),                       // outside a repository: no git at all
            // Folders that are repositories of their own (plan 15). The folder above may be in
            // no repository at all, so their row is the only thing that can speak for them.
            fake.dir("proj-clean", { git: { state: "clean", staged: false, root: true, branch: "main", detached: false } }),
            fake.dir("proj-dirty", { git: { state: "modified", staged: false, root: true, branch: "main", detached: false } }),
            fake.dir("proj-gone", { git: { state: "deleted", staged: false, root: true, branch: "main", detached: false } }),
            fake.dir("proj-new", { git: { state: "untracked", staged: false, root: true, branch: "main", detached: false } }),
            fake.dir("proj-head", { git: { state: "clean", staged: false, root: true, branch: "9f1c2ab3", detached: true } }),
            fake.dir("proj-long", { git: { state: "clean", staged: false, root: true, branch: "feature/a-branch-with-a-very-long-name-indeed", detached: false } }),
        ] }
        pane = paneC.createObject(tc)
        pane.listing.daemon = fake
        list.pane = pane; icons.pane = pane
        pane.open("file:///home/t/repo")
        wait(50)
    }
    function cleanup() {
        Kiki.Theme.yellow = wasYellow
        list.pane = null; icons.pane = null; pane.destroy(); fake.destroy()
    }

    /// The row a name is on — the daemon sorts folders first, and a test that counted would
    /// change every time a name was added.
    function indexOf(name) {
        for (let i = 0; i < pane.listing.count; i++) { const r = pane.listing.row(i); if (r && r.name === name) return i }
        return -1
    }
    function rowFor(name) { const row = findChild(list, "row-" + indexOf(name)); verify(row !== null, name + " is a row"); return row }
    function badgeFor(name) { return findChild(rowFor(name), "git-badge") }
    function capsuleFor(name) { return findChild(rowFor(name), "git-capsule") }
    function branchFor(name) { return findChild(rowFor(name), "git-capsule-text") }
    function nameFor(name) { return findChild(findChild(list, "row-" + indexOf(name)), "row-name") }
    function dotFor(name) { const tile = findChild(icons, "tile-" + indexOf(name)); verify(tile !== null, name + " is a tile"); return findChild(tile, "git-dot") }
    /// Colours as they are drawn — eight bits a channel. A colour that has been through an item
    /// and back is the same colour, not the same floating-point number.
    function sameColor(a, b, what) { compare(a.toString(), b.toString(), what || "") }

    // ---------------------------------------------------------------- the table itself
    function test_the_letters_data() {
        return [
            { tag: "modified", state: "modified", letter: "M" },
            { tag: "added", state: "added", letter: "A" },
            { tag: "deleted", state: "deleted", letter: "D" },
            { tag: "renamed", state: "renamed", letter: "R" },
            { tag: "conflicted", state: "conflicted", letter: "!" },
            { tag: "untracked", state: "untracked", letter: "?" },
            // Neither of these is marked: clean has nothing to say, and ignored is dimmed instead.
            { tag: "ignored", state: "ignored", letter: "" },
            { tag: "clean", state: "clean", letter: "" },
        ]
    }
    function test_the_letters(d) {
        compare(Kiki.Format.gitBadge({ state: d.state }), d.letter)
    }

    function test_the_colours_come_out_of_the_theme() {
        const c = s => Kiki.Format.gitColor({ state: s })
        sameColor(c("modified"), Kiki.Theme.yellow, "modified is yellow")
        sameColor(c("renamed"), Kiki.Theme.yellow, "renamed is yellow")
        sameColor(c("added"), Kiki.Theme.green, "added is green")
        // Gone and conflicted are trouble, so they are `danger` rather than the palette's red:
        // a theme whose red is green (hackerman) must not paint a lost file in it.
        sameColor(c("deleted"), Kiki.Theme.danger, "deleted is danger")
        sameColor(c("conflicted"), Kiki.Theme.danger, "conflicted is danger")
        sameColor(c("ignored"), Kiki.Theme.muted, "ignored is muted")
        sameColor(c(""), Kiki.Theme.muted, "and anything unrecognised is muted, not black")
        sameColor(Kiki.Format.gitColor(null), Kiki.Theme.muted)
        // Untracked is a muted green: it must not be the added green, or the two states would
        // look the same, and it must still read as green.
        verify(!Qt.colorEqual(c("untracked"), Kiki.Theme.green), "untracked is not the added green")
        verify(c("untracked").g > c("untracked").r && c("untracked").g > c("untracked").b, "but it is still green")
    }

    // ---------------------------------------------------------------- as list view draws them
    function test_every_state_wears_its_letter_and_its_colour() {
        const table = [["mod.txt", "M", Kiki.Theme.yellow], ["add.txt", "A", Kiki.Theme.green],
                       ["del.txt", "D", Kiki.Theme.danger], ["ren.txt", "R", Kiki.Theme.yellow],
                       ["conf.txt", "!", Kiki.Theme.danger], ["new.txt", "?", Kiki.Format.gitColor({ state: "untracked" })]]
        for (const [name, letter, colour] of table) {
            const b = badgeFor(name)
            verify(b.visible, name + " is marked")
            compare(b.text, letter, name)
            sameColor(b.color, colour, name)
        }
    }

    function test_a_clean_row_and_one_outside_a_repository_are_not_marked() {
        verify(!badgeFor("ok.txt").visible)
        verify(!badgeFor("plain.txt").visible)
        compare(Kiki.Format.gitMark(pane.listing.row(indexOf("plain.txt"))), null)
    }

    function test_an_ignored_row_is_stepped_back_rather_than_marked() {
        verify(!badgeFor("ign.txt").visible, "ignored has no letter")
        sameColor(nameFor("ign.txt").color, Kiki.Theme.muted, "the name is dimmed instead")
        verify(!Qt.colorEqual(nameFor("mod.txt").color, Kiki.Theme.muted), "and nothing else is")
        // "normal" is the setting for someone who wants build output to look like everything else.
        Kiki.Settings.git = Object.assign({}, Kiki.Settings.git, { showIgnored: "normal" })
        verify(!Qt.colorEqual(nameFor("ign.txt").color, Kiki.Theme.muted))
        verify(!badgeFor("ign.txt").visible, "it is still not marked")
    }

    function test_a_selected_row_wears_the_badge_in_the_bar_colour() {
        pane.selection.set(indexOf("mod.txt"))
        wait(0)
        sameColor(badgeFor("mod.txt").color, Kiki.Theme.bg, "over the accent bar, the letter is the background")
    }

    // ---------------------------------------------------------------- the folder's aggregate
    function test_a_folder_wears_what_its_subtree_adds_up_to() {
        const b = badgeFor("src")
        verify(b.visible, "the folder is marked from its subtree")
        compare(b.text, "M")
        sameColor(b.color, Kiki.Theme.yellow)
    }

    function test_folders_off_leaves_the_files_alone() {
        Kiki.Settings.git = Object.assign({}, Kiki.Settings.git, { folders: "off" })
        verify(!badgeFor("src").visible, "no aggregate when it is turned off")
        verify(badgeFor("mod.txt").visible, "and the files still say what they are")
        compare(Kiki.Format.gitMark(pane.listing.row(indexOf("src"))), null)
    }

    // ---------------------------------------------------------------- icon view says the same
    function test_the_icon_tile_shows_the_same_state_as_a_dot() {
        list.visible = false; icons.visible = true
        wait(50)
        const dot = dotFor("mod.txt")
        verify(dot.visible)
        sameColor(dot.color, Kiki.Theme.yellow, "the same colour as the letter")
        verify(!dotFor("ok.txt").visible, "and clean is a bare tile")
        verify(!dotFor("ign.txt").visible)
        const tile = findChild(findChild(icons, "tile-" + indexOf("ign.txt")), "tile-body")
        verify(tile.opacity < 1, "an ignored tile steps back, as the row's name does")
        compare(findChild(findChild(icons, "tile-" + indexOf("mod.txt")), "tile-body").opacity, 1)
        list.visible = true; icons.visible = false
    }

    // ---------------------------------------------------------------- the branch capsule
    //
    // A folder like `~/Projects` is not a repository, so nothing in the listing knows anything
    // about the projects in it — each is its own repository, and a row that is one wears a
    // capsule where the letter would go: `⎇ main`, coloured by how that repository stands.

    function test_a_repository_root_wears_its_branch_where_the_letter_goes() {
        const cap = capsuleFor("proj-clean")
        verify(cap.visible, "the project says which branch it is on")
        compare(branchFor("proj-clean").text, "main")
        verify(!badgeFor("proj-clean").visible, "and not a letter as well: one mark, in one place")
        sameColor(branchFor("proj-clean").color, Kiki.Theme.muted, "nothing against it, so it is muted")
    }

    /// One element says both which branch and whether it is dirty, in the badge's own colours.
    function test_the_capsule_is_coloured_by_the_repositorys_state_data() {
        return [
            { tag: "clean", name: "proj-clean", colour: Kiki.Theme.muted },
            { tag: "modified", name: "proj-dirty", colour: Kiki.Theme.yellow },
            { tag: "deleted", name: "proj-gone", colour: Kiki.Theme.danger },
            { tag: "untracked", name: "proj-new", colour: Kiki.Format.gitColor({ state: "untracked" }) },
        ]
    }
    function test_the_capsule_is_coloured_by_the_repositorys_state(d) {
        verify(capsuleFor(d.name).visible, d.name)
        compare(branchFor(d.name).text, "main", d.name + " still names its branch")
        sameColor(branchFor(d.name).color, d.colour, d.name)
    }

    function test_a_detached_head_shows_the_short_hash() {
        compare(branchFor("proj-head").text, "9f1c2ab3")
        verify(capsuleFor("proj-head").visible)
    }

    /// The file's name has priority: a branch can be called anything, and the capsule may take
    /// only a share of the name column, eliding what does not fit.
    function test_a_long_branch_elides_inside_the_capsule() {
        const cap = capsuleFor("proj-long"), text = branchFor("proj-long")
        verify(cap.width <= cap.maxWidth, "the capsule keeps to its share: " + cap.width + " > " + cap.maxWidth)
        compare(cap.maxWidth, Math.round(rowFor("proj-long").nameWidth * 0.4), "which is at most 40% of the name column")
        verify(text.truncated, "and the branch elides rather than pushing the name out")
        verify(nameFor("proj-long").width > 0, "the name is still drawn")
    }

    function test_the_capsule_does_not_change_the_row_height() {
        compare(rowFor("proj-long").height, Kiki.Theme.rowHeight)
        compare(rowFor("proj-clean").height, rowFor("plain.txt").height)
        verify(capsuleFor("proj-clean").height < Kiki.Theme.rowHeight, "it sits inside the row it is on")
    }

    function test_a_selected_repository_row_wears_the_capsule_in_the_bar_colour() {
        pane.selection.set(indexOf("proj-dirty"))
        wait(0)
        sameColor(branchFor("proj-dirty").color, Kiki.Theme.bg, "over the accent bar, as the letter is")
    }

    /// `[git] folders = "off"` takes folder marks away; the capsule is one, so it goes too.
    function test_folders_off_takes_the_capsule_with_the_rest() {
        Kiki.Settings.git = Object.assign({}, Kiki.Settings.git, { folders: "off" })
        verify(!capsuleFor("proj-dirty").visible, "no capsule when folder marks are off")
        verify(!badgeFor("proj-dirty").visible, "and no letter in its place either")
        compare(Kiki.Format.gitCapsule(pane.listing.row(indexOf("proj-dirty"))), null)
        verify(badgeFor("mod.txt").visible, "and the files still say what they are")
    }

    /// A folder *inside* a repository is not one: the breadcrumb already names the branch there,
    /// so it keeps exactly the aggregate mark it has always had.
    function test_a_folder_inside_a_repository_keeps_its_aggregate_mark() {
        verify(!capsuleFor("src").visible, "no capsule on a folder that is not a repository itself")
        compare(Kiki.Format.gitCapsule(pane.listing.row(indexOf("src"))), null)
        compare(badgeFor("src").text, "M", "the aggregate letter, as before")
        verify(badgeFor("src").visible)
    }

    function test_an_icon_tile_of_a_project_wears_its_state_on_the_dot() {
        list.visible = false; icons.visible = true
        wait(50)
        verify(dotFor("proj-dirty").visible, "a project's tile says how it stands")
        sameColor(dotFor("proj-dirty").color, Kiki.Theme.yellow, "the capsule's colour, with no room for its text")
        sameColor(dotFor("proj-clean").color, Kiki.Theme.muted)
        list.visible = true; icons.visible = false
    }

    // ---------------------------------------------------------------- one palette, not two
    function test_the_badges_follow_the_theme() {
        // What this guards: the colours used to be written into `Format` as hex, so every badge
        // stayed Tokyo Night's while the window around it changed.
        Kiki.Theme.yellow = "#00a0ff"
        sameColor(badgeFor("mod.txt").color, "#00a0ff", "the letter followed the theme")
        list.visible = false; icons.visible = true
        wait(50)
        sameColor(dotFor("mod.txt").color, "#00a0ff", "and so did the dot")
        list.visible = true; icons.visible = false
    }
}
