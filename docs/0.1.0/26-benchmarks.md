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

## Memory work driven by the first run

The first macOS run showed a 219 MB peak for `flat200k`. Splitting profiles into processes and streaming the test file showed the daemon's real share: the 200k listing is 35 MB fully enriched. Three changes landed with the suite: `Meta` stores mode, uid and gid as plain `u32`s with a sentinel instead of `Option<u32>` (48 bytes to 40, no `Option` niche loss); thumbnails and git state live in maps keyed by row instead of a slot per row; the mirror engine shares one `Arc<str>` per path between the map key, the entry and the plan action instead of three `String`s. And the inotify patch splices small changes into a name- or kind-sorted view (binary search plus one position-table memmove) instead of re-sorting the pool: 28.6 ms to 0.6 ms on 200k entries.

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
| `rss_start_mb`, `rss_listing_mb`, `rss_peak_mb`, `rss_after_release_mb` | resident set when the process starts, once the listing is fully enriched, the peak (`getrusage`), and after the listing, index and copy buffers are released | the plan-01 `mallopt` question: `rss_after_release_mb` within 5 MB of `rss_start_mb` plus the daemon's idle baseline after `flat200k`; on macOS the allocator keeps freed pages so only Linux answers it |

Output is a table on stdout and, with `--json`, a file `{ arch, os, at, version, results: { profile: { metric: number } } }`.

## Comparison (`kikid bench compare <baseline> <results> [--tolerance 25]`)

Every `_ms` and `_us` metric in the baseline is compared to the new run; a value more than the tolerance slower, and more than an absolute floor slower (3 ms, or 50 µs for the microsecond metrics), is a regression, so sub-millisecond timings never fail a run on noise. Each profile runs in its own child process (`KIKI_BENCH_CHILD`), so peaks and release numbers are independent and the first-chunk timing includes a cold process start. The command prints each one and exits non-zero, so CI fails.

## Baselines and CI

- `bench/baseline-<os>-<arch>.json` is checked in per machine class. `macos-aarch64` is the planning Mac, useful only for relative comparison of daemon changes. **`linux-x86_64` is the one that matters** and is now recorded (below); every baseline carries a `machine` object — CPU, cores, memory, kernel, filesystem and the directory the trees were generated in — because a listing benchmark measures the filesystem as much as the code.
- The CI `bench` job (x86_64, after the test job) generates `all`, runs the suite on a release build, uploads the JSON as an artifact, and compares against `bench/baseline-linux-x86_64.json` with a 40 percent tolerance when that file exists. Runner speed varies between GitHub hosts, so the tolerance is loose and the point is catching order-of-magnitude regressions, not tuning.
- Docs-only commits (`docs/**`, any `*.md`) do not run CI at all.

## The Linux baseline (2026-09-19)

`kikid bench gen all /home/gideon/kiki-bench && kikid bench run … --json`, build `6a6fc71+`, on
12th Gen Intel Core i5-1245U, 12 cores, 15.3 GB, kernel 7.2.5-3-omarchy, **btrfs** (not tmpfs —
the numbers are what a real home directory gives).

| Metric | flat10k | flat200k | deep100k | photos |
|---|---|---|---|---|
| `phase1_first_chunk_ms` | 1.59 | 3.52 | 0.27 | 0.29 |
| `phase1_done_ms` | 4.69 | **56.31** | 0.28 | 0.30 |
| `window_ms` | 0.34 | 0.34 | 0.33 | 0.32 |
| `window_meta_ms` | 1.30 | 1.27 | 1.35 | 0.98 |
| `enrich_ms` | 4.15 | 93.57 | 0 | 0.19 |
| `sort_name_ms` | 1.20 | 15.88 | 0 | 0.01 |
| `rescan_ms` | 4.55 | 100.18 | 0.06 | 0.10 |
| `patch_ms` | 0.09 | 0.62 | 0.04 | 0.06 |
| `index_build_ms` | 2.07 | 38.27 | 22.36 | 0.06 |
| `index_query_us` | 34.9 | 693.6 | 355.3 | 1.3 |
| `mirror_scan_ms` | 9.69 | 213.77 | 88.09 | 0.21 |
| `copy_mb_s` | 3140.9 | 3202.8 | 2985.1 | 2859.1 |
| `rss_start_mb` | 3.4 | 3.5 | 3.4 | 3.6 |
| `rss_listing_mb` | 7.1 | 27.0 | 5.9 | 7.9 |
| `rss_peak_mb` | 10.2 | 87.4 | 35.7 | 9.2 |
| `rss_after_release_mb` | 8.2 | **14.2** | 6.5 | 8.4 |

Against the plan-01 acceptance:

- **200,000 entries, phase 1 under 1 s** — 56 ms, eighteen times inside the budget. The macOS
  baseline reads 1211 ms for the same profile; that gap is `getdents64` against macOS's readdir,
  not a regression, and it is why a macOS-only baseline could not answer this.
- **First chunk within a frame** — 3.52 ms at 200k.
- **A `Window` at any offset under 5 ms** — 0.34 ms, flat across every profile.
- **The `mallopt` question** (`rss_after_release_mb` within 5 MB of `rss_start_mb`): answered at
  last, and the answer is *nearly*. Peak 87.4 MB for 200k entries comes back to 14.2 MB against a
  3.5 MB start — about 84% of the peak returned to the kernel, with ~11 MB retained rather than
  the 5 MB the plan hoped for. Good enough to keep the settings; not good enough to call closed.

## Shell half (Omarchy, plan 11 harness)

First paint after the first chunk, scroll frame time over 200k rows, key-to-selection latency, window open to home painted, and the mirror scan against the mock SFTP server. The driver writes the same JSON shape with a `shell` profile so the compare command covers both halves in one file.

## Not measured here

Real remote servers, real removable media, and real photo libraries; those are the plan-10 performance pass by hand, on Omarchy, with the numbers pasted into the baseline file with a note.

## Verification

- `bench::tests`: `compare` flags only slower timings and only beyond the tolerance; a 300-file tree produces every metric and `entries` counts files and folders.
- `kikid bench gen all` on a tmpfs completes in under a minute; `run` on the same trees prints all four profiles and writes valid JSON.
- Introducing a deliberate `sleep(50ms)` in phase 1 makes `compare` fail against the checked-in baseline.
