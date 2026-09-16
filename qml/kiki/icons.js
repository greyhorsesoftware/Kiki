// Generated from docs/design/gen.py: the kiki icon set, 16px grid, stroke-based.
.pragma library

var paths = {
  "folder": "<path d=\"M2 4.5A1.5 1.5 0 0 1 3.5 3h3l1.5 1.5h4.5A1.5 1.5 0 0 1 14 6v6.5a1.5 1.5 0 0 1-1.5 1.5h-9A1.5 1.5 0 0 1 2 12.5z\"></path>",
  "file": "<path d=\"M4 2h5l3 3v9a1 1 0 0 1-1 1H4a1 1 0 0 1-1-1V3a1 1 0 0 1 1-1z\"></path><path d=\"M9 2v3h3\"></path>",
  "doc": "<path d=\"M4 2h5l3 3v9a1 1 0 0 1-1 1H4a1 1 0 0 1-1-1V3a1 1 0 0 1 1-1z\"></path><path d=\"M9 2v3h3\"></path><path d=\"M5.5 8.5h5\"></path><path d=\"M5.5 11h5\"></path>",
  "image": "<rect x=\"2\" y=\"3\" width=\"12\" height=\"10\" rx=\"1\"></rect><circle cx=\"5.5\" cy=\"6.5\" r=\"1\"></circle><path d=\"m14 11-3.5-3.5L5 13\"></path>",
  "code": "<path d=\"m5 5-3 3 3 3\"></path><path d=\"m11 5 3 3-3 3\"></path><path d=\"m9.5 3-3 10\"></path>",
  "archive": "<rect x=\"2\" y=\"3\" width=\"12\" height=\"10\" rx=\"1\"></rect><path d=\"M2 6.5h12\"></path><path d=\"M6.5 9.5h3\"></path>",
  "home": "<path d=\"M2.5 7.5 8 3l5.5 4.5V13a1 1 0 0 1-1 1H3.5a1 1 0 0 1-1-1z\"></path><path d=\"M6.5 14V9.5h3V14\"></path>",
  "download": "<path d=\"M8 2v8\"></path><path d=\"m5 7 3 3 3-3\"></path><path d=\"M3 12v1.5h10V12\"></path>",
  "trash": "<path d=\"M3 4h10\"></path><path d=\"M6 4V2.5h4V4\"></path><path d=\"m4 4 .6 9.5h6.8L12 4\"></path>",
  "hdd": "<rect x=\"2\" y=\"9\" width=\"12\" height=\"4\" rx=\"1\"></rect><path d=\"M2.5 9 4 3.5h8L13.5 9\"></path><path d=\"M11 11h1\"></path>",
  "server": "<rect x=\"2\" y=\"2.5\" width=\"12\" height=\"4\" rx=\"1\"></rect><rect x=\"2\" y=\"9.5\" width=\"12\" height=\"4\" rx=\"1\"></rect><path d=\"M4.5 4.5h1\"></path><path d=\"M4.5 11.5h1\"></path>",
  "cloud": "<path d=\"M5 13a3 3 0 0 1-.5-5.96A4 4 0 0 1 12.3 8.5 2.5 2.5 0 0 1 12 13z\"></path>",
  "search": "<circle cx=\"7\" cy=\"7\" r=\"4\"></circle><path d=\"m10 10 3.5 3.5\"></path>",
  "chev-r": "<path d=\"m6 3 5 5-5 5\"></path>",
  "chev-d": "<path d=\"m3 6 5 5 5-5\"></path>",
  "arr-l": "<path d=\"M13 8H3\"></path><path d=\"m7 4-4 4 4 4\"></path>",
  "arr-r": "<path d=\"M3 8h10\"></path><path d=\"m9 4 4 4-4 4\"></path>",
  "grid": "<rect x=\"2.5\" y=\"2.5\" width=\"4.5\" height=\"4.5\"></rect><rect x=\"9\" y=\"2.5\" width=\"4.5\" height=\"4.5\"></rect><rect x=\"2.5\" y=\"9\" width=\"4.5\" height=\"4.5\"></rect><rect x=\"9\" y=\"9\" width=\"4.5\" height=\"4.5\"></rect>",
  "list": "<path d=\"M3 4h10\"></path><path d=\"M3 8h10\"></path><path d=\"M3 12h10\"></path>",
  "columns": "<rect x=\"2\" y=\"2.5\" width=\"12\" height=\"11\" rx=\"1\"></rect><path d=\"M6 2.5v11\"></path><path d=\"M10 2.5v11\"></path>",
  "plus": "<path d=\"M8 3v10\"></path><path d=\"M3 8h10\"></path>",
  "x": "<path d=\"m4 4 8 8\"></path><path d=\"m12 4-8 8\"></path>",
  "key": "<circle cx=\"5.5\" cy=\"8\" r=\"3\"></circle><path d=\"M8.5 8H14\"></path><path d=\"M12 8v2.5\"></path>",
  "split": "<rect x=\"2\" y=\"2.5\" width=\"5\" height=\"11\" rx=\"1\"></rect><rect x=\"9\" y=\"2.5\" width=\"5\" height=\"11\" rx=\"1\"></rect>",
  "info": "<circle cx=\"8\" cy=\"8\" r=\"6\"></circle><path d=\"M8 7.5v3.5\"></path><path d=\"M8 5.2v.3\"></path>",
  "mirror": "<path d=\"M2 8h4\"></path><path d=\"m4 5.5-2.5 2.5L4 10.5\"></path><path d=\"M10 8h4\"></path><path d=\"m12 5.5 2.5 2.5-2.5 2.5\"></path><path d=\"M8 2v12\" stroke-dasharray=\"1.6 1.6\"></path>",
  "arr-u": "<path d=\"M8 13V3\"></path><path d=\"m4 7 4-4 4 4\"></path>",
  "arr-dn": "<path d=\"M8 3v10\"></path><path d=\"m4 9 4 4 4-4\"></path>",
  "equals": "<path d=\"M3 6h10\"></path><path d=\"M3 10h10\"></path>",
  "check": "<path d=\"m3 8.5 3.5 3.5L13 4.5\"></path>",
  "warn": "<path d=\"M8 2.5 14 13H2z\"></path><path d=\"M8 6.5v3\"></path><path d=\"M8 11.3v.2\"></path>",
  "sort-up": "<path d=\"m4 9 4-4 4 4\"></path>"
};

function svg(name, color, size, strokeWidth) {
    var p = paths[name] || paths["file"];
    var sw = strokeWidth || 1.5;
    var s = '<svg xmlns="http://www.w3.org/2000/svg" width="' + size + '" height="' + size + '" viewBox="0 0 16 16" fill="none" stroke="' + color + '" stroke-width="' + sw + '" stroke-linecap="round" stroke-linejoin="round">' + p + '</svg>';
    return "data:image/svg+xml;utf8," + encodeURIComponent(s);
}
