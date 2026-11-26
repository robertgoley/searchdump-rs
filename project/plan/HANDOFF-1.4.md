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

## 4. Item status

| item | status | notes |
|---|---|---|
| 4. one reqwest (+ cheap duplicate removal) | done locally (L4-004) | Left: digest/crypto-common 0.10 (crc-fast via opendal-service-s3), getrandom 0.2/0.3/0.4 (ring, ahash/tempfile, uuid), hashbrown 0.14 (rust-ini), syn 2 (openssh-sftp). None removable without upstream bumps |
| 1. multi TLS flags + passphrase env | todo | |
| 2. per-child uuid logging | todo | |
| 3. `--pageBytes` | todo | |

## 5. Decisions taken (open for Robert to confirm)

- D1 (L4-004): pkcs8 0.11 taken in 1.4 (it is the Dependabot PR's bump); it removes 7 duplicate crates. It adds
  aes-gcm/ghash/polyval (pkcs5 0.8 supports AES-GCM PBES2), all RustCrypto, exempted in cargo-vet like the 0.10 ones.
- D2 (L4-004): timeouts above ~100 years are capped (they meant "never" anyway) instead of erroring.

## 6. How to resume

1. `cd /workspace/rel-1.4 && source /workspace/perf-1.2/env.sh && git status && git log --oneline ac8d8c4..`
2. Read section 3/4 for the next step. Check nothing is still running (`pgrep -af 'cargo|searchdump'`) before
   starting a new build or test chain.
