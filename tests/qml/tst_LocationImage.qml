import QtQuick
import QtTest
import "../../qml/kiki" as Kiki
import "../../qml/kiki/ui" as UI
import KikiTest

// A location can wear a picture: chosen beside its Name in the form, saved with it, and shown
// in the sidebar in place of the server glyph — which comes back if the picture cannot be read.
TestCase {
    id: tc
    name: "LocationImage"
    when: windowShown
    visible: true
    width: 700; height: 700

    // A picture that is always there: drawn here, saved to a file.
    Rectangle { id: swatch; width: 32; height: 32; color: "#7aa2f7" }
    property string picture: ""

    UI.LocationDialog { id: dlg; anchors.fill: parent }
    SignalSpy { id: asked; target: dlg; signalName: "chooseImage" }
    UI.SidebarItem { id: item; width: 200; icon: "server"; label: "nas" }

    function initTestCase() {
        let done = false
        const path = Qt.resolvedUrl("../../target/tst-location-image.png").toString().replace(/^file:\/\//, "")
        swatch.grabToImage(r => { verify(r.saveToFile(path)); tc.picture = path; done = true })
        tryVerify(() => done)
    }
    function init() {
        Wire.reset(); asked.clear()
        dlg.open(null)
        Wire.replyTo("Plugins", { plugins: [{
            scheme: "sftp", displayName: "SFTP", version: "1.0",
            form: [{ key: "name", label: "Name", kind: "text", required: true },
                   { key: "host", label: "Host", kind: "text", required: true },
                   { key: "port", label: "Port", kind: "port", required: true, default: "22" },
                   { key: "remotePath", label: "Remote path", kind: "path", required: true }],
            secretFields: []
        }] })
        dlg.values = { name: "nas", host: "nas.lan", port: "22", remotePath: "/" }
        wait(30)
    }
    function cleanup() { dlg.visible = false; item.image = "" }

    function test_the_well_sits_beside_name_and_nowhere_else() {
        const well = findChild(dlg, "location-image"), name = findChild(dlg, "field-name"), host = findChild(dlg, "field-host")
        verify(well && well.visible)
        const wx = well.mapToItem(dlg, 0, 0).x, nx = name.mapToItem(dlg, 0, 0).x
        verify(wx >= nx + name.width, "to the right of Name")
        verify(name.width < host.width + findChild(dlg, "field-port").width + 20, "Name gave up the room")
        compare(well.mapToItem(dlg, 0, well.height).y, name.mapToItem(dlg, 0, name.height).y, "on Name's line")
    }
    function test_clicking_asks_for_a_picture_and_the_answer_is_kept() {
        mouseClick(findChild(dlg, "location-image"))
        compare(asked.count, 1)
        asked.signalArguments[0][1](tc.picture)
        compare(dlg.image, tc.picture)
        tryVerify(() => findChild(dlg, "location-image").has)
        compare(dlg.build().location.image, tc.picture)
    }
    function test_no_picture_is_not_saved_as_one() {
        compare(dlg.build().location.image, undefined)
    }
    function test_the_cross_takes_it_off() {
        dlg.image = tc.picture
        const well = findChild(dlg, "location-image"), cross = findChild(dlg, "location-image-clear")
        mouseMove(well, 5, 20)
        tryVerify(() => cross.visible)
        mouseClick(cross)
        compare(dlg.image, "")
        compare(asked.count, 0, "the cross is not the well")
    }
    function test_editing_shows_the_saved_picture_and_a_new_form_starts_without() {
        dlg.visible = false
        dlg.open({ name: "nas", plugin: "sftp", remoteUri: "sftp://nas/", config: { host: "nas.lan" }, image: tc.picture })
        Wire.replyTo("Plugins", { plugins: dlg.plugins })
        compare(dlg.image, tc.picture)
        dlg.visible = false
        dlg.open(null)
        compare(dlg.image, "")
    }
    function test_in_the_rail_the_icon_grows_under_the_pointer() {
        const g = findChild(item, "sidebar-glyph")
        item.compact = false
        mouseMove(item, 10, 10); wait(200)
        compare(g.scale, 1, "only in the rail")
        mouseMove(tc, 650, 650)
        item.compact = true
        mouseMove(item, 10, 10)
        tryVerify(() => g.scale === 1.35, 1000, "on unless the setting says otherwise")
        const was = Kiki.Settings.view
        Kiki.Settings.view = Object.assign({}, was, { railHover: false })
        tryVerify(() => g.scale === 1)
        Kiki.Settings.view = was
        mouseMove(tc, 650, 650)
        tryVerify(() => g.scale === 1)
        item.compact = false
    }
    function test_the_sidebar_wears_it_and_falls_back() {
        const pic = findChild(item, "sidebar-image")
        verify(!pic.shown)
        item.image = tc.picture
        tryVerify(() => pic.shown)
        item.image = "/nowhere/gone.png"
        tryVerify(() => pic.status === Image.Error)
        verify(!pic.shown, "the glyph is back")
    }
}
