# Changelog

All notable changes to searchdump-rs. Versions follow [Semantic Versioning](https://semver.org/);
a version with a hyphen (`1.0.0-beta1`) is a pre-release. The release workflow
(`.github/workflows/release.yml`) uses the section for a version as the release notes of its draft
GitHub release.

## Unreleased

### Added

- **`--restartOnContextLoss N`** (L-804b, P3): a scroll data dump from a cluster into a file restarts from the start, rewriting the file, up to N times when the scroll context is lost (default 0 = off). Refused with a usage error for PIT/search_after dumps, `--scrollId`, and outputs other than a file.
- The scroll-context-lost error suggests a point-in-time dump (`--searchAfter --pit`) and `--restartOnContextLoss` (P4).
- When most failed writes are `cluster_block_exception`s, the exit-3 summary names the block; for a disk flood-stage block it suggests freeing disk, then rerunning with a higher `--retryAttempts` (P5).
- Error causes stay typed through the pipeline (`.context` instead of re-formatting; P2), so exit codes classify them reliably.

### Fixed

- **`sftp://` inside the container images** (owner decision 2026-11-14, L-810i). The runtime stage installs the
  OpenSSH client that OpenDAL's SFTP backend runs: `openssh-client` (no recommends, `ssh-keysign` setuid bit
  removed) on Debian-based images, `openssh-client-default` on Alpine-based images. The COPY-only `dhi-static`
  image is unchanged (no package manager, no SFTP). On Debian the package pulls in Kerberos libraries; see
  SECURITY-CVE.md for the High/Critical findings this adds, accepted by the owner on 2026-11-14 (no fix upstream;
  `make scan` ignores exactly those ids while they stay unfixed).

### Changed (behaviour change)

- **Exit status 4 for transient, rerun-safe failures** (owner decision 2026-11-14, L-804b; proposal P1 of L-804a). A run that still fails after its retries because of the network (refused, reset, timed out, body cut off), HTTP 429 or 5xx, or a lost scroll context now exits 4 instead of 1, when nothing was committed that a rerun would duplicate. A restore that already sent documents without `_id`, TLS certificate or protocol errors, credentials, mapping errors and every other permanent failure still exit 1; 0, 2, 3, 130 and 143 are unchanged. Scripts that treat any non-zero status as failure are unaffected; scripts that test for exactly 1 should also accept 4. `searchdump-multi` keeps its own codes.

- **`--httpAuthFile` now wins over every other credential source** (owner decision 2026-11-14, L-645b). When an
  auth file is given, its `user=`/`password=` pair is used for both directions even if the URL has
  `user:password@` or `--input-*`/`--output-*`/`--username`/`--password` flags are set; a warning names the sources
  it replaced, never their values. Before, the auth file only filled in a missing username or password (Node
  behaviour). Without an auth file the order is unchanged: flags, then URL, then
  `SEARCHDUMP_{INPUT,OUTPUT}_*`. `searchdump-multi` follows the same order. Auth-file values stay literal and a
  world-readable file is still only warned about.

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
