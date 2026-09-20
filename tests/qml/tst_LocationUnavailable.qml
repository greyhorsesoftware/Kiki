import QtQuick
import QtTest
import "../../qml/kiki" as Kiki
import "../../qml/kiki/ui" as UI
import KikiTest

// A kind whose plugin says it cannot work on this machine — SMB without its GVfs backend — shows
// the plugin's reason instead of a form that could only fail, and cannot be added (plan 25).
TestCase {
    id: tc
    name: "LocationUnavailable"
    when: windowShown
    visible: true
    width: 700; height: 700

    UI.LocationDialog { id: dlg; anchors.fill: parent }

    readonly property var form: [{ key: "name", label: "Name", kind: "text", required: true },
                                 { key: "host", label: "Host", kind: "text", required: true }]

    function init() {
        Wire.reset()
        dlg.open(null)
        Wire.replyTo("Plugins", { plugins: [
            { scheme: "sftp", displayName: "SFTP", version: "1.0", form: tc.form, secretFields: [] },
            { scheme: "smb", displayName: "SMB", version: "0.1", form: tc.form, secretFields: [],
              available: false, unavailableReason: "SMB needs GVfs and its SMB backend: install gvfs-smb" },
            { scheme: "dav", displayName: "WebDAV", version: "0.1", form: tc.form, secretFields: [], available: true, unavailableReason: "" }] })
        wait(30)
        Wire.reset()
    }
    function cleanup() { dlg.visible = false }

    function test_a_kind_that_says_nothing_is_available() {
        dlg.selectKind(0)
        verify(dlg.usable)
        verify(findChild(dlg, "location-form").visible)
        verify(!findChild(dlg, "location-unavailable").visible)
        verify(findChild(dlg, "location-add").enabled)
    }

    function test_an_unavailable_kind_shows_its_reason_and_no_form() {
        dlg.selectKind(1)
        verify(!dlg.usable)
        verify(!findChild(dlg, "location-form").visible)
        verify(findChild(dlg, "location-unavailable").visible)
        compare(findChild(dlg, "location-unavailable-reason").text, "SMB needs GVfs and its SMB backend: install gvfs-smb")
    }

    function test_an_unavailable_kind_cannot_be_added() {
        dlg.selectKind(1)
        dlg.values = { name: "nas", host: "nas.lan" }
        verify(!findChild(dlg, "location-add").enabled)
        verify(!findChild(dlg, "location-add-connect").enabled)
        dlg.connect(undefined, false)
        compare(Wire.count("AddLocation"), 0)
        verify(!dlg.busy)
    }

    function test_going_back_to_a_kind_that_works_brings_the_form_back() {
        dlg.selectKind(1)
        dlg.selectKind(2)
        verify(dlg.usable)
        verify(findChild(dlg, "location-form").visible)
    }
}
