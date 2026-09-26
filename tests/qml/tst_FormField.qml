import QtQuick
import QtTest
import "../../qml/kiki" as Kiki
import "../../qml/kiki/ui" as UI

// A plugin form's drop-down (0.2.0): options are { value, label }; the value is stored, the
// word shown is the catalog's for `option.<key>.<value>` in the user's language, else the
// plugin's label; a click cycles the value. Before this, the English label was the value.
TestCase {
    name: "FormField"
    when: windowShown
    visible: true
    width: 400; height: 100
    UI.FormField {
        id: f
        width: 380
        field: encryption
        value: "explicit"
    }
    SignalSpy { id: edits; target: f; signalName: "edited" }
    // As the FTPS plugin sends it: English labels, and its own words beside them.
    readonly property var encryption: ({ key: "encryption", label: "Encryption", labels: { es: "Cifrado", ja: "暗号化" }, kind: "select",
                                         options: [{ value: "explicit", label: "Explicit TLS (AUTH TLS)", labels: { es: "TLS explícito (AUTH TLS)", ja: "明示的 TLS（AUTH TLS）" } }, { value: "implicit", label: "Implicit TLS" }] })
    function init() { Kiki.T.language = "en"; f.field = encryption; f.value = "explicit"; edits.clear() }
    function cleanup() { Kiki.T.language = "en" }

    function test_the_value_is_stored_and_the_word_is_the_languages() {
        compare(findChild(f, "select-word").text, "Explicit TLS (AUTH TLS)")
        Kiki.T.language = "es"
        compare(findChild(f, "select-word").text, "TLS explícito (AUTH TLS)")
        Kiki.T.language = "ja"
        compare(findChild(f, "select-word").text, "明示的 TLS（AUTH TLS）")
        compare(f.value, "explicit", "the value never changed")
    }
    function test_a_click_cycles_the_value_not_the_word() {
        mouseClick(f)
        compare(f.value, "implicit")
        compare(edits.count, 1)
        compare(edits.signalArguments[0][0], "implicit")
        compare(findChild(f, "select-word").text, "Implicit TLS")
    }
    function test_a_value_without_words_shows_the_plugins_english_and_a_bare_string_is_both() {
        f.field = { key: "flavour", kind: "select", options: [{ value: "x", label: "Extra strong" }, "mild"] }
        f.value = "x"
        compare(findChild(f, "select-word").text, "Extra strong")
        f.value = "mild"
        compare(findChild(f, "select-word").text, "mild")
    }

    // The field's label the same way: the plugin's word in this language, else its English —
    // an option without words falls back to English while the field beside it is translated
    // ("NAME", "HOST", "PORT" were English in every language until 2026-09-25).
    function test_the_label_is_the_plugins_word_for_the_language_else_its_english() {
        compare(findChild(f, "field-label").text, "ENCRYPTION")
        Kiki.T.language = "ja"
        compare(findChild(f, "field-label").text, "暗号化")
        f.value = "implicit"
        compare(findChild(f, "select-word").text, "Implicit TLS", "no Japanese sent for it: the English")
        f.field = ({ key: "flavour", label: "Flavour", kind: "select", options: ["mild", "hot"] })
        compare(findChild(f, "field-label").text, "FLAVOUR")
    }
}
