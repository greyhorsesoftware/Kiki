# API delta — 0.2.0

What changed in the daemon's protocol since 0.1.1.

## `OpenDefault`

`{ "type": "OpenDefault", "uri": "<file or remote uri>" }` — opens the file with the default
application for its type (`xdg-open` when none is registered). A local file: `{}` at once. A
remote file: `{ "job": <id> }` — a copy job brings it to `$XDG_CACHE_HOME/kiki/open/<moment>/`
and it is opened from there when the job is done. Errors: `Io` with the reason. What a
double-click on a file sends.

## `Launch` and `OpenIn` with remote files

Both fetch any remote file among `uris` the same way before starting the application or tool
on the copies, and then answer `{ "job": <id> }` (`OpenIn` adds `class`) instead of at once;
with only local files nothing changed.

## Write-back

A fetched copy that is saved is sent back to where it came from by a copy job of the daemon's
own ("Save <name> back to <location>"), replacing the original. Nothing to ask for: it follows
from the fetch. The copies are gone at the daemon's next start.

## `title` and `policy` on an operation

An op may carry `"title"`, the words the job goes by in the activity list; without it the job is
named for what it does, as before.

## `policy` on an operation

A copy or move op may carry `"policy": "replace" | "keepBoth" | "skip"`, which answers every
collision in it without a prompt. What the write-back uses; a window may use it too.

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
