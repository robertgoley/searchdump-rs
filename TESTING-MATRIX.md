# Testing matrix

This file records which engines and images the suites ran against, what is not or cannot be tested, and every
data modification the tool makes. Where Node elasticdump is buggy, incomplete or dead, the behaviour is documented
in [NODE-DIFFERENCES.md](NODE-DIFFERENCES.md). Security scanning is in [SECURITY-CVE.md](SECURITY-CVE.md).

Minimum supported versions are **Elasticsearch 7.x** and **OpenSearch 1.x**. ES 6.8 is out of scope.

## Coverage at a glance

One-screen view of the engines in this file against dump, restore, PIT, slices, object storage, and TLS.
Cells use only facts already recorded in this file or in [TESTING-COMPLETED.md](TESTING-COMPLETED.md).
`?` means those documents do not record a result for that engine. `it` is the 45-test integration suite
(data, files, metadata, TLS/auth on secure profiles). Golden contract cases are in the tables later in this file
(L-405/L-406 covered sliced scroll and PIT on all nine versions).

| Engine | dump | restore | PIT | slices | storage | TLS |
|---|---|---|---|---|---|---|
| Elasticsearch 9.1.2 | ✓ `data-scroll`, `it` | ✓ `data-restore`, `it` | ✓ `data-pit`, E4/E5 | ✓ L-405, L-406 | ? | ✓ secure profile (`it` TLS/auth) |
| Elasticsearch 8.19.23 | ✓ | ✓ | ✓ | ✓ L-405, L-406 | ? | ✓ secure profile |
| Elasticsearch 8.6.1 | ✓ | ✓ | ✓ | ✓ L-405, L-406 | ? | ✓ secure profile |
| Elasticsearch 7.17.29 | ✓ | ✓ | ✓ | ✓ L-405, L-406; also L-807/L-808 load | ? | ✓ secure profile |
| Elasticsearch 7.10.2 (default distro) | ✓ | ✓ | ✓; no sliced PIT (needs ≥ 7.15), falls back to sliced scroll (tested, es710-secure) | ✓ L-405; sliced PIT n/a | ? | ✓ es710-secure (official image) |
| Elasticsearch 7.10.2 OSS | ✓ | ✓ | n/a (no PIT); scroll fallback tested | ✓ L-405 | ? | n/a (no security plugin; the secure profile uses the default distro) |
| OpenSearch 3.9.0 | ✓ | ✓ | ✓ | ✓ L-405, L-406 | ? | ✓ secure profile |
| OpenSearch 2.19.7 | ✓ | ✓ | ✓ | ✓ L-405, L-406 | ✓ OpenDAL `cross.sh` (s3/sftp dump, copy, load; L-809a) | ✓ secure profile |
| OpenSearch 1.3.20 | ✓ | ✓ | n/a (no PIT before 2.4); scroll fallback tested (os13) | ✓ L-405 | ? | ✓ secure profile |

## 1. Tested images and results

- **Run:** commit `958e9d5` on branch `rewrite`, 2026-10-29 12:08–12:28 America/New_York.
- **Command:** `scripts/it/run.sh matrix`, then `all`, then `OS_TAG=3 … opensearch`.
- **Resources:** one engine container at a time, except the cluster → cluster leg. Settings: `IT_THREADS=2`, `CARGO_JOBS=2`.

Suites:

- **`it`**: `tests/it`, 45 tests. Covers data, files, metadata types, renaming, ILM→ISM, cross-engine, TLS/auth and concurrency.
- **legacy**: `tests/integration_test.rs`. Runs on plain profiles only. 3 tests are `#[ignore]`d benchmarks.
- **unit + contract**: `cargo test --lib --bins` (150) and `cargo test --test cli_contract` (22), both passing at `958e9d5`.

### Single-engine profiles

| Profile | Engine version | Image reference | Digest | Source | `it` pass / fail / skip* | legacy pass / fail / ignored |
|---|---|---|---|---|---|---|
| opensearch-plain (`OS_TAG=2`) | OpenSearch 2.19.7 | `docker.io/opensearchproject/opensearch:2` | `sha256:3bfa1ec74d56928c2e018033d45c460ce2c0ea65a5cedd577de3102326c31b89` | official | 45 / 0 / 3 | 31 / 0 / 3 |
| opensearch-secure (`OS_TAG=2`) | OpenSearch 2.19.7 | same | same | official | 45 / 0 / 5 | – |
| opensearch-plain (`OS_TAG=3`) | OpenSearch 3.9.0 | `docker.io/opensearchproject/opensearch:3` | `sha256:3b55f6b6249cfdb92b52ee59adc8a266203ba9ef9f5d82544fad6f1af2f3609e` | official | 45 / 0 / 4 | 31 / 0 / 3 |
| opensearch-secure (`OS_TAG=3`) | OpenSearch 3.9.0 | same | same | official | 45 / 0 / 5 | – |
| os13-plain | OpenSearch 1.3.20 (latest 1.3) | `docker.io/opensearchproject/opensearch:1.3.20` | `sha256:0ecf20d1ec1b682fa1cf36c74900b547bd1d7da5817a14edf4236472e110863e` | official | 45 / 0 / 3 | 31 / 0 / 3 |
| os13-secure | OpenSearch 1.3.20 | same | same | official | 45 / 0 / 5 | – |
| elasticsearch-plain | Elasticsearch 9.1.2 | `docker.io/bitnamilegacy/elasticsearch:9.1.2-debian-12-r0` | `sha256:00176a47afa0c0cfc9d11bdbafda13cbad7c9098ff1e6092112df2c751de301a` | bitnamilegacy | 45 / 0 / 2 | 31 / 0 / 3 |
| elasticsearch-secure | Elasticsearch 9.1.2 | same | same | bitnamilegacy | 45 / 0 / 4 | – |
| elasticsearch8-plain | Elasticsearch 8.6.1 | `docker.io/bitnamilegacy/elasticsearch:8.6.1-debian-11-r0` | `sha256:af815f5356650ca1663092084f7bd4cc88bff50f1e4542ac63e6c61eb1cbf3b3` | bitnamilegacy | 45 / 0 / 2 | 31 / 0 / 3 |
| elasticsearch8-secure | Elasticsearch 8.6.1 | same | same | bitnamilegacy | 45 / 0 / 4 | – |
| es819-plain | Elasticsearch 8.19.23 (latest 8.19) | `docker.elastic.co/elasticsearch/elasticsearch:8.19.23` | `sha256:4d0724bb8d78d7a2330693623e26ca4d621079d062ecc8a30b3e1fc180ec14c1` | official (bitnamilegacy stops at 8.18.0) | 45 / 0 / 3 | 31 / 0 / 3 |
| es819-secure | Elasticsearch 8.19.23 | same | same | official | 45 / 0 / 4 | – |
| es717-plain | Elasticsearch 7.17.29 (latest 7.17) | `docker.elastic.co/elasticsearch/elasticsearch:7.17.29` | `sha256:3f55a7f6f29b95345dc33808d6e914f81d125d4ca90a414e26c81a3521400980` | official (bitnamilegacy stops at 7.17.26) | 45 / 0 / 3 | 31 / 0 / 3 |
| es717-secure | Elasticsearch 7.17.29 | same | same | official | 45 / 0 / 4 | – |
| es710-plain | Elasticsearch 7.10.2 **OSS** build | `docker.io/bitnamilegacy/elasticsearch:7.10.2` | `sha256:73128f92f1d370b782a32928c569772bd7563d54b39b0a11d27269ae4494c593` | bitnamilegacy | 45 / 0 / 4 | 31 / 0 / 3 |
| es710-secure | Elasticsearch 7.10.2 default distribution | `docker.elastic.co/elasticsearch/elasticsearch:7.10.2` | `sha256:d528cec81720266974fdfe7a0f12fee928dc02e5a2c754b45b9a84c84695bfd9` | official (the bitnami 7.10.2 build is OSS and has no security) | 45 / 0 / 4 | – |

\* A skipped test is counted as passed by libtest. It prints `SKIP: <reason>` and is counted separately by
`run.sh`. The reasons in this run:

| Reason | Where | Why |
|---|---|---|
| `no peer cluster (cross profile only)` | ×2, every profile | The two `cluster_to_cluster_*` tests need two nodes. They ran for real in the cross leg below. |
| `IT_CROSS_DIR not set or secured profile` | ×1, secure profiles | File exports for the other engine are written by plain profiles only. |
| `no opensearch export yet` / `no elasticsearch export yet` | ×1, when the other engine has not exported yet in this sequence | The final `opensearch-plain cross::file_dump_from…` leg restores every ES export. |
| `Elasticsearch only` | ×1, OpenSearch | Managed (`_meta.managed`) templates and ILM built-ins exist only on Elasticsearch. |
| `ILM needs X-Pack; this is an Elasticsearch OSS build (refusal checked)` | ×1, es710-plain | Policy round-trip. The refusal message is asserted before the skip. |

No `node elasticdump not installed` skips occurred, because Node elasticdump is installed on the test host.

### Cross-engine legs

Same run, commit `958e9d5`.

| Leg | Result |
|---|---|
| File: OpenSearch 2.19.7 restores the ES exports from 7.10.2, 7.17.29, 8.6.1, 8.19.23 and 9.1.2 (`cross::file_dump_from_the_other_engine_restores_here`). Includes ILM → ISM with `--convertIlmToIsm`, refusal without it, and an unsafe-action refusal unless `--lenientPolicyConversion` is given. | 1 / 0 |
| File: ES 9.1.2 and 8.6.1 restore the OpenSearch 1.3.20 and 2.19.7 exports. ISM → ILM is refused. These ran inside the elasticsearch(8)-plain profiles. | passed (no skip) |
| Cluster → cluster: ES 9.1.2 ↔ OpenSearch 2.19.7, both directions (`cluster_to_cluster_full_copy_both_directions`, `cluster_to_cluster_policy_copy_converts_only_on_request`), 256 MB heaps | 2 / 0 (both ran; no SKIP) |

### Feature coverage included in every profile above

- **Renaming:**
  - `meta_types::rename_prefix_wildcard_restore_and_copy_across_types` covers `--renamePrefix` and `--renameIndex` on `logstash-*` with `--renameAliases` / `--renamePatterns`, wildcard file restore, cluster copy with `--suffix`, and the invalid-name error.
  - Unit tests: 4 in `rename::tests`. The contract test `rename_rules_apply_to_data_and_conflict_with_an_output_index` covers the explicit `--output-index` error.
- **ILM → ISM:**
  - `meta_types::policy_round_trip_same_engine` and the cross legs above.
  - Unit tests: 3 in `meta::policy::ilm_to_ism::tests` (hot/warm/delete with rollover, without rollover with a pending state, refusals and lenient drops).
- **Full backup order:** `meta_types::full_backup_and_restore_in_documented_order`. The policy step is skipped on the OSS build.

### Container image

Built and tested with podman 5 (buildah 1.39.3) on 2026-10-29. The dhi.io pulls used podman's stored login.

| Build | Builder image (digest) | Runtime image (digest) | Size | Result |
|---|---|---|---|---|
| `make image` / `scripts/build-image.sh` (**default since L-808b**, `:<ver>` + `:latest`, built 2026-11-01) | `dhi.io/rust:1.99.1-debian13-dev` (`sha256:be6c6ec91c61ae15b80521052df1c7f60fefd303293563b3dbd9f57e58783809`) | `dhi.io/static:20250419-glibc-debian13` (index `sha256:dcf8da7f5cdeb490d825814561995ce6c6cf931aa4322dae89f4f0170b0fb693`; amd64 `sha256:ec407681d58e…`, arm64 `sha256:89095da2afbb…`) | 24.5 MB | glibc + mimalloc (`MIMALLOC_VERBOSE=1` shows `process init`). `scripts/test-image.sh … dhi-static`: 12/12 (no shell: symlink, CA bundle and executables checked from `podman export`; uid 65532 from the image config; `--read-only` dump OK; 5 executables in `docker/image-binaries.dhi-static.txt`). `--version` prints `elasticdump-rs 0.1.0`; against OpenSearch 2.19.7 (`--network host`): 1000 docs loaded, dumped (1000 lines), restored through the `elasticdump` entrypoint to a second index, 1000/1000 by `_count`, sources identical. arm64: `--platform linux/arm64 --target base` resolves and pulls the arm64 base (26.6 MB); a full arm64 build needs qemu binfmt (not on this box). grype 5 High / trivy 0 HIGH (SECURITY-CVE.md). |
| `make image-dhi-alpine` / `scripts/build-image.sh --variant alpine` (default until L-808b, now `:<ver>-alpine`) | `dhi.io/rust:1.99.0-alpine3.23-dev` (`sha256:8fd18196a3c578cb13f86d85f27c2bee8358b847dae57301bd7a01dce970b353`) | `dhi.io/alpine-base:3.24-alpine3.24-dev` (`sha256:a6349b553099dd6a1a4ca17aea9f8b3a8476dccea1234ad20e45d545df3bd025`) | 20.3 MB (20.7 MB at L-808b) | L-808b: rebuilt through `scripts/build-image.sh --variant alpine`, `scripts/test-image.sh … dhi-alpine` 13/13, dump against OpenSearch 2.19.7 1000/1000 lines, musl allocator (mimalloc not loaded). Earlier: pulls ok. `make test-image` ok: `--version` prints `elasticdump-rs 0.1.0`, `--help`, bash 5.3.9, uid/gid 65532 (`nonroot`), `elasticdump -> elasticdump-rs` symlink, CA bundle at `/etc/ssl/certs/ca-certificates.crt` with `SSL_CERT_FILE` set, musl 1.2.6-r3. Dump of index, mapping and data, then restore through the `elasticdump` entrypoint, against OpenSearch 2.19.7: **500/500 docs, `_source` identical, mapping identical**. grype 0, trivy 0 (all severities). |
| `make image-dhi-debian` (the previous default) | `dhi.io/rust:1.99.1-debian13-dev` (`sha256:e2dbe6c0d027b42522964dd97047f051afe5be75ce185137eb86f52ab15bdb40`) | `dhi.io/debian-base:trixie-debian13-dev` (`sha256:09943fbda2242d48da7d9ebe54a2112f43c37231bfc3e742daafee1846dee8f9`) | 90.3 MB | Pulls ok. The same checks pass (bash 5.2.37, uid 65532, glibc 2.41), as does the same dump and restore: 500/500 docs and the mapping identical. Fails `make scan`: 15 High CVEs (grype) / 9 HIGH (trivy), none fixable. |
| `make image-public` | `docker.io/library/rust:1.99-trixie` (`sha256:09027f873d45…`) | `docker.io/library/debian:trixie-slim` (`sha256:f6b08b09a057…`) | 89.8 MB | `make test-image` ok (bash 5.2.37). Dump and restore against OpenSearch 2.19.7: 500/500 docs by count; the restored mapping was checked by eye. Fails `make scan`. |
| `make image-alpine` | `docker.io/library/rust:1.99.1-alpine3.24` (`sha256:b1d5dd5a22a0…`) | `docker.io/library/alpine:3.24.3` (`sha256:be2656061b53…`) | 20 MB | `make test-image` ok (bash 5.3.9). trivy 0; grype 3 Medium (busybox CVE-2025-60876, not-affected). |

The integration matrix above tests the host build of the binary (glibc, `target/it`; mimalloc since L-808b, the same
allocator and libc as the default image). The Alpine image's musl build of the same source was verified only by the
checks in this table.

### All pinned images and latest results (generated)

Regenerated by `scripts/it/matrix-md.py --summary "$IT_DIR/it-summary.log"` from `scripts/it/images.lock` and the
`IT-SUMMARY` lines `scripts/it/run.sh` prints and appends to `$IT_DIR/it-summary.log` (without `--summary` the
existing result cells are kept). The hand-written tables above keep the per-run narrative; this block is the
machine-maintained list, and CI runs `matrix-md.py --check`.

<!-- BEGIN generated:digests -->
| Image | Digest | Latest IT-SUMMARY |
|---|---|---|
| `docker.elastic.co/elasticsearch/elasticsearch:7.10.2` | `sha256:d528cec81720266974fdfe7a0f12fee928dc02e5a2c754b45b9a84c84695bfd9` | – |
| `docker.elastic.co/elasticsearch/elasticsearch:7.17.29` | `sha256:3f55a7f6f29b95345dc33808d6e914f81d125d4ca90a414e26c81a3521400980` | – |
| `docker.elastic.co/elasticsearch/elasticsearch:8.19.23` | `sha256:4d0724bb8d78d7a2330693623e26ca4d621079d062ecc8a30b3e1fc180ec14c1` | – |
| `docker.io/bitnamilegacy/elasticsearch:7.10.2` | `sha256:73128f92f1d370b782a32928c569772bd7563d54b39b0a11d27269ae4494c593` | – |
| `docker.io/bitnamilegacy/elasticsearch:8.6.1-debian-11-r0` | `sha256:af815f5356650ca1663092084f7bd4cc88bff50f1e4542ac63e6c61eb1cbf3b3` | – |
| `docker.io/bitnamilegacy/elasticsearch:9.1.2-debian-12-r0` | `sha256:00176a47afa0c0cfc9d11bdbafda13cbad7c9098ff1e6092112df2c751de301a` | – |
| `docker.io/library/alpine:3.24.3` | `sha256:be2656061b536c1883c4034790e21d0a09c5aaaf51968b89368a3c0371ea2c9a` | `make image-alpine` runtime base |
| `docker.io/opensearchproject/opensearch:1.3.20` | `sha256:0ecf20d1ec1b682fa1cf36c74900b547bd1d7da5817a14edf4236472e110863e` | – |
| `docker.io/opensearchproject/opensearch:2` | `sha256:3bfa1ec74d56928c2e018033d45c460ce2c0ea65a5cedd577de3102326c31b89` | – |
| `docker.io/opensearchproject/opensearch:3` | `sha256:3b55f6b6249cfdb92b52ee59adc8a266203ba9ef9f5d82544fad6f1af2f3609e` | – |
| `ghcr.io/shopify/toxiproxy:2.12.0` | `sha256:9378ed52a28bc50edc1350f936f518f31fa95f0d15917d6eb40b8e376d1a214e` | out-of-process fault proxy (`scripts/it/toxiproxy.sh`, INF-004) |
<!-- END generated:digests -->

### Golden recordings (generated)

<!-- BEGIN generated:goldens -->
| Engine-version | Cases | Exchanges |
|---|---|---|
| elasticsearch-7.10.2 | 25 | 82 |
| elasticsearch-7.17.29 | 25 | 82 |
| elasticsearch-8.19.23 | 25 | 82 |
| elasticsearch-8.6.1 | 25 | 82 |
| elasticsearch-9.1.2 | 25 | 82 |
| elasticsearch-oss-7.10.2 | 25 | 81 |
| opensearch-1.3.20 | 25 | 82 |
| opensearch-2.19.7 | 25 | 82 |
| opensearch-3.9.0 | 25 | 82 |
<!-- END generated:goldens -->

### Per-version contract (API-002, generated)

From the committed goldens (`tests/golden/<engine>-<version>/`), which `tests/golden_replay.rs`
replays offline on every PR and `scripts/golden/record.sh <profile> --check` re-records live
(nightly drift job). A cell is green only when the recording replays exactly; an expected
failure (for example `--type policy` on Elasticsearch OSS, which has no ILM/ISM) is recorded
with its exit code and asserted, not skipped.

<!-- BEGIN generated:contract -->
Endpoint x engine-version:

| Endpoint | elasticsearch-7.10.2 | elasticsearch-7.17.29 | elasticsearch-8.19.23 | elasticsearch-8.6.1 | elasticsearch-9.1.2 | elasticsearch-oss-7.10.2 | opensearch-1.3.20 | opensearch-2.19.7 | opensearch-3.9.0 |
|---|---|---|---|---|---|---|---|---|---|
| E1 cluster info | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ |
| E2 scroll search | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ |
| E3 scroll continuation and clear | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ |
| E4 PIT open/close | ✓ | ✓ | ✓ | ✓ | ✓ | n/a | n/a | ✓ | ✓ |
| E5 PIT search with search_after | ✓ | ✓ | ✓ | ✓ | ✓ | n/a | n/a | ✓ | ✓ |
| E6 search without scroll | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ |
| E7 render search template | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ |
| E8 bulk write | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ |
| E9 refresh | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ |
| E10 mapping get/put | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ |
| E11 settings get / index create | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ |
| E13 close/open | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ |
| E15 aliases | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ |
| E16 legacy templates | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ |
| E17 component and composable index templates | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ |
| E18 ILM policies | ✓ | ✓ | ✓ | ✓ | ✓ | n/a | n/a | n/a | n/a |
| E19 ISM policies | n/a | n/a | n/a | n/a | n/a | n/a | ✓ | ✓ | ✓ |
| E20 stored scripts | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ |
| E21 cluster health wait | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ |
| E22 1.2 --serverLimits probe | n/a | n/a | n/a | n/a | n/a | n/a | n/a | n/a | n/a |
| E12 index get/create/delete | ✓/400 | ✓/400 | ✓/400 | ✓/400 | ✓/400 | ✓/400 | ✓/400 | ✓/400 | ✓/400 |
| E14 existence checks | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ |

Case x engine-version:

| Case | elasticsearch-7.10.2 | elasticsearch-7.17.29 | elasticsearch-8.19.23 | elasticsearch-8.6.1 | elasticsearch-9.1.2 | elasticsearch-oss-7.10.2 | opensearch-1.3.20 | opensearch-2.19.7 | opensearch-3.9.0 |
|---|---|---|---|---|---|---|---|---|---|
| alias | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ |
| alias-restore | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ |
| analyzer-restore | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ |
| bulk-create | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ |
| bulk-delete | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ |
| bulk-gzip | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ |
| bulk-split | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ |
| bulk-update | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ |
| bulk-version | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ |
| component-template | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ |
| data-pit | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ scroll | ✓ scroll | ✓ | ✓ |
| data-restore | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ |
| data-scroll | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ |
| data-search-after | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ |
| data-template | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ |
| index | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ |
| index-template | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ |
| index-template-restore | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ |
| mapping | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ |
| mapping-restore | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ |
| policy | ✓ | ✓ | ✓ | ✓ | ✓ | fails (exit 1) | ✓ | ✓ | ✓ |
| script | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ |
| settings | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ |
| settings-restore | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ |
| template | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ |
<!-- END generated:contract -->

### Cross-version expectation table (API-071, generated)

<!-- BEGIN generated:expect -->
_Expectation table not committed yet (API-071)._
<!-- END generated:expect -->

### Pinned images (INF-012)

Every engine and helper image is pulled by digest from `scripts/it/images.lock`; `scripts/it/up.sh` appends the
digest, and refuses an unpinned image when `CI` or `IT_REQUIRE_PIN=1` is set (`IT_UNPINNED=1` allows a bare tag
locally). `scripts/it/check-pins.sh` (CI) fails when a profile image is missing from the lock, when a lock digest is
not recorded in this file, or when a Dockerfile base or compose image lacks `@sha256:`. Engine digests are in the
tables above; the helper images are:

| Image | Digest | Used by |
|---|---|---|
| `ghcr.io/shopify/toxiproxy:2.12.0` | `sha256:9378ed52a28bc50edc1350f936f518f31fa95f0d15917d6eb40b8e376d1a214e` | out-of-process fault proxy (`scripts/it/toxiproxy.sh`, INF-004) |
| `docker.io/library/alpine:3.24.3` | `sha256:be2656061b536c1883c4034790e21d0a09c5aaaf51968b89368a3c0371ea2c9a` | `make image-alpine` runtime base |

### Injection PoCs: Node elasticdump vs searchdump (INJ-017)

- **Run:** `scripts/poc/node-oracle.sh` (L-513), 2026-10-30 12:25 America/New_York.
- **Setup:** one throwaway engine, opensearch-plain (`OS_TAG=2`, OpenSearch 2.19.7). Node elasticdump 6.125.1 on node v20.
- **Captures:** a logging proxy recorded Node's HTTP requests for the two metadata cases. Raw output is in `results/local/L-513.json`.

| PoC | searchdump | Node elasticdump 6.125.1 |
|---|---|---|
| INJ-F1 variant 1 (pretty-printed `_source` carrying a bulk action line), export → restore, checks A–D | NO-INJECTION; every check OK | NO-INJECTION; every check OK. Node re-serialises `_source` with `JSON.stringify`, so the export is one line. |
| INJ-F1 variant 2 | NO-INJECTION | NO-INJECTION |
| INJ-F2 analyzer file `{"*":{"settings":{"index":{"analysis":…}}}}`, output = cluster root (rs with `--closeIndex`) | Refused before any request (`invalid index name '*'`); exit 1; no index closed | Sends `PUT /` (405), then stops **silently with exit 0** and "Total Writes: 0". No index closed. |
| Same file, output = `…/keep1` | – (the index name is validated the same way) | `PUT /keep1` (400 already exists), then the same silent stop with exit 0. Node never sends `_close`/`_open` for an existing index. |
| INJ-F2 alias file `{"*":{"aliases":{"logs-read":{}}}}` | Refused before any request; exit 1; no alias created | **No request sent at all**, exit 0, "Total Writes: 0". From the source (`_alias.js`), a parsed file would send `POST /_aliases` with `index: "*"`, which adds the alias to every index. File-based alias restore is a silent no-op in 6.125.1, so this is not reachable. |
| Data documents with `_index: "_all"` / `_index: "*"` | Refused per line by the client (`invalid _index`); nothing sent; exit 1 | Sent to `_bulk`. The engine refuses both (`invalid_index_name_exception`); Node prints the item errors and exits 0. |

Result: Node 6.125.1 is not exploitable by these PoCs on OpenSearch 2.19.7. The engine or Node's own silent failures stop
them. However, Node reports success (exit 0) in every refused case. searchdump refuses on the client before any request and
exits non-zero. Its final error line now names both rejection reasons. Before L-513 it said only "not exactly one JSON object",
even when the line was rejected for an invalid `_index`.

## 2. Untested or untestable differences per version

These are behaviours the suite cannot exercise, or that differ between versions and are handled by gating or a fallback. Each row gives the reason.

| Version / image | What differs or is not covered | How the tool handles it | Test status |
|---|---|---|---|
| ES 7.10.2 OSS (bitnamilegacy, plain profile) | **No point-in-time.** PIT is X-Pack, and `build_flavor: "oss"`. | Warns "… (OSS build) does not support point-in-time search … using scroll instead" and switches to scroll. | Tested: the PIT test expects the warning, a `scroll=` request and no `/_pit?`. |
| ES 7.10.2 OSS | **No ILM** (X-Pack). | `--type=policy` fails with "ILM is an X-Pack feature and this is an OSS build". | Refusal tested. Policy round-trip and the policy step of the full backup are **SKIP** ("ILM needs X-Pack; this is an Elasticsearch OSS build (refusal checked)"). |
| ES 7.10.2 OSS | **No security plugin**, so a secure profile is impossible on this image. | – | The secure profile uses the official `elasticsearch:7.10.2` (default distribution, basic licence) instead. |
| ES 7.10.2 default distribution | **No sliced PIT** (needs ES ≥ 7.15). | Warns and uses sliced scroll. | Tested (es710-secure). |
| ES 7.10.2 | **No `_shard_doc` sort** (needs ≥ 7.12). | PIT sorts by `_doc` + `_id`. | Tested. |
| ES 7.10.2 | **ILM rollover `max_primary_shard_size`** (7.13+) and **policy `_meta`** (7.14+) do not exist. | Passed through unchanged; the target cluster rejects them. | The tests use only fields the version supports, so the rejection path itself is not tested. |
| ES 7.x and OpenSearch 1.x | **Hits carry `_type: "_doc"`.** | Dump files keep `_type` as Node does. Restore to a typeless engine drops `_type` from the bulk **action line** only. | Tested: the typeless test accepts `_type:"_doc"` on ES 7 / OS 1 and compares the restored documents. |
| ES 7.x | **Dynamic mapping rejects out-of-range integers.** | The document fails; it is reported in "Failed Writes" and the exit code stays 0 (Node parity). | Tested: the test pre-creates the index shape, then compares documents. |
| ES 7.0–7.7 | **No component or index templates** (added in 7.8). | Read and write fail with a version error before any request is sent. | Not run against a live cluster; no 7.0–7.7 image is in the matrix. |
| ES 6.x and older, OpenSearch < 1.0 | **Out of scope** (minimum is ES 7.x / OS 1.x). | Typed (6.x) mapping files are still unwrapped on restore, and `_all` is dropped. | Unit tests only. |
| OpenSearch 1.3 / < 2.12 | **Demo admin password is `admin`**, not a custom one. | Handled by the test harness (`OS_LEGACY_ADMIN`). | Tested (os13-secure). |
| OpenSearch < 2.4 | **No PIT.** | Scroll fallback, the same as for OSS builds. | Tested (os13). |
| Cluster → cluster across engines | Needs two nodes at once. Only **ES 9.1.2 ↔ OpenSearch `OS_TAG` (2.19.7)** runs, because the memory rules allow one pair. | – | 7.10, 7.17, 8.6, 8.19, OS 1.3 and OS 3 are covered **file-wise** (export → restore across engines); they are not covered cluster to cluster. |
| ILM features that need an Enterprise licence or tiers: `searchable_snapshot`, `frozen`, `downsample` (8.5+), `wait_for_snapshot` | Cannot be created on a basic-licence cluster. | Conversion refuses them, or drops them with `--lenientPolicyConversion`; `wait_for_snapshot` is refused even then (L-651a), since dropping it would delete without waiting for the snapshot. | Unit tests, plus an IT refusal on a policy file. Not tested against a real Enterprise cluster. |
| Data streams | Backing indices restore as plain indices (`data_stream` dropped, with a warning). | – | Unit test only. Data stream round-trips (`_data_stream`) are not a supported `--type`. |
| Container builds | The integration suites run the host build of the binary. The musl binary in the default image does not go through the 16 profiles. | – | Image checks: `make test-image`, a dump/restore against OpenSearch 2.19.7, and grype/trivy scans (section 1, Container image). |
| Node elasticdump comparisons | Some tests run the Node tool when it is installed. | – | Ran here, because Node is installed. Elsewhere they SKIP ("node elasticdump not installed"). The tests that use Node's committed fixtures (`tests/fixtures/node/`) always run. |

## 3. Data modifications by direction

**Documents (`--type=data`) are never changed.**

- The `_source` bytes of every hit are copied verbatim: the bulk encoder slices the raw JSON and never re-serialises it.
- Only the bulk **action line** is built, from:
  - `_id`, `_routing` and (with `--handleVersion`) the version;
  - the target index name (`--output-index`, `--prefix`/`--suffix`, rename rules);
  - `_type`, only for ES < 7 targets.
- `--transform` is refused rather than silently ignored.

How this is verified:

- `index_action_keeps_raw_source_and_routing` (unit test) checks that a 30-digit integer survives byte for byte.
- On every profile, the integration tests read source and target independently, through a scroll that does not use the tool, and assert `docs(src) == docs(dst)`: the map of `_id → (_routing, _source)`. This runs:
  - in the engine tests (same engine, all profiles);
  - in the `cross::file_dump_from_*` legs (ES ↔ OpenSearch through files);
  - in `cross::cluster_to_cluster_*` (ES 9.1 ↔ OS 2.19).

Metadata types are adjusted as follows. Every change listed here is logged at WARN or INFO, unless it is marked *silent* (meaning it is generated state that the target always rejects).

| Direction | Type(s) | What changes | When | How the user is told |
|---|---|---|---|---|
| any → any | `settings`, `index`, `analyzer`, template settings | Cluster-generated keys removed: `index.version`, `creation_date`, `uuid`, `provided_name`, `resize`, `routing.allocation.initial_recovery`, `history.uuid`, `verified_before_close`, `store.snapshot` | always | silent (Node strips the same family; the target would reject them) |
| any → any | `settings`, `index`, templates | Keys named in `--excludeSettings` / `--excludeTemplateFields` removed | only when the flag is given | the flag itself |
| any → any | templates | `created_date`, `created_date_millis`, `modified_date`, `modified_date_millis` removed | always | silent (read-only, generated) |
| any → any | `template`, `component_template`, `index_template`, `policy` | `_meta.managed: true` built-ins skipped | `--filterManaged` (default on); `=false` keeps them | WARN listing the skipped names |
| any → any | templates | Names matching `--templateRegex` skipped | cluster dumps; `--filterSystemTemplates=false` disables | WARN listing the skipped names |
| any → any | `script` | An empty `options: {}` is removed | always | silent (an empty object is equivalent) |
| any → any | `mapping`, `index` | Typed 6.x mappings unwrapped; `_all` / `include_in_all` dropped | file input from 6.x dumps | INFO "Unwrapping typed mapping", WARN "Dropping '_all'" |
| any → any | `index` | `data_stream` dropped; the backing index is restored as a plain index | the source index was a data stream backing index | WARN "restoring it as a plain index (data_stream dropped)" |
| any → any | `index`, `settings`, `mapping`, `analyzer`, `alias`, `data` | Index names rewritten | `--renamePrefix` / `--renameIndex` (then `--prefix`/`--suffix`) | INFO "Renaming index 'a' -> 'b' (rule)", once per index |
| any → any | `alias` | Alias names rewritten | `--renameAliases` | INFO "Renaming alias …" |
| any → any | templates, ISM `ism_template` | `index_patterns` rewritten | `--renamePatterns` | INFO |
| ES 7 → typeless | `data` | `_type` omitted from the bulk **action line** (never from `_source`) | target ES ≥ 7 / OpenSearch | silent (the target rejects or ignores typed actions) |
| ES → OpenSearch, cluster to cluster | `settings`, `index`, template settings | `index.lifecycle.*`, `index.routing.allocation.include._tier_preference` dropped | source engine known (cluster input) | WARN "Dropping Elasticsearch settings not supported by OpenSearch: …" |
| ES → OpenSearch, from a file | same keys | **kept**, because the source engine is unknown | file input | WARN with the exact `--excludeSettings=…` to add |
| ES → OpenSearch, cluster to cluster | `component_template`, `index_template` | `allow_auto_create`, `ignore_missing_component_templates`, `deprecated`, `data_stream_options`, `template.lifecycle`, `template.data_stream_options`, `data_stream.{hidden,allow_custom_routing,failure_store}` dropped | source engine known | WARN "dropping Elasticsearch-only fields: …" |
| ES → OpenSearch, from a file | same fields | **kept** | file input | WARN with the exact `--excludeTemplateFields=…` |
| ES → OpenSearch | `policy` | **Refused** | without `--convertIlmToIsm` | error naming the flag |
| ES → OpenSearch | `policy` | ILM → ISM conversion (mapping table in README "ILM → ISM conversion"). Approximations (`min_age` → `min_index_age`, rollover conditions, allocate) are logged; `migrate`, `delete_searchable_snapshot`, `_meta` keys other than `description` are dropped | `--convertIlmToIsm` | WARN per approximated or dropped feature; INFO that converted policies are not attached to indices |
| ES → OpenSearch | `policy` | Unsafe parts (`searchable_snapshot`, `frozen`, `downsample`, `wait_for_snapshot`, …) | `--convertIlmToIsm` | **error** naming the phase and action; with `--lenientPolicyConversion` they are dropped with a `DROPPED …` WARN, except `wait_for_snapshot`, which stays an error naming policy and phase (L-651a). A cluster rejection mid-set rolls back what the run wrote (L-651a) |
| OpenSearch → ES | `settings`, `index`, template settings | `index.replication.type`, `index.plugins.*`, `index.opendistro.*` dropped (from a file: kept and warned) | cluster to cluster | WARN, as above |
| OpenSearch → ES | templates | `data_stream.timestamp_field`, `context` dropped (from a file: kept and warned) | cluster to cluster | WARN |
| OpenSearch → ES | `policy` | **Always refused** (no ISM → ILM conversion) | always | error |
| OpenSearch → OpenSearch | `policy` | `_seq_no` / `_primary_term` used for optimistic-concurrency overwrite; the policy body is unchanged | existing policy | – |
| any | `analyzer` on an existing index | Index **closed**, analysis updated, then always reopened | `--closeIndex` | WARN that the index is offline meanwhile |
