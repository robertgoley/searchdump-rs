# Handoff: searchdump-rs 1.4 (branch `release/1.4`)

Self-contained pickup document. Owner: Robert Goley. Repo: https://github.com/robertgoley/searchdump-rs.
Last update: see the newest row of section 3 (times are America/New_York).

## 1. Where things are

- `release/1.4` was cut from `main` at ac8d8c4 (v1.3.1) on 2026-11-25 and pushed to origin as its own branch
  (like `release/1.3`). Worktree on grokbox: `/workspace/rel-1.4` (the repo is `/workspace/push-fork`).
- Not merged into `main`; no tag; no PR. `Cargo.toml` still says `1.3.1`; the version bump, tag and release are
  for Robert to start.
- Headline (Robert, t248u/t250u): **limits and multi polish**. Approved items (t243u): multi TLS flags,
  per-child uuid logging, `--pageBytes`, remove the duplicate reqwest. The key passphrase reaches children
  through the environment, never argv (t253u).
- **Not in scope until Robert reviews it (t254u):** the `searchdump-multi.rs` split
  (`/workspace/split-plan-doc/SPLIT-searchdump-multi.md`). Do not start it.
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

## 4. Item status

| item | status | notes |
|---|---|---|
| 4. one reqwest (+ cheap duplicate removal) | done locally (L4-004) | Left: digest/crypto-common 0.10 (crc-fast via opendal-service-s3), getrandom 0.2/0.3/0.4 (ring, ahash/tempfile, uuid), hashbrown 0.14 (rust-ini), syn 2 (openssh-sftp). None removable without upstream bumps |
| 1. multi TLS flags + passphrase env | done locally (L4-001) | IT against private-CA OpenSearch 2 with client-cert auth (`it-os-sec-l4tls`, port 19252, store edrs-1.4-podman) |
| 2. per-child uuid logging | done locally (L4-002) | run uuid per child + target index uuid in child stats (D9) |
| 3. `--pageBytes` | done locally (L4-003) | see D11-D13 |

## 5. Decisions taken (open for Robert to confirm)

- D1 (L4-004): pkcs8 0.11 taken in 1.4 (it is the Dependabot PR's bump); it removes 7 duplicate crates. It adds
  aes-gcm/ghash/polyval (pkcs5 0.8 supports AES-GCM PBES2), all RustCrypto, exempted in cargo-vet like the 0.10 ones.
- D2 (L4-004): timeouts above ~100 years are capped (they meant "never" anyway) instead of erroring.
- D3 (L4-004a): geiger per-crate rises from the RustCrypto 0.11 stack accepted into the baseline (total went down).
- D4 (L4-001): passphrase env names `SEARCHDUMP_TLS_PASS` (both), `SEARCHDUMP_{INPUT,OUTPUT}_TLS_PASS`; order
  `--pass` > `--{dir}-pass` > shared env > per-direction env; empty = unset; `ELASTICDUMP_*` aliases accepted.
- D5 (L4-001): multi `--pass` accepted (warned as `ps`-visible); env preferred. Children always get it via env only.
- D6 (L4-001): multi refuses other-side TLS flags, TLS flags with `--direction copy`, cert without key, and a
  passphrase with a Node elasticdump child (it cannot read the env var).
- D7 (L4-001): the multi parent still ignores `NODE_TLS_REJECT_UNAUTHORIZED` for its own requests (unchanged; use
  `--insecure`). Open question.
- D8 (L4-002): `child ID start:` / `end:` lines go to stderr for every child (can be noisy with many indices/parts;
  could move behind `--debug`). `children.json` in the log dir is the report. Open question.
- D9 (L4-002): "uuid" done both ways: a run UUID per child, plus the restore target's index uuid (noted/final) in
  child stats (HANDOFF-1.3 8.6 meaning).
- D10 (env): new isolated podman store `edrs-1.4-podman` (wrapper `/workspace/perf-1.4/bin/podman`) because
  `edrs-1.2-podman` has damaged layers (`podman system check`: damage detected). The 1.2 store is left untouched.
- D11 (L4-003): `--pageBytes` units as `--maxBulkBytes` (bytesize: `kb`=1000, `kib`=1024); minimum 1000 bytes, `0`
  refused (leave it out for no cap). It never raises `--limit`, and lowers an explicit `--limit` too.
- D12 (L4-003): with auto/min the cap may go below the 1.3 1 MiB page floor (user's explicit choice wins); with
  `off` one read-only `_stats` call is still made, only because the flag was given.
- D13 (L4-003): run-time: search_after/PIT keep an over-cap page and shrink the next ones; scroll drops an over-cap
  FIRST page unread, clears its context and restarts smaller (later scroll pages cannot be resized: only the
  start-up estimate bounds them). Shrinks log a warning (like the 429 halving). `--maxResponseMb` stays the hard
  limit. multi refuses it for load/copy (error, like `--fileSize` on load).

## 6. How to resume

1. `cd /workspace/rel-1.4 && source /workspace/perf-1.2/env.sh && git status && git log --oneline ac8d8c4..`
2. Read section 3/4 for the next step. Check nothing is still running (`pgrep -af 'cargo|searchdump'`) before
   starting a new build or test chain.
