# 19 — Jarvis (AI query)

Builds on: `04-operations-and-undo.md` (context menu), `03-inspector.md` (inspector panel), `13-code-viewer-and-editor.md` (text detection), `06-remote-locations.md` (keyring, helper framing), `14-open-in.md` (Claude Code entry).

## Goal

Right-click a text file (or select several), choose **Jarvis ▸ Query…**, and a chat box opens beside the listing where the user asks questions about the file in plain words: "count lines", "how many times does `Ping` appear", "summarise this", "what does this config do". Answers stream in. Jarvis has one mode: it runs the AI the user has selected as that AI's own command-line tool in print mode, from the file's folder, with a prompt that names the file, and streams the tool's output into the panel. The tool's own login is used, so kiki holds no API keys. Which AI: Omarchy's choice by default (the AI web app in the keybinding in `~/.config/hypr/bindings.conf`, a heuristic), overridable in Settings, or a custom command.

## UI

- **Context menu**: a **Jarvis ▸** submenu on files whose kind is `text`, `code`, `document` (text mime) or `pdf`, and on folders. Items: **Query…** first, then **Summarise** (a canned query), **Explain this file**, and **Open in Claude Code** (plan 14's entry, when detected). The submenu is hidden when no provider is configured, with a single item "Set up AI…" that opens Settings (plan 20).
- **Chat panel**: replaces the inspector area (300 px, widens to 420 px) with a header showing the file name and a Close button, a transcript (user turns right-aligned in the surface colour, assistant turns plain, streamed), and an input box at the bottom with `Enter` to send and `Shift+Enter` for a newline. `Esc` closes. The panel remembers the transcript per file for the session.
- **Attached context**: the file's contents (up to the size cap below) are sent with the first question and kept in the conversation. For several selected files, each is attached with its name. For a folder, the first level of names and sizes is attached, not contents.
- **Local shortcuts first**: before calling the model, the helper answers a small set of exact questions itself so they are instant and free: line, word and byte counts; "how many times does X appear" for a quoted or backticked term; file size and dates. These show with a small "computed locally" label. Anything else goes to the model.
- **Key**: `Alt+Q` opens Query on the selection.

## Running the tool

| AI | Command | Notes |
|---|---|---|
| anthropic | `claude -p "{prompt}" --output-format text` | Claude Code's print mode |
| openai | `codex exec "{prompt}"` | Codex CLI |
| gemini | `gemini -p "{prompt}"` | Gemini CLI |
| xai | `grok "{prompt}"` | when a Grok CLI is installed |
| custom | the command from Settings with `{prompt}` and `{files}` | any tool |

The prompt is "Read `<file>` and answer concisely in plain text. Question: …", preceded by the session's earlier turns as `User:` / `Assistant:` lines so follow-ups keep context. The process runs in the first file's directory with `KIKI_SELECTION` in its environment; stdout streams to the panel as it arrives, stderr is shown on failure, Esc kills it. Local shortcuts (counts, occurrences) still answer without running anything.

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
