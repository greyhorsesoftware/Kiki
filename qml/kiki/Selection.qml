import QtQuick

// Selected row positions for one pane; shared by every view of that pane.
QtObject {
    property var rows: ({})       // position -> true
    property int anchor: -1
    property int current: -1
    signal changed()
    function has(i) { return rows[i] === true }
    function count() { return Object.keys(rows).length }
    function set(i) { rows = ({}); if (i >= 0) rows[i] = true; anchor = i; current = i; changed() }
    function toggle(i) { const r = Object.assign({}, rows); if (r[i]) delete r[i]; else r[i] = true; rows = r; current = i; changed() }
    function range(i) { const a = anchor < 0 ? i : anchor; const r = {}; for (let k = Math.min(a, i); k <= Math.max(a, i); k++) r[k] = true; rows = r; current = i; changed() }
    function clear() { rows = ({}); anchor = -1; current = -1; changed() }
    /// Exactly these positions — what a lasso holds at this moment. The anchor stays where it
    /// was, so a Shift+click afterwards extends from the last row chosen by hand.
    function setMany(list, cur) { const r = {}; for (const i of list) r[i] = true; rows = r; if (cur !== undefined) current = cur; changed() }
    /// Rows came or went above some of these positions (`WindowCache.spliced`): what was selected
    /// stays selected, wherever it now is. A selected row that was removed is simply no longer.
    function splice(ops) {
        let r = rows, a = anchor, c = current
        const move = (p, op) => p < 0 ? p : op.op === "remove" ? (p === op.pos ? -1 : p > op.pos ? p - 1 : p) : (p >= op.pos ? p + 1 : p)
        for (const op of ops) {
            const next = {}
            for (const k in r) { const p = move(Number(k), op); if (p >= 0) next[p] = true }
            r = next; a = move(a, op); c = move(c, op)
        }
        rows = r; anchor = a; current = c
        changed()
    }
    function positions() { return Object.keys(rows).map(Number).sort((a, b) => a - b) }
}
