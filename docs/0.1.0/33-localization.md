# 33 — Localization: English and Spanish, chosen by the OS

**Status:** plan, written 2026-09-21. **Nothing here is built** — checked again on 2026-09-21: no `qml/kiki/i18n/`, no `Kiki.T`, and `qsTr` still appears 0 times in `qml/`. Whether it lands before the 0.1.0 tag or after it is the owner's call; it touches every file with a word in it, so it should not run alongside another phase.

Asked for (owner, 2026-09-21): *"create a plan to support localization, implement en, sp localizations … have it key off the OS setting — defaulting to en if not supported."* Spanish is `es` everywhere below (ISO 639-1); "sp" is not a language code and no OS will ever report it.

## Where it stands — measured, not remembered

- **There is no localization at all.** `qsTr` appears **0** times in `qml/`. Every word on screen is an English literal.
- **How many words**: about **300** literal strings in 65 QML files by a rough count that misses sentences built in code. By file: `Shell.qml` 197 · `SettingsWindow` 79 · `MirrorWorkspace` 56 · `LocationDialog` 51 · `Inspector` 39 · `ShortcutsOverlay` 31 · `Keymap` 28 · `Format` 25 · `Ops` 21 · `JobLogWindow` 18 · `Jobs` 18 · `IntegrationDialog` 16.
- **Sentences are glued together**, with English grammar in the glue: `n + " items" + … + " selected"` (`Shell.countText`), `"These " + n + " items are on " + host + ", which has no trash. They will…"` (`Ops.trashSelection`), `"yesterday " + hm`, `"5 min ago"` (`Format`). Word order and plurals differ between languages; none of these survive translation as they are.
- **The daemon speaks English too**: about **72** sites build an error string (`VfsError::Io("a server has no trash: use Delete (Shift+Del)…")`), and those strings are what a failed job and a toast show.
- **Plugins supply words**: the Add-location form is drawn from each plugin's `Describe` — field labels and option lists. **And a trap**: a select field stores its *label* as its *value* — `sdk::select_field("encryption", "Encryption", &["Explicit TLS (AUTH TLS)", "Implicit TLS"], …)` (`kiki-plugin-ftps/src/main.rs:365`), and `locations.toml` holds `encryption = "Explicit TLS (AUTH TLS)"`. Translate that label and every saved location stops matching.
- **Tests and the IPC lean on English**: `shell contextMenu <label>` finds a menu item *by its label*; flows assert `"Delete permanently?"`; QML tests compare label text.
- **Qt's own translation system is not available to us.** Quickshell 0.3.1 never installs a `QTranslator` (nothing in the binary refers to one, to `.qm` files or to an `i18n` directory), and QML cannot install one for itself. So `qsTr()` would compile, run, and return English forever. `lupdate`/`lrelease` are not installed either, and would be a new build dependency.
- **Qt already reads the OS correctly**, without needing the glibc locale to be generated. Measured with `Qt.locale()`:

  | Environment | `uiLanguages` | numbers · dates |
  |---|---|---|
  | `LANG=en_US.UTF-8` | en-Latn-US, en-US, en-Latn, **en** | `1,234,567.5` · `9/21/26` |
  | `LANG=es_ES.UTF-8` | es-Latn-ES, es-ES, es-Latn, **es** | `1.234.567,5` · `21/9/26` |
  | `LANG=en_US` + `LC_MESSAGES=es_MX` | es-Latn-MX, es-MX, es-Latn, **es** | `1,234,567.5` · `9/21/26` |
  | `LANG=en_US` + `LANGUAGE=es:en` | es-…, **es**, en-…, **en** | `1,234,567.5` · `9/21/26` |
  | `LANG=fr_FR.UTF-8` | fr-…, fr | `1 234 567,5` · `21/09/2026` |
  | `LANG=C` | C | `1234567.5` |

  The precedence is the platform's own (`LANGUAGE`, then `LC_ALL`/`LC_MESSAGES`, then `LANG`), and **language and formats are separate settings**: a user with Spanish messages and US formats gets exactly that. The list is already ordered most-specific first and ends in the bare language.

## Decisions — defaults chosen here, each the owner's to overrule

1. **kiki's own catalog, not Qt's.** One file per language, as JavaScript libraries — `qml/kiki/i18n/en.js`, `es.js`, `.pragma library`, the pattern `viewmenu.js` already uses — so they load the same under Quickshell and under `qmltestrunner`, with no file I/O, no build step and no new tools. A singleton, `Kiki.T`, is the only thing the rest of the code talks to: `T.tr("trash.remote.one", { name: n, host: h })`.
2. **Keys are names, not English.** `"trash.remote.one"`, not `"%1 is on %2, which has no trash…"`. The English copy can then be edited without orphaning the Spanish. `en.js` is the source of truth **and the fallback, key by key**: a key missing from `es` shows the English, never the key, and is logged once.
3. **Whole sentences with named placeholders; plurals by form.** `{name}`, `{host}`, `{n}` — never concatenation. A plural key holds `{ one: "…", other: "…" }` and `T.tr` picks by `n`; English and Spanish share the rule (`n == 1`), and the shape leaves room for a language that does not.
4. **The language is the OS's**: walk `Qt.locale().uiLanguages`, take the first whose language has a catalog, else **`en`**. `es_MX`, `es_AR`, `es_ES` all land on `es` — one neutral Spanish; a regional file (`es-MX.js`) can be added later and the walk will find it first without code changing. Read once at start: changing the OS language means logging in again anyway.
5. **Numbers, sizes, dates and times keep following `Qt.locale()`** — the OS's *format* settings — independently of the language. `Format.qml`'s hand-rolled pieces (`"just now"`, `"5 min ago"`, `"yesterday 14:02"`) become keys; the digits, separators and clock in them come from the locale.
6. **Errors travel as codes; the words live in the shell.** A daemon error gains a stable `code` and its parameters beside the English `message` it has today (`{ code: "trash.remote", message: "a server has no trash…" }`). The shell shows `T.tr("error." + code, params)` when it has that key and **the daemon's English otherwise** — so nothing regresses while the 72 sites are worked through, and a plugin's raw library error (`russh`, `suppaftp`) still reaches the user in the only language it exists in.
7. **Plugin forms**: field and option *labels* are looked up in the shell's catalog by id (`location.ftps.encryption`, `location.ftps.encryption.explicit`), falling back to what the plugin sent. **First the trap is fixed**: `select_field` options become `(value, label)` pairs with stable values (`"explicit"`, `"implicit"`), and saved locations holding the old label are read as the value they meant. Until that is done, option labels stay English — translating them first would break saved locations.
8. **Not translated**: file names, paths, URIs and hosts; the job log and the connection log (diagnostic text from libraries — English is what a bug report needs); IPC and `shell state` JSON; key names in `KeyChip` (`Ctrl`, `Del`, `F2` — a Spanish keyboard prints `Supr`, and that is a later refinement, not a blocker).
9. **No in-app language switch in this plan.** The ask was the OS's setting; a `[general] language` override is ten lines if someone asks for it later. Tests pick a language by setting `T.language` directly.
10. **The harness pins English**: `tests/e2e/run.sh` exports `LANGUAGE=en`, and `make test-qml` does the same, so a developer whose desktop is in Spanish gets the same results. `shell contextMenu` and `shell action` take **ids**, not labels, from then on — the label lookup is kept only as a fallback for existing flows until they are moved.

## To build, in this order

| # | What | Size |
|---|---|---|
| L1 | **The mechanism**: `Kiki.T` (language walk, lookup, per-key fallback, `{placeholders}`, plural forms, the once-only missing-key log), `i18n/en.js` with its first keys, and `tst_T` — every row of the table above, a missing key, a missing placeholder, plural 0/1/2, `es-MX → es`, `fr → en`, `C → en`. | ½ day |
| L2 | **Extraction**, file by file in the order of the counts above. Each glued sentence becomes one key. `Format.ago/when`, `Shell.countText`, the `Ops` confirmations and `Jobs.headline/statusLine/completion` are the ones with grammar in them and get tests of their own in both languages. Existing tests that compare label text are moved to keys or pinned to `en`. | 2–3 days |
| L3 | **Spanish**: `es.js` complete, against a short glossary agreed first so the same thing has the same name everywhere (*Papelera*, *Carpeta*, *Ubicaciones*, *Copiar / Mover*, *En paralelo*…). **A machine draft is a draft**: it needs one pass by a native reader before it is called done, and the Status line says which it has had. | 1 day + the review |
| L4 | **Guards**, so it stays translated: a test that reads the sources for `T.tr("…")` and fails on a key missing from `en`, a key missing from `es`, an unused key, or placeholders that differ between the two; and a lint that fails on a new bare literal in `text:` / `label:` / `title:` / `tip:` (symbols and an allowlist excepted). **A look at every dialog in Spanish** — it runs 20–30 % longer than English — by running the e2e harness with `LANGUAGE=es` and photographing each (`grim` is already there); anything clipped is fixed by layout, never by abbreviating the Spanish. | 1 day |
| L5 | **Daemon error codes**: audit the 72 sites for which reach a toast or a failed job; codes and params for those; `API-DAEMON.md` amended. Then decision 7's `(value, label)` split in the plugin SDK, with the migration for saved locations and its test. | 1½–2 days |
| L6 | **Packaging and documents**: `org.kiki.App.desktop` gains `Name[es]` / `GenericName[es]` / `Comment[es]`; README says how to add a language (copy `en.js`, translate, the guard tells you what is missing). | 2 h |

About **6–7 working days**, most of it L2. L1–L4 stand on their own: after them the window is fully bilingual and only daemon-made error text is still English.

## Verification

- With `LANG=es_ES.UTF-8` kiki starts in Spanish; with `LANG=en_US.UTF-8` in English; with `LANG=fr_FR.UTF-8`, `LANG=C` and no `LANG` at all, in English. `LC_MESSAGES=es_MX` over `LANG=en_US` gives Spanish words with US numbers and dates.
- No key is ever shown on screen: a key missing from `es` shows English and is logged once.
- `1 elemento` / `2 elementos`, `1 item` / `2 items`; the remote-trash question reads as a sentence in both languages, for one item and for several.
- The guard test fails when a `T.tr` key is missing from either catalog, when placeholders differ, or when a bare literal is added to a `text:`.
- Every dialog photographed in Spanish, nothing clipped.
- A location saved before L5 with `encryption = "Explicit TLS (AUTH TLS)"` still connects after it.
- `make test` is green on a machine whose own language is Spanish.
