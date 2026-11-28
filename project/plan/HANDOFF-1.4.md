# Handoff: searchdump-rs 1.4 (branch `release/1.4`)

Self-contained pickup document. Owner: Robert Goley. Repo: https://github.com/robertgoley/searchdump-rs.
Last update: see the newest row of section 3 (times are America/New_York).

## 1. Where things are

- `release/1.4` was cut from `main` at ac8d8c4 (v1.3.1) on 2026-11-25 and pushed to origin as its own branch
  (like `release/1.3`). Worktree on grokbox: `/workspace/rel-1.4` (the repo is `/workspace/push-fork`).
- **Released as 1.4.0, then rebuilt on the newest base images (Robert t269u / t275u, option A 2026-11-28):**
  first ship was c3d3242 (tag object 39c3d00, release id 460471424, run 45980239972). On 2026-11-28 the
  existing release was deleted (no `--cleanup-tag`), the annotated tag `v1.4.0` was moved to 99241a1
  (tag object 328a45e, message `searchdump-rs 1.4.0`), and a full release-build ran. New release
  https://github.com/robertgoley/searchdump-rs/releases/tag/v1.4.0 (id 461378710) is published and Latest.
  `main` = `release/1.4` = 99241a1 plus any later docs commit. Verification: section 7. No PR.
- Headline (Robert, t248u/t250u): **limits and multi polish**. Approved items (t243u): multi TLS flags,
  per-child uuid logging, `--pageBytes`, remove the duplicate reqwest. The key passphrase reaches children
  through the environment, never argv (t253u).
- The `searchdump-multi.rs` split (Robert t264u) is done on this branch: steps 0-17 as L4-008..L4-028, no
  behaviour change, help text byte-identical. Details, decisions and tooling: **`project/plan/HANDOFF-SPLIT-1.4.md`**.
  Module map (`src/bin/searchdump-multi/`, lines incl. tests): main 84, cli 339, tls 308, cluster 482,
  selection 288, types 235, files 398, manifest 94, discover 305, sizing 443, planner 172, flags 183,
  child/{mod 9, probe 84, runner 541, stats 157}, ctx 136, dump 417, load 264, storage 178,
  run_local/{mod 63, setup 301, dump_plan 300, load_plan 107, dry_run 104, report 115}.
- Reference: `/workspace/review-1.4/REVIEW-1.4.md` (1.4 review; items C6, C9, C11, C21).

## 2. Rules (standing)

- Run `scripts/sup/secret-scan.sh` before every push (check its exit status, not a piped tail).
- Do not touch `main`, do not tag, no PRs, no force push, never delete branches, tags or files.
- `git add` named files only (never `-A`). Never print secrets. Commit prefix `L4-NNN:`.
- Containers: only the isolated podman store (`source /workspace/perf-1.2/env.sh`); never other agents'
  VMs/containers, the uid 100999 OpenSearch, or port 19222. No AWS / B2.
- Toolchain: `source /workspace/perf-1.2/env.sh` puts `~/.cargo/bin` (rustup, 1.91.1) first; `/usr/bin/cargo`
  is 1.85 and cannot build this crate.
- Behaviour questions not covered by Robert: take the conservative default, record it in section 5, keep going.

## 3. Progress log

Commit ids follow the item numbers of the brief: L4-001 multi TLS, L4-002 per-child uuid, L4-003 `--pageBytes`,
L4-004 one reqwest; L4-005+ are handoff/CI follow-ups. They land in whatever order is ready.

| when (ET) | commit | what |
|---|---|---|
| 2026-11-25 22:10 | L4-004 | reqwest 0.12 -> 0.13 (cluster side; `rustls-no-provider`), pkcs8 0.10 -> 0.11 (RustCrypto 0.11 stack), tools on sha2 0.11 / reqwest 0.13; timeouts capped (reqwest 0.13 panicked on `--idleTimeout 2^63`); deny skips and vet exemptions updated, notices regenerated. Local: nextest unit 387/387, contract 129/129, chaos_brk 63/63, clippy -D warnings, deny/audit/vet OK |
| 2026-11-25 23:00 | L4-004a | geiger baseline for the RustCrypto 0.11 stack (CI supply-chain failed on it: der/inout/polyval/scrypt per-crate rises; total 35383 -> 34702); pkcs8 scrypt + hmacWithSHA256 decrypt test |
| 2026-11-26 00:30 | L4-001 + L4-002 | multi TLS flags + passphrase env (children via env only); searchdump `SEARCHDUMP_*TLS_PASS`; per-child ids (parent lines, child log lines, stats `child_id` + `target_index`, manifest `children`, `children.json`). Local: nextest unit 393/393, contract 131/131, clippy -D warnings, deny/vet/audit/machete/notices/pins/native OK; IT on private-CA OpenSearch (demo CA, kirk client cert auth, encrypted PKCS#8 key) 91 passed / 1 ignored incl. new `tls_auth::multi_private_ca_client_cert_and_env_passphrase` |
| 2026-11-25 22:05 | (CI) | ci 45744818606 + security 45744821344 on c1194b3: both success (L4-004a geiger fix confirmed) |
| 2026-11-25 22:30 | L4-003 | `--pageBytes` (searchdump + multi): start-up sizing from avg stored doc (auto/min: min with budget, below floor allowed; off/fallback: one `_stats` call), run-time shrink (search_after/PIT next pages; scroll first page dropped + context cleared + restarted smaller), multi `sizing.reason=page-bytes` + `--pageBytes` to searchdump data children, load/copy refuse. Tests: limits unit, multi Plan unit, chaos_brk 4 mock tests, IT `data::l4_003_page_bytes_caps_dump_pages` (scroll auto/off, search_after, multi) on private-CA OpenSearch. Local: fmt, clippy -D warnings, no-default-features build, nextest unit 393/393, contract 131/131, chaos_brk 67/67, router_mock 17/17, IT 92 passed / 1 ignored, matrix-md, native-code, machete OK |
| 2026-11-25 22:55 | (CI) | ci 45746923521 + security 45746925694 on 4625cec: both success (all it legs incl. opensearch-secure, supply-chain, coverage, image). All four 1.4 items done; test container `it-os-sec-l4tls` stopped (not removed) |
| 2026-11-25 22:50 | L4-006 | t255u-t259u: multi honours `NODE_TLS_REJECT_UNAUTHORIZED=0` (parent + children, searchdump's warnings), `--pageBytes` < 1 MiB doc note, D3/D4/D7/D8/D10/D12 resolved; edrs-1.2-podman deleted and recreated (t258u), 11 images re-pulled, smoke OK. Local: fmt, clippy, unit 393/393, contract 131/131, IT 93 passed / 1 ignored (new `tls_auth::multi_honours_node_tls_reject_unauthorized`) |
| 2026-11-25 23:10 | (CI) | ci 45750955415 + security 45750958177 on 7ecf996: both success (all it legs incl. opensearch-secure). No open questions left from the 1.4 items |
| 2026-11-26 00:55 | L4-008..L4-011 | searchdump-multi split started (t264u): step 0 safety net, step 1 git mv, step 2 http::Method, split tooling; details and status in `project/plan/HANDOFF-SPLIT-1.4.md` |
| 2026-11-27 04:45 | L4-012..L4-029 | searchdump-multi split done (steps 3-17); final verification green locally (tests, help goldens, supply chain, IT 93/1, smoke); ci 45972304648 + security 45972307714 success on d869392. See HANDOFF-SPLIT-1.4 |
| 2026-11-27 05:00 | c3d3242 | Version 1.4.0 (Cargo, Dockerfile ARG, README/examples tags, CHANGELOG `## 1.4.0`, RELEASE-NOTES-1.4.0); ci 45978472578 + security 45978476476 success |
| 2026-11-27 05:20 | (release) | `main` ff to c3d3242 (ci 45980219359 + security 45980219397 success); tag `v1.4.0`; release-build 45980239972 success (all jobs, 5 image jobs) |
| 2026-11-27 05:47 | (release) | draft published, marked Latest; deployment verification in section 7 |
| 2026-11-27 17:20–23:30 | 99241a1 | L4-032: refresh image base pins (dhi debian-base 120051dd…, dhi alpine-base ab3130d4…, debian:trixie-slim 8aa5a7bf…, rust:1.99-trixie 8a4e90c7…, rust:1.99.1-alpine3.24 44819b27…, alpine 3.24.3 -> 3.24.4 f59abe65…). base_refresh 46076115811 success. CI on 99241a1: ci 46077472956 success; security image dhi-debian 46077475228, dhi-static 46077482085, dhi-alpine 46077489404 success. Public alpine/debian security runs failed at crate-vendor (dhi.io login missing; pre-existing, not used by release.yml) |
| 2026-11-28 01:51–02:22 | (release) | Robert chose option A: delete release 460471424, move annotated tag v1.4.0 to 99241a1 (328a45e), release-build 46118591562 success, new release 461378710 published Latest at 02:22 ET, main ff 169a6bb -> 99241a1. Verification in section 7 (rebuild) |

## 4. Item status

| item | status | notes |
|---|---|---|
| 4. one reqwest (+ cheap duplicate removal) | done, CI green (L4-004/004a) | Left: digest/crypto-common 0.10 (crc-fast via opendal-service-s3), getrandom 0.2/0.3/0.4 (ring, ahash/tempfile, uuid), hashbrown 0.14 (rust-ini), syn 2 (openssh-sftp). None removable without upstream bumps |
| 1. multi TLS flags + passphrase env | done, CI green (L4-001) | IT against private-CA OpenSearch 2 with client-cert auth (`it-os-sec-l4tls`, port 19252, store edrs-1.4-podman) |
| 2. per-child uuid logging | done, CI green (L4-002) | run uuid per child + target index uuid in child stats (D9) |
| 3. `--pageBytes` | done, CI green (L4-003) | see D11-D13 |

## 5. Decisions taken (open for Robert to confirm)

- D1 (L4-004): pkcs8 0.11 taken in 1.4 (it is the Dependabot PR's bump); it removes 7 duplicate crates. It adds
  aes-gcm/ghash/polyval (pkcs5 0.8 supports AES-GCM PBES2), all RustCrypto, exempted in cargo-vet like the 0.10 ones.
- D2 (L4-004): timeouts above ~100 years are capped (they meant "never" anyway) instead of erroring.
- D3 (L4-004a, RESOLVED t259u: baseline accepted): geiger per-crate rises from the RustCrypto 0.11 stack accepted into the baseline (total went down).
- D4 (L4-001, RESOLVED t255u: names approved): passphrase env names `SEARCHDUMP_TLS_PASS` (both), `SEARCHDUMP_{INPUT,OUTPUT}_TLS_PASS`; order
  `--pass` > `--{dir}-pass` > shared env > per-direction env; empty = unset; `ELASTICDUMP_*` aliases accepted.
- D5 (L4-001): multi `--pass` accepted (warned as `ps`-visible); env preferred. Children always get it via env only.
- D6 (L4-001): multi refuses other-side TLS flags, TLS flags with `--direction copy`, cert without key, and a
  passphrase with a Node elasticdump child (it cannot read the env var).
- D7 (L4-006, RESOLVED t255u): the multi parent honours `NODE_TLS_REJECT_UNAUTHORIZED=0` for its own requests
  (searchdump's two warnings printed) and children get `--insecure` plus the inherited variable. Only `0` counts.
  The parent now also prints the `DISABLED for URL` warning for `--insecure` (it had no logger, so it was silent).
- D8 (L4-002, RESOLVED t255u: keep as is): `child ID start:` / `end:` lines go to stderr for every child.
  `children.json` in the log dir is the report.
- D9 (L4-002): "uuid" done both ways: a run UUID per child, plus the restore target's index uuid (noted/final) in
  child stats (HANDOFF-1.3 8.6 meaning).
- D10 (env, RESOLVED t258u): new isolated podman store `edrs-1.4-podman` (wrapper `/workspace/perf-1.4/bin/podman`) because
  `edrs-1.2-podman` had damaged layers (`podman system check`: damage detected). Robert (t258u) approved deleting
  and replacing it: on 2026-11-25 ~22:45 ET, with no container in it and no process using it, only its graphroot
  `/home/box/.local/share/edrs-1.2-podman` and runroot `/tmp/edrs-1.2-podman/run` were removed (`podman unshare
  rm -rf`, then the empty re-initialised dirs) and recreated at the same paths, so `/workspace/perf-1.2/env.sh` and
  its wrapper are unchanged. The 11 images it held (list in `/workspace/perf-1.2/notes/images-before-reset-20261125.txt`)
  were re-pulled by digest. Other stores (default, edrs-1.4, edrs-beta1-rerun, edrs-odt) untouched.
  `podman system check` on the fresh store only reports the rootless false positives (`/dev/*` device nodes that a
  rootless store cannot create; directory mtimes), the same as the healthy edrs-1.4 store; no missing layer
  content (the old damage). Smoke: OpenSearch 2.19.7 (digest 3bfa1ec7...) from the fresh store on 127.0.0.1:19252
  answered, a 1-document searchdump dump worked; container `smoke-l4-os` stopped (kept).
- D11 (L4-003): `--pageBytes` units as `--maxBulkBytes` (bytesize: `kb`=1000, `kib`=1024); minimum 1000 bytes, `0`
  refused (leave it out for no cap). It never raises `--limit`, and lowers an explicit `--limit` too.
- D12 (L4-003, RESOLVED t256u: below the floor stays allowed, min 1kb; docs note that < 1 MiB is rarely
  practical): with auto/min the cap may go below the 1.3 1 MiB page floor (user's explicit choice wins); with
  `off` one read-only `_stats` call is still made, only because the flag was given.
- D13 (L4-003): run-time: search_after/PIT keep an over-cap page and shrink the next ones; scroll drops an over-cap
  FIRST page unread, clears its context and restarts smaller (later scroll pages cannot be resized: only the
  start-up estimate bounds them). Shrinks log a warning (like the 429 halving). `--maxResponseMb` stays the hard
  limit. multi refuses it for load/copy (error, like `--fileSize` on load).

## 6. How to resume

1. `cd /workspace/rel-1.4 && source /workspace/perf-1.2/env.sh && git status && git log --oneline ac8d8c4..`
2. Read section 3/4 for the next step. Check nothing is still running (`pgrep -af 'cargo|searchdump'`) before
   starting a new build or test chain.

## 7. Release 1.4.0 verification (2026-11-27, ET)

- **Release:** https://github.com/robertgoley/searchdump-rs/releases/tag/v1.4.0, published 05:47 ET, `releases/latest`
  = v1.4.0. Tag `v1.4.0` -> c3d3242 = `main` = `release/1.4`. release-build run 45980239972.
- **Assets (11, same set as v1.3.1):** 4 tarballs (x86_64/aarch64 x gnu/musl) + `.sha256` each, CycloneDX SBOM
  `searchdump-rs-1.4.0.cdx.json` + `.sha256`, `SHA256SUMS`. All download anonymously and match; `sha256sum -c`
  OK; `gh attestation verify` (build provenance) OK for the 4 tarballs and the SBOM; SBOM is CycloneDX 1.3,
  280 components, version 1.4.0.
- **Binaries:** x86_64 gnu + musl: `searchdump`, `searchdump-multi` and the symlinks `elasticdump`, `elasticdump-rs`,
  `multielasticdump` print 1.4.0; `--help` OK; multi help == `tests/fixtures/multi/help.txt`. aarch64: `file` only
  (ELF aarch64, gnu dynamic / musl static); not executed (no qemu on the box).
- **Images (ghcr.io/robertgoley/searchdump-rs, same tag set as 1.3.1; no cosign in 1.3.1 or 1.4.0):**
  - `latest` = `1.4.0` = `1.4.0-dhi-debian`: `sha256:630d89a3af203873b01ae4878bb86bd37ffad9c72dd2480a95e8ba28f1b5c6a8`
  - `1.4.0-dhi-alpine`: `sha256:3e2ddf43f2edd5739d6526bebcd27a1dcfa280d810909e3b996c0ab33a21d0fd`
  - `1.4.0-dhi-static`: `sha256:2a757f512d0661b21c3f4d8aeb4a9588cebb4f90fb7df06f6a3005c6a38abbbc`
  - `1.4.0-public`: `sha256:005391a41d40b007efb4533822d6bb4a0e5613cb086cb405f89ad73d2b1d69af`
  - `1.4.0-alpine`: `sha256:ab761016952391ae52d5c833259d80a3a7e8f81e7df2e3e9566fc14168e835ef`
  Pulled digests == release-build log; each prints `searchdump 1.4.0` / `searchdump-multi 1.4.0`, `--help` OK;
  `gh attestation verify oci://...@digest` OK for all 5.
- **Smoke (it-os-sec-l4tls, port 19252, private CA):** released x86_64 gnu binary dumped 500 docs and loaded them
  (500/500); `searchdump-multi` dump OK (500 records). Index creation was then refused by the node's own
  `cluster.blocks.create_index` (disk watermark: /workspace at 91%), so the mapping restore and the multi load
  failed on index creation, not on the tool. Without new indices: binary and image (`:1.4.0`) dumps of
  `it_tls_multi_node_471526_0` identical (54 docs), image and binary idempotent loads back OK (count 54), image
  `searchdump-multi` dump identical to the binary dump. The block is still set on that test node.
- **Scan (grype 0.121.0 / trivy, Makefile rules):** dhi-debian, dhi-alpine, dhi-static pass both. `public` fails
  (grype 24 ids, trivy 19 ids) and `alpine` fails grype (5 OpenSSL 3.5.9 ids). **No id outside SECURITY-CVE.md:**
  every public id is in the dhi-debian accepted lists; the alpine ids are exactly the 5 accepted for dhi-alpine
  (L-831). Open for Robert: SECURITY-CVE.md says public Alpine has 0 High and `alpine` uses the shared
  `docker/grype-ignore.yaml`, so either accept the 5 OpenSSL ids for `alpine` (per-variant ignore file) or note
  it as open; public Debian stays the documented not-recommended base.
- **Alpine CVEs accepted (Robert t273u, 2026-11-27 14:04 ET, L4-031):** the 5 OpenSSL 3.5.9 ids (CVE-2026-107493,
  -107494, -121482, -124709, -84786) on libssl3/libcrypto3 for the public `alpine` image, as for dhi-alpine. The image
  ships 3.5.9-r1 (Alpine main), not dhi's r3, so the new `docker/grype-ignore.alpine.yaml` /
  `docker/trivyignore.alpine.yaml` cover 3.5.9-r1 and -r3 (fix states unknown / not-fixed / wont-fix); no other
  variant's rules changed. Rescan of the published `1.4.0-alpine` (`sha256:ab761016…`): grype before rc=2 (shared
  file), after rc=0; trivy rc=0. SECURITY-CVE.md updated (status row, last scan, Alpine table, L-831 section).

- **Rebuild on newest bases (Robert t275u + option A, 2026-11-28 01:51–02:22 ET):** old release 460471424
  deleted (no `--cleanup-tag`); annotated tag `v1.4.0` moved from c3d3242 (tag object 39c3d00) to 99241a1
  (tag object 328a45e, message `searchdump-rs 1.4.0`); only that tag ref was force-updated. release-build
  46118591562 success (all jobs, including all 5 image jobs). New release id 461378710 published 02:22 ET
  and marked Latest: https://github.com/robertgoley/searchdump-rs/releases/tag/v1.4.0 (`releases/latest` =
  v1.4.0). Notes are the CHANGELOG `## 1.4.0` section (disclaimer included). `main` was ff'd 169a6bb -> 99241a1.
  The GitHub Releases assets were replaced (same 11 names; checksums differ from the 2026-11-27 ship because
  this is a new CI build and the tarball includes the pin-refresh docs). SHA256SUMS:
  - aarch64-gnu `3165aef7bb8f1df2a2e9feff5b4a5e8eeb87ae5cdd72a309bfa80bb95532ec8e`
  - aarch64-musl `add52f046440bd8e7d8f1fe7de58f6d8e541972624c576a9c960acbd81d4a9e9`
  - x86_64-gnu `d804b9ba78a0db588dcae1fa2fcdf508f332f34b2f1a0425efd27c0408d418bc`
  - x86_64-musl `e5c1fbd5a23d7e662dc62684bbf13028e2217f4d949de01d199e1af75fd747e4`
  - SBOM `0884470532f4515be61e7560a737c95b946c1bb583c515a6a18e36c6e215478f`
  All 11 download anonymously; `sha256sum -c` and each `.sha256` OK; `gh attestation verify` OK for the 4
  tarballs and the SBOM. x86_64 gnu + musl: `searchdump`, `searchdump-multi`, `elasticdump`, `elasticdump-rs`,
  `multielasticdump` print 1.4.0; `--help` OK. aarch64: `file` only (ELF aarch64, gnu dynamic / musl static).
- **Images after the rebuild (all 7 tags replaced; attestations OK):**
  - `latest` = `1.4.0` = `1.4.0-dhi-debian`: `sha256:dc0bf921f3d5f6b35180143b6a92788a03e5c96a622219c93b7abf1c9409822d`
    (was `630d89a3…`)
  - `1.4.0-dhi-alpine`: `sha256:419027b840db7189680298ef9dae3cc1516b0bb7aa348cd1a2d7cec8e90140a8` (was `3e2ddf43…`)
  - `1.4.0-dhi-static`: `sha256:d03116409ddd286490a66be03d3e50358d88d1d8a09d8ee4352390e20371faa5` (was `2a757f51…`)
  - `1.4.0-public`: `sha256:4f6d7489850c20ab249d2160339a927ce640166b3333bd4a0e6c0f532af96b06` (was `005391a4…`)
  - `1.4.0-alpine`: `sha256:ee1e7eb73ebfa68aaa1893b39f010d463d52a42a923b150551d951e07fd24e4e` (was `ab761016…`)
  Each prints `searchdump 1.4.0` / `searchdump-multi 1.4.0`; `--help` OK; `gh attestation verify oci://…@digest` OK.
- **Scan of the rebuilt images (grype / trivy, Makefile ignore rules):** dhi-debian, dhi-alpine, dhi-static and
  alpine pass both. `public` fails as documented (grype 24 ids, trivy 19 ids), the same IDs as the 2026-11-27
  public image, all already in SECURITY-CVE.md; no new accept rules added. alpine still ships OpenSSL 3.5.9-r1
  on Alpine 3.24.4; the L4-031 alpine ignore files still match.
- **Smoke (it-os-sec-l4tls, port 19252, private CA, 2026-11-28):** disk had recovered to 59%, so the leftover
  `cluster.blocks.create_index` was cleared. Released x86_64 gnu binary: dump+load 200/200, `searchdump-multi`
  dump+load restored 200. Released `:1.4.0` image: dump+load 200/200, dump identical to the binary (sorted);
  image `searchdump-multi` dump 200 records. Indices removed; container stopped again.
- Not done: aarch64 execution (no qemu), cosign (not used by the release process). Dependabot PR runs (sha2,
  rand, rand_chacha) failed; left alone.

