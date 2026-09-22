# 19 — Jarvis (AI query)

**Status:** not in 0.1.0 — replaced by "Open AI here…" (plan 29 L2).

The in-app Jarvis panel this plan described was built, then removed on 2026-09-19 (plan 31, decision 4): kiki does not hold a conversation; it hands one to the tool that is good at it. **"Open AI here…"** (`Alt+Q`) and "Open Terminal here…" open a terminal in the folder, the AI chosen in Settings → AI told which files are selected.

What is left in the tree: `kikid/src/ai.rs` (provider choice, the terminal launch), the `AiStatus`, `AiConfigure`, `AiOpen` and `OpenTerminal` requests, and the Settings → AI page. Gone: `AiPanel.qml`, `AiQuery`, `AiCancel`, the `AiDelta` / `AiDone` / `AiError` events, attachments, the local answers, the `aiQuery` and `aiClose` IPC calls, and the mockup.
