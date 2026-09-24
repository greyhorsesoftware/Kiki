# kikid socket API — 0.1.1 delta

What changed on the wire since `docs/0.1.0/API-DAEMON.md`, which stays the authority for everything not listed here. Version stays `1`: every change below adds a field or a form and removes nothing, which the compatibility rules allow within a version.

## Jobs

### `chmod` over a selection (plan 01, owner 2026-09-24: "one job for the selection")

The `chmod` op takes a second form. The row in the 0.1.0 table becomes:

| op | fields | undoable |
|---|---|---|
| `chmod` | `items: [Uri]`, `recursive: bool`, and **either** `mode: u32` **or** `mask: u32` + `bits: u32` | yes |

- **`mode`** (as in 0.1.0): every item gets exactly this mode. Unchanged.
- **`mask` + `bits`** (new): for each item, and for everything under a folder when `recursive`, the new mode is `(old & !mask) | (bits & mask)`. The bits the mask names are set to `bits`; the rest stay as each file has them. This is what the Permissions grid over a selection sends: the boxes that were touched, and nothing else. Both are permission words in the `0o7777` space (setuid, setgid, sticky included); anything above is ignored. `mode` alone is the same as `mask: 0o7777, bits: mode`, and that is how the daemon runs it.
- One without the other (`mask` with no `bits`, or the reverse) fails the job with `mask and bits go together`; neither and no `mode` fails it with `missing mode`.
- A symlink is left alone, as before. A path whose mode is already what it would be is not touched and not journalled.

**Progress**: `total` is the number of items, `done` counts one per item as it finishes (a folder with `recursive` is one item however much is under it). `current` names the item in hand. The title is the 0.1.0 one, pluralised as a copy's is: `Change permissions of index.html`, `Change permissions of 3 items`.

**An item it cannot change** (gone, denied, a folder it cannot read into) is noted and the rest go on, the way a copy carries on past a file it cannot read. Every failure is a line in the job's log (`JobLog`: `level: "error"`, `text: "<path>: <why>"`). At the end the job is **`state: "failed"`** with `error` naming the first three and counting the rest:

```
1 of 3 could not be changed: never-was.txt (NotFound)
5 of 40 could not be changed: a (Denied), b (Denied), c (Denied), and 2 more — see the log
```

`<why>` is the error code (`NotFound`, `Denied`, …) or the message where there is one. Whatever did change is journalled as the partial inverse, so the job is `undoable: true` and the `Toast` is the usual one for a job that stopped part way:

```
← {"event":"Toast","job":41,"text":"Change permissions of 3 items — stopped part-way","undoable":true}
```

A `chmod` that changed nothing at all (every item failed) journals nothing and shows no toast: the failure is in the job's `error`.

**Undo and redo**: the inverse is one `chmodList` naming **every path actually changed** with the mode it had before — for a recursive chmod that is the folder and everything under it, not the one item named — so one `Undo` puts them all back and one `Redo` runs the same `chmod` again. The journal entry is the usual `{ job, title, inverse, redo }`:

```
{"op":"chmodList","list":[["file:///home/david/site/a.txt",420],["file:///home/david/site/c.key",384]]}
```

(`chmodList` is unchanged from 0.1.0 and still `hidden`.)

### Example

The Permissions tab over three files, "group write" ticked and Apply:

```
→ {"id":3,"type":"Submit","op":{"op":"chmod","items":["file:///home/david/site/a.txt","file:///home/david/site/b.sh","file:///home/david/site/c.key"],"mask":16,"bits":16,"recursive":false}}
← {"id":3,"ok":{"job":41}}
← {"event":"JobEvent","job":{"id":41,"op":"chmod","state":"done","done":3,"total":3,"title":"Change permissions of 3 items","error":null,"undoable":true,"name":"a.txt","count":3,…}}
← {"event":"Toast","job":41,"text":"Change permissions of 3 items","undoable":true}
```

`a.txt` 644 → 654, `b.sh` 755 → 755 (had the bit; not in the inverse), `c.key` 600 → 610.

## Everything else

No other request, event or field has changed in 0.1.1 so far.

## Log events

A plugin's `Log` event may carry `target: "wire"`: one line for a request made of the server (`→ …`) and one for its answer (`← …`). The daemon files them as every other line (`source: "<plugin> wire"`, `level: "debug"`), under the location's connection log and under the jobs using the plugin. They arrive whether or not a job is running.
