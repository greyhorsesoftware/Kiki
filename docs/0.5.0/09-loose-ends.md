# 09 — Loose ends from 0.3 and 0.4

**Status:** built, 2026-10-02 (items 1, 3, 4; item 2 is the owner's, its check done). Small things that were decided quickly during the 0.3/0.4
work and deserve a second look, each an hour or less.

1. **The thumbnail fail marker's day.** A file kiki could not thumbnail gets a marker with the
   reason (`kiki:Reason` tEXt) and is not tried again for 24 h (`thumbs.rs`, `fail_ttl`,
   `KIKI_THUMB_FAIL_TTL_S`). A day was a guess (owner: "why a day?"). What the day is for: a
   file that is *bad* (truncated, wrong extension) will still be bad in an hour, and retrying
   costs a decode per paint; a file that was bad *for a moment* (still being written, on a
   share that dropped) is fine in a minute. The marker knows which (`Why::BadFile` is the only
   one written; `CouldNotRun` never is), and the mtime is in the marker, so a file that
   changed is retried at once regardless. **Decision:** keep the day for a file whose mtime
   has not moved — it is the right length for "bad" — and make the number a setting in
   `settings.toml` (`thumbnails.retryAfterS`) rather than an environment variable, so it is
   one line to change without a restart. No Settings-window row. **Done 2026-10-02:** `[thumbnails] retryAfterS` (`config.rs` default 86 400, `thumbs.rs` reads it on every look, the environment outranks it for the tests; `20-settings.md` names it).
2. **`~/kiki-transfer-demo*`** in the owner's home: nothing in the tree or its history names
   them, so they were made by hand for a transfer test and left. Yours to delete; noted here
   so it is not forgotten. While there: check `tests/e2e/flows/*.py` for any `expanduser("~")`
   that *writes* (reads of `~/Pictures` and the like are fine) and there must be none. **Checked 2026-10-02:** none — the five uses read `~/Pictures`, `~/Downloads`, `~/Videos`, the Omarchy theme file and the thumbnail cache's path.
3. **The 0.2 video flow (`whatsnew020.py`)** still uses the image-over-black title card; the
   recorder now has the About-mark card. Either it is switched to the mark or it is deleted —
   a flow nobody records is a file to keep compiling. **Decision:** delete it; the 0.2 video
   exists. **Deleted 2026-10-02.**
4. **`Recorder` keeps its raw capture beside the output** (`kiki-whatsnew.mp4.raw.mp4`) so a
   caption change is a re-composite; nothing re-composites yet. Either a `--recomposite` flag on
   the flow (`KIKI_E2E_RECOMPOSITE=1` skips the recording and runs `caption` on the raw) or the
   raw goes. **Decision:** the flag — it is a dozen lines and the reason the raw is kept. **Done 2026-10-02:** `KIKI_E2E_RECOMPOSITE=1` on any recording flow; a real take writes `<out>.marks.json` (when each mark was made, and the end) beside the raw, and a re-composite pairs the flow's marks, in order, with those times — today's words at the original moments. A flow with a different number of marks is told to record again.
