# searchdump-rs 1.3.0 release notes (DRAFT)

Status: **draft, not released.** Work is on the local branch `release/1.3` (not pushed). Entries are added as
Robert decides the 1.3 questions (results/local/L3-SUMMARY.md, "Decisions needed"). The same material is summarised
in [`CHANGELOG.md`](../../CHANGELOG.md) under **1.3.0**.

## Disclaimer

searchdump-rs is provided **as is, without warranty of any kind**, as set out in its
[MIT license](../../LICENSE). See [Disclaimer](../../README.md#disclaimer). Try it on a non-production cluster first
and verify every backup independently (restore it, compare counts) before relying on it.

## Highlights

- **Minimum sizes for server-aware sizing and 429 handling** (decision 3, 2026-11-17). In 1.2, a probe taken while
  a small heap was full of garbage (for example a dump right after a load) could size a 0 MB page (1-2 documents
  per page), and repeated HTTP 429 bulk splits on OpenSearch 3 small heaps took the bulk size from 6-16 MB down to
  54-390 KB for the rest of the run (3-13x slower). 1.3 keeps:
  - sizing from at least a quarter of the parent-breaker limit (not only the momentary headroom);
  - a page / in-flight budget of at least 1 MiB and bulk requests of at least 1 MiB (unless the node's
    indexing-pressure budget shared by all bulk workers is smaller; then each worker gets its share, as in 1.2);
  - after a 429 split, later bulks at 1 MiB or more (the refused request itself is still split as far as needed,
    so the floor never fails a run);
  - a 429 page halving that stops once at 10 documents and goes lower only if that page is refused too.

  A smaller `--limit` or `--maxBulkBytes` you set still applies. `SEARCHDUMP_LIMITS_EXPERIMENT=nofloor` restores
  the 1.2 sizing if you need it.
- **Clearer 429 warnings** (decision 4): split and halve warnings now name the cause the server reported (parent
  circuit breaker "real usage", indexing pressure, ...).

## Fixes

- **`searchdump-multi` false index failure on a busy cluster** (decision 4; affects 1.1.0 and 1.2.0): the parent's
  final `POST /<index>/_refresh` (and `_count`, GET/HEAD checks) had no retry, so one HTTP 429/503 from a busy node
  (seen on OpenSearch 3 with a 256 MB heap) failed an index whose documents were all written. These read-only calls
  are now retried up to 6 times with 250 ms to 4 s back-off.

## Not changed (still being evaluated)

- The 429 policy itself (split vs. retry at the same size first, and growing the bulk size back after a split) is
  unchanged from 1.2 apart from the floor; `retryfirst` and `growback` stay opt-in measurements (decision 2 open).
