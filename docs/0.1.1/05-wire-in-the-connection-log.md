# 05 — The wire in the connection log

**Status:** **done 2026-09-24** ("for sftp/ftps — can we get commands sent/received to the
connection log? I'm sure the libs have something we can tap into").

## What

A location's connection log (and a job's log, for the requests made on its session) shows the
conversation with the server: one line for each request, one for each answer. `→` is what kiki
sent, `←` what came back, under the source `<plugin> wire`.

```
+0.41s  debug  ftps wire   → PASV
+0.52s  debug  ftps wire   ← 227 Entering Passive Mode (…)
+0.53s  debug  ftps wire   → MLSD /home/djclark
+0.61s  debug  ftps wire   ← 150 Here comes the directory listing
+0.62s  debug  sftp wire   → OPENDIR /home/djclark/site
+0.70s  debug  sftp wire   ← READDIR 41 names; CLOSE
```

## Where it comes from

- **FTPS**: suppaftp traces its control channel itself — `CC OUT: PASV` on the way out and the
  reply line, as bytes, on the way in — at `Trace`, which the SDK never listened to (russh's
  trace is packet dumps). The SDK now lets suppaftp's trace through, turns those two lines into
  wire lines (decoding the bytes; dropping the duplicate it traces of a reply's first line) and
  refuses everything else at that level, before it is formatted. `PASS hunter2` is redacted like
  every line, after the arrow.
- **SFTP**: russh-sftp has nothing to tap — `packet type 12` on decode is all it says — so the
  plugin says it at each call: `exec find …` / `exit 0, 41 entries` for the fast scan, `OPENDIR`
  / `READDIR n names; CLOSE`, `LSTAT` / `size, mode`, `OPEN (read)` / `bytes in n READs; CLOSE`
  (one line for the whole download, never one per chunk), `OPEN (write, create)`, `SETSTAT`,
  `MKDIR`, `RENAME`, `REMOVE`, `RMDIR`; a refusal is logged in the server's own words
  (`answered`).

The `wire` target is heard at `Debug` whether or not a job is running — a browse is exactly when
someone opens the connection log — so a plugin keeps to one line per request. The SDK exposes it
as `sdk::wire!(…)`.

## Tests

`kiki-plugin-sdk` (`suppaftps_control_channel_is_the_wire`,
`the_wire_is_heard_without_a_job_and_trace_is_suppaftps_alone`);
`mock_sftp::the_connection_log_carries_the_wire`;
`mock_ftps::the_connection_log_carries_the_control_channel` (the password never appears).
