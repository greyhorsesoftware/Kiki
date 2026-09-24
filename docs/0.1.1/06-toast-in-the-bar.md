# 06 — The message in the bar

**Status:** **done 2026-09-24** ("instead of toast, can we show the message on the bottom bar
where the shortcuts are? have the shortcuts roll up and be replaced with the toast, then a
time later roll back in place").

The last destructive job's line with its Undo, an error from a tool that would not start, "Nothing
to undo" — what was a card floating over the view now takes the shortcut chips' place in the
bottom bar: the chips roll up out of the strip (180 ms), the message rolls in from below, and
when it is done — the toast's timer (`timers.toastMs`), its ×, or Undo — they change places
again. The status text at the right and the orb stay where they are. Nothing else changed:
`Kiki.Jobs.toast` still drives it, the e2e flows still find `toast-undo`, and Ctrl+Z is still
the keyboard's Undo. `ShortcutBar.qml` (`toast`, `undo`, `dismiss`); `Toast.qml` removed.

Tests: `tst_ShortcutBar::test_a_message_rolls_the_chips_up_and_takes_their_place`,
`test_undo_and_the_cross_are_the_bars_word`.
