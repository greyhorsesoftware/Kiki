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
    function positions() { return Object.keys(rows).map(Number).sort((a, b) => a - b) }
}
