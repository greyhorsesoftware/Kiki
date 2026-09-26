// The headings of a Markdown document, from its source, for Quick Look's table of contents
// (docs/0.2.0/05-quicklook.md). Pure: text in, headings out. Qt renders the document; this only
// has to agree with it about what is a heading, which is CommonMark's rule set kept to what a
// contents column can tell apart.
.pragma library

/// `[{ level: 1..6, text, line }]` in document order. `line` is the 0-based line of the heading's
/// own text (the text line of a setext heading, not its underline).
///
/// ATX: `#`…`######`, at most three spaces in, a space after the hashes, optional closing hashes.
/// Setext: a paragraph line under `===` (level 1) or `---` (level 2) — but not a `---` after a
/// blank line, which is a horizontal rule, nor one under a list item, a quote or another heading.
/// Nothing inside a ``` or ~~~ fence counts, nor YAML front matter at the top.
function headings(text) {
    const out = []
    if (!text) return out
    const lines = text.split(/\r?\n/)
    let fence = null                  // { ch, n } while inside a fenced block
    let i = 0
    // Front matter: `---` on the first line up to the next `---` is metadata, and `title: x`
    // over that second rule would otherwise read as a setext heading.
    if (/^---[ \t]*$/.test(lines[0] || "")) {
        let j = 1
        while (j < lines.length && !/^(---|\.\.\.)[ \t]*$/.test(lines[j])) j++
        if (j < lines.length) i = j + 1
    }
    for (; i < lines.length; i++) {
        const l = lines[i]
        const open = l.match(/^ {0,3}(`{3,}|~{3,})/)
        if (fence) {
            // The fence closes on the same character, at least as many of them, and nothing else.
            if (open && open[1][0] === fence.ch && open[1].length >= fence.n && /^ {0,3}[`~]+[ \t]*$/.test(l)) fence = null
            continue
        }
        if (open) { fence = { ch: open[1][0], n: open[1].length }; continue }
        const atx = l.match(/^ {0,3}(#{1,6})(?:[ \t]+(.*?))?[ \t]*$/)
        if (atx) {
            // Closing hashes are decoration only when a space sets them off: `# C#` keeps its #.
            let t = (atx[2] || "").replace(/(^|[ \t])#+[ \t]*$/, "").trim()
            if (t !== "") out.push({ level: atx[1].length, text: t, line: i })
            continue
        }
        const under = l.match(/^ {0,3}(=+|-+)[ \t]*$/)
        if (under && i > 0 && isParagraphLine(lines[i - 1]))
            out.push({ level: under[1][0] === "=" ? 1 : 2, text: lines[i - 1].trim(), line: i - 1 })
    }
    return out
}

/// A line that a setext underline can turn into a heading: text, not blank, not indented code,
/// not a list item, a quote, a rule or a fence.
function isParagraphLine(l) {
    if (l === undefined || !/^ {0,3}\S/.test(l)) return false
    if (/^ {0,3}(#{1,6}([ \t]|$)|[-*+][ \t]|\d+[.)][ \t]|>|`{3,}|~{3,})/.test(l)) return false
    if (/^ {0,3}([-*_])([ \t]*\1){2,}[ \t]*$/.test(l)) return false        // a rule: `* * *`
    if (/^ {0,3}(=+|-+)[ \t]*$/.test(l)) return false                    // an underline itself
    return true
}

/// The heading as the rendered document shows it: inline marks stripped, so the column reads
/// like the page and the jump can find it in `getText`. Light on purpose — emphasis, code,
/// links, images — nothing that needs a real parser.
function plain(text) {
    let t = text || ""
    t = t.replace(/!\[([^\]]*)\]\([^)]*\)/g, "$1")            // ![alt](src) → alt
    t = t.replace(/\[([^\]]+)\]\([^)]*\)/g, "$1")              // [text](url) → text
    t = t.replace(/\[([^\]]+)\]\[[^\]]*\]/g, "$1")             // [text][ref] → text
    t = t.replace(/`+([^`]*)`+/g, "$1")                        // `code`
    t = t.replace(/(\*\*|__)(\S(?:.*?\S)?)\1/g, "$2")          // **strong**
    t = t.replace(/(\*|_)(\S(?:.*?\S)?)\1/g, "$2")             // *emphasis*
    t = t.replace(/~~(\S(?:.*?\S)?)~~/g, "$1")                 // ~~struck~~
    t = t.replace(/\\([\\`*_{}\[\]()#+\-.!~])/g, "$1")         // \* → *
    return t.replace(/\s+/g, " ").trim()
}
