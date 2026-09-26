# API delta — 0.2.0

What changed in the daemon's protocol since 0.1.1.

## `OpenDefault`

`{ "type": "OpenDefault", "uri": "<file uri>" }` — opens a LOCAL file with the default
application for its type (`xdg-open` when none is registered): `{}`. A file on a server is
refused with 1330 ("{name} is on a server; Quick Look shows it") — the window sends such a
double-click to Quick Look and never asks; 1331 when the application could not be started.
What a double-click on a file sends. (For a day, 2026-09-24, this fetched a remote file, opened
the copy and sent a save back; withdrawn 2026-09-25 — remote editing is `docs/0.3.0`.)

## `Launch` and `OpenIn` with remote files

Both refuse a remote file among `uris` with 1330 the same way; with local files nothing changed.

## Quick Look

The window's own verbs (`05-quicklook.md`); every failure is a number in the 1300 block.

`{ "type": "ReadText", "uri": "<file uri>" }` — the whole text of a LOCAL file, UTF-8 (bytes that
are not become the replacement mark), cut at 4 MiB: `{ "text", "bytes", "truncated" }`, where
`bytes` is how many of the file's bytes the text holds (the cap when cut — "cut at 4 MB") and
`truncated` says the text stops short of the file (a character split by the cut is dropped
rather than shown broken). For code the reply also carries `language` (the grammar's name,
"Rust") and `runs`: `[[offset, length, kind], …]` in UTF-16 units over `text`, `kind` one of
keyword, string, comment, number, type, function, attribute, punctuation — absent for prose,
for a name no grammar claims, and for text over 512 KiB; the colours are the window's
(`Theme.code`). Errors: 1300 for a binary (a NUL in the first 8 KiB), 1301 for a remote file,
1302 when it cannot be opened or read.

`{ "type": "PdfInfo", "uri" }` — a LOCAL PDF: `{ "pages", "width", "height" }` (points of the
first page, from `pdfinfo`). `{ "type": "PdfPage", "uri", "page", "width" }` — page `page`
(from 1) rendered `width` pixels wide (16–8192, default 1024; height by the page's own
proportions) by `pdftoppm` into the thumbnail cache's `pdf/`, keyed by file, mtime, page and
width so a page asked twice is rendered once: `{ "path", "width", "height" }`. Errors: 1310
when poppler is not installed, 1311 when the file will not read as a PDF, 1312 for a page
beyond the end ({name} has {pages} pages, not page {page}).

`{ "type": "QuickLookFetch", "uri" }` — a LOCAL uri: `{ "path" }` at once. A remote one:
`{ "job", "path" }` — a copy job brings it to `$XDG_CACHE_HOME/kiki/open/<moment>/` and `path`
is where it will be when the job is done; **the folder is not watched**: a look writes nothing,
so nothing is sent back. `{ "type": "QuickLookDrop", "path" }` removes a fetched copy and the
folder of the moment it came in: the window sends it when the look is over (closed, or moved on
to another file), so a look leaves nothing; a path not under the cache's `open/` is refused
with 1321; a copy already gone is nothing to do. Errors: 1320 when the fetch could not start.

## `title` and `policy` on an operation

An op may carry `"title"`, the words the job goes by in the activity list; without it the job is
named for what it does, as before.

## `policy` on an operation

A copy or move op may carry `"policy": "replace" | "keepBoth" | "skip"`, which answers every
collision in it without a prompt. A window may use it.

## Numbered errors

A failure the user is told about carries a number: `err: { code, message, n, params }` on a
reply and `errorN` / `errorParams` on a failed job, beside the English `message` / `error`. The
window says `n` in its own language from its catalog (`error.<n>`, with `params` filled in) and
shows `message` only for a number it has no words for. The numbers so far:

| n | said |
|---|---|
| 1201 | a server has no trash |
| 1202 / 1203 / 1204 | {n} of {total} could not be copied / moved / changed: {first} (`more` = how many beyond the three named) |
| 1210 | copied, but the original could not be removed: {which} |
| 1211 | arrived as {got} bytes of {size} |
| 1212 | a name cannot be empty or contain / |
| 1220 / 1221 / 1222 | mirror: same folder / destination inside source / source inside destination |
| 1223 / 1224 | mirror safety: would delete {n} of {total} / outside the replica root: {path} |
| 1230 | cancelled |
| 1240–1243 | bsdtar missing / nothing to compress / items must share a parent / unknown archive format |
| 1250 | {name} is not installed |
| 1251–1253 | AI and terminal: local folders only / copy here first / no tool set |
| 1260 / 1261 | no location for {scheme}://{host} / the keyring refused the secret |
| 1270–1272 | plugin timed out / exited / none for {scheme} |
| 1280 / 1281 | listing timed out / not a folder |
| 1300 / 1301 / 1302 | Quick Look text: {name} is not a text file / {name} is on a server; fetch it first / {name} could not be read: {error} |
| 1310 / 1311 / 1312 | Quick Look PDF: poppler (pdftoppm) not installed / {name} could not be read as a PDF / {name} has {pages} pages, not page {page} |
| 1320 / 1321 | Quick Look fetch: {name} could not be fetched: {error} / {path} is not a fetched copy (`QuickLookDrop` on anything outside the cache's `open/`) |

Errors without a number are what they were: the daemon's or a library's English words.

## Plugin forms: select options are `{ value, label }`, and labels carry their words

`Describe`'s form fields of kind `select` carry `options: [{ "value": "implicit", "label": "Implicit TLS" }, …]`
and a `default` that is a value. The window stores and sends the value; what it shows is the
option's `labels[<language>]` when the plugin sent one, else its `label`. A field's own label
goes the same way: `"label": "Host", "labels": { "es": "Servidor", "ja": "ホスト" }` — the words
are the plugin's, from its own `i18n/<lang>.json` files (`sdk::localised(form, &words)`), not the window's; a plugin without them is
shown in English. The SDK's `select_field` takes `(value, label)` pairs. Until 0.2.0 an option was one string,
shown and stored alike — the FTPS plugin's `encryption` was `"Explicit TLS (AUTH TLS)"` /
`"Implicit TLS"`; it is `"explicit"` / `"implicit"` now. **Migration**: a location saved before 0.2.0
holds the label where the value goes; the daemon reads it as the value the plugin's own form
gives that label (`"Implicit TLS"` → `"implicit"`) wherever locations are read, and rewrites
the file once at its start. A value no option knows is left as it is.

### `Search`

`indexAge` is left out of the reply while the index has never been built (`built_at` 0); it used to come back as the seconds since 1970. The window then says "no index yet" instead of an age.

### `Open` on a server, and `Count`

`Open` on a remote folder no longer connects before answering: the connect happens on the
listing's scan, so the reply comes at once and a window's other pane is not kept waiting. A
connect that fails is reported the way a scan failure is — the `Count` event with `done: true`
carries `error` (the sentence, English) and now also `errorN` and `errorParams` when the failure
has a number (1260 for an unknown host, the 127x plugin failures, …), so the window says it in
its language. An unknown host used to fail the `Open` reply itself.

### The socket, and `Hello`

The socket is `$XDG_RUNTIME_DIR/kiki-<version>.sock` (`kiki-0.2.0.sock`), not `kiki.sock`:
named for the version, so a window and a daemon of the same build find each other and no other;
`KIKI_SOCKET` overrides it as before. `Hello`'s reply gains `kikid` (the daemon's version,
"0.2.0") beside `daemon` ("kikid 0.2.0"); a window refuses a daemon whose `kikid` is not its own.
