import QtQuick
import QtTest
import "../../qml/kiki" as Kiki
import "../../qml/kiki/ui" as UI
import KikiTest

// The Settings panel (plan 20), by its rules rather than its pages: every control writes through
// at once with a "Saved" flash, invalid input is shown inline at the field and never written, and
// the two pages that do not write `settings.toml` themselves — AI, which the daemon owns because
// of the keyring, and Reset all — ask the daemon and then read everything back.
//
// What goes out on the wire is the assertion: `SetSettings { patch }` is how a control reaches
// settings.toml, and the patch is a patch — one section, one key — so two windows editing
// different pages cannot overwrite each other.
TestCase {
    id: tc
    name: "SettingsWindow"
    when: windowShown
    visible: true
    width: 900; height: 700

    UI.SettingsWindow { id: sw; anchors.fill: parent }

    property var wasView: null
    property var wasProject: null
    property var wasJarvis: null

    function init() {
        wasView = Kiki.Settings.view; wasProject = Kiki.Settings.project; wasJarvis = Kiki.Settings.jarvis
        Kiki.Settings.view = Object.assign({}, Kiki.Settings.view, { "default": "list" })
        Kiki.Settings.project = ({ width: 320, arrange: true, agent: true })
        Kiki.Settings.jarvis = ({ provider: "omarchy", cliCommand: "" })
        Wire.reset(); Wire.connectAll()
        sw.open("general")
        Wire.reset()
    }
    function cleanup() {
        sw.close()
        Kiki.Settings.view = wasView; Kiki.Settings.project = wasProject; Kiki.Settings.jarvis = wasJarvis
    }

    /// The page's control, by the name the page gives it.
    function control(name) { const c = findChild(sw, name); verify(c !== null, name + " is on the page"); return c }
    function patch() { const r = Wire.last("SetSettings"); return r ? r.patch : null }

    // ---------------------------------------------------------------- General: the default view
    function test_the_default_view_is_written_to_settings_toml() {
        const choice = control("default-view")
        compare(choice.value, "list", "the file's value is what the control shows")
        choice.picked("columns")                     // what clicking the option in the drop list does
        const p = patch()
        verify(p !== null, "a SetSettings went out")
        compare(p.view["default"], "columns")
        compare(Object.keys(p).length, 1, "a patch of one section")
        compare(Object.keys(p.view).length, 1, "and one key: nothing else on the page is rewritten")
        compare(Kiki.Settings.view["default"], "columns", "and the window shows it without waiting for the file")
        compare(sw.flash, "Saved")
        compare(choice.value, "columns")
    }

    function test_every_view_can_be_the_default_but_side_by_side_is_not_a_view() {
        const choice = control("default-view")
        compare(choice.options, ["list", "icon", "columns", "gallery"])
        for (const v of choice.options) {
            choice.picked(v)
            compare(patch().view["default"], v)
        }
        // Side by side is how many folders the window shows, not how one is drawn (29 J). A file
        // left saying "mirror" by an older build is shown as List rather than as a blank control.
        Kiki.Settings.view = Object.assign({}, Kiki.Settings.view, { "default": "mirror" })
        compare(choice.value, "list")
    }

    // ---------------------------------------------------------------- an invalid value
    function test_a_bad_tree_width_is_refused_at_the_field_and_never_written() {
        sw.page = "project"
        const box = control("project-width")
        const field = findChild(box, "number-input")
        verify(field !== null)
        for (const bad of ["abc", "", "0", "47", "2001", "12.5.6"]) {
            field.text = bad
            box.commit(field.text)                   // what leaving the field does
            compare(box.error, "48 to 2000", "\"" + bad + "\" is refused")
            compare(Wire.count("SetSettings"), 0, "and nothing is written")
            compare(Kiki.Settings.project.width, 320, "the setting is untouched")
            compare(field.text, bad, "what was typed stays there to be corrected")
            compare(box.border.color, Kiki.Theme.danger, "the field itself says so")
        }
        // And a good one clears the message and goes through.
        field.text = "480"
        box.commit(field.text)
        compare(box.error, "")
        compare(patch().project.width, 480)
        compare(Kiki.Settings.project.width, 480)
        compare(sw.flash, "Saved")
    }

    // ---------------------------------------------------------------- Reset all
    function test_reset_all_asks_the_daemon_and_then_reads_everything_back() {
        sw.page = "about"
        control("reset-all").clicked()
        verify(Wire.last("ResetSettings") !== null, "the daemon removes the files")
        compare(Wire.count("Settings"), 0, "nothing is read back before it says it has")
        compare(Wire.count("About"), 0)
        Wire.replyTo("ResetSettings", {})
        compare(Wire.count("Settings"), 1, "then the panel shows what the defaults are")
        compare(Wire.count("About"), 1)
        compare(Wire.count("PluginStatus"), 1)
    }

    // ---------------------------------------------------------------- the AI provider
    function test_choosing_the_ai_provider_goes_through_the_daemon() {
        sw.page = "ai"
        const choice = control("ai-provider")
        compare(choice.value, "omarchy", "the shipped default: whatever Omarchy's own keybinding uses")
        Wire.reset()
        choice.picked("anthropic")
        const r = Wire.last("AiConfigure")
        verify(r !== null)
        compare(r.provider, "anthropic")
        compare(Wire.count("SetSettings"), 0, "the panel does not write this one itself: the daemon owns the key")
        compare(Wire.count("Settings"), 0, "and nothing is read back before the daemon has answered")
        Wire.replyTo("AiConfigure", {})
        compare(Wire.count("Settings"), 1, "the provider is read back from the file it was written to")
        compare(Wire.count("AiStatus"), 1, "and the line above the control is asked again")
        compare(sw.flash, "Saved")
        // What that line says is the daemon's answer, not a guess: until it has one, the page
        // says what is missing rather than that everything is ready.
        Wire.replyTo("AiStatus", { configured: true, provider: "anthropic", cli: "claude" })
        compare(Kiki.Settings.jarvis.provider, "omarchy", "the file still says what it said: only a Settings reply changes it")
        Wire.replyTo("Settings", { jarvis: { provider: "anthropic" } })
        compare(Kiki.Settings.jarvis.provider, "anthropic")
        compare(choice.value, "anthropic")
    }

    // ---------------------------------------------------------------- Share: a plugin's own form
    // Not one field of a share plugin's form was ever drawn: the delegate asked for a property
    // no Repeater supplies, and the only sign was "Cannot create delegate" in the log, once a
    // field, every time the page was opened.
    function test_a_share_plugins_form_is_drawn_and_a_secret_goes_to_the_keyring() {
        sw.sharePlugins = [{ id: "mail", name: "Mail", targets: "none", version: "1", enabled: true, config: { smtpHost: "mail.example" }, secretFields: ["smtpPassword"],
                             form: [{ key: "smtpHost", label: "SMTP host", kind: "text" }, { key: "smtpPassword", label: "Password", kind: "password" }] }]
        sw.page = "share"
        const host = control("share-field-smtpHost")
        compare(host.value, "mail.example", "with what is saved in it")
        const pass = control("share-field-smtpPassword")
        pass.edited("hunter2")
        const sent = Wire.last("ShareConfigure")
        verify(sent, "an edit is sent")
        compare(sent.plugin, "mail")
        compare(sent.secrets, { smtpPassword: "hunter2" }, "a secret field travels as a secret")
        verify(!("smtpPassword" in sent.config), "and never in the config that is written to a file")
        compare(sent.config.smtpHost, "mail.example")
    }
}
