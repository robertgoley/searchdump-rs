# Changelog

All notable changes to searchdump-rs. Versions follow [Semantic Versioning](https://semver.org/);
a version with a hyphen (`1.0.0-beta1`) is a pre-release. The release workflow
(`.github/workflows/release.yml`) uses the section for a version as the release notes of its draft
GitHub release.

## 1.4.0 (unreleased)

Headline: limits and multi polish. Branch `release/1.4` (from v1.3.1); not released.

### Security

- One HTTP client version: the cluster side moves from reqwest 0.12 to 0.13, the version OpenDAL's storage path
  already used, so reqwest 0.12, base64 0.22, quinn and h2 leave the dependency tree. reqwest is built with
  `rustls-no-provider`: searchdump always hands it its own rustls config (ring), so no aws-lc code is built.
- Encrypted PKCS#8 `--key` decryption moves to pkcs8 0.11 (RustCrypto 0.11 generation, shared with the S3
  signer), which removes the duplicate sha2/hmac/digest/const-oid/cpufeatures/block-buffer versions. The same key
  formats (PBES2 with PBKDF2 or scrypt, AES-CBC) decrypt as before.

### Fixed

- A huge `--idleTimeout`, `--requestTimeout` or `--timeout` (for example `--idleTimeout 9223372036854775807`)
  is capped at about 100 years instead of panicking when the HTTP client adds it to the current time.

## 1.3.1

Release date: 2026-11-25 (tag `v1.3.1`). Security patch release; no behaviour changes in `searchdump` or
`searchdump-multi`. User-facing notes: [`project/release/RELEASE-NOTES-1.3.1.md`](project/release/RELEASE-NOTES-1.3.1.md).

### Disclaimer

searchdump-rs is provided "as is", without warranty of any kind, as stated in the MIT license
(the same warranty disclaimer as the elasticdump-rs code it derives from; Node elasticdump,
whose behaviour it follows, has the equivalent disclaimer in Apache-2.0, sections 7 and 8).
The authors are not liable for any damage from its use, including data loss, corruption,
downtime or service disruption, in production or elsewhere. Test in a non-production environment
first, and verify every backup and restore yourself (restore it, compare document counts) before
you rely on it. See [Disclaimer](README.md#disclaimer).

### Security

- `url` 2.5.9 (RUSTSEC-2026-0592, stack overflow in `Url::origin()` on nested `blob:` URLs; searchdump never calls
  it), with `form_urlencoded` 1.2.3, `rustls-webpki` 0.103.17 and `webpki-roots` 1.0.12; `base64` 0.23.1 (PR #2).
- Container images rebuilt on refreshed dhi.io base digests, resolved in CI (`security` `base_refresh` job):
  glibc 2.41-12+deb13u5 (CVE-2026-5435, CVE-2026-19499), krb5 1.21.3-5+deb13u2 (15 Kerberos ids) and zstd
  deb13u1 are fixed in the default `dhi-debian` image.
- The Debian images run `apt-get upgrade` before installing the OpenSSH client, which brings openssl
  3.5.9-1~deb13u1+dhi2 with DHI's backports for 8 OpenSSL ids (including the L-824 six). CVE-2026-84786 (DANE client
  NULL dereference, fixed upstream in 3.5.10) and five OpenSSL ids in the `-dhi-alpine` image (libssl3/libcrypto3
  3.5.9-r3) have no fixed package yet and are accepted with version-scoped scanner rules (see
  [SECURITY-CVE.md](SECURITY-CVE.md#openssl-359-l-831)). searchdump itself uses rustls; OpenSSL is in the images for
  the OpenSSH client behind `sftp://`, which stays in every image with a package manager.

### Changed

- CI: GitHub Actions bumped (checkout 7.0.1, upload-artifact 7.0.2, download-artifact 8.0.2, setup-zig 2.2.1,
  install-action 2.90.4; PR #8). New `security` dispatch option `base_refresh=true`: resolves the current dhi.io
  digests, probes the package repos, builds `dhi-debian` and `dhi-alpine` on them, runs an `sftp://` smoke test
  through each image's own `ssh` (`scripts/sup/image-sftp-smoke.sh`) and attaches grype/trivy reports.
- Supply chain: the SUP-019 publisher baseline is refreshed after review (bitflags, uuid, regex, regex-automata,
  regex-syntax and socket2 each lost owners under the Rust project's crate-ownership policy or an org move; no
  owner was added).

## 1.3.0

Release date: 2026-11-20 (tag `v1.3.0`). User-facing notes: [`project/release/RELEASE-NOTES-1.3.md`](project/release/RELEASE-NOTES-1.3.md).
The kvmsvr1 performance runs (Suite A/B/C and the 1.3-G retest) are run after tagging.

### Disclaimer

searchdump-rs is provided "as is", without warranty of any kind, as stated in the MIT license
(the same warranty disclaimer as the elasticdump-rs code it derives from; Node elasticdump,
whose behaviour it follows, has the equivalent disclaimer in Apache-2.0, sections 7 and 8).
The authors are not liable for any damage from its use, including data loss, corruption,
downtime or service disruption, in production or elsewhere. It has been stable and fast in
our testing, but no test suite covers every cluster version, configuration, plugin and
storage backend. Test in a non-production environment first, and verify every backup and
restore yourself (restore it, compare document counts) before you rely on it. See
[Disclaimer](README.md#disclaimer).

### Changed

- **Minimum sizes for `--serverLimits=auto`/`min` and the HTTP 429 handling** (1.3 decision 3, 2026-11-17): sizing never
  uses less headroom than a quarter of the parent-breaker limit, pages / in-flight budgets and bulk requests are at
  least 1 MiB (bulks: unless the indexing-pressure budget split over the workers is smaller), a 429 split keeps later bulks at 1 MiB or more (1.2 could go down to 54-390 KB for the rest of a run on
  OpenSearch 3 small heaps), and a 429 page halving stops once at 10 documents. A smaller `--limit` / `--maxBulkBytes`
  you set still applies; `SEARCHDUMP_LIMITS_EXPERIMENT=nofloor` restores the 1.2 sizing.
- HTTP 429 split / halve warnings name the cause the server gave (for example the parent circuit breaker's "real
  usage" or indexing pressure), so a log shows why a request was made smaller (1.3 decision 4).
- `searchdump` (and so each `searchdump-multi` child log) writes one `bulk size summary` warning at the end of a load that met size 429s
  (429s, splits, resends, grows, lowest and final bulk size).
- Unchanged by decision (2, 2026-11-17): the 429 policy itself; `SEARCHDUMP_LIMITS_EXPERIMENT=retryfirst` and
  `growback` stay opt-in evaluations.

### Fixed

- `searchdump-multi`: a load could report an index as failed although every document was written, when the parent's
  final `POST /<index>/_refresh` (or a `_count`/GET/HEAD check) got a single HTTP 429 or 503 from a busy node (seen on
  OpenSearch 3 with a 256 MB heap). The parent now retries these read-only calls up to 6 times (250 ms to 4 s
  back-off). Affects 1.1.0 and 1.2.0 (1.3 decision 4).
- `--type alias` restore skips aliases of reserved system indices (`.security*`, `.async-search*`,
  `.fleet-secrets*`, `.opendistro_security`, `.plugins-security*`) with a warning. A host-only alias dump from a
  secured Elasticsearch 8/9 lists `.security-7` (alias `.security`), and the cluster refuses alias changes on it
  even for a superuser (HTTP 403), which failed the whole atomic `POST /_aliases` (also on `main`, CI-FIX-1).

## 1.2.0

Release date: 2026-11-16 (tag `v1.2.0`). User-facing notes, including what `--serverLimits` does and the
possible 1.3 changes: [`project/release/RELEASE-NOTES-1.2.md`](project/release/RELEASE-NOTES-1.2.md).

### Disclaimer

searchdump-rs is provided "as is", without warranty of any kind, as stated in the MIT license
(the same warranty disclaimer as the elasticdump-rs code it derives from; Node elasticdump,
whose behaviour it follows, has the equivalent disclaimer in Apache-2.0, sections 7 and 8).
The authors are not liable for any damage from its use, including data loss, corruption,
downtime or service disruption, in production or elsewhere. It has been stable and fast in
our testing, but no test suite covers every cluster version, configuration, plugin and
storage backend. Test in a non-production environment first, and verify every backup and
restore yourself (restore it, compare document counts) before you rely on it. See
[Disclaimer](README.md#disclaimer).

### Added

- **`--serverLimits=off|auto|min|check`** in `searchdump` and `searchdump-multi`: one read-only probe of the smallest
  data node's heap, breakers, indexing pressure and limits sizes pages, bulks and multi parallelism; `auto` and `min` only tighten.
  `check` prints the limits and exits 0 or 2 without moving data (it never validates the output path, so `--output=/dev/null` works).
- HTTP 429 handling by size: search pages halve and oversized bulks split mid-run.
- `searchdump-multi`: one probe and one `_stats` call in the parent; worker-first byte-budget scheduling; manifest `server_limits` and per-index sizing.
- Safe-simple examples (bash, docker/podman, kubernetes) with a `--serverLimits=check` preflight.

### Fixed

- `--serverLimits=check` with `--output=/dev/null` was refused as a usage error (L2-009 candidate, folded into L2-008).
- `safe-simple-dump` examples wrote an uncompressed file named `.json.gz`; they now pass `--fsCompress`.

## 1.1.0

Release date: 2026-11-16 (tag `v1.1.0`; release candidate verified on 2026-11-15). Roadmap: `FEATURES-TODO.md`, "Roadmap: 1.1 release"; order and checks:
`project/plan/ROADMAP-1.1-PLAN.md`. The user-facing notes, including the upgrade guide, are in
[`project/release/RELEASE-NOTES-1.1.md`](project/release/RELEASE-NOTES-1.1.md).

### Disclaimer

searchdump-rs is provided "as is", without warranty of any kind, as stated in the MIT license
(the same warranty disclaimer as the elasticdump-rs code it derives from; Node elasticdump,
whose behaviour it follows, has the equivalent disclaimer in Apache-2.0, sections 7 and 8).
The authors are not liable for any damage from its use, including data loss, corruption,
downtime or service disruption, in production or elsewhere. It has been stable and fast in
our testing, but no test suite covers every cluster version, configuration, plugin and
storage backend. Test in a non-production environment first, and verify every backup and
restore yourself (restore it, compare document counts) before you rely on it. See
[Disclaimer](README.md#disclaimer).

### Added

- **`--refreshInterval=<seconds>`** (default `5`). Refresh written indices at most this often
  while writing, plus once at the end of the run. Accepts seconds, optional decimals, and an
  optional trailing `s` (`0` to `86400`). `0` restores a refresh after every batch.
- **`--scrollDocSort`** (opt-in, default off). Adds `"sort":["_doc"]` to an unscored scroll
  body that has no user sort.
- **`--sliceCap=auto|min|total|false`** (default `auto`; `true` is an alias of `auto`) and
  **`--sliceField=<field>`**. `auto` caps a scroll dump's `--slices` to the smallest
  primary-shard count on Elasticsearch 7.10 only and keeps it as given on every other engine
  and undetectable versions (1.1-4a); `min` / `total` force the cap on any engine; `false`
  never caps. `--sliceField` slices on a numeric or date field with doc values.
- **`searchdump-multi --verifyCounts`**. Always re-read dumped or loaded data and fail the
  index when the child's reported record count differs.

- **`--restartOnContextLoss N`** (from the 1.0 line, L-804b). A scroll data dump from a cluster
  into a file restarts from the start, rewriting the file, up to N times when the scroll context
  is lost (default 0 = off). Refused with a usage error for PIT/search_after dumps, `--scrollId`,
  and outputs other than a file. The scroll-context-lost error suggests a point-in-time dump and
  this flag; when most failed writes are `cluster_block_exception`s, the exit-3 summary names the
  block (for a disk flood-stage block it suggests freeing disk and a higher `--retryAttempts`).
- **`sftp://` inside the container images** (from the 1.0 line, L-810i). Images built with a
  package manager install the OpenSSH client OpenDAL's SFTP backend runs (`openssh-client` on
  Debian-based images, `ssh-keysign` setuid bit removed; `openssh-client-default` on Alpine). The
  COPY-only `dhi-static` image is unchanged. The added Debian OpenSSH/Kerberos CVEs are accepted
  in SECURITY-CVE.md (owner, 2026-11-14). The user-admin and init-script tools its dependencies
  pull in (`useradd`/`usermod`/`chpasswd`/`vipw`, `service`, `update-rc.d`, `deb-systemd-*`, their
  dpkg maintainer scripts) are deleted from the Debian images (L-810j); the documented executable
  set is unchanged.

- **Examples updated for 1.1** (from the 1.0 line, L-810k). The `examples/` scripts rerun a step that
  exits 4 (`RERUNS`, `RERUN_DELAY`), the very-large-index dumps take an opt-in
  `RESTART_ON_CONTEXT_LOSS`, the copy scripts note that `--httpAuthFile` wins, the multi-index
  restores pass `--maxBulkBytes`, and 7 Kubernetes manifests with numeric `env` values (invalid) are
  fixed.
- **Low-memory test set** (L-818). `scripts/load/lowheap.py` runs `searchdump-multi` dump and load
  with the default flags against ES 7.10.2, 8.6.1, 8.19.23 and 9.1.2 at 256 MB and 384 MB heap;
  `--regression` shows the pre-1.1 flags failing.

### Changed

- **Safer `searchdump-multi` data defaults (1.1-10).** Data runs used `--limit=5000` and
  `--maxResponseMb=0`. On a small heap a single 5000-document scroll page took the cluster down:
  Elasticsearch 7.10.2 and 8.6.1 (bitnamilegacy, 256 MB heap) died with `OutOfMemoryError`
  (also at 384 MB with ~80 KB documents), and 8.19.23 answered HTTP 429 (parent breaker) until
  the retries ran out. Data runs now default to `--limit=1000` and keep searchdump's 256 MB
  response cap. A dump of an index whose documents average more than 16 KB stored uses
  proportionally fewer per page (about 16 MB stored per page, at least 1) and prints a note; an
  explicit `-- --limit=N` turns that off. With the new defaults all four engines (7.10.2, 8.6.1,
  8.19.23, 9.1.2) dump the same data at a 256 MB heap. Loads send at most 1000 documents and
  16 MB per bulk (`--maxBulkBytes=16mb`): 5000 PMC articles (~105 MB) were refused with HTTP 429
  even by a 1 GB heap (`indexing_pressure`), and 1000 of them by a 256 MB heap (parent breaker).
- **Refresh cadence (1.1-1).** The writer used to refresh the target after every bulk batch, as
  Node elasticdump does. It now refreshes the indices it wrote at most every 5 seconds (the
  clock starts at the first finished batch) and always once at the end of the run, including
  the `--concurrency 1` path, which before relied on the last batch's refresh.
  `--refreshInterval=0` restores the per-batch refresh (Node-identical request pattern);
  `--noRefresh` still sends no refresh and wins over `--refreshInterval`. After the run the
  same documents are visible; only readers watching *during* a restore see them up to the
  interval later. NODE-DIFFERENCES.md #26.

- **`_doc` sort for scroll dumps (1.1-3).** `--scrollDocSort` adds `"sort":["_doc"]` to a
  scroll body with no `sort` and a `match_all`/`match_none` query (or none). It is skipped,
  with an info log, for PIT, `--searchAfter`, `--scrollId`, `--searchWithTemplate`, a user
  sort, scoring queries and `track_scores`/`min_score`/`rescore`. The injected `sort` member
  is stripped from written hits; `"_score": null` remains. Default off: without the flag,
  requests and dump lines are unchanged. Node sets `sort: ['_doc']` on its request options
  object, not the body, so it never actually sends the sort and writes `"_score": 1`.
  NODE-DIFFERENCES.md #27.

- **Scroll slices capped to the shard count on Elasticsearch 7.10 (1.1-4, limited in 1.1-4a).**
  1.1-4a (owner decision 2026-11-14): the default `--sliceCap=auto` now caps only on
  Elasticsearch 7.10.x. The point-in-time perf run (`scripts/perf/results/pit-1.1.md`) showed the
  cap fixing ES 7.10 (`dump-scroll-s2` -63%, `s4` -34%) but slowing sliced scrolls on ES 8.19
  (`s4` +45%, `s8` +59%) and OpenSearch 3 (`s4` +62%, `s8` +101%), so every other engine keeps
  the pre-1.1-4 behaviour (`--slices` as given; `auto` reads the version with `GET /`). The
  version-aware basis below now applies only when the cap runs (ES 7.10, or a forced
  `--sliceCap=min|total`). Original 1.1-4 text: a sliced scroll with more slices than
  primary shards makes the engine filter every document of a shard per extra slice, which can
  be slower than an unsliced scroll. A scroll dump now reads `index.number_of_shards` of the
  input indices and lowers `--slices` to it with a warning (one shard: unsliced). The count is
  version-aware (`--sliceCap=auto`, default): Elasticsearch 7.11-9.4 assign slices across all
  shards of the request (elastic/elasticsearch#65706), so the total primary-shard count is the
  limit; Elasticsearch 5.0-7.10 and OpenSearch assign them per index, and Elasticsearch 9.5+
  scroll filters per shard again (elastic/elasticsearch#148664), so the smallest count is the
  limit there and when the version cannot be detected. The version (`GET /`) is only read when
  the input names several indices. `--sliceCap=min` / `total` force one count;
  `--sliceCap=false` keeps the requested number. `--sliceField` adds `"field"` to
  every slice (numeric/date field with doc values, checked with
  `GET /<index>/_mapping/field/<field>`; a missing, non-numeric or doc-values-less field is
  an error before the first search) and disables the cap. PIT is not capped. When the
  settings or mapping cannot be read, a warning is printed and the flags are used as given.
  Node has no slicing.

- **Compression runs on its own thread (1.1-9).** A compressed file output (`--fsCompress`,
  `--compressionType gzip|zstd`, split parts too) now feeds each part's encoder on a
  dedicated `searchdump-compress` thread, so gzip/zstd no longer stall the writer between
  batches. The compressed bytes are identical to 1.0. An encoder error surfaces on the next
  write, flush or seal; dropping a part (cancel, abort, error) stops its thread. The memory
  estimate counts 3 extra batches for compressed output. No flag.

- **Bulk encode/send overlap at `--concurrency 1` (1.1-6).** A batch is now validated and
  encoded while the previous one is in flight, then sent once that send has finished
  (written, retried or reported). Sends never overlap, so the requests and their order are
  unchanged; an error in batch N means batch N+1 is never sent. A run that ends with an
  error finishes the one batch in flight before exiting. `SEARCHDUMP_BULK_PIPELINE=0` turns
  it off (A/B switch, not a CLI flag). The version-conflict (409) and data-stream hints are
  now shared by all workers, so they are printed once per run also with `--concurrency N > 1`
  (they were printed once per batch there). The memory estimate counts 2 extra batches.

- **searchdump-multi runs its children asynchronously (1.1-8).** Child `searchdump` processes
  are awaited without holding a runtime thread per child. Ctrl-C now stops running children:
  SIGTERM first, SIGKILL after 5 s, and each stopped child is named with its log
  (`interrupted: stopped searchdump --type=data for INDEX (...)`); before, running children
  were left to finish. Exit status 130 is unchanged.

- **searchdump-multi uses the record counts its children report (1.1-7).** A data run of
  searchdump gets a hidden `--statsFile` and writes written/failed/overwritten/rejected
  counts when it succeeds. The dump summary and manifest `records` use that count; without
  it, the child's last `Total Writes: N` log line (searchdump and Node elasticdump formats);
  without both, the data files are read back as before. A load adds up each part's written +
  failed instead of reading every part before loading it. `--verifyCounts` always reads the
  files back and fails the index when the reported count differs. `--statsFile` is passed
  only when the child binary resolves to `searchdump*` (compatibility symlinks included; a
  Node `--elasticdumpBin` gets no unknown flag). `SEARCHDUMP_MULTI_DEBUG=1` prints the count
  source. Summary and manifest formats are unchanged.

- **Perf harness timer (1.1-5).** `scripts/load/measure.py` now blocks in `wait4` and samples
  `/proc` on a side thread, instead of polling `wait4` every 0.1 s. Wall times are no longer
  rounded up to the next ~0.1 s tick. Results carry `"timer": "blocking-wait4"`; 1.0.0-beta1
  numbers measured with the old harness are re-measured before any 1.1 comparison. Binaries
  are unchanged.

- **Exit status 4 for transient, rerun-safe failures** (from the 1.0 line, L-804b; behaviour
  change). A run that still fails after its retries because of the network, HTTP 429 or 5xx, or
  a lost scroll context now exits 4 instead of 1 when nothing was committed that a rerun would
  duplicate. A restore that already sent documents without `_id`, and every permanent failure,
  still exit 1; 0, 2, 3, 130 and 143 are unchanged. `searchdump-multi` keeps its own codes.
- **`--httpAuthFile` wins over every other credential source** (from the 1.0 line, L-645b;
  behaviour change). Its `user=`/`password=` pair is used for both directions even when the URL,
  `--input-*`/`--output-*`/`--username`/`--password` flags or `SEARCHDUMP_*` variables also give
  credentials; a warning names the sources it replaced, never their values.
- **Restore CPU and memory** (from the 1.0 line, L-808n to L-808r). memchr line splitting, one
  checked parse per document, the encoded batch moved (not copied) into the bulk request,
  pre-sized file batch buffers, and batches shared as `Bytes` through the sink. On 1.0 these cut
  100k-doc restore CPU by about half and peak RSS by about 17%; on 1.1 they were merged into the
  1.1-6 encode/send pipeline (not re-measured with the PIT batch).

### Fixed

- **No whole-cluster refresh (1.1-2).** A restore with no output index (documents keep their
  `_index`) used to send `POST /_refresh`, refreshing every index in the cluster after every
  batch. It now refreshes only the indices the run wrote (after `--prefix`/`--suffix`/rename),
  as `POST /a,b,…/_refresh?ignore_unavailable=true&allow_no_indices=true`, split so each URL
  stays under ~3 KB, and sends nothing when nothing was written. With an output index the
  request is unchanged. Node elasticdump refreshes the whole cluster after every batch in
  this case (`_data.js` `reindex()` posts `${base.url}/_refresh`); this is a deliberate
  difference. The data visible after the run is the same. NODE-DIFFERENCES.md #25.

- **Scroll dumps that pass the open scroll limit** (from the 1.0 line, L-809). "Too many scroll
  contexts" (`search.max_open_scroll_context`) is no longer retried: exit 1 at once, naming the
  setting, `--slices` and the point-in-time dump.
- **HTTP 413 names the limit** (from the 1.0 line, L-810). The empty-bodied 413 now says the
  request is larger than `http.max_content_length` and suggests a lower `--maxBulkBytes`.
- **Routed documents into a data stream are explained once** (from the 1.0 line, L-812). A data
  stream without `data_stream.allow_custom_routing: true` refuses documents that carry a routing
  value; the restore warns once per run (shared across `--concurrency` workers) instead of only
  listing item errors. Still exit 3.
- **The final HTTP 429 names the retry and load knobs** (from the 1.0 line, L-804a).
- **searchdump-multi names a stopped child cleanly.** After Ctrl-C, an index loaded from a single
  data file was reported as `FAILED  IX: : interrupted: ...` (an empty context between the
  colons); the part is named only when the index has several.
- **A lost scroll context names the point-in-time way out** (from the 1.0 line, L-804c): the
  context-lost error suggests `--searchAfter --pit` and `--restartOnContextLoss N`; exit 4.

### Upgrade notes

- **`searchdump-multi` page size.** Data runs now use 1000 documents per page (fewer for large
  documents) and the default 256 MB response cap. To get the 1.0 behaviour, pass
  `-- --limit=5000 --maxResponseMb=0`. Loads split bulks at 16 MB (`-- --maxBulkBytes=100mb` for the
  1.0 size).
- **Exit status 4.** Scripts that test for exactly 1 should also accept 4 (transient, safe to
  rerun). Scripts that treat any non-zero status as failure are unaffected.
- **`--httpAuthFile`.** If you pass an auth file together with URL or flag credentials, the file
  now wins.
- **Refresh default.** Scripts that need documents to become searchable after every batch
  should pass `--refreshInterval=0`. `--noRefresh` is unchanged and wins over
  `--refreshInterval`.
- **Scroll `--slices`.** On Elasticsearch 7.10 a value higher than the primary-shard count is
  now lowered, with a warning (the smallest count of the input indices); other engines keep it
  as given (1.1-4a; see README "Tuning"). Pass `--sliceCap=false` to keep the requested number, or
  `--sliceCap=min|total` to choose the count. PIT is not capped.
- **Dump lines.** Unchanged unless `--scrollDocSort` is set (then `"_score": null`, and the
  returned `sort` member is not written). Leave the flag off to stay identical to 1.0 and
  Node.
- **searchdump-multi Ctrl-C.** Running children are now stopped (SIGTERM, then SIGKILL after
  5 s). Previously they were left to finish.
- **searchdump-multi counts.** Manifest and summary formats are unchanged. Pass
  `--verifyCounts` to keep the old always-rescan check and fail the index on a mismatch.
- **Compatibility.** Dump files, `multielasticdump.json`, `SEARCHDUMP_*` / `ELASTICDUMP_*`
  names, and the `elasticdump` / `elasticdump-rs` / `multielasticdump` symlinks are
  unchanged. These changes are for 1.1 only; they are not backported to 1.0.

### Performance

Point-in-time results for each 1.1 commit (es710, es819, os3; plain) finished 2026-11-14: see
[`project/release/RELEASE-NOTES-1.1.md`](project/release/RELEASE-NOTES-1.1.md#performance) and the full tables in
[`scripts/perf/results/pit-1.1.md`](scripts/perf/results/pit-1.1.md) (grokbox) and
[`scripts/perf/results/v1.1-pit-kvmsvr1.md`](scripts/perf/results/v1.1-pit-kvmsvr1.md) (kvmsvr1, 27/27 pass,
beta1 on every engine, Node median 2.41× / 1.97× / 1.99×). Short version: the 5 s refresh
cadence (1.1-1) cut restore wall time by roughly 20-40%; the slice cap (1.1-4) fixes the es710 over-sliced
scroll dumps (-34..-63%) but slowed already-fast scroll slices on ES 8.19 / OpenSearch 3, so 1.1-4a limits the
default cap to ES 7.10 (not re-measured with the full batch); the other items are within noise.

## 1.0.0

First stable release. The program source is identical to 1.0.0-beta1 (only the version number changes). This tag adds
documentation, the `examples/` tree, the release-eval performance harness, and the
completed performance-suite results.

### Disclaimer

searchdump-rs is provided "as is", without warranty of any kind, as stated in the MIT license
(and the Apache-2.0 license of the code it is modelled on). The authors are not liable for any
damage from its use, including data loss, corruption, downtime or service disruption, in
production or elsewhere. It has been stable and fast in our testing, but no test suite covers
every cluster version, configuration, plugin and storage backend. Test in a non-production
environment first, and verify every backup and restore yourself (restore it, compare document
counts) before you rely on it. See
[Disclaimer](README.md#disclaimer).

### Added

- **Examples.** The whole `examples/` tree ships in the repository and in every release archive:
  bash, Docker/Podman, and Kubernetes dump and restore starters for minimum resources, fastest
  dump, very large indexes, secure TLS and auth, object storage, cluster-to-cluster copy, and
  multi-index runs, plus the `.ini.example` templates and READMEs.
- **Release-eval harness.** `scripts/perf/release-eval.sh` (and the rest of `scripts/perf`,
  including `run-suite.sh`, `suite.py`, `aggregate.py`, `selftest.py`, `RELEASE-EVAL.md`, and
  the recorded baselines and v1.0.0-beta1 results) ships in every release archive so a Linux
  host can time a published build against the usual engine set and, optionally, Node
  elasticdump, then commit results back. See [`scripts/perf/RELEASE-EVAL.md`](scripts/perf/RELEASE-EVAL.md).
  A live run also needs `scripts/it` from a repo checkout (engine images, `up.sh` / `down.sh`).
- Performance-suite results for this code (measured on v1.0.0-beta1, whose binaries match this
  release): 576 passed, 0 failed, 16 skipped; searchdump was a median 2.01× faster than Node
  elasticdump and was never slower. See
  [`scripts/perf/results/v1.0.0-beta1-L0.md`](scripts/perf/results/v1.0.0-beta1-L0.md).

### Changed

- Version is 1.0.0. No `src/` changes relative to 1.0.0-beta1. `Cargo.toml`, `Cargo.lock` and
  the Dockerfile `VERSION` label are bumped to 1.0.0. The release archives now also include
  `examples/` and `scripts/perf`.

### Known issues

- `sftp://` URLs do not work inside the container images. OpenDAL's SFTP backend runs the system OpenSSH `ssh`
  client, which the images do not include (Dropbear's client is not compatible). SFTP works with the host binaries
  and release archives on systems with OpenSSH installed. S3, B2 and `fs://` are not affected.
- Backblaze B2 (`b2://`) is compile-tested only. It has not been run against a real B2 account (pending owner
  credentials; see TESTING-TODO.md).

### Upgrade notes

- From 1.0.0-beta1: no binary or flag changes. Point image and archive tags at `1.0.0` instead of
  `1.0.0-beta1`. The `examples/` and `scripts/perf` trees are new in the archives.
- From earlier names (`elasticdump-rs` / `multielasticdump`): see the 1.0.0-beta1 upgrade notes below.

## 1.0.0-beta1

First tagged release under the new name, as a beta. Version 1.0.0 will follow after a full
regression run.

### Disclaimer

searchdump-rs is provided "as is", without warranty of any kind, as stated in the MIT license
(and the Apache-2.0 license of the code it is modelled on). The authors are not liable for any
damage from its use, including data loss, corruption, downtime or service disruption, in
production or elsewhere. It has been stable and fast in our testing, but no test suite covers
every cluster version, configuration, plugin and storage backend, and a beta release in particular
may still contain bugs. Test in a non-production environment first, and verify every backup and
restore yourself (restore it, compare document counts) before you rely on it. See
[Disclaimer](README.md#disclaimer).

### Changed

- **Renamed** from elasticdump-rs-fork to searchdump-rs (repository
  github.com/robertgoley/searchdump-rs). The binaries are now `searchdump` (was `elasticdump-rs`)
  and `searchdump-multi` (was `multielasticdump`); the library crate is `searchdump_rs`.
- Environment variables are now `SEARCHDUMP_*`. The old `ELASTICDUMP_*` names still work as
  deprecated aliases and print a one-time warning; when both are set, the new name wins.
- Minimum Rust version is 1.91 (`rust-toolchain.toml` pins 1.91.1).
- The default container image is the dhi.io Debian 13 image with a shell (`dhi-debian`, linux/amd64).
  The shell-less `dhi.io/static` image, the previous default, is still built as `dhi-static`.
- README rewritten: origin and credits (elasticdump-rs by TenYears, Node elasticdump), install,
  usage, compatibility notes, performance figures from recorded runs only, trademarks.

### Added

- **Object storage and SFTP** through OpenDAL 0.59: `s3://`, `b2://`, `sftp://` and `fs://` as
  `--input`/`--output` for `searchdump` and `searchdump-multi` (copy and staged dump/load),
  settings in `--inputStorageConfig`/`--outputStorageConfig` files or environment variables (no
  secrets in URLs). Tested against MinIO and RustFS (including SSE-C), OpenSSH and Dropbear SFTP,
  and the local filesystem; Backblaze B2 and AWS S3 are compile-tested only so far.
- `searchdump-multi` (formerly `multielasticdump`) ships in the release archives next to
  `searchdump`.
- Compatibility names in every container image and release archive: `elasticdump` and
  `elasticdump-rs` -> `searchdump`, `multielasticdump` -> `searchdump-multi`.
- Release workflow: archives for x86_64/aarch64 glibc and musl, SHA-256 checksums, a CycloneDX
  SBOM, GitHub build-provenance attestations, container images on ghcr.io, and a draft GitHub
  release for review.

### Known issues

- `sftp://` URLs do not work inside the container images. OpenDAL's SFTP backend runs the system OpenSSH `ssh`
  client, which the images do not include (Dropbear's client is not compatible). SFTP works with the host binaries
  and release archives on systems with OpenSSH installed. S3, B2 and `fs://` are not affected.
- Backblaze B2 (`b2://`) is compile-tested only. It has not been run against a real B2 account (pending owner
  credentials; see TESTING-TODO.md).
- The full performance-suite numbers for this tag were still being collected at release time. Earlier recorded
  runs are in [README Performance](README.md#performance); the v1.0.0-beta1 suite results will be added when that
  run finishes.

### Upgrade notes

- **Binaries.** `elasticdump-rs` is now `searchdump`, and `multielasticdump` is now `searchdump-multi`. Every
  container image and release archive also ships compatibility names: `elasticdump` and `elasticdump-rs` →
  `searchdump`, `multielasticdump` → `searchdump-multi`.
- **Environment variables.** The new names are `SEARCHDUMP_*`. The old `ELASTICDUMP_*` names still work as
  deprecated aliases and print a one-time warning; when both are set, the new name wins.
- **Manifest.** `searchdump-multi` still writes and reads the manifest file as `multielasticdump.json` (unchanged,
  so existing dump directories keep working).

### Security

- opendal 0.59.5 removes the dependency advisories RUSTSEC-2026-0194, RUSTSEC-2026-0195 and
  RUSTSEC-2026-0385; `deny.toml` and the cargo-audit configuration no longer ignore any advisory.
