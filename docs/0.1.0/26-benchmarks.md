# 26 — Benchmarks

Builds on: `01-daemon-and-listing.md` (the listing budgets), `11-testing.md` (the performance table and the e2e harness), `10-polish-and-packaging.md` (the performance pass).

## Goal

The speed claims in plans 01 and 11 become numbers that are measured the same way every time, kept in a file, compared across runs, and fail CI when they slip. Two halves: the daemon half runs anywhere `cargo` runs and is built here; the shell half needs Quickshell under `cage` and stays with the plan-11 harness on Omarchy.

## Synthetic trees

`kikid bench gen <profile> <dir>` builds fixed inputs so runs are comparable:

| Profile | Contents | Exercises |
|---|---|---|
| `flat10k` | 10,000 files of seven kinds by name, 100 folders | the plan-01 budget directory |
| `flat200k` | 200,000 files, 2,000 folders | the string pool, parallel sort, window latency at scale |
| `deep100k` | 100 × 10 directories × 100 files | the search index build, mirror scans, the directory walk |
| `photos` | 200 PNGs of 256×256 with real pixels | the thumbnail pipeline, the icon-view smart default |
| `all` | one of each under the given directory | one `run` measures everything |

Files are one to six bytes so the trees are cheap to create and metadata dominates, which is what kiki's hot paths touch.

## Measurements (`kikid bench run <dir> [--json out]`)

Per profile, in one process, cold listing first:

| Metric | What it times | Budget (plan 01/11, x86_64 Omarchy) |
|---|---|---|
| `phase1_first_chunk_ms` | open to the first `Count` event | 16 ms (10k) |
| `phase1_done_ms` | open to the last chunk | 100 ms (10k), 1,000 ms (200k) |
| `window_ms` | a 60-row `Window` answered from the pool | 5 ms |
| `window_meta_ms` | until every row in that window has `Meta` (phase 2) | 100 ms |
| `enrich_ms` | stat every row | informational |
| `sort_size_ms`, `sort_mtime_ms`, `sort_name_ms` | sort plus the `Reset` | 300 ms (200k) |
| `filter_ms` | a name filter over the pool | 16 ms |
| `rescan_ms`, `patch_ms` | re-list keeping `Meta`; one in-place add and remove | 100 ms, 5 ms |
| `index_build_ms`, `index_query_us` | index the tree; a prefix query, averaged over 100 | informational; 1,000 µs |
| `mirror_scan_ms`, `mirror_actions` | scan and diff against an empty replica | 5 s for 5k files over SFTP; local is informational |
| `thumbs_ms`, `thumbs_made` | 50 thumbnails through the cache pipeline | informational |
| `copy64m_ms`, `copy_mb_s` | a 64 MiB copy through the job copier | disk-bound; informational |
| `rss_peak_mb` | peak resident set at the end of the profile (`getrusage`) | the plan-01 `mallopt` question: within 5 MB of idle after `flat200k` is released |

Output is a table on stdout and, with `--json`, a file `{ arch, os, at, version, results: { profile: { metric: number } } }`.

## Comparison (`kikid bench compare <baseline> <results> [--tolerance 25]`)

Every `_ms` and `_us` metric in the baseline is compared to the new run; a value more than the tolerance slower, and more than one millisecond slower in absolute terms, is a regression. The command prints each one and exits non-zero, so CI fails.

## Baselines and CI

- `bench/baseline-<os>-<arch>.json` is checked in per machine class. The first is this planning Mac (`macos-aarch64`), useful only for relative comparison of daemon changes; the Omarchy x86_64 baseline is taken on the real machine during the plan-10 performance pass and is the one that matters.
- The CI `bench` job (x86_64, after the test job) generates `all`, runs the suite on a release build, uploads the JSON as an artifact, and compares against `bench/baseline-linux-x86_64.json` with a 40 percent tolerance when that file exists. Runner speed varies between GitHub hosts, so the tolerance is loose and the point is catching order-of-magnitude regressions, not tuning.
- Docs-only commits (`docs/**`, any `*.md`) do not run CI at all.

## Shell half (Omarchy, plan 11 harness)

First paint after the first chunk, scroll frame time over 200k rows, key-to-selection latency, window open to home painted, and the mirror scan against the mock SFTP server. The driver writes the same JSON shape with a `shell` profile so the compare command covers both halves in one file.

## Not measured here

Real remote servers, real removable media, and real photo libraries; those are the plan-10 performance pass by hand, on Omarchy, with the numbers pasted into the baseline file with a note.

## Verification

- `bench::tests`: `compare` flags only slower timings and only beyond the tolerance; a 300-file tree produces every metric and `entries` counts files and folders.
- `kikid bench gen all` on a tmpfs completes in under a minute; `run` on the same trees prints all four profiles and writes valid JSON.
- Introducing a deliberate `sleep(50ms)` in phase 1 makes `compare` fail against the checked-in baseline.
