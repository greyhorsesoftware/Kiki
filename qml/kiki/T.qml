pragma Singleton
import QtQuick
import "i18n/en.js" as En
import "i18n/es.js" as Es
import "i18n/ja.js" as Ja

// The words on screen, in the language the OS is set to (docs/0.2.0/02-localization.md). One
// catalog per language under `i18n/`, English the source of truth and the fallback key by key;
// `T.tr("count.items", { n: 3 })` is the whole of what the rest of the code says to it.
//
// The language is the first of `Qt.locale().uiLanguages` that has a catalog — `es-MX` lands on
// `es`, `ja-Jpan-JP` on `ja` — else `en`; read once at start. Tests set `language` outright.
QtObject {
    id: t
    readonly property var catalogs: ({ en: En.strings, es: Es.strings, ja: Ja.strings })
    property string language: pick(Qt.locale().uiLanguages)

    function pick(uiLanguages) {
        for (const l of uiLanguages || []) {
            const lang = String(l).split(/[-_]/)[0].toLowerCase()
            if (catalogs[lang]) return lang
        }
        return "en"
    }

    /// The sentence for `key`, with `{name}` placeholders filled from `args`; a plural key
    /// picks its form by `args.n`. A key the language lacks shows English and is logged once;
    /// a key nobody has shows the key itself, logged once, so it is seen and never silent.
    function tr(key, args) {
        const own = catalogs[language] || En.strings
        let s = own[key]
        if (s === undefined) {
            s = En.strings[key]
            if (s === undefined) { _missing("no such key: " + key); return key }
            if (language !== "en") _missing(language + " has no " + key)
        }
        if (typeof s === "object") {
            const n = args && args.n !== undefined ? Number(args.n) : NaN
            s = (n === 1 && s.one !== undefined) ? s.one : (s.other !== undefined ? s.other : s.one)
        }
        return fill(s, args)
    }
    function fill(s, args) {
        if (!args) return String(s)
        return String(s).replace(/\{(\w+)\}/g, (m, k) => {
            const v = args[k]
            if (v === undefined) return m
            return typeof v === "number" ? Number(v).toLocaleString(Qt.locale(), "f", 0) : String(v)
        })
    }
    /// What a plugin sent, in this language: a form field or a select option carries `label`
    /// (English) and, from a plugin that has words for it, `labels: { es, ja }`. The plugin owns
    /// its words (docs/0.2.0/02-localization.md, decision 7); the window only picks.
    function sent(o) {
        if (!o) return ""
        if (o.labels && o.labels[language] !== undefined) return o.labels[language]
        return o.label !== undefined ? o.label : (o.value !== undefined ? o.value : "")
    }
    /// Whether the language in use, or English, has words for `key`.
    function has(key) { return (catalogs[language] || En.strings)[key] !== undefined || En.strings[key] !== undefined }
    /// A daemon error in this language: `{ n, params, message }` — the number's sentence when
    /// there is one (its `.more` form when `params.more` counts extras), else the daemon's
    /// English `message` (docs/0.2.0/02-localization.md, L5).
    function errorText(e) {
        if (!e) return ""
        if (e.n === undefined || e.n === null) return e.message || ""
        if (!has("error." + e.n)) return e.message || String(e.n)
        const base = "error." + e.n
        const more = e.params && Number(e.params.more) > 0 && has(base + ".more") ? base + ".more" : base
        return tr(more, e.params || {})
    }
    property var _said: ({})
    function _missing(what) { if (_said[what]) return; _said[what] = true; console.warn("T:", what) }
}
