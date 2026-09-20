import QtQuick
import QtTest
import "../../qml/kiki" as Kiki
import "../../qml/kiki/ui" as UI
import KikiTest

// Adding an SFTP location: the server's key has to be shown and accepted before anything is
// saved. The daemon answers the first AddLocation with `verify: <fingerprint>` instead of saving;
// the shell asks; saying yes sends the same request back with `trust` set, and that is what the
// daemon writes as `trustedFingerprint`.
TestCase {
    id: tc
    name: "LocationVerify"
    when: windowShown
    visible: true
    width: 700; height: 700

    property string fingerprint: "SHA256:8cTx0dQrVoQmM5Wd7bS1Zr0e0L8i2vQeUt3kq1A9m0k"

    UI.LocationDialog { id: dlg; anchors.fill: parent }

    function init() {
        Wire.reset()
        dlg.open(null)
        // The dialog asks what plugins exist before it shows anything.
        Wire.replyTo("Plugins", { plugins: [{
            scheme: "sftp", displayName: "SFTP", version: "1.0",
            form: [{ key: "name", label: "Name", kind: "text", required: true },
                   { key: "host", label: "Host", kind: "text", required: true },
                   { key: "username", label: "Username", kind: "text", required: true }],
            secretFields: ["password"]
        }] })
        dlg.values = { name: "homelab", host: "homelab.lan", username: "gideon" }
        Wire.reset()
    }
    function cleanup() { dlg.visible = false }

    function panel() { return findChild(dlg, "verify-host") }

    function test_a_new_server_is_shown_before_anything_is_saved() {
        dlg.connect()
        const first = Wire.last("AddLocation")
        verify(first !== null)
        compare(first.location.config.host, "homelab.lan")
        verify(first.trust === undefined)        // nothing trusted yet

        Wire.reply(first.id, { verify: tc.fingerprint, host: "homelab.lan" })
        verify(panel() !== null && panel().visible)
        compare(findChild(dlg, "verify-fingerprint").text, tc.fingerprint)
        verify(dlg.visible)                      // the dialog stays up: nothing has been saved
        compare(Wire.count("AddLocation"), 1)
    }

    // Accepting sends the fingerprint back, which is what makes the daemon save it.
    function test_accepting_sends_the_key_back_as_trust() {
        dlg.connect()
        Wire.reply(Wire.last("AddLocation").id, { verify: tc.fingerprint, host: "homelab.lan" })
        let saved = 0
        dlg.saved.connect(() => saved++)

        const trust = findChild(dlg, "verify-trust")
        verify(trust !== null)
        mouseClick(trust, trust.width / 2, trust.height / 2)

        compare(Wire.count("AddLocation"), 2)
        compare(Wire.last("AddLocation").trust, tc.fingerprint)
        Wire.reply(Wire.last("AddLocation").id, {})
        compare(saved, 1)
        verify(!dlg.visible)
    }

    // Refusing saves nothing and says so.
    function test_refusing_saves_nothing() {
        dlg.connect()
        Wire.reply(Wire.last("AddLocation").id, { verify: tc.fingerprint, host: "homelab.lan" })
        dlg.verifyFingerprint = ""               // what Cancel does
        compare(Wire.count("AddLocation"), 1)
        verify(dlg.visible)
        verify(panel() === null || !panel().visible)
    }

    // A key that has changed comes back as an error, not as a question.
    function test_a_changed_key_is_an_error() {
        dlg.connect()
        Wire.fail(Wire.last("AddLocation").id, "Auth", "the host key of homelab.lan has changed")
        verify(panel() === null || !panel().visible)
        verify(dlg.status.indexOf("has changed") >= 0, dlg.status)
        verify(dlg.visible)
    }

    // A location that was added without being checked is verified on its first connect. That is a
    // question about a server, not an edit: only the question is shown, never the form.
    readonly property var saved: ({ name: "homelab", plugin: "sftp", remoteUri: "sftp://homelab/", config: { host: "homelab.lan", username: "gideon" } })
    readonly property var plugins: [{ scheme: "sftp", displayName: "SFTP", version: "1.0", form: [{ key: "name", label: "Name", kind: "text", required: true }, { key: "host", label: "Host", kind: "text", required: true }], secretFields: [] }]
    function verifySaved() {
        dlg.visible = false
        Wire.reset()
        dlg.verify(tc.saved)
        Wire.replyTo("Plugins", { plugins: tc.plugins })
    }
    function test_verifying_a_saved_location_shows_the_question_and_not_the_form() {
        verifySaved()
        verify(dlg.visible)
        verify(!findChild(dlg, "location-card").visible, "no edit form")
        verify(findChild(dlg, "verify-waiting").visible, "something to look at while the server is asked")
        const asked = Wire.last("UpdateLocation")
        verify(asked !== null)
        Wire.reply(asked.id, { verify: tc.fingerprint, host: "homelab.lan" })
        verify(panel().visible)
        verify(!findChild(dlg, "verify-waiting").visible)
        verify(!findChild(dlg, "location-card").visible, "still no form")
    }
    function test_cancelling_it_closes_everything() {
        verifySaved()
        Wire.reply(Wire.last("UpdateLocation").id, { verify: tc.fingerprint, host: "homelab.lan" })
        mouseClick(findChild(dlg, "verify-cancel"))
        verify(!dlg.visible, "not left standing in an edit form nobody asked for")
        verify(!dlg.verifyOnly)
    }
    function test_a_failure_closes_it_too_and_the_form_is_a_form_again_next_time() {
        verifySaved()
        Wire.fail(Wire.last("UpdateLocation").id, "Network", "homelab.lan:22: connection refused")
        verify(!dlg.visible)
        dlg.open(tc.saved)
        Wire.replyTo("Plugins", { plugins: tc.plugins })
        verify(findChild(dlg, "location-card").visible, "Edit… is still an edit")
    }
}
