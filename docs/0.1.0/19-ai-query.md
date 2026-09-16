# 19 — Jarvis (AI query)

Builds on: `04-operations-and-undo.md` (context menu), `03-inspector.md` (inspector panel), `13-code-viewer-and-editor.md` (text detection), `06-remote-locations.md` (keyring, helper framing), `14-open-in.md` (Claude Code entry).

## Goal

Right-click a text file (or select several), choose **Jarvis ▸ Query…**, and a chat box opens beside the listing where the user asks questions about the file in plain words: "count lines", "how many times does `Ping` appear", "summarise this", "what does this config do". Answers stream in. Jarvis is provider-agnostic: by default it follows the AI Omarchy is set up to launch (read from the AI web-app keybinding in `~/.config/hypr/bindings.conf`, a heuristic the user can override in Settings), and it speaks each provider's API directly from the `jarvis` service plugin: Anthropic's Messages API, the OpenAI-compatible chat completions API (OpenAI, xAI, Ollama, any base URL), and Gemini's streaming API. Keys never enter the daemon or the shell.

## UI

- **Context menu**: a **Jarvis ▸** submenu on files whose kind is `text`, `code`, `document` (text mime) or `pdf`, and on folders. Items: **Query…** first, then **Summarise** (a canned query), **Explain this file**, and **Open in Claude Code** (plan 14's entry, when detected). The submenu is hidden when no provider is configured, with a single item "Set up AI…" that opens Settings (plan 20).
- **Chat panel**: replaces the inspector area (300 px, widens to 420 px) with a header showing the file name and a Close button, a transcript (user turns right-aligned in the surface colour, assistant turns plain, streamed), and an input box at the bottom with `Enter` to send and `Shift+Enter` for a newline. `Esc` closes. The panel remembers the transcript per file for the session.
- **Attached context**: the file's contents (up to the size cap below) are sent with the first question and kept in the conversation. For several selected files, each is attached with its name. For a folder, the first level of names and sizes is attached, not contents.
- **Local shortcuts first**: before calling the model, the helper answers a small set of exact questions itself so they are instant and free: line, word and byte counts; "how many times does X appear" for a quoted or backticked term; file size and dates. These show with a small "computed locally" label. Anything else goes to the model.
- **Key**: `Alt+Q` opens Query on the selection.

## Helper process

`kiki-plugin-jarvis`, a service plugin on the plugin framing (`API-PLUGIN.md`), spawned by the daemon on first use, idle-exit after 10 minutes. Its dependencies (TLS, HTTP) live in the helper only.

| Request | Fields | Reply |
|---|---|---|
| `Describe` | | `{ providers: ["anthropic"], configured: bool, model: string }` |
| `Configure` | `provider`, `secrets: { apiKey }`, `model?` | `{}` or `Auth` after a one-token test request |
| `Query` | `id`, `session: string`, `question`, `attachments: [{ name, text }]`, `history: [{ role, text }]` | streamed `Delta { id, text }` then `{ text, local: bool, usage: { input, output } }` |
| `Cancel` | `id` | `{}` |

**Calling Claude**: the Messages API over raw HTTPS (Rust has no official SDK). Defaults per the Claude API reference: model `claude-opus-5`, streaming, adaptive thinking (omit the `thinking` parameter, which is the default), `max_tokens` 16000, `output_config.effort` `medium` for short factual answers and `high` when the question asks for analysis, and the server-side refusal fallback enabled (`anthropic-beta: server-side-fallback-2026-07-01`, `fallbacks: "default"`); the helper checks `stop_reason` and shows a refusal as a message rather than a blank. A system prompt tells the model it is answering about the attached files, to be concise, and to say when a question needs the whole file when only a head was attached. Attachments are placed first in the user turn so the prefix caches across follow-up questions in the same session (`cache_control` on the attachment block).

**Modes.** `cli` runs the selected provider's own command-line tool in print mode from the file's directory (`claude -p … --output-format text`, `codex exec …`, `gemini -p …`, or a custom command with `{prompt}` and `{files}`), with a prompt that names the files so the tool reads them itself, and streams its stdout into the panel. It uses the tool's own login, so no key is needed, and Esc kills the process. `api` calls the provider directly from the `jarvis` plugin. `auto`, the default, uses `cli` when the tool is on `PATH`, else `api`.

**Credentials** (API mode), per provider, in order: the provider's environment variable (`ANTHROPIC_API_KEY`, `OPENAI_API_KEY`, `GEMINI_API_KEY`, `XAI_API_KEY`, `JARVIS_API_KEY` for custom); the keyring entry `{app: kiki, location: jarvis, field: <provider>}` from Settings; for anthropic, the `ant` CLI's login (`ant auth print-credentials --access-token` with the OAuth beta header) so Claude Code users need no separate key; Ollama needs none. The Settings page shows the provider, who chose it (Omarchy, settings or default) and the credential source. Model and base URL are settings with per-provider defaults (`claude-opus-5`, `gpt-5`, `gemini-2.5-pro`, `grok-4`, `llama3.1`).

**Size cap**: attachments are capped at 200,000 characters total (a head, with a note); larger files get the first and last 50,000 characters and the model is told. PDFs go through `pdftotext` first. Binary files are refused with a message.

**Cost visibility**: the panel footer shows tokens used this session from `usage`. There is no per-question price shown, since pricing changes; the Settings page links to the pricing page.

## Protocol additions

Daemon: `AiQuery { session, uris, question } -> { id }` then events `AiDelta { id, text }`, `AiDone { id, text, local, usage }`, `AiError { id, code, message }`; `AiCancel { id }`; `AiStatus -> { configured, model, source: "env" | "keyring" | "ant" | null }`; `AiConfigure { apiKey?, model? }`.

**IPC added**: `aiQuery(uri, question)`, `aiClose()`.

**Mockup**: a `AiQuery.dc.html` artboard, the chat panel open beside the list view on `main.rs`, with one local answer ("count lines") and one streamed model answer. Add before building.

## Verification

- With no credentials, the submenu shows only "Set up AI…"; after adding a key in Settings, `Query…` appears without restart.
- "count lines" on a 10,000-line file answers instantly with the exact count and the "computed locally" label; no network request is made (helper counts requests in a test mode).
- A model question streams its first delta within 2 s on a warm session and the transcript survives switching to another file and back.
- Attachments over the cap are truncated head and tail with a visible note, and the model is told.
- Cancel mid-stream stops the output and the helper's HTTP request within 200 ms.
- Follow-up questions in the same session report cache reads in `usage` (the attachment prefix caches).
- With the `ant` CLI logged in and no key, `AiStatus.source` is `ant` and a query succeeds.
