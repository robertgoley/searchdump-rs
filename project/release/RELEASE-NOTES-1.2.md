# searchdump-rs 1.2.0 release notes

Status: **released 2026-11-16** as tag `v1.2.0` (`release/1.2` merged into `main`).
All ROADMAP-1.2 section 9 questions are decided; the `min` margin will be evaluated again in 1.3.
The same material is summarised in [`CHANGELOG.md`](../../CHANGELOG.md) under **1.2.0**.

## Disclaimer

searchdump-rs is provided **as is, without warranty of any kind**, as set out in its
[MIT license](../../LICENSE) (the same warranty disclaimer as the elasticdump-rs code it derives from; Node
elasticdump, whose behaviour it follows, has the equivalent disclaimer in Apache-2.0, sections 7 and 8). The authors
accept no liability for any damage from its use, including data loss. See [Disclaimer](../../README.md#disclaimer). Try it on a non-production
cluster first and verify every backup independently (restore it, compare counts) before relying on it.

## Highlights

- **Server-aware sizing, `--serverLimits=off|auto|min|check`** (`searchdump` and `searchdump-multi`).
  One read-only probe at the start reads the smallest data node's heap, circuit breakers, indexing pressure,
  `http.max_content_length` and thread pools, and sizes pages, bulks and `searchdump-multi`'s parallelism from
  them. Default `auto`. `auto` and `min` only ever **tighten** the defaults; they never raise a
  value above the 1.1 / 1.0 defaults or above a value you set.
- **`check`** prints the limits and the sizes it would use and exits 0 without moving data, or exits 2 when a
  value you set is above them. The safe-simple examples run it as a preflight.
- **429s by size are handled mid-run** (`searchdump`): a search page refused by a circuit breaker halves the page
  for the rest of the run; a bulk refused as a whole for its size is split in half (in order, no duplicate writes)
  and later bulks stay at the size that fit. Queue-full 429s keep the existing retry. This also applies under
  `--serverLimits=off`.
- **`searchdump-multi` probes once in the parent** and reads every selected index's docs and stored bytes in a
  single `_stats` call (1.1 made one `_stats` call per index). It prefers fewer workers to smaller pages and keeps
  a byte budget next to `--parallel`, so large-document indices run fewer at once. The manifest records
  `server_limits` and each index's `sizing`.
- **Safe-simple examples** (bash, docker/podman, kubernetes): no `--limit`, `--maxBulkBytes`, `--parallel`
  tuning; `--serverLimits` is the only sizing option.

## Upgrade guide (from 1.1)

- Remove your 1.1 small-heap overrides (`-- --limit=500 --maxResponseMb=64`, `--maxBulkBytes=16mb`,
  `--parallel=1`): `--serverLimits=auto` replaces them; `--serverLimits=min` for a fragile or shared cluster.
- `--serverLimits=off` restores the 1.1.0 sizes exactly and sends no probe. `--noDefaultDataFlags` (multi) implies
  `off`.
- Dump users need the `monitor` cluster privilege for the probe; without it the run prints one fallback line and
  uses the 1.1 sizes (it never fails because of the probe).
- AWS OpenSearch Service: bulks stay at or below 10 MiB unless you give `--maxBulkBytes`.
- Children of `searchdump-multi` always run with `--serverLimits=off` and the sizes the parent computed; an older
  child binary without the flag gets only `--limit` / `--maxBulkBytes`.

## New flags and defaults

| Flag | Default | Tool | Meaning |
|---|---|---|---|
| `--serverLimits=off\|auto\|min\|check` | `auto` | both | See above. `min` uses half of `auto`'s budget. |
| data-run `--limit` / `--maxBulkBytes` | from the probe (`auto`); 1000 / 16 MB under `off` | `searchdump-multi` | 1.1-10 fixed defaults are now the `off` profile. |

## Not in 1.2

- `--serverLimits=automax` (sizes that may also grow above the defaults, mid-run) is designed only
  (ROADMAP-1.2 section 13, FEATURES-TODO 1.2-2).
- `check` is not a separate subcommand; byte paging is internal (no `--pageBytes`); both may change in 1.3.

## Verification (release/1.2 candidate, tagged as 1.2.0)

Evidence is in [`results/local/L2-008.md`](../../results/local/L2-008.md) and the earlier `results/local/L2-00N.md` files.

- **Tests (release candidate tree):** clippy and fmt clean; unit/bin tests 335, `cli_contract` 73, chaos/fault/golden/injection/secrets suites, 0 failed.
  Integration tests against real engines: ES 7.10 / 8.19 / latest and OpenSearch plain 121 passed each, ES 7.10 secure 90 passed, 0 failed.
- **Low-heap acceptance (L2-006):** ES 7.10-9.12 and OpenSearch 1.3 / 2 / 3 at 256m and 384m: 14/14 cells pass in every mode (`auto`, `off`, 1.1.0 flags, `min`);
  the 1.0 flags fail 20 of 56 runs, 15 of them taking the node down.
- **Safe-simple examples (live):** `safe-simple-dump/restore/multi-dump/multi-restore` pass on ES 7.10.2, ES 8.19.23 and OpenSearch 2.19.7 at a 256m heap
  with digests equal to the source. Two example bugs were found and fixed while doing this: `--serverLimits=check --output=/dev/null` was refused as a
  usage error, and the example dump wrote plain JSON under a `.json.gz` name.
- **Performance (16 engines, L0, no Node comparison):** per-step median wall time vs 1.0.0-beta1: median 0.78x (faster) over 573 cases; `searchdump-multi` p4 0.83x,
  p8 0.77x on dumps and 0.87x / 0.86x on restores. Back-to-back with 1.1.0 on ES 7.10 secure: 0.91x-1.04x. A few noisy first-run cases were re-measured
  (1.2 within 4-17 % of 1.1.0, tens of ms absolute on the small cases). Timings come from per-step timers, because the benchmark VM was paused several times.
- **Not verified:** automax (design only), Node elasticdump comparison for 1.2, Windows, clusters other than the above.
