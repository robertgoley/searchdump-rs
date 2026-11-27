# searchdump-rs 1.4.0 release notes

Status: **released 2026-11-27** (tag `v1.4.0`). Headline: **limits and multi polish**. The full list is in
[`CHANGELOG.md`](../../CHANGELOG.md) under **1.4.0**.

## Disclaimer

searchdump-rs is provided **as is, without warranty of any kind**, as set out in its
[MIT license](../../LICENSE). See [Disclaimer](../../README.md#disclaimer). Try it on a non-production cluster first
and verify every backup independently (restore it, compare counts) before relying on it.

## Upgrade

- Binaries: replace the 1.3.1 tarball with `searchdump-rs-1.4.0-<target>.tar.gz` (checksums and provenance as before).
- Images: `ghcr.io/robertgoley/searchdump-rs:1.4.0` (= `:1.4.0-dhi-debian` = `:latest`), or the other types
  (`:1.4.0-dhi-alpine`, `:1.4.0-dhi-static`, ...).
- No option was removed or renamed; existing command lines behave as in 1.3.1, except that `searchdump-multi` now
  honours `NODE_TLS_REJECT_UNAUTHORIZED=0` (as searchdump and Node elasticdump always did).

## What changed

- **`searchdump-multi` TLS:** `--ca`, `--cert`, `--key`, `--pass` (and the `--input-*` / `--output-*` forms) for the
  parent and every child, so private-CA clusters no longer need `--insecure`. The key passphrase reaches children only
  through `SEARCHDUMP_{INPUT,OUTPUT}_TLS_PASS`, never their command line. `searchdump` also reads the passphrase from
  `SEARCHDUMP_TLS_PASS` / `SEARCHDUMP_{INPUT,OUTPUT}_TLS_PASS`.
- **Per-child ids in `searchdump-multi`:** every child run has a UUID in the parent's start/end lines, the child's log
  lines, its stats file, the manifest and `LOGDIR/children.json`.
- **`--pageBytes=SIZE`:** caps each dump page by bytes (start-up sizing from the average document, shrinking at run
  time), for `searchdump` and `searchdump-multi` dumps.
- **One HTTP client:** reqwest 0.13 only (0.12, base64 0.22, quinn, h2 leave the tree); encrypted PKCS#8 keys via
  pkcs8 0.11 (same supported formats).
- **Fixed:** a huge `--idleTimeout` / `--requestTimeout` / `--timeout` no longer panics.
- **Internal:** `searchdump-multi` source split into modules; no behaviour change (help text pinned by golden tests).
