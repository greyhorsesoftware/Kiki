import QtQuick
import QtTest
import "../../qml/kiki" as Kiki

// The words on screen come from a catalog per language (0.2.0): the OS's language picked from
// `uiLanguages`, English the fallback key by key, {placeholders} filled, plurals by form —
// Japanese with one form — and a key nobody has shown as itself, logged once.
TestCase {
    name: "T"
    function init() { Kiki.T.language = "en" }
    function cleanup() { Kiki.T.language = "en" }

    function test_the_language_is_the_first_of_the_os_that_has_a_catalog() {
        compare(Kiki.T.pick(["en-Latn-US", "en-US", "en-Latn", "en"]), "en")
        compare(Kiki.T.pick(["es-Latn-MX", "es-MX", "es-Latn", "es"]), "es", "a regional Spanish lands on es")
        compare(Kiki.T.pick(["ja-Jpan-JP", "ja-JP", "ja-Jpan", "ja"]), "ja")
        compare(Kiki.T.pick(["fr-Latn-FR", "fr-FR", "fr"]), "en", "a language without a catalog: English")
        compare(Kiki.T.pick(["C"]), "en")
        compare(Kiki.T.pick([]), "en")
        compare(Kiki.T.pick(["fr", "ja", "en"]), "ja", "the first that has one, in the OS's order")
    }
    function test_a_sentence_with_its_placeholders_in_each_language() {
        compare(Kiki.T.tr("date.yesterdayAt", { time: "14:02" }), "yesterday 14:02")
        Kiki.T.language = "es"
        compare(Kiki.T.tr("date.yesterdayAt", { time: "14:02" }), "ayer 14:02")
        Kiki.T.language = "ja"
        compare(Kiki.T.tr("date.yesterdayAt", { time: "14:02" }), "昨日 14:02")
    }
    function test_plurals_by_form_and_japanese_with_one() {
        compare(Kiki.T.tr("count.items", { n: 1 }), "1 item")
        compare(Kiki.T.tr("count.items", { n: 2 }), "2 items")
        compare(Kiki.T.tr("count.items", { n: 0 }), "0 items")
        Kiki.T.language = "es"
        compare(Kiki.T.tr("count.items", { n: 1 }), "1 elemento")
        compare(Kiki.T.tr("count.items", { n: 2 }), "2 elementos")
        Kiki.T.language = "ja"
        compare(Kiki.T.tr("count.items", { n: 1 }), "1項目")
        compare(Kiki.T.tr("count.items", { n: 2 }), "2項目")
    }
    function test_a_key_the_language_lacks_shows_english_and_one_nobody_has_shows_itself() {
        Kiki.T.language = "es"
        Kiki.T.catalogs.es["only.in.english"] = undefined
        Kiki.T.catalogs.en["only.in.english"] = "the English words"
        compare(Kiki.T.tr("only.in.english"), "the English words")
        ignoreWarning(/T: no such key: nobody.has.this/)
        compare(Kiki.T.tr("nobody.has.this"), "nobody.has.this")
        compare(Kiki.T.tr("nobody.has.this"), "nobody.has.this", "said once, not twice")
        delete Kiki.T.catalogs.en["only.in.english"]
    }
    // The daemon's errors come as a number and params; the window says them (L5).
    function test_a_numbered_error_is_said_in_the_language_and_an_unnumbered_one_as_it_came() {
        compare(Kiki.T.errorText({ n: 1202, params: { n: "1", total: "3", first: "a.txt (denied)", more: "0" }, message: "raw" }), "1 of 3 could not be copied: a.txt (denied)")
        compare(Kiki.T.errorText({ n: 1202, params: { n: "5", total: "9", first: "a, b, c", more: "2" }, message: "raw" }), "5 of 9 could not be copied: a, b, c, and 2 more — see the log")
        Kiki.T.language = "es"
        compare(Kiki.T.errorText({ n: 1230, params: {}, message: "cancelled" }), "Cancelado")
        Kiki.T.language = "ja"
        compare(Kiki.T.errorText({ n: 1220, params: {}, message: "same folder" }), "送信元と送信先が同じフォルダーです")
        compare(Kiki.T.errorText({ message: "plain words from the daemon" }), "plain words from the daemon", "no number: the message as it came")
        compare(Kiki.T.errorText({ n: 9999, params: {}, message: "a number nobody has words for" }), "a number nobody has words for")
    }
    function test_a_missing_placeholder_is_left_as_it_is_not_swallowed() {
        compare(Kiki.T.tr("date.yesterdayAt", {}), "yesterday {time}")
        compare(Kiki.T.tr("date.yesterdayAt"), "yesterday {time}")
    }
}
