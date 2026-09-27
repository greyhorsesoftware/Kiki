# API delta — 0.3.0

What changed in the daemon's protocol since 0.2.2. A window and its daemon are the same build
by construction — the socket is named for the version and `Hello` refuses any other
(`01-daemon-on-demand.md`, decision 7) — so nothing here is kept as an alias: there is nobody an
alias would protect. Plugins never send requests, and the tests move with the code.

## `Launch`

`{ "type": "Launch", "uris": [...], "with": "<where>", "line"?: n, "dir"?: "<folder uri>" }` — the
one verb for opening things elsewhere (`01-daemon-on-demand.md`, decision 5). `Launch`, not `Open`:
`Open` was already a listing's — a folder shown in the window — and is asked first, so an opening
verb of that name would never have been reached. `with` is:

- `default` — the file type's application (a double-click). Reply `{}`; 1331 when it would not
  start.
- `app:<desktop id>` — an application the person chose from Open with…. Reply `{}`; a launch that
  fails is logged, not an error.
- `tool:<id or role>` — an `open-in.toml` tool (`editor`, `agent`, or an id), with `line` for an
  editor. Reply `{ pid, reused, class }`; `Invalid` in the tool's words when it is not installed
  or not configured.
- `terminal` — a terminal in `dir` (else the first uri). Reply `{}`; 1251 for a folder on a server.
- `ai` — the chosen AI in a terminal in `dir`, told about `uris`. Reply `{ tool }`; 1252 for a
  server's folder or files, 1253 with no tool set, 1250 when it is not installed.

One rule table: the access log is written for everything asked for; a server's file with
`default`, `app:` or `tool:` is refused with 1330 ("{name} is on a server; Quick Look shows it")
before anything else is tried — the window sends such a double-click to Quick Look and never
asks; a folder with `default` is the window's to enter, and it never sends one.

## Removed

`OpenDefault`, `OpenIn`, `OpenTerminal`, `AiOpen` — each was one row of the table above with
its own copy of the rules; the old `Launch { app, uris }` is the `app:` row of the new one. `OpenWith` stays: it is the *question* the menu asks
(which applications open these?) and opens nothing; so do the tool-management verbs
(`OpenInList`, `OpenInTest`, `OpenInSessions`, `OpenInClose`, `SetOpenIn`) and `AiStatus`,
`AiConfigure`.

## `Hello`

Unchanged from 0.2.1; the daemon is started by the window now rather than by a socket unit, and
the pairing check is what lets it stay a build of its own.
