import QtQuick
import QtTest
import "../../qml/kiki" as Kiki
import "../../qml/kiki/ui" as UI
import KikiTest

// Side by side, as far as the chrome goes: a toolbar toggle of its own, a path over each pane's
// own listing, and a title-bar path that steps aside while those are up.
TestCase {
    id: tc
    name: "SideBySideChrome"
    when: windowShown
    visible: true
    width: 900; height: 200

    property var fake: null
    property var local: null
    property var remote: null
    Component { id: fakeC; FakeDaemon {} }
    Component { id: paneC; Kiki.Pane {} }

    UI.Toolbar { id: bar; width: parent.width; home: "/home/t" }
    UI.PaneHeader { id: header; y: 80; width: 440; home: "/home/t" }
    SignalSpy { id: toggled; target: bar; signalName: "toggleSplit" }
    SignalSpy { id: navigated; target: bar; signalName: "navigate" }
    SignalSpy { id: headerClicked; target: header; signalName: "clicked" }
    SignalSpy { id: mirrored; target: bar; signalName: "toggleMirror" }

    // The panes live for the whole case: the toolbar and the header are written for a window
    // that always has panes, and taking one away from under them is not a state they can be in.
    function initTestCase() {
        fake = fakeC.createObject(tc)
        fake.tree = {
            "file:///home/t": [fake.dir("Projects")],
            "file:///home/t/Projects": [fake.file("a.txt")],
            "sftp://homelab/srv": [fake.dir("site")],
            "sftp://homelab/srv/site": [fake.file("index.html")],
        }
        local = paneC.createObject(tc); local.listing.daemon = fake
        remote = paneC.createObject(tc); remote.listing.daemon = fake
    }
    function init() {
        Kiki.Settings.viewPrefs = ({})
        Wire.reset()
        local.open("file:///home/t/Projects")
        remote.open("sftp://homelab/srv/site")
        bar.pane = remote                 // focus is on the remote pane…
        bar.crumbPane = local             // …and the title bar's path is still the local one
        bar.split = true
        bar.remoteOpen = true
        header.pane = remote
        header.showPath = true
        wait(30)
        toggled.clear(); navigated.clear(); headerClicked.clear(); mirrored.clear()
    }

    function test_the_toolbar_has_its_own_side_by_side_toggle() {
        const b = bar.sideBySideButton
        verify(b.visible)
        verify(b.active)                  // lit while it is on
        mouseClick(b, b.width / 2, b.height / 2)
        compare(toggled.count, 1)
        bar.split = false
        verify(!b.active)
    }

    // Side by side is a server and the folder kept beside it (owner, 2026-09-21): the button is
    // there only while a server is open — and while the layout is on, so there is a way out.
    function test_it_is_offered_only_with_a_server_open() {
        const b = bar.sideBySideButton
        bar.split = false; bar.remoteOpen = false
        verify(!b.visible, "two folders on this machine, one pane: nothing to put side by side")
        bar.remoteOpen = true
        verify(b.visible)
        bar.remoteOpen = false; bar.split = true
        verify(b.visible, "on, it stays: the way back to one pane")
        bar.remoteOpen = true
    }
    // On, it stands with Mirror over the line between the panes; off, where the path ends — and
    // the path gives it the room rather than running under it.
    function test_it_stands_with_mirror_when_on_and_after_the_path_when_off() {
        const b = bar.sideBySideButton, m = bar.mirrorButton, crumb = bar.breadcrumb
        bar.remoteOpen = true
        verify(m.visible)
        compare(b.x + b.width + 4, m.x)
        bar.split = false
        wait(20)
        const end = crumb.mapToItem(bar, crumb.width, 0).x
        verify(b.x >= end, "on top of the path: " + b.x + " < " + end)
        bar.split = true
    }

    // Two panes, two paths, each over its own listing — so the one in the title bar steps aside.
    function test_the_title_bar_path_steps_aside_side_by_side() {
        const crumb = bar.breadcrumb
        verify(!bar.pathShown)
        compare(crumb.opacity, 0)
        verify(!crumb.enabled)                 // and cannot be clicked while it cannot be seen
        bar.split = false
        verify(bar.pathShown)
        compare(crumb.opacity, 1)
        verify(crumb.enabled)
    }

    function test_with_one_pane_the_title_bar_path_is_that_panes() {
        bar.split = false
        bar.crumbPane = Qt.binding(() => bar.pane)
        bar.pane = remote
        compare(bar.breadcrumb.uri, "sftp://homelab/srv/site")
        bar.breadcrumb.navigate("sftp://homelab/srv")
        compare(navigated.count, 1)            // handed to the window, which opens it
    }

    function test_a_panes_own_breadcrumb_navigates_that_pane_only() {
        const crumb = header.breadcrumb
        verify(crumb.visible)
        compare(crumb.uri, "sftp://homelab/srv/site")
        crumb.navigate("sftp://homelab/srv")
        compare(remote.uri, "sftp://homelab/srv")
        compare(local.uri, "file:///home/t/Projects")     // the local pane did not move
        compare(headerClicked.count, 1)                   // and using it focuses that pane
    }

    // The way into a mirror run: in the middle of the toolbar, where the path used to be, and
    // only side by side.
    function test_the_mirror_button_sits_in_the_middle_of_the_toolbar() {
        const b = bar.mirrorButton
        verify(b.visible)
        let mid = b.mapToItem(bar, b.width / 2, b.height / 2)
        verify(Math.abs(mid.x - bar.width / 2) <= 1, "x " + mid.x + " of " + bar.width)
        verify(Math.abs(mid.y - bar.height / 2) <= 1, "y " + mid.y + " of " + bar.height)
        // The window says where the middle of the PANES is — further right when a sidebar is
        // showing — and the button stands over that.
        bar.mirrorCenterX = 560
        mid = b.mapToItem(bar, b.width / 2, 0)
        verify(Math.abs(mid.x - 560) <= 1, "follows the panes' middle: " + mid.x)
        // …but never out over the buttons either side of the path's room.
        bar.mirrorCenterX = 5
        verify(b.mapToItem(bar, 0, 0).x >= bar.breadcrumb.mapToItem(bar, 0, 0).x - 1)
        bar.mirrorCenterX = 5000
        verify(b.mapToItem(bar, b.width, 0).x <= bar.breadcrumb.mapToItem(bar, bar.breadcrumb.width, 0).x + 1)
        bar.mirrorCenterX = Qt.binding(() => bar.width / 2)
        mouseClick(b, b.width / 2, b.height / 2)
        compare(mirrored.count, 1)
        bar.mirror = true;  verify(b.active)      // lit while a mirror run is open
        bar.mirror = false; verify(!b.active)
        bar.split = false
        verify(!b.visible)
    }

    // One drive for this machine, the green pair for a remote — and no words.
    function test_the_badge_is_an_icon_only() {
        header.pane = local
        const icon = findChild(header, "pane-badge-icon")
        compare(icon.name, "hdd")
        header.pane = remote
        compare(icon.name, "server")
        verify(Qt.colorEqual(icon.color, Kiki.Theme.green))
        const badge = findChild(header, "pane-badge")
        for (let i = 0; i < badge.children.length; i++) verify(badge.children[i].text === undefined, "no label in the badge")
        verify(badge.width <= 20)
    }

    // The local pane shows its path the same way as the other one.
    function test_the_local_pane_shows_its_path_too() {
        header.pane = local
        const crumb = header.breadcrumb
        verify(crumb.visible)
        compare(crumb.uri, "file:///home/t/Projects")
        crumb.navigate("file:///home/t")
        compare(local.uri, "file:///home/t")
        compare(remote.uri, "sftp://homelab/srv/site")
    }

}
