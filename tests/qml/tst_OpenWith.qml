import QtQuick
import QtTest
import "../../qml/kiki" as Kiki
import "../../qml/kiki/views" as Views
import KikiTest

// "Open with" is about the selection, not about one file: several files chosen used to get a
// dead row saying "Select one file". The daemon is asked about all of them — it answers with
// what opens every one — and the application chosen is handed the lot.
TestCase {
    id: tc
    name: "OpenWith"
    when: windowShown
    visible: true
    width: 200; height: 200

    property var fake: null
    Component { id: fakeC; FakeDaemon {} }
    Kiki.Shell { id: shell; width: 1200; height: 760 }
    Views.ListPane { id: list; width: 600; height: 300; pane: shell.pane }

    function init() {
        Wire.reset(); Wire.connectAll()
        fake = fakeC.createObject(tc)
        fake.tree = { "file:///home/t": [fake.file("a.png"), fake.file("b.png"), fake.file("c.txt")] }
        shell.left.listing.daemon = fake
        shell.pane.open("file:///home/t")
        wait(50)
        Wire.reset()
    }
    function cleanup() { shell.pane.selection.clear(); fake.destroy() }

    function sub() { return shell.contextItemsNow().find(i => i.label === "Open with") }
    function offered() { return shell.openWithSub.map(i => i.label) }

    function test_one_file_is_asked_about_as_before() {
        shell.pane.selection.set(0)
        verify(sub().enabled)
        compare(Wire.last("OpenWith").uris, ["file:///home/t/a.png"])
    }
    function test_a_selection_is_asked_about_whole_and_opened_whole() {
        shell.pane.selection.set(0); shell.pane.selection.toggle(1)
        verify(sub().enabled)
        compare(Wire.last("OpenWith").uris, ["file:///home/t/a.png", "file:///home/t/b.png"])
        compare(offered(), ["Looking…"])
        Wire.replyTo("OpenWith", { mime: "image/png", mimes: ["image/png"], apps: [{ id: "imv.desktop", name: "imv", icon: "imv", default: true }, { id: "gimp.desktop", name: "GIMP", icon: "gimp", default: false }] })
        compare(offered(), ["imv  ·  default", "GIMP"])
        shell.openWithSub[1].action()
        const l = Wire.last("Launch")
        compare(l.app, "gimp.desktop")
        compare(l.uris, ["file:///home/t/a.png", "file:///home/t/b.png"])
    }
    function test_kinds_nothing_opens_together_say_so() {
        shell.pane.selection.set(0); shell.pane.selection.toggle(2)
        sub()
        Wire.replyTo("OpenWith", { mime: "", mimes: ["image/png", "text/plain"], apps: [] })
        compare(offered(), ["No application opens all of these"])
        verify(!shell.openWithSub[0].enabled)
    }
    function test_one_kind_nothing_opens_keeps_its_own_words() {
        shell.pane.selection.set(2)
        sub()
        Wire.replyTo("OpenWith", { mime: "text/plain", mimes: ["text/plain"], apps: [] })
        compare(offered(), ["No application for this kind"])
    }
}
