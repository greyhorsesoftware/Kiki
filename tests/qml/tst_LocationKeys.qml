import QtQuick
import QtTest
import "../../qml/kiki" as Kiki
import "../../qml/kiki/ui" as UI
import KikiTest

// Signing in to an SFTP location: every key found on this machine, listed with a tick beside
// each, the first ticked to begin with and the rest the user's to choose; none ticked is a
// password-only location; and a password as an equal, not a last resort.
TestCase {
    id: tc
    name: "LocationKeys"
    when: windowShown
    visible: true
    width: 700; height: 1200

    readonly property var found: [
        { value: "/home/t/.ssh/id_ed25519", label: "id_ed25519 · ED25519 · t@omarchy" },
        { value: "/home/t/.ssh/id_rsa", label: "id_rsa · RSA" },
        { value: "/home/t/.ssh/work", label: "work · ED25519 · work · passphrase" },
    ]

    UI.LocationDialog { id: dlg; anchors.fill: parent }
    SignalSpy { id: chosen; target: dlg; signalName: "chooseFolder" }
    SignalSpy { id: savedSpy; target: dlg; signalName: "saved" }
    SignalSpy { id: openSpy; target: dlg; signalName: "openRequested" }

    function init() {
        Wire.reset()
        dlg.open(null)
        Wire.replyTo("Plugins", { plugins: [{
            scheme: "sftp", displayName: "SFTP", version: "1.0",
            form: [{ key: "name", label: "Name", kind: "text", required: true },
                   { key: "host", label: "Host", kind: "text", required: true },
                   { key: "port", label: "Port", kind: "port", required: true, default: "22" },
                   { key: "username", label: "Username", kind: "text", required: true },
                   { key: "password", label: "Password", kind: "password", required: false, group: "Password" },
                   { key: "identityFile", label: "Keys", kind: "keys", required: false, group: "Key" },
                   { key: "passphrase", label: "Key passphrase", kind: "password", required: false, group: "Key" },
                   { key: "remotePath", label: "Remote path", kind: "path", required: true, page: "Locations" },
                   { key: "localPath", label: "Local path", kind: "path", required: false, page: "Locations" }],
            secretFields: ["passphrase", "password"]
        }] })
        dlg.values = { name: "homelab", host: "homelab.lan", port: "22", username: "gideon", remotePath: "/" }
        dlg._keysDefaulted = ({})
        dlg.loadKeyChoices()
        const asked = Wire.last("PluginBrowse")
        if (asked) Wire.reply(asked.id, { options: tc.found })
        wait(30)
        Wire.reset(); savedSpy.clear(); openSpy.clear()
    }
    function cleanup() { dlg.visible = false; dlg.busy = false; dlg.verifyFingerprint = "" }

    function keysField() { return findChild(dlg, "field-identityFile") }

    function test_the_form_asks_the_plugin_what_keys_there_are() {
        dlg.loadKeyChoices()
        const asked = Wire.last("PluginBrowse")
        verify(asked !== null)
        compare(asked.plugin, "sftp")
        compare(asked.field, "identityFile")
    }

    function test_every_key_found_is_listed() {
        const f = keysField()
        compare(f.choices.length, 3)
        for (let i = 0; i < 3; i++) verify(findChild(f, "key-" + i) !== null, "row " + i)
        verify(findChild(f, "keys-box").visible)
    }

    function test_a_new_location_starts_with_the_first_key_ticked() {
        const f = keysField()
        compare(f.value, "/home/t/.ssh/id_ed25519")
        verify(findChild(f, "key-0").ticked)
        verify(!findChild(f, "key-1").ticked && !findChild(f, "key-2").ticked)
        verify(findChild(f, "key-any") === null)            // there is no "any key" choice
        dlg.connect()
        compare(Wire.last("AddLocation").location.config.identityFile, "/home/t/.ssh/id_ed25519")
    }

    function test_several_keys_can_be_ticked_and_go_in_listed_order() {
        const f = keysField()
        const work = findChild(f, "key-2")
        mouseClick(work, 20, work.height / 2)
        verify(work.ticked && findChild(f, "key-0").ticked && !findChild(f, "key-1").ticked)
        compare(f.value, "/home/t/.ssh/id_ed25519\n/home/t/.ssh/work")
        // Swap which one is first by unticking and re-ticking: still the listed order.
        const first = findChild(f, "key-0")
        mouseClick(first, 20, first.height / 2)
        mouseClick(first, 20, first.height / 2)
        compare(f.value, "/home/t/.ssh/id_ed25519\n/home/t/.ssh/work")
        dlg.connect()
        compare(Wire.last("AddLocation").location.config.identityFile, "/home/t/.ssh/id_ed25519\n/home/t/.ssh/work")
    }

    // Unticking everything is a choice — password only — and must not tick one back.
    function test_every_key_can_be_unticked_and_stays_unticked() {
        const f = keysField()
        const first = findChild(f, "key-0")
        mouseClick(first, 20, first.height / 2)
        compare(f.value, "")
        compare(f.picked.length, 0)
        dlg.loadKeyChoices()                                 // the list arriving again…
        const asked = Wire.last("PluginBrowse"); if (asked) Wire.reply(asked.id, { options: tc.found })
        compare(f.value, "")                                 // …does not undo it
        dlg.connect()
        compare(Wire.last("AddLocation").location.config.identityFile, "")
    }

    // The ticks belong to the form's record, not to the widget: resetting the form clears them.
    function test_resetting_the_form_clears_the_ticks() {
        const f = keysField()
        compare(f.picked.length, 1)
        dlg.values = { name: "other", host: "other.lan", port: "22", username: "gideon" }
        compare(f.value, "")
        compare(f.picked.length, 0)
    }

    function test_no_keys_found_says_so() {
        dlg._keysDefaulted = ({})
        dlg.values = { name: "homelab", host: "homelab.lan", port: "22", username: "gideon" }
        dlg.loadKeyChoices()
        const asked = Wire.last("PluginBrowse"); Wire.reply(asked.id, { options: [] })
        // With no key to be found a new location starts on the Password tab, where there is
        // something to fill in; the Key tab still says why it is empty.
        compare(dlg.authGroup, "Password")
        dlg.chooseGroup("Key")
        const f = keysField()
        compare(f.value, "")
        verify(findChild(f, "keys-none").visible)
    }

    // A server that takes only a password needs nothing else filled in.
    function test_a_password_alone_is_enough_and_is_sent_as_a_secret() {
        dlg.chooseGroup("Password")
        const nv = Object.assign({}, dlg.values); nv.password = "hunter2"; dlg.values = nv
        dlg.connect()
        const sent = Wire.last("AddLocation")
        verify(sent !== null)
        compare(sent.secrets.password, "hunter2")
        compare(sent.location.config.auth, "password")
        verify(sent.location.config.identityFile === undefined, "the Key tab's fields are not part of a Password location")
        verify(sent.location.config.password === undefined, "never in the saved config")
        compare(Object.keys(dlg.errors).length, 0)
    }

    // A saved location can name a key that has since gone: shown, so it can be unticked.
    function test_a_named_key_that_is_gone_is_still_shown() {
        const nv = Object.assign({}, dlg.values); nv.identityFile = "/home/t/.ssh/old_key"; dlg.values = nv
        wait(20)
        const f = keysField()
        compare(f.picked.length, 1)
        compare(f.picked[0], "/home/t/.ssh/old_key")
    }

    // ---- the pages

    function test_the_paths_are_a_page_of_their_own() {
        compare(dlg.pages().join(","), "Connection,Locations")
        compare(dlg.page, "Connection")
        verify(findChild(dlg, "page-tabs").visible)
        verify(findChild(dlg, "field-host") !== null && findChild(dlg, "field-remotePath") === null)
        const tab = findChild(dlg, "page-tab-locations")
        mouseClick(tab, tab.width / 2, tab.height / 2)
        compare(dlg.page, "Locations")
        verify(findChild(dlg, "field-remotePath") !== null && findChild(dlg, "field-localPath") !== null)
        verify(findChild(dlg, "field-host") === null && findChild(dlg, "auth-tabs") === null)
    }

    // Pages are sections, not alternatives: whichever is showing, the whole location is sent.
    function test_every_page_is_sent_whichever_is_showing() {
        const nv = Object.assign({}, dlg.values); nv.remotePath = "/srv/site"; nv.localPath = "/home/t/Sites"; dlg.values = nv
        dlg.page = "Connection"
        dlg.connect()
        let sent = Wire.last("AddLocation")
        compare(sent.location.remoteUri, "sftp://homelab/srv/site")
        compare(sent.location.localUri, "file:///home/t/Sites")
        compare(sent.location.config.host, "homelab.lan")
        dlg.busy = false; Wire.reset()
        dlg.page = "Locations"
        dlg.connect()
        sent = Wire.last("AddLocation")
        compare(sent.location.config.host, "homelab.lan")
        compare(sent.location.config.identityFile, "/home/t/.ssh/id_ed25519")
    }

    // A required field left empty on the page you are not looking at: you are taken to it.
    function test_an_error_on_another_page_takes_you_there() {
        const nv = Object.assign({}, dlg.values); nv.remotePath = ""; dlg.values = nv
        dlg.page = "Connection"
        dlg.connect()
        compare(Wire.count("AddLocation"), 0)
        verify(dlg.errors.remotePath !== undefined)
        compare(dlg.page, "Locations")
        // …but not away from an error that is in front of you.
        const nv2 = Object.assign({}, dlg.values); nv2.host = ""; dlg.values = nv2
        dlg.page = "Connection"
        dlg.connect()
        compare(dlg.page, "Connection")
        verify(findChild(dlg, "page-tab-locations").bad)
    }

    // The kinds stand down the left, centred, and the form has the height they used to take.
    function test_the_kinds_are_down_the_left_hand_side() {
        const kinds = findChild(dlg, "location-kinds"), form = findChild(dlg, "location-form"), card = findChild(dlg, "location-card")
        const k = kinds.mapToItem(card, 0, 0), f = form.mapToItem(card, 0, 0)
        verify(k.x + kinds.width <= f.x, "to the left of the form")
        verify(Math.abs((k.y + kinds.height / 2) - (f.y - 22 + (form.height + 44) / 2)) <= 40, "about centred beside it")
        compare(kinds.orientation, ListView.Vertical)
    }

    // The rows are built from the form's SHAPE (page, chosen tab), never from its values: if
    // typing rebuilt them, every character would cost the field its focus.
    function test_typing_does_not_rebuild_the_form() {
        const host = findChild(dlg, "field-host"), user = findChild(dlg, "field-username")
        const nv = Object.assign({}, dlg.values); nv.host = "homelab.example.org"; dlg.values = nv
        compare(findChild(dlg, "field-host"), host)
        compare(findChild(dlg, "field-username"), user)
        compare(host.value, "homelab.example.org")
    }

    // ---- the tabs

    function test_credentials_are_two_tabs_with_the_username_above_both() {
        verify(findChild(dlg, "auth-tabs").visible)
        compare(dlg.groups().join(","), "Password,Key")
        compare(dlg.authGroup, "Key")                                  // a key was found, so it starts there
        verify(findChild(dlg, "auth-tab-key").on && !findChild(dlg, "auth-tab-password").on)
        verify(findChild(dlg, "field-identityFile") !== null && findChild(dlg, "field-passphrase") !== null)
        verify(findChild(dlg, "field-password") === null)
        verify(findChild(dlg, "field-username") !== null)             // on neither tab: both need it

        const tab = findChild(dlg, "auth-tab-password")
        mouseClick(tab, tab.width / 2, tab.height / 2)
        compare(dlg.authGroup, "Password")
        verify(findChild(dlg, "field-password") !== null)
        verify(findChild(dlg, "field-identityFile") === null && findChild(dlg, "field-passphrase") === null)
        verify(findChild(dlg, "field-username") !== null)
    }

    function test_only_the_chosen_tab_is_sent() {
        // Type a password, then change your mind: the Key tab's location carries no password.
        dlg.chooseGroup("Password")
        let nv = Object.assign({}, dlg.values); nv.password = "hunter2"; dlg.values = nv
        dlg.chooseGroup("Key")
        dlg.connect()
        const sent = Wire.last("AddLocation")
        compare(sent.location.config.auth, "key")
        compare(sent.location.config.identityFile, "/home/t/.ssh/id_ed25519")
        verify(sent.secrets.password === undefined, "the other tab's password is neither saved nor sent")
    }

    function test_the_tab_comes_back_when_a_location_is_edited() {
        const nv = Object.assign({}, dlg.values); nv.auth = "password"; dlg.values = nv
        compare(dlg.authGroup, "Password")
    }

    // A port is five digits: it shares Host's line.
    function test_the_port_sits_to_the_right_of_the_host() {
        const host = findChild(dlg, "field-host"), port = findChild(dlg, "field-port")
        const h = host.mapToItem(dlg, 0, 0), p = port.mapToItem(dlg, 0, 0)
        verify(Math.abs(h.y - p.y) <= 1, "same line: " + h.y + " vs " + p.y)
        verify(p.x >= h.x + host.width, "to its right")
        verify(port.width <= 110 && host.width > port.width * 2, host.width + " / " + port.width)
        const user = findChild(dlg, "field-username")
        verify(user.mapToItem(dlg, 0, 0).y > h.y + 10)                // and Username is on the next line, full width
        verify(user.width > host.width)
    }

    // The local path is a folder on this machine, so its folder icon opens a chooser; the remote
    // path's icon does not — a local chooser could not answer for a server.
    function test_the_local_paths_folder_icon_asks_for_a_folder() {
        dlg.page = "Locations"
        wait(30)                                   // the page's rows are built and laid out
        const local = findChild(dlg, "field-localPath"), remote = findChild(dlg, "field-remotePath")
        verify(local.pickable && !remote.pickable)
        verify(!findChild(remote, "field-pick").visible)
        const nv = Object.assign({}, dlg.values); nv.localPath = "/home/t/Projects"; dlg.values = nv
        chosen.clear()
        // The form is longer than its card: the local path is at the bottom of the scroll.
        const form = findChild(dlg, "location-form")
        form.contentY = Math.max(0, form.contentHeight - form.height)
        const hit = findChild(local, "field-pick")
        mouseClick(hit, hit.width / 2, hit.height / 2)
        compare(chosen.count, 1)
        compare(chosen.signalArguments[0][0], "/home/t/Projects")     // starts where the field points
        chosen.signalArguments[0][1]("/home/t/Sites/homelab")          // the window answers
        compare(dlg.values.localPath, "/home/t/Sites/homelab")
        compare(local.value, "/home/t/Sites/homelab")
        compare(dlg.values.host, "homelab.lan")                        // and nothing else moved
    }

    // "Add" keeps it; "Add and Connect" opens it too. Centred as a pair; the way out is the
    // close box and Esc.
    function test_add_and_add_and_connect() {
        verify(findChild(dlg, "location-cancel") === null)
        const add = findChild(dlg, "location-add"), both = findChild(dlg, "location-add-connect")
        compare(add.text, "Add"); compare(both.text, "Add and Connect")
        verify(both.primary && !add.primary)
        const pair = findChild(dlg, "location-buttons"), card = findChild(dlg, "location-card")
        verify(Math.abs(pair.mapToItem(card, pair.width / 2, 0).x - card.width / 2) <= 1, "centred")
        verify(!findChild(dlg, "location-status").visible)

        mouseClick(add, add.width / 2, add.height / 2)
        // Add takes what was typed: the daemon is told not to look anything up or connect.
        compare(Wire.last("AddLocation").check, false)
        Wire.replyTo("AddLocation", {})
        compare(savedSpy.count, 1)
        compare(openSpy.count, 0)                          // Add does not open anything
        verify(!dlg.visible)
    }

    function test_add_and_connect_opens_what_it_saved() {
        const both = findChild(dlg, "location-add-connect")
        mouseClick(both, both.width / 2, both.height / 2)
        verify(Wire.last("AddLocation").check === undefined, "Add and Connect signs in to check")
        Wire.replyTo("AddLocation", {})
        compare(savedSpy.count, 1)
        compare(openSpy.count, 1)
        compare(openSpy.signalArguments[0][0], "homelab")
    }

    // The server has to be trusted first: answering that does what was originally asked.
    function test_the_choice_survives_verifying_the_server() {
        const both = findChild(dlg, "location-add-connect")
        mouseClick(both, both.width / 2, both.height / 2)
        Wire.replyTo("AddLocation", { verify: "SHA256:abc", host: "homelab.lan" })
        compare(openSpy.count, 0)
        const trust = findChild(dlg, "verify-trust")
        mouseClick(trust, trust.width / 2, trust.height / 2)
        Wire.replyTo("AddLocation", {})
        compare(openSpy.count, 1)

        // …and "Add" followed by trusting the key still only adds.
        dlg.open(null); dlg.values = { name: "other", host: "other.lan", port: "22", username: "gideon", remotePath: "/" }
        openSpy.clear()
        dlg.connect(undefined, false)
        Wire.replyTo("AddLocation", { verify: "SHA256:def", host: "other.lan" })
        dlg.connect(dlg.verifyFingerprint)
        Wire.replyTo("AddLocation", {})
        compare(openSpy.count, 0)
    }

    function test_editing_they_are_save_and_save_and_connect() {
        dlg.editingName = "homelab"
        compare(findChild(dlg, "location-add").text, "Save")
        compare(findChild(dlg, "location-add-connect").text, "Save and Connect")
        dlg.editingName = ""
    }
}
