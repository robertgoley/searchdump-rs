# Testing completed

Until the 2026-11-11 rename (L-810a), this project was elasticdump-rs-fork and the binaries were `elasticdump-rs` and `multielasticdump`. Historical rows below keep those pre-rename names; they refer to this same codebase, not to upstream tenyears-tech/elasticdump-rs, unless a row says otherwise.

A one-screen engine coverage matrix (dump, restore, PIT, slices, storage, TLS) is at the top of [TESTING-MATRIX.md](TESTING-MATRIX.md).

Every test task with a commit on `main`, oldest first within each group. The commit message is the short result; full results, numbers and findings for each task are in [`project/plan/PROGRESS.md`](project/plan/PROGRESS.md), and the requirement IDs are in [`project/plan/TEST-REQUIREMENTS.md`](project/plan/TEST-REQUIREMENTS.md).

Total: 217 task commits (including follow-up fixes with a letter suffix). Last updated from commit L-808r (`git log -1 --grep '^L-808r'`; the next commit fills in its sha).

## Setup and planning (L-0xx)

| Task | Commit | Date | Result |
|---|---|---|---|
| L-000 | `d876d57` | 2026-10-29 | env-check script and results/local/env.json |
| L-003 | `b647de0` | 2026-10-29 | real cgroup v2 mode for cg-run.sh (sudo-approved; OOM kill + cleanup), cg-selftest.sh incl. elasticdump-rs OOM atomicity |
| L-002 | `1238e84` | 2026-10-29 | tools (sudo-approved part): apt sources -> HTTPS, strace/perf/valgrind installed, tc present; tools.json updated |

## Repository hygiene and tooling (L-1xx)

| Task | Commit | Date | Result |
|---|---|---|---|
| L-101 | `9d344de` | 2026-10-29 | INJ-F1, TST-INJ-001/002/003/010: flatten hits to one NDJSON line; reject partial bulk lines; live PoC script |
| L-102 | `202c683` | 2026-10-29 | INJ-F2, TST-INJ-006/007/016: validate every index name from files/servers; url() errors on . .. and empty segments; close_put_open takes one concrete index |
| L-103 | `7bfea42` | 2026-10-29 | INJ-F3, TST-INJ-005: type-check _id/routing/parent/version; reject bad documents one by one |
| L-104 | `5ad4579` | 2026-10-29 | INJ-F4: validate and de-duplicate --parseExtraFields names; JSON-escape them |
| L-105 | `18b3a0b` | 2026-10-29 | TST-INJ-008/009/011/012: reject duplicate keys and non-object _source; header CR/LF/NUL is a usage error; query-encoding golden |
| L-106 | `dfc044f` | 2026-10-29 | F-05..F-08: mask --debug query values, redact unparseable endpoints, announce env proxies (--noProxy), redacting Debug impls |
| L-107 | `78c2446` | 2026-10-29 | F-01/02/03/09/10, SUP-013: pinned checksum-verified scanners, SHA-pinned actions, read-only permissions, env-passed inputs, pinned zigbuild |
| L-108 | `17b16d7` | 2026-10-29 | F-04/F-11/F-12/F-13/F-14, INF-014: repo hygiene, tempdir test outputs, loopback+secure compose, private random IT secrets, digest-pinned images |
| L-109 | `f938f91` | 2026-10-29 | SUP-018: findings table F-01..F-14 + INJ-F1..F4 -> fix/test; lint-hygiene.sh, workflow lint F-03/F-09/F-10, Debug redaction test |
| L-108a | `7976c8c` | 2026-10-31 | check-clean-tree.sh no longer fails when target/ is the only ignored path |
| L-108b | `38fb47c` | 2026-10-31 | handoff SHA256SUMS lists the uncommitted b4134af bundle as a note (the git history replaces it) so sha256sum -c passes |

## Supply chain (static) (L-2xx)

| Task | Commit | Date | Result |
|---|---|---|---|
| L-201 | `ade740a` | 2026-10-29 | SUP-004: cargo-vet with Mozilla/Google/BCA/Embark/ISRG/Zcash imports, annotated baseline exemptions, CI gate |
| L-202 | `afa6201` | 2026-10-29 | SUP-005: deny multiple-versions with skip list, banned crates, build-script allow-list, executables/interpreted deny, Linux-only graph |
| L-203 | `2151160` | 2026-10-29 | SUP-006: forbid(unsafe_code), cargo-geiger baseline and diff gate |
| L-204 | `ab01aed` | 2026-10-29 | SUP-007: remove unused thiserror; machete (PR) + udeps (weekly) jobs; enabled-feature justification table |
| L-205 | `9a6a0ba` | 2026-10-29 | SUP-008: llvm-cov over T0+T1+T2 (91.04% fns), never-executed function report + review; remove 7 dead pub fns/impls; weekly coverage job |
| L-206 | `43eb1c5` | 2026-10-29 | SUP-009: build-time code inventory (45 build.rs/18 proc-macros; linux shipped 32/14) with diff gate; offline release build under podman --network none; CI job |
| L-207 | `10cf733` | 2026-10-29 | SUP-011: SBOM + build-time inventory diff per PR (crates/licences/build.rs/proc-macros/image packages), sbom-reviewed label gate, artefact |
| L-209 | `6fd6bf6` | 2026-10-29 | SUP-014: new-dependency policy, CODEOWNERS (owner placeholder), PR template checklist, Dependabot cargo/actions/docker with grouped updates and 7-day cooldown |
| L-213 | `40922bf` | 2026-10-29 | SUP-021: native-code allow-list check (ring, zstd-sys) on the cargo-tree Linux graph; build inventory switched to the feature-resolved graph (26 build.rs/10 proc-macros shipped) |
| L-210 | `cbd2d8b` | 2026-10-29 | SUP-015: gitleaks history/tracked-tree scan + secrets canary suite (secret-scan.sh, CI job); 0 findings |
| L-211 | `9f9a8d7` | 2026-10-29 | SUP-017: image hardening tests (non-root, --read-only + /data dump, cap-drop ALL, executable allow-list per variant, base digest); make test-image + CI |
| L-208 | `e1285b8` | 2026-10-30 | SUP-012: hostile-code review of 4,263 upstream-origin src lines (REVIEW.md, upstream-lines.py checklist); fix U-1 (redact scroll/PIT ids in debug logs), U-2 (benchmark refuses to delete an existing named index) |
| L-212 | `38129a0` | 2026-10-30 | SUP-019: crate publisher snapshot (220 crates/142 publishers) + publishers.py diff + weekly CI job |
| L-214 | `146ec0e` | 2026-10-30 | SUP-023: drop reqwest http2 (h2 out of graph), gate pkcs8 encryption behind default feature encrypted-keys with clear error when off; geiger baseline lowered |

## Test infrastructure: mocks, goldens, fault proxy, generators, load runner (L-3xx)

| Task | Commit | Date | Result |
|---|---|---|---|
| L-305 | `99f1a7f` | 2026-10-29 | INF-005: edverify document-set verifier (multiset SHA-256 digest, buckets, diff names ids, files gz/zst/split, own sliced-scroll cluster reader) |
| L-309/L-318/L-320/L-321 | `702f87a` | 2026-10-29 | INF-009, DATA-001/005/010: edgen seeded generator (8 profiles, mappings, edge catalogue, golden hashes) and converters (rally/clf/loghub, deterministic ids) |
| L-319 | `6b3ff5a` | 2026-10-29 | DATA-004, INF-008: datasets.toml manifest (19 entries, 11 pinned, licence classes), fetch.sh cache manager with resume/checksum/licence gate/prefix, convert.sh, offline selftest |
| L-307 | `48ef459` | 2026-10-29 | INF-007: resource guards (flock serialisation, per-profile memory/disk preflight with SKIP or IT_REQUIRED=1 failure, container --memory/--cpus caps, loopback ports) |
| L-306 | `b49c5c1` | 2026-10-29 | INF-006: measure.sh metrics wrapper (rusage + /proc sampling, stderr parsing, cluster stats), edrs-metrics/v1 schema + validator, compare.py regression gate |
| L-314 | `8591e4d` | 2026-10-29 | LOAD-001/002/003/012: load runner scripts/load/run.sh <tier> <scenario> (tier/place fit, guards, one-engine provisioning, edgen seeding, edverify per step, metrics validation, leak check, teardown); shared scripts/it/matrix.sh; measure.py argv redaction |
| L-301 | `86dfded` | 2026-10-29 | INF-001: nextest tiers (unit/contract/it/load/chaos) with JUnit, slow-timeouts and retries=0; features load/chaos/fuzz (large_scale_test/manual_test -> load aliases); integration_test requires it; CI runs unit+contract via nextest |
| L-302 | `1c2d3c2` | 2026-10-30 | INF-002: router mock (method/path/query/body routing, per-route scripts, keep-alive + concurrent conns, full recording, 10 fault modes) with self-tests; MockEs now wraps it |
| L-312 | `368d347` | 2026-10-30 | INF-012: CI refuses unpinned images (up.sh strict mode under CI/IT_REQUIRE_PIN), check-pins.sh gate (lock vs profiles vs TESTING-MATRIX vs Dockerfile/compose), helper image digests recorded |
| L-313 | `c017ba2` | 2026-10-30 | INF-013: flake policy (retries 0 gate, dated/owned quarantine list in nextest.toml expiring after 14 days, CI exclusion filter) + CHAOS_SEED helper (tests/support/chaos.rs) |
| L-311 | `4e35736` | 2026-10-30 | INF-011: TESTING-MATRIX.md generated blocks (all pinned digests + latest IT-SUMMARY, golden index, API-071 table slot) via scripts/it/matrix-md.py; run.sh appends IT-SUMMARY to $IT_DIR/it-summary.log; CI --check |
| L-310 | `9f3f93c` | 2026-10-30 | INF-010: CI layout - PR T0/T1/T8 + T2 subset (OS2.19 plain/secure, ES9.1 plain, ES7.10 OSS) + T4 load smoke on tool changes; nightly.yml (T2 full, T3 cross, T5 L1R, T7 chaos); soak.yml (T6 on SOAK_RUNNER); failure artefact collector |
| L-303 | `34dd52c` | 2026-10-30 | INF-003: golden record/replay - recording proxy + normaliser (tests/support/golden.rs), router-mock handler hook, IT recorder cases (scroll data, mapping, settings, bulk restore), scripts/golden/record.sh [--check], offline replay binary golden_replay (T1); first goldens for OpenSearch 2.19.7 |
| L-304 | `f96a33d` | 2026-10-30 | INF-004: fault proxy promoted to tests/support/fault_proxy.rs with policy DSL (refuse-every-N, per-direction cut/latency/bandwidth/half-close, RST) + toxiproxy client; scripts/it/toxiproxy.sh; fault_conformance suite passes on both proxy kinds; concurrency IT migrated |

## API behaviour, Node interop and cross-version matrix (L-4xx)

| Task | Commit | Date | Result |
|---|---|---|---|
| L-401 | `42c2974` | 2026-10-30 | API-001: endpoint inventory tests/golden/endpoints.toml (E1-E21 + response pointers), matcher/pointer checker, guard on every MockEs (contract suites) and on committed goldens; golden response bodies no longer NDJSON-decoded |
| L-402 | `13c865c` | 2026-10-30 | API-002: live per-version contract suite (19 golden cases x 9 engine versions, generated endpoint/case x version matrix) |
| L-403 | `d7962ea` | 2026-10-30 | API-003: golden replay budget (<120 s, all 171 cases replay offline in ~5 s) and non-empty guard |
| L-404 | `742cd3d` | 2026-10-30 | API-004: nightly golden drift job (record.sh --check on every plain profile, diff artefact) |
| L-405 | `c1db665` | 2026-10-30 | API-010: scroll unsliced/sliced (=shards, >shards, >max_slices) digest + no open contexts on 9 versions; scroll id rotation and clear-404 contract |
| L-406 | `46b2296` | 2026-10-30 | API-011: PIT unsliced/sliced 2/4/16 digest, no open PITs, scroll fallback warning on versions without PIT (9 versions) |
| L-407 | `ff45336` | 2026-10-30 | API-012: search_after without PIT fails on a shortfall vs the exact total (non-unique sort silently returned 100 of 1000 docs) |
| L-408 | `444e7cc` | 2026-10-30 | API-013: integer hits.total (rest_total_hits_as_int / ES 6) no longer fails extraction; total variants reconciled only when exact |
| L-409 | `961fbb9` | 2026-10-30 | API-014: inline and stored mustache search templates return the plain-query documents (9 versions) |
| L-410 | `709a2d2` | 2026-10-30 | API-015: searchBody variants (_source includes/excludes, stored_fields, docvalue_fields, fields, size override, user sort, user slice) and fields._routing restore on 9 versions; meta lookup order unit test |
| L-411 | `d827729` | 2026-10-30 | API-020: bulk request-shape goldens per action (create/update/delete), external version+routing, gzip body, maxBulkBytes split on 9 versions; normalise ILM in_use_by |
| L-412 | `8fe2c27` | 2026-10-30 | API-021: bulk response accounting equals _count for mapper/document_parsing, version_conflict, document_missing, cluster_block (9 versions); item 429 retry, 413 and item-count mismatch via mock |
| L-413 | `f25976f` | 2026-10-30 | API-022: one _refresh per batch, none with --noRefresh, refresh failure only warns (mock request counts) |
| L-414 | `30633be` | 2026-10-30 | API-030: same-engine mapping round trips lossless per feature (properties-named field, runtime, flattened/flat_object, dense/knn vectors, _source disabled/synthetic, subobjects:false) on 9 versions |
| L-415 | `fd7d236` | 2026-10-30 | API-031: settings round trip equals source modulo generated keys (analysis incl.), generated keys absent from dumps, --excludeSettings (9 versions) |
| L-416 | `ce44e7b` | 2026-10-30 | API-032: --type index inclusion per version; data stream backing indices skipped with a warning (were dumped as plain indices) |
| L-417 | `3c38a1e` | 2026-10-30 | API-033: analyzer restore new/existing/--skip-existing/--closeIndex and reopen after an injected failure (9 versions) |
| L-418 | `ee5fe9d` | 2026-10-30 | API-034: alias round trip keeps is_write_index, is_hidden, filter, index/search routing; per-index and host-only dumps (9 versions) |
| L-419 | `5a98f6b` | 2026-10-30 | API-035: legacy _template round trip on 9 versions; server Warning headers logged once per text, never fatal |
| L-420 | `8f26a3d` | 2026-10-30 | API-036: component/index template round trip (composed_of order, priority, data_stream); per-version built-in list snapshot (builtins.json) for drift; --filterManaged checked against each version's managed list |
| L-421 | `c96d234` | 2026-10-30 | TST-API-037: ILM/ISM policy round trip (1050, paging, overwrite, second 409) |
| L-422 | `99cbc4e` | 2026-10-30 | TST-API-038: OpenSearch 1.3 legacy _opendistro policies are read via _plugins/_ism |
| L-423 | `cc3fa1b` | 2026-10-30 | API-039: stored scripts round trip; 403 on version check or cluster state names the 'monitor' cluster privilege |
| L-424 | `cef2467` | 2026-10-30 | API-040: GET / variants: no silent Elasticsearch 5.0.0 fallback; OpenSearch by tagline; override_main_response_version treated as OpenSearch 1.x |
| L-425 | `9fef496` | 2026-10-30 | API-041: recorded Warning headers logged exactly once in golden replay; X-Elastic-Product missing/unexpected tolerated |
| L-426 | `6e675d3` | 2026-10-30 | API-050: 429 per request kind with exact attempts/delays under paused time; honour Retry-After (seconds) as a capped minimum |
| L-427 | `ed75ade` | 2026-10-30 | API-051: 5xx per request kind; scroll continuation failure says why it was not retried and how to recover |
| L-428 | `f1cedfa` | 2026-10-30 | API-052: partial results name the first shard failure; one-slice failure and timed_out continuation commit nothing; skipped shards accepted |
| L-429 | `6f95cc2` | 2026-10-30 | API-053: version conflicts (create on existing ids, external/external_gte lower versions) counted per item; exit follows --ignore-es-write-errors |
| L-430 | `db01dc0` | 2026-10-30 | API-054: 401/403 name the endpoint (no userinfo/query) and the needed privilege; auth failure matrix on 8 secure profiles |
| L-431 | `18ec847` | 2026-10-30 | API-055: TLS failures: cert factory, one clear message per case, TLS errors never retried |
| L-432 | `a948735` | 2026-10-30 | API-056: server errors lead with type: reason (and caused_by); quoted body capped at 2 KB |
| L-433 | `b917b5a` | 2026-10-30 | API-060: Node interop for all 11 types both directions on ES 8.19 + OS 2.19; Node dumps committed as fixtures |
| L-434 | `b8ac9ce` | 2026-10-30 | TST-API-061: gzip interop both ways with Node; zstd refused by Node |
| L-435 | `5e6fd91` | 2026-10-30 | TST-API-062: split naming interop and non-ASCII boundary difference |
| L-436 | `484f522` | 2026-10-30 | TST-API-063: data file variants restore to the same digest |
| L-437 | `55d5299` | 2026-10-30 | docs: Node interop findings for compression, big ints and split sizes |
| L-438 | `73a26a9` | 2026-10-30 | API-070: cross-version matrix, export once and import everywhere; name the unknown setting in the --excludeSettings hint |
| L-439 | `6832f3d` | 2026-10-30 | API-071: committed cross-version expectation table and asserter |
| L-440 | `cf928e1` | 2026-10-30 | API-072: live cluster-to-cluster pairs checked against the expectation table |
| L-441 | `00a7ea2` | 2026-10-30 | BRK-NET-003: never re-send auto-id bulk batches that may have been applied; create 409s after a re-send count as written |
| L-442 | `175700f` | 2026-10-30 | BRK-NET-004: default 300 s idle timeout (--idleTimeout) so a stalled server cannot hang a run |
| L-443 | `3671ab2` | 2026-10-30 | BRK-NET-006: stale keep-alive connections (LB idle close, FIN and RST) do not fail scroll continuations |
| L-444 | `5f2c817` | 2026-10-30 | BRK-SRV-001: failed writes exit 3 unless accepted explicitly; item errors kept after retries |
| L-445 | `0ce4a4d` | 2026-10-30 | BRK-SRV-006: response size cap (--maxResponseMb, default 256) with a --limit hint; 512 MB cgroup check |
| L-446 | `74a0e57` | 2026-10-30 | BRK-SRV-009: scroll/PIT reads must retrieve exactly the snapshot total |
| L-447 | `58b588c` | 2026-10-30 | BRK-IN-002: truncated or corrupt gzip/zstd input names the byte offset and documents read |
| L-448 | `1f90058` | 2026-10-30 | BRK-IN-004: refuse metadata dumps restored as data and data dumps restored as metadata |
| L-449 | `2c184dc` | 2026-10-30 | BRK-IN-005: malformed values rejected per line with their input number |
| L-450 | `adf897d` | 2026-10-30 | BRK-IN-007: --maxDocMb guard for giant single values (default 100 MB) |
| L-451 | `b083fb6` | 2026-10-30 | BRK-RES-002: all-or-nothing split commit with rollback; disk-full tmpfs check |
| L-452 | `be1a5eb` | 2026-10-30 | BRK-CLI-003: refuse many-to-one renames unless --allowMerge |
| L-453 | `25aa9a5` | 2026-10-30 | BRK-CLI-007: secret registry scrubs logs/errors; canary matrix; connect_timeout = idleTimeout |
| L-454 | `199b51b` | 2026-10-30 | BRK-SIG-001: SIGTERM handled like SIGINT; exit 128+signal |
| L-455 | `aa5fe31` | 2026-10-30 | BRK-XV-002: hits without _source stop the dump unless --allowMissingSource |
| L-456 | `9e30921` | 2026-10-30 | IT: typed mapping only on Elasticsearch < 7 |
| L-431a | `b0f9106` | 2026-10-31 | a TLS client-certificate rejection is never retried (fixes the tls_failures_have_one_clear_message flake) |
| L-455a | `a74656d` | 2026-10-31 | a missing _source caused by stored_fields in the search body is named as such (still refused by default); was already failing on 38fb47c |
| L-455b | `f2818bf` | 2026-10-31 | refuse a search body that turns _source off unless --allowMissingSource |
| L-455c | `3c2ebbf` | 2026-10-31 | behaviour change: an empty _source ({}) is refused like a missing one unless --allowMissingSource; an excludes-all _source filter is refused before the first search |

## Property tests, fuzzing and injection (L-5xx)

| Task | Commit | Date | Result |
|---|---|---|---|
| L-501 | `524f014` | 2026-10-30 | API-080: proptest properties for the NDJSON splitter (10k cases in T0) |
| L-502 | `b5bd899` | 2026-10-30 | Node comparison: integer-like keys are ordered first in JavaScript (documented difference) |
| L-503 | `78d2997` | 2026-10-30 | API-082: URL redaction properties; mask passwords that URL parsing moves into host/path |
| L-504 | `e3a5291` | 2026-10-30 | API-083: rename rule properties vs a model; index-name oracle against live clusters; refuse titlecase |
| L-505 | `a5f1e13` | 2026-10-30 | API-084: fuzz file_to_bulk 300s: ok |
| L-506 | `f2f010b` | 2026-10-30 | API-085: differential bulk encoding against Node (10k docs, 0 undocumented differences) |
| L-507 | `9af84bf` | 2026-10-30 | API-086: ILM->ISM property oracle on OS 1.3/2.19/3.9; refuse phase-less policies in lenient mode (panic); no ISM shrink before OpenSearch 2.0 |
| L-508 | `50d9f33` | 2026-10-30 | SUP-020: zstd_in/gzip_in fuzz targets, 300 s runs clean; cgroup bomb script; ruzstd decision recorded |
| L-509 | `7dd0206` | 2026-10-30 | SUP-022: sonic_vs_serde differential fuzz target (ASan, 300 s clean); pin sonic-rs =0.5.10 |
| L-510 | `53e6736` | 2026-10-30 | INJ-004: encoder differential proptest (100k cases vs a serde_json reference); 5 min slow-timeout for property suites in the unit profile |
| L-511 | `f1cd5d2` | 2026-10-30 | INJ-014: escaping corpus round trip byte for byte on OS 2.19 / ES 8.19; Node differences documented |
| L-512 | `79b34de` | 2026-10-30 | INJ-015: fuzz targets complete (6 targets x 300 s, 2 findings fixed) |
| L-513 | `a103356` | 2026-10-30 | INJ-017: Node elasticdump injection-PoC comparison; clearer rejected-line summary |

## Break tests: resources, CLI, signals, ILM, cross-version (L-6xx)

| Task | Commit | Date | Result |
|---|---|---|---|
| L-601 | `91fe4b5` | 2026-10-30 | BRK-NET-001: RST mid scroll-continuation body: not retried, cleared, rerun recipe; chaos tier |
| L-602 | `28f762d` | 2026-10-30 | BRK-NET-002: RST mid PIT page (unsliced, 16 slices, rotating ES PIT ids): retried, output equals fault-free run over 50 seeds |
| L-603 | `12f14a9` | 2026-10-30 | BRK-NET-005: --timeout bounds silence (Node request semantics), --requestTimeout is the total deadline; slow drip test |
| L-604 | `ce89dcb` | 2026-10-30 | BRK-NET-007: half-closed sockets (FIN after headers / FIN without answer) on search and bulk: retried or clear failure |
| L-605 | `44fcd40` | 2026-10-30 | BRK-NET-008: DNS: unknown host fails fast (not retried), black-holed resolver bounded by --timeout, mid-run lookup failure retried |
| L-606 | `96d2397` | 2026-10-30 | BRK-NET-009: node restart mid-dump/restore: context-lost and PIT rerun messages, connection-refused hint, live restart script |
| L-607 | `a435a3c` | 2026-10-30 | BRK-NET-010: HTML from proxies: 2xx text/html refused (not retried) as 'expected JSON from <host>', HTML error pages summarised by title |
| L-608 | `9019595` | 2026-10-30 | BRK-NET-011: broken encodings (gzip header on plain body, cut gzip, unrequested gzip, wrong Content-Length): clear errors, never a partial page |
| L-609 | `40ae490` | 2026-10-30 | BRK-NET-012: redirects are never followed; 3xx stops the run naming the target |
| L-610 | `5558383` | 2026-10-30 | BRK-TLS-001: server cert failures name reason, host and --ca/--insecure way out |
| L-611 | `09bdfe9` | 2026-10-30 | BRK-TLS-002: client cert faults: server-rejected cert, DER, empty, key mismatch, CA bundle with key; clear per-case messages |
| L-612 | `5c43565` | 2026-10-30 | BRK-TLS-003: TLS 1.0-only, RSA-1024 server key, silent https port: clear, permanent, bounded |
| L-613 | `8044ebc` | 2026-10-30 | BRK-TLS-004: SSL_CERT_FILE/DIR honoured; unusable ones fall back to bundled roots with a warning |
| L-614 | `28412c0` | 2026-10-30 | BRK-SRV-002: clippy format_collect fix in the test |
| L-615 | `604b40f` | 2026-10-30 | BRK-SRV-003: mixed bulk items: status under any single key, failed docs named by the id sent |
| L-616 | `8f1337a` | 2026-10-30 | BRK-SRV-004: bulk answers without one item per document (even errors:false) or not JSON fail; never counted as written |
| L-617 | `2658b9f` | 2026-10-30 | BRK-SRV-005: malformed search page never reaches stdout partially |
| L-618 | `3e5a706` | 2026-10-30 | BRK-SRV-007: lost scroll/PIT context mid-dump names --scrollTime/--pitKeepAlive (live IT) |
| L-619 | `89a834e` | 2026-10-30 | fix: default (non-test) build: read the _settings body without reqwest's json feature |
| L-620 | `19810f1` | 2026-10-30 | BRK-SRV-010: duplicate hits across pages fail the scroll total check or are written as served |
| L-621 | `b3279fb` | 2026-10-30 | BRK-SRV-011: parallel create restores count 409 conflicts correctly, named once |
| L-622 | `37a8c45` | 2026-10-30 | BRK-SRV-012: red index (missing shards) fails with a clear 'index is red' hint |
| L-623 | `13c556d` | 2026-10-30 | BRK-IN-001: truncated/garbage gzip names file, offset and docs read (test) |
| L-624 | `aa6d98b` | 2026-10-30 | BRK-IN-003: bzip2/xz/lzma/lz4/zip/7z and tar archives are refused by magic bytes |
| L-625 | `3755fd2` | 2026-10-30 | BRK-IN-006: invalid UTF-8 names the document; UTF-16 input refused with an iconv hint |
| L-626 | `1148f32` | 2026-10-30 | BRK-IN-008: deep nesting never overflows the stack (bigger stacks, 1000-level guard) |
| L-627 | `57e7f4c` | 2026-10-30 | BRK-IN-009: duplicate keys: metadata rejected, inside _source forwarded verbatim (test) |
| L-628 | `c7dc5d9` | 2026-10-30 | BRK-IN-010: summary reports writes that overwrote an existing _id |
| L-629 | `a9014b2` | 2026-10-30 | BRK-IN-011: numeric _id/_routing stringified as Node does; _source byte-exact; non-finite ids refused |
| L-630 | `f2441ee` | 2026-10-30 | BRK-IN-012: special characters: ids sent verbatim in the body; invalid --output index names refused up front |
| L-631 | `7071c6a` | 2026-10-30 | BRK-IN-013: Node meta encodings: lone surrogate escapes repaired, odd encodings refused by name |
| L-632 | `8485788` | 2026-10-30 | BRK-IN-014: empty inputs: data warns (exit 0), meta names the empty file |
| L-633 | `c042e0c` | 2026-10-30 | BRK-IN-015: split part gaps and staging leftovers are reported; a staging file as input is refused |
| L-634 | `344c5cb` | 2026-10-30 | BRK-IN-016: input truncated/replaced/appended while reading has defined behaviour |
| L-635 | `d7ea13c` | 2026-10-30 | BRK-RES-001: memory estimate warning; cgroup 64/128/256 MB runs stay within the README formula |
| L-636 | `01635bf` | 2026-10-30 | BRK-RES-003: ENOSPC on stdout names stdout; closed stderr/stdout behaviour defined |
| L-637 | `172a44d` | 2026-10-30 | fix: read the open-file limit once at start (unreadable once exhausted); test asserts the real limit |
| L-638 | `3dadea7` | 2026-10-30 | BRK-RES-005: extreme numeric flags never panic; page/slice limits explained |
| L-639 | `e2561ee` | 2026-10-30 | BRK-RES-006: --fileSize below 256b refused; a split past 10,000 parts warns |
| L-640 | `d4e2a1e` | 2026-10-30 | BRK-RES-007: --maxSockets caps requests in flight under high --concurrency (test, README) |
| L-641 | `70e0898` | 2026-10-30 | conflicting-flag table (42 combos); fix --offset/--size overflow panic, refuse --offset with --slices |
| L-642 | `b4134af` | 2026-10-30 | refuse same input and output file/index unless --force |
| L-643 | `c38c46d` | 2026-10-31 | TST-BRK-CLI-004: regex abuse is a clear usage error |
| L-644 | `4f9115c` | 2026-10-31 | BRK-CLI-005: output path abuse is a clear usage error; symlink targets never replaced |
| L-645 | `48ca62e` | 2026-10-31 | per-direction credentials (D-4), documented precedence, no password leaks |
| L-646 | `7802d7e` | 2026-10-31 | refuse mistyped/scheme-less cluster URLs (BRK-CLI-008) |
| L-644a | `a3b00c5` | 2026-10-31 | follow symlinked parent dirs for --output |
| L-647 | `4ccb2fb` | 2026-10-31 | SIGINT during a restore finishes in-flight bulks and prints the resume --offset (BRK-SIG-002) |
| L-648 | `34ecd28` | 2026-10-31 | kill -9 crash consistency (BRK-SIG-003) and D-5 leftover warning |
| L-649 | `9736389` | 2026-10-31 | signals while --closeIndex has an index closed; force-quit names what it leaves (BRK-SIG-004) |
| L-650 | `acb12de` | 2026-10-31 | BRK-SIG-005 resume semantics end to end; first signal during setup no longer leaves a staged output |
| L-650a | `384df3e` | 2026-10-31 | a first signal during a metadata dump to a file no longer leaves its staged output |
| L-651 | `e5319c7` | 2026-10-31 | unconvertible ILM policies are refused before any write; OpenSearch accepts every converted policy |
| L-638a | `73a482f` | 2026-10-31 | size the l638 time limit to the --intervalCap worst case |
| L-652 | `ed53602` | 2026-10-31 | odd but valid ILM policies convert with lifecycle order and units kept |
| L-650b | `ab6de91` | 2026-10-31 | l650a timeout captures process state, wchan, gdb stacks and stderr |
| L-653 | `44ee5bc` | 2026-10-31 | ES built-in policies convert or refuse clearly; ISM → ILM refusal names the policies |
| L-654 | `89a0f46` | 2026-10-31 | OpenSearch with override_main_response_version is detected through GET _nodes; an unresolvable 7.10.2 fails clearly |
| L-655 | `f3e9884` | 2026-10-31 | field types without an equivalent on the target are refused before any write, naming path, type and closest type; cross-engine copy to a missing target refused |
| L-656 | `9c92547` | 2026-10-31 | BRK-XV-005: features the target lacks (ES 7.10 OSS, OS 1.3) refused clearly before any write |
| L-657 | `88b576d` | 2026-10-31 | BRK-XV-006: integer hits.total and _type in hits pinned end to end (scroll, search_after, total check, typed restore); tests only |
| L-651a | `a3f090a` | 2026-10-31 | roll back a policy set rejected mid-way; refuse wait_for_snapshot in lenient mode |
| L-642a | `748a15b` | 2026-10-31 | same-target check applies --prefix/--suffix to the output index |
| L-629a | `0f08c23` | 2026-10-31 | keep numeric and odd _id/routing literals byte for byte |
| L-642b | `a38c3bd` | 2026-10-31 | same-target check resolves a host-only --output |
| L-645b | `8798ac5` | 2026-11-14 | owner decision: `--httpAuthFile` wins over flags, URL and env in both directions (behaviour change; warning names replaced sources); values literal and world-readable warning unchanged |

## Runtime supply chain (L-7xx)

| Task | Commit | Date | Result |
|---|---|---|---|
| L-701 | `62f0c7d` | 2026-10-31 | SUP-001 egress-denied test runs |
| L-702 | `948f73f` | 2026-10-31 | syscall and file-access tracing against a reviewed allow-list (SUP-002) |
| L-702a | `259d3c4` | 2026-10-31 | lint every workflow with reinstalled actionlint/zizmor; fix own nightly jobs |
| L-704 | `fdab304` | 2026-10-31 | reproducible-build check (SUP-010) |
| L-705 | `73c3ad6` | 2026-10-31 | release-candidate behaviour diff (SUP-016) |

## Load and data generation (L-8xx)

| Task | Commit | Date | Result |
|---|---|---|---|
| L-801 | `8cad039` | 2026-10-31 | LOAD-004 scenario matrix with correctness and perf gates (25 cells; five baselines; L1 10M needs a dedicated load host) |
| L-802 | `64e7f27` | 2026-10-31 | LOAD-005 memory checks (scripts/load/memcheck.py matrix/files/plateau): README formula changed to x3 + 15 MB (lead review); concurrent restores grow with doc count and plateau at 10M only with MALLOC_ARENA_MAX=2; full L1 at 10M needs a dedicated load host |
| L-802a | `767d422` | 2026-11-01 | Allocator study for LOAD-005 growth (scripts/load/allocbench.py): heaptrack shows no leak (restore heap flat, copy heap bounded by the bufferSize queues); RSS growth is glibc malloc retention; optional `jemalloc`/`mimalloc` cargo features (default unchanged); ValueSplitter/FileSink scratch buffers give back capacity after an outsized document; 100k/1M x3 reps and 10M x1 rep for 8 allocator/build variants; builds verified on x86_64/aarch64 gnu/musl |
| L-803 | `03156fc` | 2026-10-31 | LOAD-006 kill/resume runs (scripts/load/resume.py): dump kill -9 x10 + `--overwrite`, restore kill -9 rerun, `--offset N` after SIGINT/SIGTERM and after kill -9 from `_count`, `--scrollId` recipe; all digests equal at 100k and 1M on es717-plain; 10M needs a dedicated load host; OpenSearch PIT resume waits on owner Q4 |
| L-804 | `56aa0c6` | 2026-11-01 | LOAD-007 fault injection under load (scripts/load/faults.py) at 1M on es717-plain: connection/request cuts, 429 storm, 30 s flood-stage block, `podman restart`; 27 runs, 16 complete with equal digests, 11 exit non-zero with a clear message, none exit 0 with loss; lead classifies the non-zero exits; 10M with real watermarks is Phase B |
| L-805 | `18c4334` | 2026-11-01 | LOAD-008 compression levels at 1M (scripts/load/compress.py): gzip 1/6/9 and zstd 1/3/9/19, file-to-file and dump; all outputs decode independently and read back byte-equal, `cat` of `--fileSize` parts readable; README per-core numbers were outside ±20% and are replaced with the measured table; zlib-rs gzip 9 is larger than gzip 6; 10M and multi-GB files are Phase B |
| L-806 | `92a2a69` | 2026-11-01 | LOAD-009 splitting at scale at 1M (scripts/load/split.py): 13 cases with --fileSize 256mb / scaled ~25 MB and --maxRows 1000000 / 100000 / 300000, file and cluster input, gzip, zstd, --slices 4; part counts as expected, no document spans parts, each part <= limit + 1 doc, all parts digest-equal, parts committed together (BRK-RES-002); per-part restore digest-equal; kill -9 and --overwrite-with-fewer-parts checked; no code change; 10M with 256 MB parts is Phase B. |
| L-807 | `e9d8cc5` | 2026-11-01 | LOAD-010 concurrency curves at 1M (scripts/load/curves.py): dump --slices 1-16 with workers = slices (scroll and PIT, 1 and 4 shards) and restore --concurrency 1-16, 3 reps; docs/s, tool and server CPU-s, CV per point; all runs exit 0 with full counts and first-rep digests equal; box baseline committed and a second run gates clean (no point > 10% worse), doubled-baseline negative check fails as it should; README slice ratios within ±20%; no code change; authoritative curves are B10. |
| L-808 | `27d3b7a` | 2026-11-01 | LOAD-011 Node comparison at 100k and 1M (scripts/load/nodecmp.py): Node elasticdump 6.125.1 vs elasticdump-rs, same flags, dump defaults / --limit 5000 / --fsCompress / sliced, restore and copy, 3 reps; Node output digest-equal to Rust output in every dump scenario, restore/copy digest-equal, no Rust scroll left open; ratios recorded against PLAN §8 (sliced, gzip, CPU met; restore/copy engine-bound on the box); README Performance table re-measured on OpenSearch 2.19.7 and replaced; Rust restore/copy CPU about 2x M6 (A/B-confirmed, commit unattributed) reported to the lead; no code change. |
| L-808a | 4a0dc41 | 2026-11-01 | Docs only: TESTING-TODO.md row for networked backup/restore performance with compression at scale (separate machines, multi-node cluster, 10M+ docs, measured network), baselined on the single-machine L-805/L-807 box runs. |
| L-808b | 9cc4de2 | 2026-11-01 | User decision: glibc builds default to mimalloc (musl keeps its allocator; `system-allocator` opt-out), zigbuild matrix 15/15, allocbench 100k/1M matches L-802a mimalloc; default image glibc + mimalloc on dhi.io/static glibc (24.5 MB; Alpine 20.7 MB as :<ver>-alpine), test-image 12/12 + 13/13, OpenSearch 2.19.7 dump/restore smoke 1000/1000, arm64 base resolves; scripts/build-image.sh; gnu release targets. |
| L-808c | de51469 | 2026-11-01 | User decision: license attribution, project stays MIT: 12 elasticsearch-dump fixtures verified byte-identical, tests/fixtures/node/LICENSE-APACHE-2.0 + SOURCE.md, THIRD-PARTY-NOTICES.md (scripts/third-party-notices.py) in release archives and both images, README Acknowledgements, tagline reworded. |
| L-808d | 16e4482 | 2026-11-01 | User decisions: the default image's 5 glibc/libgcc-s High CVEs (no fix) accepted; `make scan` ignores exactly those ids while not-fixed/wont-fix and passes; TESTING-TODO rows for the l638 timing flake (stable machine) and the low-priority arm64 image build. |
| L-808e | 09bdab8 | 2026-11-01 | Handoff state (docs only): usage rule, `wip/L-NNN` branches, queue and skip list, 2026-11-01 user decisions and open items recorded in RESUME-FOR-GROK-BOT.md and the handoff docs. |
| D-2 | f96c434 | 2026-11-01 | gzip stays the default codec and zstd is opt-in (code, `--help`, README) verified; `compression_default_tests` added (4 unit tests); help and README wording made explicit. |
| L-811 | aaff19a | 2026-11-01 | LOAD-017: leak check (scroll, PIT or ES open_contexts, `.part` files incl. hidden staging files, stray/hidden indices) asserted in the teardown of every load script; `leakcheck_selftest.py` PASS on es717-plain and opensearch-plain. |
| L-808f | 78a29f6 | 2026-11-01 | CI: `scripts/third-party-notices.py --check` added to security.yml (dependencies job); passes locally, fails (rc 1) on a stale file. |
| L-808g | d138b07 | 2026-11-01 | rc-signoff/trace-allow: the 2 mimalloc start-up reads confirmed by the owner (read-only allocator topology/THP probes); trace.sh plain passes. |
| L-805a | ee1384e | 2026-11-01 | Warnings (not errors) for gzip level 9 and zstd 19+ (README table numbers) and, with `--overwrite`, a note that killed-run leftovers are not removed; 2 unit + 2 contract tests; README/help updated; defaults unchanged. |
| L-808h | 8a4a06c | 2026-11-01 | security.yml `image` job skipped cleanly on Dependabot PRs; missing-secret hard failure kept elsewhere; actionlint clean; no repo settings or secrets touched. |
| L-808i | 9348352 | 2026-11-01 | Offline container build: host-vendored crates, `RUN --network=none` cargo step; glibc and alpine images built and test-image.sh passes for both; README updated. |
| L-808j | ac67505 | 2026-11-09 | CI Rust 1.91.1: dtolnay/rust-toolchain pinned to its master SHA with explicit `toolchain:` (the 1.85.1 branch pin ignored it, so nightly steps got 1.85.1); 3 clippy 1.91 lints fixed, no behaviour change; MSRV stays 1.85; scanner downloads retried. |
| L-808k | 74b1b16 | 2026-11-09 | Three approved CI fixes: remove image.tar before saving again; l654 CLI uses the authenticated URL (passes OpenSearch 2.19.7 plain and secure); load-smoke builds all release workspace binaries. Local unit 283/283, contract 113/113, strict clippy and static gates pass; exact load-smoke gets past binary discovery but seed hits HTTP 429 parent circuit breaker (not changed). |
| L-808l | `30f2122` | 2026-11-09 | Scope l618 scroll/PIT deletion to the CLI's own context and assert bystanders survive; full parallel ES7.10 suite passes twice. Bound L0 seed bulks (500 docs / 1 MiB), retry HTTP/item 429 with backoff, preserve duplicate-ID ordering, and retry harness control-request 429s. Three exact L0 round trips pass at the unchanged 1024m heap, with digest/leak checks; 16 Python regression tests and full applicable gates pass. |
| L-808m | `f872f87` | 2026-11-09 | L0 verification uses configurable 100-doc pages / one slice; larger-tier and standalone defaults stay 1000 / four. edverify retries HTTP 429 with bounded exponential backoff and clears every known own scroll ID after all slice tasks finish, on success/error/panic; cleanup failures are fatal. 13 HTTP/unit regressions, 18 Python tests and all applicable gates pass. Three consecutive exact L0 runs pass on the unchanged 1024m heap with all digests and zero scroll/PIT leaks. |
| L-804a | 8bcc92c | 2026-11-14 | exit-code review of the 11 L-804 non-zero exits by error point (results/local/L-804a.md): all codes correct, exit 1 mixes transient and permanent failures; final HTTP 429 message now names the retry/load knobs; proposals P1-P6 for the owner |
| L-808n | b158898 | 2026-11-14 | restore CPU profile vs M6 (results/local/L-808n.md): memchr line split + SIMD depth shortcut, restore CPU -24% (0.80 -> 0.61 s per 100k docs), wall and RSS unchanged; next candidates listed |
| L-810i | 15f049f | 2026-11-14 | OpenSSH client in the Debian/Alpine images for sftp:// (dhi-static unchanged); SFTP round trip OK in public Debian and Alpine builds; 22 OpenSSH/Kerberos High/Critical ids accepted by the owner (t85u) and ignored by `make scan` only while unfixed |
| L-808o | b1af6c0 | 2026-11-14 | restore validates each document once (check_and_parse) instead of twice; same verdicts and messages (differential test); restore CPU -14% vs L-808n (results/local/L-808o.md) |
| L-808p | 7c5391f | 2026-11-14 | restore moves the encoded batch into the bulk request instead of copying it and pre-sizes file batch buffers; CPU -12%, peak RSS -17% vs L-808o (results/local/L-808o.md) |
| L-808q | f875663 | 2026-11-14 | file-input splitter skips non-structural bytes (SIMD/table) with identical output (proptest); restore CPU -25% vs L-808p, about half of pre-L-808n and below M6 (results/local/L-808o.md) |
| L-808r | this commit | 2026-11-14 | batches pass through the Sink as shared Bytes, no copy at --concurrency > 1; RSS -3% (c4) / -8% (c1), CPU level with M6 (results/local/L-808r.md) |

## Supply-chain scans (Grok Bot)

Run 2026-11-02 at 37fa3ac (read-only). Moved from TESTING-TODO §2.

| Test | Result | Findings |
|---|---|---|
| Commit provenance audit | 0 unknown authors; 2/351 signed (web-flow); no branch protection on main | [security/findings/2026-11-02-commit-provenance.md](security/findings/2026-11-02-commit-provenance.md) |
| Hidden-code scans | 0 suspicious; vendor hits are ZWJ/LRM in emoji/RTL test strings and URL test vectors | [security/findings/2026-11-02-hidden-code-scan.md](security/findings/2026-11-02-hidden-code-scan.md) |
| Build scripts and proc macros | 63 build-time crates; rustc probes, cc (ring, mimalloc, zstd), no network or exfil | [security/findings/2026-11-02-build-scripts-proc-macros.md](security/findings/2026-11-02-build-scripts-proc-macros.md) |
| Workflow hardening review | all 72 actions SHA-pinned, contents: read, no pull_request_target, secrets via env only | [security/findings/2026-11-02-workflow-hardening.md](security/findings/2026-11-02-workflow-hardening.md) |
| Secret exposure on the host | flag/URL passwords visible in ps/cmdline (as Node); env and --httpAuthFile safe; logs clean | [security/findings/2026-11-02-secret-exposure.md](security/findings/2026-11-02-secret-exposure.md) |
| Maintainer and typosquat review | 0 suspicious; single-maintainer crates noted (ring, reqwest, zstd); 1 benign look-alike (rend) | [security/findings/2026-11-02-maintainer-typosquat.md](security/findings/2026-11-02-maintainer-typosquat.md) |
| C code verification | zstd (signed tarball), mimalloc v2/v3, ring sources and 28 x86_64/aarch64 asm files identical to upstream | [security/findings/2026-11-02-c-code-verification.md](security/findings/2026-11-02-c-code-verification.md) |
| Binary inspection | no exec/spawn imports, no unexpected URLs/IPs/domains; build paths embedded | [security/findings/2026-11-02-binary-inspection.md](security/findings/2026-11-02-binary-inspection.md) |
| Toolchain integrity | zizmor, gitleaks, actionlint identical to releases; rustup toolchain and other plugins not verified | [security/findings/2026-11-02-toolchain-integrity.md](security/findings/2026-11-02-toolchain-integrity.md) |
| CR/LF header injection | headers rejected (TST-INJ-011), params percent-encoded, creds base64; added TST-INJ-012 | [security/findings/2026-11-02-crlf-injection.md](security/findings/2026-11-02-crlf-injection.md) |
| Hidden-Unicode lint (F-15) | scripts/lint-hygiene.sh F-15; fails on planted RLO/ZWSP | [security/findings/2026-11-02-hidden-code-scan.md](security/findings/2026-11-02-hidden-code-scan.md) |
| zizmor in CI | security.yml `zizmor` job, pinned 1.31.1, SHA-256 checked; 0 findings | [security/findings/2026-11-02-workflow-hardening.md](security/findings/2026-11-02-workflow-hardening.md) |
| Signing and branch protection steps | owner instructions; not applied | [security/findings/2026-11-02-signing-branch-protection-steps.md](security/findings/2026-11-02-signing-branch-protection-steps.md) |

## multielasticdump (Grok Bot, 2026-11-07)

| Test | Result | Where |
|---|---|---|
| Unit tests (selection, group break-out, types incl. cluster-wide and `all`, mapping body shapes, credential order and redaction, record counts for gzip/zstd/plain, discovery in flat/index/group layouts and from the manifest, alias/data-stream target selection, split-part names, gaps, mixed codecs and cleanup, `--fileSize`/`--maxRows` detection, cluster-run flags, `--interval` pacing) | 11/11 pass; clippy `-D warnings` and fmt clean on 1.85.1 | `src/bin/multielasticdump.rs` |
| Dump/load round trip against in-memory fake nodes (Elasticsearch- and OpenSearch-flavoured) | 20/20 checks: group layout + zstd3 + alias, counts 7/3/5, lifecycle dropped, mapping and alias restored, existing target refused then `--skipExisting`, hidden and empty indices, `oslogs.py` file names, load without a manifest, `--dryRun` | `scripts/it/multielasticdump-roundtrip.sh` |
| Round trip, second pass (debug and release) | 38/38 checks: `--matchType alias,index`, analyzer dump and analyzer-only load, all five cluster-wide types dumped to `_cluster/` and loaded first (templates, component template, legacy template, ILM policy, stored script), template lifecycle dropped, ILM converted to ISM on the OpenSearch-flavoured node, `--maxRows` split dump (2 and 5 parts) loaded with summed counts, `--resume` with parts, data stream dump (marker + data, no backing-index files) and load by create through its template, existing stream refused, `--interval` 400 ms spacing, `--order desc`, `--fileSize` with `--maxRows` refused | `scripts/it/multielasticdump-roundtrip.sh` (`KEEP_W=DIR` keeps the work directory) |
| `--nodeCompat` against the real Node elasticdump 6.125.1 (npm), unit test 12/12 | 49/49 round-trip checks (debug and release): Node names and gzip, only index files in DIR with manifest and `_cluster/policy` in `DIR.meta`, split/zstd/layout/datastream refused, our load finds `DIR.meta`; Node `multielasticdump --direction=load` restores our dump (counts 7/3/5, mapping, legacy template); our load restores Node's own dump; our settings/mapping/template files byte-identical to Node's | `scripts/it/multielasticdump-roundtrip.sh` with `NODE_ED_BIN` |
| Node flag names (`--ignoreX[=true\|false]`, `--includeType`, `--ignoreIndex`, `--ignoreChildError`) | unit test 13/13; round trip 53/53 with Node checks (dump with `--ignoreData --ignoreAlias=false`, load with `--includeType=index`) | `src/bin/multielasticdump.rs`, `scripts/it/multielasticdump-roundtrip.sh` |
| Against a real cluster | not run (no cluster on the Grok Bot box); see TESTING-TODO | |

## OpenDAL storage path (L-809a, Grok Bot, 2026-11-11)

Rootless podman 5.4.2 with `--network=host`, debug build, Rust 1.85.1 (L-809a) and 1.91.1 (L-809d). Scripts in `scripts/opendal-tests/` (`run-all.sh` runs them all; `KEEP=1` keeps the work dir; `ODT_START_ONLY=1` only starts a server and writes its settings).

| Test | Result | Where |
|---|---|---|
| Unit tests: scheme detection, s3/b2/sftp/fs parsing, query settings, URL passwords and secret params refused, redaction (Debug/Display/`redact`), INI aliases and section precedence over env, secrets registered for scrubbing, SSE-C key forms (base64, raw 32-byte file, base64 file, wrong length), b2 required settings, sftp `known_hosts` values, `copy_tree` on fs | pass (lib 261/261 default features, 259/259 `--no-default-features`; whole suite `cargo test --all-targets` all pass); clippy `-D warnings` clean for default, `--no-default-features` and `--no-default-features --features opendal`; fmt clean | `src/remote.rs`, `src/pipeline/file_sink.rs` |
| fs: zstd split write through `fs://`, parts in place with no temp files, `file://` read back, existing file refused, directory refused as output, multielasticdump copy to `fs://` identical; lib test: split + compression round trip and abort cleanup | 7/7 | `scripts/opendal-tests/fs.sh`, lib test `opendal_fs_split_compressed_round_trip_and_abort` |
| S3 on MinIO (`bitnamilegacy/minio:2025.7.23`, official MinIO binary; `minio/minio` images not pullable here) over TLS with a private CA (`ca_file` in the INI) | 24/24: plain, gzip `--maxRows` split and zstd round trips with every document kept, existing object refused without `--overwrite` then replaced, secret in the URL refused, wrong secret fails without printing it, `--debug` log of an SSE-C run holds neither the secret key nor the SSE-C key, multielasticdump copy local -> s3 -> local identical (`diff -r`) and refused onto existing objects; SSE-C: write, read with the key, all documents back, read without the key fails, wrong key fails, plain GET of the object refused (HTTP 400) | `scripts/opendal-tests/s3.sh minio` |
| S3 on RustFS (`rustfs/rustfs:latest`, 1.0.5) over TLS | 24/24, same checks; SSE-C supported (no key, wrong key and plain GET all fail) | `scripts/opendal-tests/s3.sh rustfs` |
| SFTP on OpenSSH (alpine 3.24.3, `sshd` + its sftp-server) | 14/14: ed25519 key via `key_file`, unknown host key refused with the default `known_hosts=strict` (error hints at known_hosts), plain round trip and file present on the server, gzip `--maxRows` split round trip with gzip parts on the server, existing file refused then `--overwrite`, password in the URL refused, multielasticdump copy local -> sftp -> local identical; other runs use `known_hosts=accept` | `scripts/opendal-tests/sftp.sh openssh` |
| SFTP on Dropbear (alpine 3.24.3, Dropbear paired with OpenSSH's `/usr/lib/ssh/sftp-server`, since Dropbear has no SFTP subsystem of its own) | 14/14, same checks | `scripts/opendal-tests/sftp.sh dropbear` |
| Cross-store and cluster (MinIO + OpenSSH + OpenSearch 2 in an isolated podman store) | 20/20: elasticdump-rs local -> s3 -> sftp -> local (gzip kept); multielasticdump `--direction copy` local -> s3 -> sftp -> local byte-exact incl. `_cluster/` and manifest; elasticdump-rs OpenSearch -> s3 zstd in `--maxRows` parts, mapping to s3, s3 mapping + parts -> OpenSearch with all documents; multielasticdump dump OpenSearch -> `s3://` (staged), copy s3 -> sftp, load `sftp://` -> OpenSearch with `--suffix` and matching counts | `scripts/opendal-tests/cross.sh` |
| Supply-chain gates with the new dependencies | cargo deny (advisories with 3 documented ignores, bans, licenses, sources), cargo audit `--deny warnings` (same ignores in `.cargo/audit.toml`), cargo machete, cargo vet (80 new exemptions), native-code check, build inventory and geiger baselines updated, THIRD-PARTY-NOTICES regenerated: all pass | `deny.toml`, `supply-chain/` |
| L-809d re-run after rust-version 1.91 and opendal 0.59.5 (Rust 1.91.1) | fs 7/7, MinIO 24/24, RustFS 24/24 (both with SSE-C), OpenSSH 14/14, Dropbear 14/14, cross-store + OpenSearch 2 20/20; `cargo test --workspace --all-targets` 481 passed, 0 failed; clippy `-D warnings` clean for default, `--no-default-features`, `--no-default-features --features opendal` and the CI `--features elasticdump-rs/load` set (39 collapsible-if lints now firing at MSRV 1.91 fixed with let-chains, one `is_multiple_of`); cargo deny and cargo audit `--deny warnings` with no advisory ignores; machete, vet, native-code, build inventory, geiger, notices pass | `scripts/opendal-tests/run-all.sh` |
| L-810a rename to searchdump-rs (`searchdump`, `searchdump-multi`; `ELASTICDUMP_*` kept as deprecated aliases) | Same counts: fs 7/7, MinIO 24/24, RustFS 24/24, OpenSSH 14/14, Dropbear 14/14, cross-store 20/20; `cargo test --workspace --all-targets` 482 passed, 0 failed (new checks: old env names still work with a deprecation warning and the new name wins); clippy `-D warnings` clean on all feature sets; deny, audit, machete, vet, native-code, build inventory, geiger, notices pass; `scripts/it/multielasticdump-roundtrip.sh` passes (Node checks skipped, no Node install); public image (rust 1.99-trixie builder, debian trixie-slim) builds and `searchdump`, `searchdump-multi` and the `elasticdump` symlink print their versions | `scripts/opendal-tests/run-all.sh` |
| L-810c image default dhi-debian (amd64) and compatibility symlinks | `scripts/build-image.sh` default (`dhi.io/rust:1.99.1-debian13-dev` -> `dhi.io/debian-base:trixie-debian13-dev`, pinned digests from the local image cache, no login) and `--type dhi-static` built; `scripts/test-image.sh` all PASS on both, including `elasticdump`/`elasticdump-rs` -> searchdump and `multielasticdump` -> searchdump-multi, each run by bare name with `--entrypoint NAME --version`; `--type public` built; host check: all five names print the right `--version`, `--help` usage shows the invoked name only. Not run locally: dhi-alpine and public alpine builds and the image tests of the three non-default types (CI). grype `--fail-on high` on the default image: 19 distinct High CVEs (90 matches), one fixable (apt CVE-2026-109518), so `make scan` fails for dhi-debian until the base is refreshed or the CVEs are accepted | `scripts/test-image.sh` |
| L-810d release workflow, version 1.0.0-beta1, CHANGELOG, disclaimer | Static checks only (no tag pushed, no release created): actionlint and zizmor (`scripts/lint-workflows.sh`) clean; `--help` footers checked; `cargo test --workspace --all-targets` 482 passed, 0 failed; fmt, clippy `-D warnings` (load, default, no-default, no-default+opendal, it), deny, audit, machete, vet, notices, native, build inventory, check-pins, matrix-md, hygiene, geiger pass. The release run itself (archives, attestations, ghcr push, draft) is first exercised by the v1.0.0-beta1 tag | `.github/workflows/release.yml` |
| L-810e dhi-debian base refresh and accepted CVEs | New pin `dhi.io/debian-base:trixie-debian13-dev@sha256:94571f3d…` (Docker Hub DHI catalog, pushed 2026-11-08, apt 3.0.4+dhi0) could not be pulled here (no dhi.io login). Isolated podman store (`--root ~/.local/share/edrs-odt-podman`, graphroot verified): dhi-debian rebuilt on the cached previous base (tag override, not committed); `scripts/test-image.sh` all PASS except the base-digest check (expected: the stand-in uses a tag without digest). grype `--fail-on high` with `docker/grype-ignore.dhi-debian.yaml` and trivy with `docker/trivyignore.dhi-debian.yaml`: only CVE-2026-109518 (apt 3.0.3, fixed in the new base) left. check-pins, hygiene and workflow lints pass. Confirming scan on the new digest: CI (`security.yml`, base dhi-debian) | `make scan IMAGE_VARIANT=dhi-debian` |
| L-810f offline-build toolchain, publishers refresh, SFTP known issue | `scripts/sup/offline-build.sh` (isolated podman store, `docker.io/library/rust:1.99-trixie`, `--network none`): probe network unreachable, `toolchain: 1.99.1-x86_64-unknown-linux-gnu (image default)`, `cargo build --offline --locked --release` finished, `offline-build: OK` (it failed in CI run 42994732897 when rustup tried to download the 1.91.1 pin). `scripts/sup/repro.sh` gets the same guard (no-op in the default dhi.io builder, which has no rustup); the Dockerfile never copies `rust-toolchain.toml`. `supply-chain/publishers.json` regenerated with `publishers.py snapshot` from the CI `sc.json` (331 crates / 189 publishers); `publishers.py diff` reports no changes. shellcheck, check-pins, hygiene and workflow lints pass | `scripts/sup/offline-build.sh`; `scripts/sup/publishers.py diff supply-chain/publishers.json sc.json` |
| Backblaze B2 | not run: no usable mock (OpenDAL's B2 service authorizes against the fixed `https://api.backblazeb2.com`); real account pending, see TESTING-TODO | `scripts/opendal-tests/b2-real.sh` |
