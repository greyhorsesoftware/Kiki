# 19 — Jarvis (AI query)

**Status:** not in 0.1.0, and not planned. The in-app Jarvis panel this plan described — a chat about the selected files, local answers for counts, the provider's CLI in print mode, streaming — was built, then removed on 2026-09-19 (plan 31, decision 4). kiki does not hold a conversation; it hands one to the tool that is good at it.

What replaced it is in `29-release-readiness.md` section L2: **"Open AI here…"** (`Alt+Q`) and **"Open Terminal here…"**, in every file menu. The first starts the AI chosen in Settings → AI — Omarchy's own choice unless overridden — as its command-line tool in conversation mode (`claude`, `codex`, `gemini -i`, `grok`, or a custom command with `{prompt}`), in a terminal window opened in the folder, told which files are selected and to wait for the question.

What is left of this plan in the tree: `kikid/src/ai.rs` (provider choice, the terminal launch), the `AiStatus`, `AiConfigure`, `AiOpen` and `OpenTerminal` requests (`API-DAEMON.md`), and the Settings → AI page. Gone: `AiPanel.qml`, `AiQuery`, `AiCancel`, the `AiDelta` / `AiDone` / `AiError` events, attachments and their truncation, the local answers, the `shell aiQuery` and `aiClose` IPC calls, and the design mockup.
