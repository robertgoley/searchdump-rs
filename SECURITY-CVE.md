# Security: dependency advisories and image CVEs

This file tracks every advisory and image CVE that the scanners below report for searchdump, with
its status. Status values:

- **open**: affects a shipped artifact and has no fix yet;
- **resolved**: fixed, and the "Resolved in" column says where;
- **not-affected**: present, with a justification.

Code and supply-chain findings (F-01…F-14, INJ-F1…F4) are mapped to their fixes and regression tests in [Code and supply-chain findings](#code-and-supply-chain-findings-sup-018).

Related docs: [TESTING-MATRIX.md](TESTING-MATRIX.md) and [NODE-DIFFERENCES.md](NODE-DIFFERENCES.md).

## Status summary

Counts below are taken from the sections in this file (High+Critical image findings unless noted). "Won't-fix (accepted)" means a Debian/DHI won't-fix or not-fixed High finding that is documented here and ignored by `make scan` only while that fix state lasts. "Open" means a finding on that image that is not accepted for shipping that image.

| Artifact | Open | Resolved | Won't-fix (accepted) | Not-affected |
|---|---:|---:|---:|---:|
| Cargo.lock (Rust dependencies) | 0 | 41 | 0 | 0 |
| `dhi-debian` (default `make image` since L-810c) | 0 | 1 (CVE-2026-109518, apt, L-810e) + 2 glibc + 15 Kerberos ids (1.3.1 base refresh, L-831) + 8 OpenSSL ids (DHI backport in openssl +dhi2, L-831; the scanners still list them, see [OpenSSL 3.5.9 (L-831)](#openssl-359-l-831)) | 17 base-OS ids (L-810e, incl. trivy-only CVE-2026-16742) + 7 OpenSSH/Kerberos ids (L-810i) + 1 OpenSSL id, CVE-2026-84786 (L-831, accepted 2026-11-25) | 0 |
| `dhi-static` (default from L-808b to L-810c) | 0 | 0 | 5 | 0 |
| `dhi-alpine` | 0 | 0 | 5 OpenSSL ids in libssl3/libcrypto3 3.5.9-r3 (L-831, accepted 2026-11-25; grype High, trivy not High) | 0 |
| Public Alpine (`make image-alpine`, `:<ver>-alpine`) | 0 | 0 | 5 OpenSSL ids in libssl3/libcrypto3 3.5.9-r1 (L4-031, accepted 2026-11-27, Robert t273u, same ids and rationale as dhi-alpine; grype High, trivy not High), see [OpenSSL 3.5.9 (L-831)](#openssl-359-l-831) | 1 (CVE-2025-60876, Medium, busybox `wget`) |
| Public Debian (`make image-public`) | 15 High (this base is not recommended) | 0 | 15 are Debian won't-fix / not-fixed; still open if this image is shipped | 0 |

The 15 public-Debian High CVEs are the same kind of base-OS won't-fix findings; they stay **open on that base** because that image is not an accepted default. Choosing Alpine or a dhi.io base avoids them.

## Last scan

| Date (America/New_York) | Commit | Tool (DB) | Target | Result |
|---|---|---|---|---|
| 2026-11-27 | L4-031 (1.4.0) | grype 0.121.0 + trivy, box | published `ghcr.io/robertgoley/searchdump-rs:1.4.0-alpine` (`sha256:ab761016…`) | Before: `make scan` rules (shared `docker/grype-ignore.yaml`) failed grype on 5 OpenSSL 3.5.9 ids in libssl3/libcrypto3 3.5.9-r1 (fix state unknown), trivy passed. Accepted by Robert (t273u) as for dhi-alpine; new `docker/grype-ignore.alpine.yaml` / `docker/trivyignore.alpine.yaml`. After: grype and trivy pass. dhi-debian, dhi-alpine, dhi-static 1.4.0 pass unchanged; public Debian keeps its documented open findings |
| 2026-11-25 | L-830/L-831 (1.3.1) | grype 0.121.0 + trivy 0.76.1 in CI, `security` `base_refresh` runs [45723093599](https://github.com/robertgoley/searchdump-rs/actions/runs/45723093599) (digests) and [45728110685](https://github.com/robertgoley/searchdump-rs/actions/runs/45728110685) (rebuild + rescan) | `dhi-debian` and `dhi-alpine` on the refreshed digests (debian-base `36cada6c…`, alpine-base `7e00fc0f…`) | Before (1.3.0 / main, run 45133930173): grype failed on glibc CVE-2026-5435/-19499 and krb5 ids (fixed upstream, rules matched only wont-fix) and 3 new OpenSSL ids. After: glibc 2.41-12+deb13u5, krb5 1.21.3-5+deb13u2, zstd deb13u1 and (via `apt-get upgrade`) openssl 3.5.9-1~deb13u1+dhi2; 17 ids gone and their rules removed; only the 9 OpenSSL ids remain, 8 of them backported by DHI. dhi-alpine: 5 OpenSSL ids (no fixed build). `sftp://` smoke test through each image's own `ssh` passed on both. See [OpenSSL 3.5.9 (L-831)](#openssl-359-l-831). |
| 2026-11-20 | L-824 | grype 0.121.0 + trivy 0.76.1, box, on the CI SBOM of [run 44684346604](https://github.com/robertgoley/searchdump-rs/actions/runs/44684346604) | `dhi-debian` default image (base `94571f3d…`) | before: grype fails on 6 High OpenSSL ids (not-fixed, unfixed in Debian); trivy rates them MEDIUM. **Accepted by Robert 2026-11-20**, see [OpenSSL 3.5.7 (L-824)](#openssl-357-l-824). With the new rules grype passes; the confirming CI run is the `security` run on the L-824 commit. |
| 2026-11-14 | L-810j | grype + trivy, box | `public` and `alpine` builds after deleting the passwd/adduser/init-system-helpers tools | Unchanged findings (files deleted, packages still in the dpkg database): Alpine 0 High/Critical; public Debian only its documented open base-OS findings, no OpenSSH/Kerberos id reported. `make test-image` PASS on both, SFTP round trip OK. dhi-debian confirmed by CI (see [results/local/L-810j.md](results/local/L-810j.md)). |
| 2026-11-14 | L-810i (merged to `main` after the owner accepted the findings, t85u) | grype + trivy, box | `public` (debian:trixie-slim) and `alpine` image builds with the OpenSSH client added | Alpine: 0 High/Critical. Debian: openssh-client adds 1 Critical (CVE-2026-60002) and 5 High, its krb5 dependencies about 15 High ids, none fixable; **accepted by the owner 2026-11-14 (t85u)**, see [OpenSSH client (L-810i)](#openssh-client-l-810i) (details and options: [results/local/L-810i.md](results/local/L-810i.md)). dhi.io types not built (no login). |
| 2026-11-11 | `800ff96` (L-810f) | CI [security.yml](https://github.com/robertgoley/searchdump-rs/actions/workflows/security.yml) (grype + trivy), `base=dhi-debian` | `dhi-debian` default image on digest `sha256:94571f3d…` (apt 3.0.4+dhi0) | **pass**. Confirming scan of the L-810e base refresh: [run 42998300960](https://github.com/robertgoley/searchdump-rs/actions/runs/42998300960) (manual dispatch, 04:48–05:00 America/New_York). CVE-2026-109518 is gone; remaining High findings are the accepted won't-fix set. Later pushes on `main` also passed the same workflow: 42998301407 (`800ff96` push), 43070931258 (`e1a0f8e`), 43071665377 (`f38af3f`), 43077967669 (`94e4afb`), 43085265717 (`a98be22`) |
| 2026-11-11 | L-810e | grype 0.121.0 + trivy 0.76.1 | `dhi-debian` image (default since L-810c) on the previous base digest `cc4e05ff…` | before: grype 19 distinct High (90 matches; 1 fixable, CVE-2026-109518 in apt), trivy 12 distinct HIGH. With the new dhi-debian ignore files: only CVE-2026-109518 left in both scanners, which the refreshed base (`94571f3d…`, apt 3.0.4+dhi0) fixes. The confirming CI scan on the new digest is the row above |
| 2026-10-29 | 55fa662 | cargo-audit 0.22.2 (re-run for L-109) | Cargo.lock | **0 vulnerabilities, 0 warnings**; cargo-deny: all four checks ok |
| 2026-10-29 | ae2531f | cargo-audit 0.22.2 (RustSec advisory-db, 1386 advisories) | Cargo.lock (271 crates) | **0 vulnerabilities, 0 warnings** |
| 2026-10-29 | ae2531f | cargo-deny 0.20.2, `check advisories bans licenses sources` | Cargo.lock + deny.toml | **advisories ok, bans ok, licenses ok, sources ok** |
| 2026-10-29 | c620b80 | grype (DB v6.1.10, built 2026-10-29 02:23 EDT) | `make image-alpine` (rust:1.99.1-alpine3.24 → alpine:3.24.3) | **0 Critical, 0 High**, 3 Medium (one CVE, see below) |
| 2026-10-29 | c620b80 | trivy | `make image-alpine` | **0** (all severities) |
| 2026-10-29 | ae2531f | grype | `make image-public` (rust:1.99-trixie → debian:trixie-slim) | 54 High, 47 Medium, 10 Low, 46 Negligible (157 matches, 0 fixable) |
| 2026-10-29 | ae2531f | trivy | `make image-public` | 43 HIGH, 58 MEDIUM, 60 LOW, 1 UNKNOWN (162) |
| 2026-10-29 | d36b906 | grype DB v6.1.10 + trivy | `make image` (default until L-808b: dhi.io/rust:1.99.0-alpine3.23-dev → dhi.io/alpine-base:3.24-alpine3.24-dev) | **0 at every severity** in both scanners |
| 2026-11-01 | L-808b | grype 0.121.0 + trivy | `make image` (**default since L-808b**: dhi.io/rust:1.99.1-debian13-dev → dhi.io/static:20250419-glibc-debian13, glibc + mimalloc) | grype: 5 High, 9 Medium, 3 Low, 7 Negligible (24 distinct CVEs, all in libc6 / libgcc-s1, 0 fixable: Debian `wont-fix` / `not-fixed`); trivy: **0 HIGH/CRITICAL** (16 MEDIUM, 8 LOW). Failed `make scan` until L-808d accepted the five (grype ignore rules scoped to fix state `not-fixed`/`wont-fix`), see [the default image](#dhiio-static-glibc-make-image-dhi-static-the-default-from-l-808b-to-l-810c) |
| 2026-11-01 | L-808b | grype + trivy | dhi.io glibc runtime candidates, bare bases (`results/local/L-808b.md`) | `static:20250419-glibc-debian13`: grype 5 High (24 distinct), trivy 0 HIGH; `debian-base:trixie-debian13`: grype 14 High (46 distinct), trivy 6 HIGH; `debian-base:trixie-debian13-dev`: grype 46 High (71 distinct), trivy 35 HIGH. 0 fixable in all three |
| 2026-11-01 | L-808d | grype 0.121.0 + trivy | `make scan` on `make image` (dhi.io/static glibc default) and `IMAGE_VARIANT=dhi-alpine` | default: **passes**; grype ignores exactly the 5 accepted High CVEs (10 matches, fix state `wont-fix`/`not-fixed`, `docker/grype-ignore.yaml`), 0 other High+; trivy 0 HIGH/CRITICAL. dhi-alpine: passes, 0 findings. Check: the same rules restricted to fix state `fixed` leave all 5 unignored and grype exits 2 |
| 2026-11-01 | L-808b | build + `scripts/test-image.sh` | `make image-dhi-alpine` (the previous default, now `:<ver>-alpine`) | builds unchanged; 0 at every severity at the 2026-10-29 scan |
| 2026-10-29 | fb46cea | grype | `make image-dhi-debian` (dhi.io/rust:1.99.1-debian13-dev → dhi.io/debian-base:trixie-debian13-dev), the default before this change, scanned before any fix | 284 raw matches. grype reports every package twice, from `/var/lib/dpkg/status` and from DHI's `status.d`; deduplicated: 44 High, 46 Medium, 10 Low, 42 Negligible (142 package matches). Distinct CVEs: 15 High, 24 Medium, 6 Low, 25 Negligible. 0 fixable |
| 2026-10-29 | fb46cea | trivy | `make image-dhi-debian` | 33 HIGH, 56 MEDIUM, 54 LOW, 1 UNKNOWN (144). Distinct CVEs: 9 HIGH, 30 MEDIUM, 32 LOW, 1 UNKNOWN. 0 fixable |

## Before / after

| Scan | Before (2d1ab85) | After |
|---|---|---|
| `cargo audit` | 24 vulnerabilities + 17 warnings (2 unmaintained, 15 unsound) | 0 + 0 (ae2531f) |
| `cargo deny check advisories bans licenses sources` | advisories: 13 vulnerability, 13 unsound, 2 unmaintained errors; licenses: 1 rejected (tiny-keccak CC0-1.0); no deny.toml before | all four checks ok (ae2531f) |
| Default image (`make image`), grype High+Critical | dhi.io Debian 13 `-dev`: 88 raw / 44 deduplicated (15 distinct CVEs) | dhi.io Alpine `-dev`: **0** (0 at any severity) |
| Default image (`make image`), trivy HIGH+CRITICAL | dhi.io Debian 13 `-dev`: 33 (9 distinct CVEs) | dhi.io Alpine `-dev`: **0** (0 at any severity) |
| Default image since L-808b (glibc requested), grype / trivy High+Critical | dhi.io Alpine `-dev` (musl): 0 / 0 | dhi.io `static` glibc: 5 / 0 (glibc and libgcc-s, none fixable); the Alpine image stays available as `make image-dhi-alpine` / `:<ver>-alpine` |
| Public fallback, grype High+Critical | Debian 13 (`image-public`): 54 | Alpine (`image-alpine`): 0 |
| Public fallback, trivy HIGH+CRITICAL | Debian 13 (`image-public`): 43 | Alpine (`image-alpine`): 0 |

## Scanning method

- **Rust dependencies**
  - `cargo audit` (RustSec database, fetched at scan time) reads Cargo.lock, which is committed.
  - `cargo deny check advisories bans licenses sources` uses [deny.toml](deny.toml):
    - advisories: yanked, unmaintained and unsound crates are all errors;
    - licenses: an explicit allow list;
    - bans: wildcard versions are denied, duplicate versions only warn;
    - sources: crates.io only.
  - CI runs `cargo audit --deny warnings`, so a new unmaintained or unsound warning also fails the job.
- **Container image**
  - `make save` writes `image.tar`; `make scan` runs `grype -c docker/grype-ignore.yaml --fail-on high` and `trivy --severity HIGH,CRITICAL --exit-code 1` on it. For `IMAGE_VARIANT=dhi-debian` (L-810e) it uses `docker/grype-ignore.dhi-debian.yaml` and `--ignorefile docker/trivyignore.dhi-debian.yaml` instead.
  - Both scanners see the OS packages only. The Rust binary is not built with `cargo auditable`, so they cannot see the crates inside it; cargo-audit covers those.
- **SBOMs**
  - `make sbom` uses syft to write `sbom-image.cdx.json` (CycloneDX) and `sbom-image.spdx.json` (SPDX).
  - CI also runs `cargo cyclonedx` to produce the crate SBOM (`sbom-cargo.cdx.json`).
  - SBOMs are uploaded as workflow artifacts and are not committed.
- **CI**
  - [.github/workflows/security.yml](.github/workflows/security.yml) runs on push to `main`/`rewrite`, on manual dispatch, and weekly.
  - It fails on any RustSec advisory or warning, a cargo-deny violation, or a HIGH/CRITICAL image CVE.
  - It never logs in to push and never publishes images. The dhi.io login is pull-only and is used for the `dhi*` bases (the default `base=dhi` is the static glibc image; `base=dhi-alpine` the Alpine one).
- **Reproduce locally**
  - `cargo audit`
  - `cargo deny check advisories bans licenses sources`
  - `make image-alpine scan sbom`

## Rust dependency advisories (Cargo.lock)

All entries below were reported against Cargo.lock at 2d1ab85. Each was fixed by upgrading or replacing the crate.

| ID | CVE | Crate (before) | Kind / severity | Title | Status | Resolved by | Commit |
|---|---|---|---|---|---|---|---|
| RUSTSEC-2026-0342 | – | quinn-proto 0.11.10 | vulnerability; no CVSS in advisory | Zero-length DATAGRAM frames bypass the receive buffer limit in quinn-proto | resolved | 0.11.19: `cargo update` (transitive, reqwest) | ae2531f |
| RUSTSEC-2026-0338 | – | quinn-proto 0.11.10 | vulnerability; no CVSS in advisory | Remote peer can panic quinn-proto with zero-length STREAM frames during unordered reads | resolved | 0.11.19: `cargo update` (transitive, reqwest) | ae2531f |
| RUSTSEC-2026-0037 | CVE-2026-31812 | quinn-proto 0.11.10 | vulnerability; `CVSS:4.0/AV:N/AC:L/AT:N/PR:N/UI:N/VC:N/VI:N/VA:H/SC:N/SI:N/SA:N` | Denial of service in Quinn endpoints | resolved | 0.11.19: `cargo update` (transitive, reqwest) | ae2531f |
| RUSTSEC-2026-0339 | – | quinn-proto 0.11.10 | vulnerability; no CVSS in advisory | Unbounded growth of pending connection ID retirements in quinn-proto | resolved | 0.11.19: `cargo update` (transitive, reqwest) | ae2531f |
| RUSTSEC-2026-0340 | – | quinn-proto 0.11.10 | vulnerability; `CVSS:3.1/AV:N/AC:L/PR:N/UI:N/S:U/C:N/I:N/A:H` | Peer minimum ACK delay can panic quinn-proto when ACK_FREQUENCY transmission is enabled | resolved | 0.11.19: `cargo update` (transitive, reqwest) | ae2531f |
| RUSTSEC-2026-0185 | CVE-2026-25800 | quinn-proto 0.11.10 | vulnerability; `CVSS:3.1/AV:N/AC:L/PR:N/UI:N/S:U/C:N/I:N/A:H` | Remote memory exhaustion in quinn-proto from unbounded out-of-order stream reassembly | resolved | 0.11.19: `cargo update` (transitive, reqwest) | ae2531f |
| RUSTSEC-2026-0235 | – | rkyv 0.8.10 | vulnerability; no CVSS in advisory | Insufficient archive validation can cause out-of-bounds reads in archives containing Rc/Arc | resolved | 0.8.18: `cargo update` (transitive) | ae2531f |
| RUSTSEC-2026-0001 | – | rkyv 0.8.10 | vulnerability; no CVSS in advisory | Potential Undefined Behaviors in `Arc<T>`/`Rc<T>` impls of `from_value` on OOM | resolved | 0.8.18: `cargo update` (transitive) | ae2531f |
| RUSTSEC-2026-0234 | – | rkyv 0.8.10 | vulnerability; no CVSS in advisory | Insufficient archive validation can cause out-of-bounds reads in archives containing hash tables | resolved | 0.8.18: `cargo update` (transitive) | ae2531f |
| RUSTSEC-2026-0233 | – | rkyv 0.8.10 | vulnerability; no CVSS in advisory | Crafted archives can cause a use-after-free during deserialization | resolved | 0.8.18: `cargo update` (transitive) | ae2531f |
| RUSTSEC-2026-0349 | – | rustls 0.23.26 | vulnerability; `CVSS:3.1/AV:N/AC:H/PR:N/UI:N/S:U/C:L/I:N/A:N` | Unauthenticated ServerHello can downgrade later connections to classical key exchange | resolved | 0.23.46: `cargo update` | ae2531f |
| RUSTSEC-2026-0348 | – | rustls 0.23.26 | vulnerability; `CVSS:3.1/AV:N/AC:H/PR:N/UI:N/S:U/C:N/I:N/A:L` | Brotli certificate decompression can allocate up to 1 GiB before authentication | resolved | 0.23.46: `cargo update` | ae2531f |
| RUSTSEC-2026-0346 | – | rustls 0.23.26 | vulnerability; `CVSS:3.1/AV:L/AC:H/PR:H/UI:R/S:U/C:H/I:N/A:N` | `Debug` impls for client session values expose TLS secrets | resolved | 0.23.46: `cargo update` | ae2531f |
| RUSTSEC-2026-0285 | – | rustls 0.23.26 | vulnerability; `CVSS:3.1/AV:N/AC:L/PR:N/UI:N/S:U/C:L/I:N/A:N` | TLS 1.3 handshake messages incorrectly accepted across encryption level boundaries | resolved | 0.23.46: `cargo update` | ae2531f |
| RUSTSEC-2026-0382 | CVE-2026-111943 | rustls-webpki 0.103.1 | vulnerability; no CVSS in advisory | Certificate verification may accept distrusted intermediates | resolved | 0.103.16: `cargo update` | ae2531f |
| RUSTSEC-2026-0104 | CVE-2026-93599 | rustls-webpki 0.103.1 | vulnerability; no CVSS in advisory | Reachable panic in certificate revocation list parsing | resolved | 0.103.16: `cargo update` | ae2531f |
| RUSTSEC-2026-0098 | CVE-2026-93600 | rustls-webpki 0.103.1 | vulnerability; no CVSS in advisory | Name constraints for URI names were incorrectly accepted | resolved | 0.103.16: `cargo update` | ae2531f |
| RUSTSEC-2026-0049 | CVE-2026-93602 | rustls-webpki 0.103.1 | vulnerability; no CVSS in advisory | CRLs not considered authoritative by Distribution Point due to faulty matching logic | resolved | 0.103.16: `cargo update` | ae2531f |
| RUSTSEC-2026-0099 | CVE-2026-93601 | rustls-webpki 0.103.1 | vulnerability; no CVSS in advisory | Name constraints were accepted for certificates asserting a wildcard name | resolved | 0.103.16: `cargo update` | ae2531f |
| RUSTSEC-2026-0009 | CVE-2026-25727 | time 0.3.41 | vulnerability; `CVSS:4.0/AV:N/AC:H/AT:N/PR:L/UI:A/VC:N/VI:N/VA:H/SC:N/SI:N/SA:H` | Denial of Service via Stack Exhaustion | resolved | removed (no longer in the tree): dropped the `elasticsearch` 8.19.0-alpha.1 dev-dependency (serde_with → time); the in-tree fix needs Rust 1.88, above the 1.85 MSRV | ae2531f |
| RUSTSEC-2026-0381 | CVE-2026-111943 | webpki-roots 0.26.8 | vulnerability; no CVSS in advisory | Bundled root certificates allow trust in distrusted emSign intermediates | resolved | 1.0.11: direct dependency bumped 0.26 → 1 | ae2531f |
| RUSTSEC-2026-0391 | – | zstd 0.13.3 | vulnerability; no CVSS in advisory | Data from an abandoned stream leaks into the next stream on a reused context | resolved | 0.14.3: direct dependency bumped 0.13 → 0.14 | ae2531f |
| RUSTSEC-2026-0398 | – | zstd-safe 7.3.0 | vulnerability; no CVSS in advisory | `Seekable::decompress` sets a `Vec` length beyond its capacity for offsets past the end | resolved | 8.2.0: via zstd 0.14 | ae2531f |
| RUSTSEC-2026-0403 | – | zstd-safe 7.3.0 | vulnerability; no CVSS in advisory | Uninitialized memory exposed when streaming decompression of a legacy frame fails | resolved | 8.2.0: via zstd 0.14 | ae2531f |
| RUSTSEC-2025-0119 | – | number_prefix 0.4.0 | unmaintained (warning) | number_prefix crate is unmaintained | resolved | removed (indicatif 0.18 uses unit-prefix): indicatif 0.17 → 0.18, console 0.15 → 0.16 | ae2531f |
| RUSTSEC-2025-0134 | – | rustls-pemfile 2.2.0 | unmaintained (warning) | rustls-pemfile is unmaintained | resolved | removed (rustls-pki-types `PemObject`): replaced in `src/http/tls.rs` | ae2531f |
| RUSTSEC-2026-0190 | – | anyhow 1.0.102 | unsound (warning) | Unsoundness in `Error::downcast_mut()` | resolved | 1.0.104: `cargo update` | ae2531f |
| RUSTSEC-2026-0097 | – | rand 0.8.5 | unsound (warning) | Rand is unsound with a custom logger using `rand::rng()` | resolved | 0.8.8 / 0.10.3: `cargo update` (rand 0.8 is a lipsum dev-dependency) | ae2531f |
| RUSTSEC-2026-0097 | – | rand 0.9.1 | unsound (warning) | Rand is unsound with a custom logger using `rand::rng()` | resolved | 0.8.8 / 0.10.3: `cargo update` (rand 0.8 is a lipsum dev-dependency) | ae2531f |
| RUSTSEC-2026-0122 | – | rkyv 0.8.10 | unsound (warning) | Potential use-after-free due to lack of panic safety in `InlineVec::clear` and `SerVec::clear` | resolved | 0.8.18: `cargo update` (transitive) | ae2531f |
| RUSTSEC-2026-0386 | – | tokio 1.44.2 | unsound (warning) | `AsyncFd::new` and related constructors violate I/O safety | resolved | 1.54.1: `cargo update` | ae2531f |
| RUSTSEC-2026-0396 | – | zstd-safe 7.3.0 | unsound (warning) | `Seekable` reads a NULL or out-of-bounds seek table | resolved | 8.2.0: via zstd 0.14 | ae2531f |
| RUSTSEC-2026-0397 | – | zstd-safe 7.3.0 | unsound (warning) | `OutBuffer::around_pos` exposes uninitialized memory | resolved | 8.2.0: via zstd 0.14 | ae2531f |
| RUSTSEC-2026-0405 | – | zstd-safe 7.3.0 | unsound (warning) | Out-of-bounds read with an out-of-range `InBuffer::pos` | resolved | 8.2.0: via zstd 0.14 | ae2531f |
| RUSTSEC-2026-0401 | – | zstd-safe 7.3.0 | unsound (warning) | Out-of-bounds read in `train_from_buffer` when sample sizes overflow | resolved | 8.2.0: via zstd 0.14 | ae2531f |
| RUSTSEC-2026-0399 | – | zstd-safe 7.3.0 | unsound (warning) | Use-after-free through `init_using_cdict` and `init_using_ddict` | resolved | 8.2.0: via zstd 0.14 | ae2531f |
| RUSTSEC-2026-0402 | – | zstd-safe 7.3.0 | unsound (warning) | Heap buffer overflow in `CCtx::try_clone` on a multithreaded context | resolved | 8.2.0: via zstd 0.14 | ae2531f |
| RUSTSEC-2026-0406 | – | zstd-safe 7.3.0 | unsound (warning) | Use-after-free when changing a multithreaded context with an unfinished frame | resolved | 8.2.0: via zstd 0.14 | ae2531f |
| RUSTSEC-2026-0404 | – | zstd-safe 7.3.0 | unsound (warning) | Double free in `DCtx::try_clone` | resolved | 8.2.0: via zstd 0.14 | ae2531f |
| RUSTSEC-2026-0400 | – | zstd-safe 7.3.0 | unsound (warning) | Use-after-free of a dictionary after a one-shot call on a context with an open stream | resolved | 8.2.0: via zstd 0.14 | ae2531f |
| RUSTSEC-2026-0395 | – | zstd-safe 7.3.0 | unsound (warning) | Use-after-free with `CParameter::StableInBuffer` | resolved | 8.2.0: via zstd 0.14 | ae2531f |

Related cargo-deny finding:

- **License: tiny-keccak 2.0.2 is CC0-1.0.**
  - It reaches the build through rust-ini → ordered-multimap → dlv-list → const-random.
  - CC0-1.0 is a permissive public-domain dedication, so it was added to the allow list in deny.toml.
  - Commit: ae2531f.

## Container image CVEs

### Runtime bases

| Build | Builder | Runtime | libc | Scan result (High+Critical) |
|---|---|---|---|---|
| `make image` (**default**, L-808b) | `dhi.io/rust:1.99.1-debian13-dev` (`sha256:be6c6ec91c61…`) | `dhi.io/static:20250419-glibc-debian13` (`sha256:dcf8da7f5cde…`, multi-arch index), COPY-only stage, no shell | glibc 2.41 / glibc 2.41, mimalloc | grype 5 High (24 distinct CVEs at any severity), trivy 0 HIGH; none fixable |
| `make image-dhi-alpine` (`:<ver>-alpine`; default until L-808b) | `dhi.io/rust:1.99.0-alpine3.23-dev` (`sha256:8fd18196a3c5…`) | `dhi.io/alpine-base:3.24-alpine3.24-dev` (`sha256:a6349b553099…`), plus apk `upgrade`, `bash`, `ca-certificates`, `libgcc` | musl / musl | 0 (0 at any severity) |
| `make image-dhi-debian` | `dhi.io/rust:1.99.1-debian13-dev` (`sha256:e2dbe6c0d027…`) | `dhi.io/debian-base:trixie-debian13-dev` (`sha256:09943fbda224…`) | glibc 2.41 / glibc 2.41 | 15 distinct CVEs (grype) / 9 (trivy), none fixable |
| `make image-public` | `docker.io/library/rust:1.99-trixie` | `docker.io/library/debian:trixie-slim` | glibc 2.41 / glibc 2.41 | 54 High matches (grype) |
| `make image-alpine` | `docker.io/library/rust:1.99.1-alpine3.24` | `docker.io/library/alpine:3.24.3`, plus the same apk steps | musl / musl | 0 (2026-10-29); 1.4.0: 5 accepted OpenSSL 3.5.9 ids, see [OpenSSL 3.5.9 (L-831)](#openssl-359-l-831) |

**How the default was fixed: dhi.io tag choice plus Dockerfile changes.**

- **Before.** The dhi.io Debian 13 `-dev` runtime (`trixie-debian13-dev`) was the default and was scanned as built, before any change. The results are in the dhi.io Debian table below.
  - The `-dev` variant brings apt, gpgv, perl-base, util-linux, ncurses, pam, systemd libraries and tar.
  - Debian marks every one of these findings `wont-fix` or `not-fixed`, and the image carries the same Debian package versions (`+dhiN` rebuilds).
  - So no Debian-based tag can clear them. `dhi.io/debian-base` only has the `trixie*` tags (`trixie`, `trixie-debian13`, `-dev`, `-fips`, `-fips-dev`, `-kit`), with no dated tags. The non-dev tags have no bash.
  - `dhi.io/bash:5.2.37-debian13-dev` is worse: 14 Critical and 70 High (grype).
- **Alternatives scanned on their own.**
  - `dhi.io/bash:5-alpine3.24-dev` has 2 High (curl/libcurl, which it ships).
  - `dhi.io/alpine-base:3.24-alpine3.24-dev` has only one Unknown finding: musl CVE-2026-107891, fixed upstream.
- **After.** The default moved to Alpine/musl dhi.io images, with a matching dhi.io musl builder, `dhi.io/rust:1.99.0-alpine3.23-dev`. That is the newest Alpine Rust tag on dhi.io; dhi.io has no `1.99.1-alpine` tag.
- **Dockerfile changes for Alpine runtimes.**
  - `apk upgrade --no-cache` picks up musl 1.2.6-r3, the fix for CVE-2026-107891.
  - `apk add bash ca-certificates libgcc` adds the shell and the CA bundle. `libgcc` is there because Alpine-packaged rustc links `libgcc_s` dynamically.
  - The packages come from the image's own repositories, `https://dhi.io/apk/alpine/v3.24/main` (no credentials needed inside the build), so they are recorded in the apk database and stay visible to scanners.
  - `CARGO_HOME=/usr/local/cargo` is pinned so the registry cache mount works with the Alpine-packaged cargo.
- **Size.** The image is ~20 MB, versus ~90 MB on dhi.io Debian.
- **Credentials.** The dhi.io credentials stay in podman's auth file on the build host. `podman history` shows no login or auth step, and there is no `auth.json`/`config.json` in the image.

### Alpine image (`make image-alpine`)

| ID | Component | Severity | Status | Justification / resolution |
|---|---|---|---|---|
| CVE-2025-60876 | busybox 1.37.0-r31, busybox-binsh, ssl_client | Medium (grype, CVSS 6.5); trivy does not report it | not-affected; no Alpine fix yet | The flaw is in the busybox `wget` applet (and `ssl_client`, which only `wget` uses). Nothing in the image calls `wget`: the tool does HTTP itself through reqwest/rustls, and the entrypoint is the Rust binary. Re-check at the next scan. |
| CVE-2026-107493, CVE-2026-107494, CVE-2026-121482, CVE-2026-124709, CVE-2026-84786 | libssl3, libcrypto3 3.5.9-r1 | High (grype, fix state unknown); trivy not High | **accepted** (Robert 2026-11-27, t273u), as for dhi-alpine | OpenSSL 3.5.9, no fixed Alpine build yet (upstream 3.5.10); searchdump uses rustls, libcrypto3 is there for the OpenSSH client (`sftp://`) and apk. See [OpenSSL 3.5.9 (L-831)](#openssl-359-l-831) |

### dhi.io static glibc (`make image-dhi-static`; the default from L-808b to L-810c)

L-808b made the glibc + mimalloc build the default image at the user's request. Of the dhi.io glibc runtimes,
`dhi.io/static:20250419-glibc-debian13` has the fewest findings (comparison above and in `results/local/L-808b.md`): it ships
only `base-files`, `ca-certificates`, `libc6`, `libgcc-s1` and `tzdata`, with no shell or package manager. trivy reports no
HIGH/CRITICAL finding; grype rates five CVEs High that trivy rates MEDIUM. All five are Debian `wont-fix` / `not-fixed`,
Status: **accepted, open** (user decision 2026-11-01, L-808d): the default stays on this base, and the five CVEs are
accepted as known while no fix exists. `make scan` ignores exactly these five ids through
[`docker/grype-ignore.yaml`](docker/grype-ignore.yaml): each rule names one CVE id and its package, and matches only
while grype reports the fix state as `not-fixed` or `wont-fix`. When Debian/DHI publish a fix, the state becomes
`fixed`, no rule matches and `make scan` fails again; then update the base pin and drop the rule. Revisit when Debian
or DHI publishes a fix (check at every scan). The Alpine image (`make image-dhi-alpine`, `:<ver>-alpine`) has none of
them.

| CVE | Package | Severity (grype / trivy) | Debian status | Fix | Accepted | Notes |
|---|---|---|---|---|---|---|
| CVE-2026-19499 | libc6 2.41-12+deb13u4+dhi1 | High / MEDIUM | wont-fix | no fix available | 2026-11-01 (L-808d), revisit when Debian/DHI publishes a fix | also in every Debian 13 base below |
| CVE-2026-5435 | libc6 2.41-12+deb13u4+dhi1 | High / MEDIUM | wont-fix | no fix available | 2026-11-01 (L-808d), revisit when Debian/DHI publishes a fix | also in every Debian 13 base below |
| CVE-2026-84243 | libc6 2.41-12+deb13u4+dhi1 | High / MEDIUM | not-fixed | no fix available | 2026-11-01 (L-808d), revisit when Debian/DHI publishes a fix | new since the 2026-10-29 scan |
| CVE-2026-102010 | libgcc-s1 14.2.0-19+dhi5 | High / MEDIUM | wont-fix | no fix available | 2026-11-01 (L-808d), revisit when Debian/DHI publishes a fix | also in every Debian 13 base below |
| CVE-2026-95619 | libgcc-s1 14.2.0-19+dhi5 | High / MEDIUM | wont-fix | no fix available | 2026-11-01 (L-808d), revisit when Debian/DHI publishes a fix | also in every Debian 13 base below |

### dhi.io Debian 13 `-dev` (`make image`, the default since L-810c)

Image type `dhi-debian`: builder `dhi.io/rust:1.99.1-debian13-dev`, runtime `dhi.io/debian-base:trixie-debian13-dev`
(shell, apt). It became the default image in L-810c (owner decision 2026-11-11, pt146u). It was already built as
`make image-dhi-debian` before that and failed `make scan`.

**L-810e (owner decision 2026-11-11).**
- **Base refresh.** The runtime pin moves from `sha256:cc4e05ff…` (old) to the index digest
  `sha256:94571f3dc07fbc7166f6f1cbf1fe068a7647198ee2554e6b90d59a68cc8b053c`.
  - That is the `trixie-debian13-dev` tag as listed in the Docker Hub DHI catalog on 2026-11-11, pushed 2026-11-08.
  - Per-platform manifests: amd64 `sha256:da8ceb0d…`, arm64 `sha256:27f54125…`.
  - Its package list has `apt` and `libapt-pkg7.0` 3.0.4+dhi0, which fixes **CVE-2026-109518** (the only fixable
    High finding, in apt 3.0.3+dhi2); status **resolved in L-810e**.
- **Accepted CVEs.** Every remaining High finding is Debian/DHI won't-fix and is accepted below. Each one has a
  scoped rule that applies only to this image type (`IMAGE_VARIANT=dhi-debian`):
  - grype, in [`docker/grype-ignore.dhi-debian.yaml`](docker/grype-ignore.dhi-debian.yaml): one rule per CVE id
    and Debian package, matched only while the fix state is `wont-fix`, with anchored package names;
  - trivy, in [`docker/trivyignore.dhi-debian.yaml`](docker/trivyignore.dhi-debian.yaml): CVE id plus package purls.
- **Other images are unaffected.** `make scan` uses these files only for dhi-debian; every other image keeps
  `docker/grype-ignore.yaml` and plain trivy.
- **Revisit trigger, for every entry.**
  - Debian/DHI publish a fix: grype's fix state leaves `wont-fix`, the rule stops matching and `make scan` fails.
  - Or a new base digest changes the package set.
  - Or searchdump starts executing or linking the affected component.
  - Then refresh the pin, re-scan and drop the rule.
- **Review date** for every entry: 2026-11-11.

**Why none of these is reachable from searchdump.**
- **What the binaries use.** `searchdump` and `searchdump-multi` are Rust binaries. In the image they link only
  `libc.so.6`, `libm.so.6` and `libgcc_s.so.1` (`ldd`, 2026-11-11). They execute no program from the image;
  `searchdump-multi` runs only `searchdump`.
- **How they run.** As uid 65532 (nonroot). The image's only setuid/setgid file is `unix_chkpwd`. No daemon runs:
  the entrypoint is `searchdump`.
- **What the CVEs are in.** The other packages are part of the base image's shell and package-manager userland,
  present because this type keeps a shell:
  - the vulnerable programs (`infocmp`, `nsenter`, `gpgv`, `perl`, apt's tools) are only reached if someone runs
    them inside the container;
  - the vulnerable libraries (libstdc++, libsqlite3, libacl, libsystemd, libudev) are not loaded by searchdump.
- **For deployments that don't need a shell.** Use the `dhi-static` image (5 accepted libc6/libgcc-s1 CVEs,
  section above) or `dhi-alpine` (0).

| CVE | Package(s), version in the scanned image | grype / trivy | Upstream | Why not reachable / exploitable for searchdump | Reviewed | Revisit when |
|---|---|---|---|---|---|---|
| CVE-2026-109518 | apt, libapt-pkg7.0 3.0.3+dhi2 | High / HIGH | **fixed** in 3.0.4 | n/a: **resolved in L-810e** by the base refresh (3.0.4+dhi0) | 2026-11-11 | — |
| CVE-2025-69720 | libtinfo6, ncurses-base, ncurses-bin, ncurses-term 6.5+20250216-2+dhi4 | High / HIGH | won't-fix | Stack overflow in the `infocmp` program; searchdump never runs `infocmp` and does not link libtinfo | 2026-11-11 | fix published, or searchdump links ncurses |
| CVE-2026-102010 | gcc-14-base, libgcc-s1, libstdc++6 14.2.0-19+dhi5 | High / MEDIUM | won't-fix | Flaw in libstdc++'s priority-queue `erase_if`; searchdump does not link libstdc++ (Rust; libgcc_s only provides unwinding) | 2026-11-11 | fix published, or a C++ dependency is added |
| CVE-2026-95619 | gcc-14-base, libgcc-s1, libstdc++6 14.2.0-19+dhi5 | High / MEDIUM | won't-fix | Integer overflow in libstdc++'s aligned `operator new`; libstdc++ is not linked or loaded | 2026-11-11 | fix published, or a C++ dependency is added |
| CVE-2026-19499 | libc-bin, libc6 2.41-12+deb13u4+dhi1 | High / MEDIUM | **fixed** in 2.41-12+deb13u5 | n/a: **resolved in 1.3.1 (L-831)** by the base refresh (`36cada6c…`, libc6 2.41-12+deb13u5+dhi0); rule removed | 2026-11-25 | — |
| CVE-2026-5435 | libc-bin, libc6 2.41-12+deb13u4+dhi1 | High / MEDIUM | **fixed** in 2.41-12+deb13u5 | n/a: **resolved in 1.3.1 (L-831)** by the base refresh; rule removed | 2026-11-25 | — |
| CVE-2026-84243 | libc-bin, libc6 2.41-12+deb13u4+dhi1 | High / MEDIUM | won't-fix | gettext path traversal through a crafted `LANGUAGE` value. glibc may translate `strerror` text, but exploiting it needs control of the process environment and writable locale files; whoever sets the container environment already controls the process | 2026-11-11 | fix published, or the image starts taking `LANGUAGE` from untrusted input |
| CVE-2026-119373 | libsqlite3-0 3.46.1-7+deb13u2+dhi1 | High / HIGH | won't-fix | SQLite FTS5 overflow; searchdump does not link SQLite (it is there for other base-image tooling) | 2026-11-11 | fix published, or SQLite becomes a dependency |
| CVE-2026-119374 | libsqlite3-0 3.46.1-7+deb13u2+dhi1 | High / HIGH | won't-fix | SQLite FTS5 secure-delete overflow; SQLite not linked | 2026-11-11 | fix published, or SQLite becomes a dependency |
| CVE-2026-119379 | libsqlite3-0 3.46.1-7+deb13u2+dhi1 | High / HIGH | won't-fix | SQLite session-extension overflow; SQLite not linked | 2026-11-11 | fix published, or SQLite becomes a dependency |
| CVE-2026-24882 | gpgv 2.4.7-21+deb13u1+dhi4 | High / HIGH | won't-fix | Overflow in `tpm2daemon`, which is not in the image (only `gpgv`); searchdump runs no GnuPG tool | 2026-11-11 | fix published |
| CVE-2026-54369 | libacl1 2.3.2-2+dhi1 | High / HIGH | won't-fix | Symlink traversal in libacl path functions, a privilege escalation that needs a privileged process using libacl on attacker paths; searchdump does not link libacl and nothing privileged runs (uid 65532) | 2026-11-11 | fix published |
| CVE-2026-54370 | libacl1 2.3.2-2+dhi1 | High / MEDIUM | won't-fix | TOCTOU race in libacl, same preconditions as above | 2026-11-11 | fix published |
| CVE-2026-76642 | libblkid1, liblastlog2-2, libmount1, libsmartcols1, libuuid1, util-linux 2.41.5-0+deb13u1+dhi3 | High / HIGH | won't-fix | Mount-helper post-hook issue; there is no `mount` binary in the image, mounting needs privileges the container does not have, and searchdump does not link util-linux libraries | 2026-11-11 | fix published |
| CVE-2026-78408 | (same util-linux packages) | High / HIGH | won't-fix | `nsenter --join-cgroup` fd leak; needs root running `nsenter` (not setuid here) and searchdump never runs it | 2026-11-11 | fix published |
| CVE-2026-78409 | (same util-linux packages) | High / HIGH | won't-fix | `X-mount.subdir` symlink escape during a privileged mount; no mounts happen in the container | 2026-11-11 | fix published |
| CVE-2026-78410 | (same util-linux packages) | High / HIGH | won't-fix | Restricted bind mounts from fstab; no mounts, no fstab use | 2026-11-11 | fix published |
| CVE-2026-82560 | perl-base 5.40.1-6+deb13u1+dhi3 | High / UNKNOWN | won't-fix | Pod::Text resource exhaustion; searchdump runs no Perl | 2026-11-11 | fix published |
| CVE-2026-9538 | perl-base 5.40.1-6+deb13u1+dhi3 | High / HIGH | won't-fix (trivy: fix_deferred) | Archive::Tar memory exhaustion; searchdump runs no Perl | 2026-11-11 | fix published |
| CVE-2026-16742 | libsystemd0, libudev1 257.13-1~deb13u1+dhi2 | Medium / HIGH | won't-fix (trivy: affected) | Privilege escalation in `systemd-homed`, which is neither in the image nor running; searchdump does not link libsystemd/libudev. trivy rule only (grype rates it Medium) | 2026-11-11 | fix published |

The package versions in the table are those of the image scanned locally on 2026-11-11 (built on the previous digest,
`cc4e05ff…`). The new digest could not be pulled on the review host (no dhi.io login). The confirming CI `make scan`
on `sha256:94571f3d…` passed (run 42998300960 and the later `main` runs listed under [Last scan](#last-scan)). If a
new base adds a High CVE or changes a fix state, that scan fails and the new finding needs an entry here first.

### OpenSSH client (L-810i)

Owner decision 2026-11-14 (t85u): the High/Critical findings that `openssh-client` and the Kerberos libraries it
depends on (`libgssapi-krb5-2`, `libk5crypto3`, `libkrb5-3`, `libkrb5support0`) add to the Debian-based images
are **accepted as known and open** until Debian/DHI publish fixes, in the same way as L-808d / L-810e. Scanned on
the `public` Debian build (openssh-client 1:10.0p1-7+deb13u4, krb5 1.21.3-5+deb13u1); the dhi-debian image uses the
same packages unless DHI patches them, and its CI `make scan` is the confirming run. Alpine images: 0 findings.

| CVE | Severity | Packages | grype fix state | Why accepted |
|---|---|---|---|---|
| CVE-2026-60002 | Critical | openssh-client | wont-fix | Client side only, used by `sftp://` to a host the operator names; no sshd in the image; runs as uid 65532 |
| CVE-2026-59999, CVE-2026-60000, CVE-2026-106553, CVE-2026-106555, CVE-2026-106583 | High | openssh-client | wont-fix | Same |
| CVE-2026-107778 | High | krb5 libraries | wont-fix | Pulled in for GSSAPI; searchdump's SFTP does not use GSSAPI/Kerberos auth |
| CVE-2026-112314, CVE-2026-112320, CVE-2026-112321, CVE-2026-112322, CVE-2026-112323, CVE-2026-119079 | High | krb5 libraries | **fixed** in 1.21.3-5+deb13u2 | **Resolved in 1.3.1 (L-831)** on dhi-debian (krb5 1.21.3-5+deb13u2+dhi0); dhi-debian rules removed |
| CVE-2026-107364, CVE-2026-112315, CVE-2026-112316, CVE-2026-112317, CVE-2026-112318, CVE-2026-112319, CVE-2026-118380, CVE-2026-71199, CVE-2026-71200 | High | krb5 libraries | (trivy only: affected) | **Resolved in 1.3.1 (L-831)** on dhi-debian (neither scanner reports them on krb5 deb13u2+dhi0); dhi-debian rules removed |

`make scan` ignores exactly these ids in exactly these packages: grype rules in `docker/grype-ignore.dhi-debian.yaml`
and `docker/grype-ignore.yaml`, only while the fix state is not-fixed or wont-fix; trivy rules in
`docker/trivyignore.dhi-debian.yaml` (purl-scoped). Verified 2026-11-14 on the local `public` Debian build: with the
rules, grype and trivy report none of these ids. Revisit when a fix is published (grype then fails `make scan`).

### OpenSSL 3.5.7 (L-824)

**Superseded in 1.3.1 (L-831):** the image now carries openssl 3.5.9-1~deb13u1+dhi2, so the 3.5.7-versioned rules
below no longer match anything and were removed; the same six ids are handled in [OpenSSL 3.5.9 (L-831)](#openssl-359-l-831).

Owner decision 2026-11-20 (Robert, option (a)): six High findings in the dhi-debian image's OpenSSL packages
(`libssl3t64`, `openssl`, `openssl-provider-legacy`, all **3.5.7-1~deb13u3+dhi0**) are **accepted by Robert 2026-11-20**
as known and open, in the same way as L-808d / L-810e / L-810i. They come from the OpenSSL security advisory of
2026-11-10 and first failed the `security` `image` job on 2026-11-17 (runs 44172063678 and 44684346604).

| CVE | grype / trivy severity | Packages (version 3.5.7-1~deb13u3+dhi0) | grype fix state | Debian tracker |
|---|---|---|---|---|
| CVE-2026-88823 | High / MEDIUM | libssl3t64, openssl, openssl-provider-legacy | not-fixed | trixie and trixie-security 3.5.7-1~deb13u3: vulnerable; forky/sid 3.6.5-1: vulnerable (unfixed) |
| CVE-2026-95691 | High / MEDIUM | same | not-fixed | same |
| CVE-2026-95692 | High / MEDIUM | same | not-fixed | same (AES-GCM-SIV IV length; bookworm not affected) |
| CVE-2026-107493 | High / MEDIUM | same | not-fixed | same |
| CVE-2026-107496 | High / MEDIUM | same | not-fixed | same |
| CVE-2026-124709 | High / MEDIUM | same | not-fixed | same |

- **Why accepted:** no fix is available. Debian lists openssl as unfixed in trixie and sid, so no package upgrade or
  base-pin refresh removes them. `searchdump` and `searchdump-multi` do not link OpenSSL (TLS is rustls; no
  `openssl-sys` in Cargo.lock); the library is in the image for the OS tools, mainly the OpenSSH client used by `sftp://`.
- **Scope of the ignores:** these six ids only, in these three packages, at this exact version. grype rules in
  `docker/grype-ignore.dhi-debian.yaml` match package name + version and only while the fix state is not-fixed or
  wont-fix; trivy rules in `docker/trivyignore.dhi-debian.yaml` use versioned purls
  (`pkg:deb/debian/<name>@3.5.7-1~deb13u3%2Bdhi0`). Verified 2026-11-20 on the box against the CI SBOM of run
  44684346604 (grype 0.121.0, trivy 0.76.1): without the rules grype fails with exactly these six ids (36 matches);
  with them grype passes and trivy drops these six while still listing the other openssl ids (all MEDIUM).
- **Removal trigger:** once Debian/DHI ship a fixed openssl, refresh the dhi-debian base pin, rebuild and rescan, then
  delete these rules (a different openssl version no longer matches them, so a rebuilt image is scanned in full).

### OpenSSL 3.5.9 (L-831)

1.3.1, 2026-11-25. Owner decisions: Robert t244u-t246u (ship 1.3.1 now; accept the three new OpenSSL ids as an L-824
addendum; keep SFTP in the images and resolve what can be resolved with it enabled), t247u (the DHI refresh runs in CI).

**dhi-debian** (`libssl3t64`, `openssl`, `openssl-provider-legacy` **3.5.9-1~deb13u1+dhi2**). The refreshed base ships
+dhi1; the Dockerfile now runs `apt-get upgrade` before installing `openssh-client` (as the Alpine build already runs
`apk upgrade`), which installs +dhi2 from the DHI repo. Its `changelog.Debian.gz` (printed by the `base_refresh` probe)
backports the upstream fixes for eight of the nine ids. Debian's tracker, which grype and trivy follow, still lists
trixie (3.5.9-1~deb13u1) and trixie-security as vulnerable, so both scanners keep reporting them.

| CVE | grype / trivy | Status in the image | Note |
|---|---|---|---|
| CVE-2026-88823, CVE-2026-95691, CVE-2026-95692, CVE-2026-107493, CVE-2026-107496, CVE-2026-124709 (L-824 ids) | High / MEDIUM (124709: HIGH) | **resolved** by the DHI backport in +dhi2 | scanner false positives until Debian's tracker catches up |
| CVE-2026-107494, CVE-2026-121482 (new) | High / HIGH, MEDIUM | **resolved** by the DHI backport in +dhi2 | same |
| CVE-2026-84786 (new) | High (CNA: Low) / MEDIUM | **accepted** (Robert 2026-11-25) | DANE client NULL dereference, fixed upstream in 3.5.10; not in +dhi2. Only programs that use OpenSSL's DANE API with PKIX-TA/EE plus DANE-TA TLSA records are affected; OpenSSH does not use it (SSHFP is separate) and searchdump does not link OpenSSL |

**dhi-alpine** (`libssl3`, `libcrypto3` **3.5.9-r3**, the newest build in `dhi.io/apk/alpine/v3.24` and the Alpine 3.24
secdb on 2026-11-25): CVE-2026-107493, CVE-2026-107494, CVE-2026-121482, CVE-2026-124709, CVE-2026-84786 (grype High,
fix state unknown; trivy does not rate them High). **Accepted** by Robert 2026-11-25 until Alpine/DHI ship a fixed
openssl (upstream 3.5.10). Before 1.3.1 this file listed dhi-alpine as 0 findings; the 1.3.0-dhi-alpine image already
had these five (on 3.5.9-r2).

**Public `alpine`** (`make image-alpine`, `:<ver>-alpine`; `libssl3`, `libcrypto3` **3.5.9-r1** from Alpine 3.24 main after
`apk upgrade`, as shipped in 1.4.0 `sha256:ab761016…`): the same five ids, grype High with fix state unknown, trivy not
High. **Accepted** by Robert 2026-11-27 (t273u, L4-031) with the same rationale and removal trigger as dhi-alpine. Before
this, the alpine variant was scanned with the shared `docker/grype-ignore.yaml` and failed `make scan` on them.

- **Why accepted / not reachable:** `searchdump` and `searchdump-multi` use rustls (no `openssl-sys` in Cargo.lock).
  OpenSSL is in the images for the base OS tools and for the OpenSSH client that `sftp://` runs. The listed QUIC, CRL
  and DANE code paths are not used by `ssh`, which uses libcrypto for its own SSH ciphers and keys.
- **Scope of the ignores:** these ids, these packages, this exact version. grype: `docker/grype-ignore.dhi-debian.yaml`
  (version `3.5.9-1~deb13u1+dhi2`, not-fixed / wont-fix) and the new `docker/grype-ignore.dhi-alpine.yaml` (version
  `3.5.9-r3`, unknown / not-fixed / wont-fix); trivy: versioned purls in `docker/trivyignore.dhi-debian.yaml` and the
  new `docker/trivyignore.dhi-alpine.yaml`. For the public `alpine` variant (L4-031): `docker/grype-ignore.alpine.yaml`
  and `docker/trivyignore.alpine.yaml`, versions `3.5.9-r1` (shipped) and `3.5.9-r3` (the dhi-alpine build, in case a
  rebuild picks it up), unknown / not-fixed / wont-fix. Any other openssl build is scanned in full.
- **Removal trigger:** Debian marks the eight ids fixed (then grype stops matching and the rules are dead; delete them),
  or a fixed openssl (3.5.10 or a backport of CVE-2026-84786) reaches the DHI Debian or Alpine repos (or Alpine main,
  for the public `alpine` image). Rerun
  `security` with `base_refresh=true`, refresh the pins, rescan, delete the rules.

**What was tried to remove OpenSSL / Kerberos while keeping SFTP (1.3.1):**
1. *Fixed packages first.* Refreshed every dhi.io pin in CI (`base_refresh` job, dhi-pull login, no credentials outside
   CI) and added `apt-get upgrade`: this removed the glibc ids (deb13u5), all 15 Kerberos ids (deb13u2) and 8 of the 9
   OpenSSL ids (+dhi2). A DHI per-package record left in `/var/lib/dpkg/status.d/` by the base would otherwise make
   scanners report the old +dhi1 too; the Dockerfile drops such stale records.
2. *Install fewer dependencies.* `openssh-client` is already installed with `--no-install-recommends`. In trixie it
   hard-depends on `libssl3t64` (libcrypto) and `libgssapi-krb5-2`; GSS-API key exchange moved to the separate
   `openssh-client-gssapi`, but the client still links libgssapi for GSSAPI authentication, so Kerberos cannot be left
   out with the Debian package. No other ssh client package in the repos works: Dropbear's client is not compatible with
   OpenDAL's sftp backend (L-810i), `openssh-client-ssh1` is legacy.
3. *Drop the OpenSSL CLI.* `openssl` and `openssl-provider-legacy` are part of the dhi-debian base itself, not pulled
   in by ssh (and `ca-certificates` depends on `openssl`); removing them would not remove `libssl3t64`, which ssh needs,
   and the CVEs are scanned per source package, so nothing would change.
4. *Alpine.* `openssh-client-default` (already the krb5-free variant, `!openssh-client-krb5`) needs `so:libcrypto.so.3`;
   `libssl3` and `libcrypto3` come from the base. There is no OpenSSL-free OpenSSH build in Alpine.
5. *Not done in a patch release:* building OpenSSH `--without-openssl` (only ed25519 keys and chacha20/AES-CTR from
   OpenSSH's internal crypto: RSA/ECDSA keys and many servers would stop working), or a pure-Rust SFTP client. Both are
   1.4 candidates; the SSH-free default image was ruled out by Robert (t246u).

`sftp://` was verified on both rebuilt images by `scripts/sup/image-sftp-smoke.sh` in the `base_refresh` job: local ->
sftp, sftp -> local (500 documents identical), and a `searchdump-multi` tree copy to sftp, through the image's own `ssh`
(`OpenSSH_10.0p2 Debian-7+deb13u4+dhi3` and `OpenSSH_10.3p1`, both OpenSSL 3.5.9).

### Debian 13 public fallback (`make image-public`)

- All 157 grype matches are Debian `wont-fix` or `not-fixed`, so none can be fixed by upgrading packages.
- The HIGH/CRITICAL findings are listed below (15 distinct CVEs, grouped by CVE).
- They come from the base OS (util-linux, glibc, gcc runtime libs, ncurses, perl-base, zlib, acl, systemd libs). They do not come from searchdump.
- The binary links only glibc (libc, libm) and libgcc_s. Even so, glibc CVE-2026-19499 and CVE-2026-5435 cannot be ruled out for a dynamically linked program, which is why this base is not recommended.
- Status: **open on this base**, and resolved by choosing another base: the Alpine/musl build or the dhi.io base.

| ID | Packages | Severity | Status | Resolution |
|---|---|---|---|---|
| CVE-2025-69720 | libtinfo6, ncurses-base, ncurses-bin | grype High / trivy HIGH | open on this base (Debian: wont-fix / affected); not present in the Alpine image | no Debian fix; avoided by `make image-alpine` (or dhi.io base) |
| CVE-2026-102010 | gcc-14-base, libgcc-s1, libstdc++6 | grype High / trivy MEDIUM | open on this base (Debian: wont-fix / affected); not present in the Alpine image | no Debian fix; avoided by `make image-alpine` (or dhi.io base) |
| CVE-2026-16742 | libsystemd0, libudev1 | grype Medium / trivy HIGH | open on this base (Debian: wont-fix / affected); not present in the Alpine image | no Debian fix; avoided by `make image-alpine` (or dhi.io base) |
| CVE-2026-19499 | libc-bin, libc6 | grype High / trivy MEDIUM | open on this base (Debian: wont-fix / affected); not present in the Alpine image | no Debian fix; avoided by `make image-alpine` (or dhi.io base) |
| CVE-2026-5435 | libc-bin, libc6 | grype High / trivy MEDIUM | open on this base (Debian: wont-fix / affected); not present in the Alpine image | no Debian fix; avoided by `make image-alpine` (or dhi.io base) |
| CVE-2026-54369 | libacl1 | grype High / trivy HIGH | open on this base (Debian: wont-fix / affected); not present in the Alpine image | no Debian fix; avoided by `make image-alpine` (or dhi.io base) |
| CVE-2026-54370 | libacl1 | grype High / trivy MEDIUM | open on this base (Debian: wont-fix / affected); not present in the Alpine image | no Debian fix; avoided by `make image-alpine` (or dhi.io base) |
| CVE-2026-76642 | bsdutils, libblkid1, liblastlog2-2, libmount1, libsmartcols1, libuuid1, login, mount, util-linux | grype High / trivy HIGH | open on this base (Debian: wont-fix / affected); not present in the Alpine image | no Debian fix; avoided by `make image-alpine` (or dhi.io base) |
| CVE-2026-78408 | bsdutils, libblkid1, liblastlog2-2, libmount1, libsmartcols1, libuuid1, login, mount, util-linux | grype High / trivy HIGH | open on this base (Debian: wont-fix / affected); not present in the Alpine image | no Debian fix; avoided by `make image-alpine` (or dhi.io base) |
| CVE-2026-78409 | bsdutils, libblkid1, liblastlog2-2, libmount1, libsmartcols1, libuuid1, login, mount, util-linux | grype High / trivy HIGH | open on this base (Debian: wont-fix / affected); not present in the Alpine image | no Debian fix; avoided by `make image-alpine` (or dhi.io base) |
| CVE-2026-78410 | bsdutils, libblkid1, liblastlog2-2, libmount1, libsmartcols1, libuuid1, login, mount, util-linux | grype High / trivy HIGH | open on this base (Debian: wont-fix / affected); not present in the Alpine image | no Debian fix; avoided by `make image-alpine` (or dhi.io base) |
| CVE-2026-82560 | perl-base | grype High / trivy UNKNOWN | open on this base (Debian: wont-fix / affected); not present in the Alpine image | no Debian fix; avoided by `make image-alpine` (or dhi.io base) |
| CVE-2026-85091 | zlib1g | grype High / trivy MEDIUM | open on this base (Debian: not-fixed / affected); not present in the Alpine image | no Debian fix; avoided by `make image-alpine` (or dhi.io base) |
| CVE-2026-9538 | perl-base | grype High / trivy HIGH | open on this base (Debian: wont-fix / fix_deferred); not present in the Alpine image | no Debian fix; avoided by `make image-alpine` (or dhi.io base) |
| CVE-2026-95619 | gcc-14-base, libgcc-s1, libstdc++6 | grype High / trivy MEDIUM | open on this base (Debian: wont-fix / affected); not present in the Alpine image | no Debian fix; avoided by `make image-alpine` (or dhi.io base) |

## Code and supply-chain findings (SUP-018)

These are the review findings in TEST-REQUIREMENTS.md §7.3 (F-01…F-14) and §8 (INJ-F1…F4). Each one has a regression test or lint that fails on the code before the fix. Status values are the same as above. Workflow fixes are lint-verified only, because this fork is never pushed and the workflows cannot run here.

| ID | Finding (short) | Status | Fix commit | Regression test / lint |
|---|---|---|---|---|
| INJ-F1 | Raw CR/LF in `_source` split one hit into several NDJSON/bulk lines (forged `_bulk` actions); reproduced live on ES 7.17.29 and OpenSearch 2.19.7 | resolved | 9d344de | `tests/injection.rs` `inj_f1_*` (5); `scripts/poc/inj-f1.sh` (live PoC, `results/local/L-101.json`) |
| INJ-F2 | Index names (`--output-index`, `_index`, rename, wildcards) reached URL paths unvalidated; `.`/`..` segments vanished | resolved | 202c683 | `inj_f2_*` (3); unit `url_segments_never_vanish` |
| INJ-F3 | `_id`/routing/parent/version copied into action lines without type checks | resolved | 7bfea42 | `inj_f3_*` (2) |
| INJ-F4 | `--parseExtraFields` names written raw into action metadata | resolved | 5ad4579 | `inj_f4_*` (2) |
| (L-105) | Duplicate top-level keys, non-object `_source`, header CR/LF, query encoding | resolved (Q13: duplicates are **rejected**) | 18b3a0b | `guards_*` (2); units `duplicate_keys_are_detected`, `raw_control_bytes_are_rejected`, `query_values_are_encoded_exactly` |
| F-01 | Scanners installed with `curl \| sh` from `main` | resolved | 78c2446 | `scripts/lint-workflows.sh` (grep for `\| sh`/`main/install`; SHA-256 checks present) |
| F-02 | Actions pinned to mutable tags | resolved | 78c2446 | `scripts/lint-workflows.sh` (every `uses:` is a 40-hex SHA), zizmor `unpinned-uses`; `.github/dependabot.yml` |
| F-03 | No `permissions:`; checkout persisted credentials | resolved | 78c2446 | `scripts/lint-workflows.sh` (`permissions:` in every workflow, `persist-credentials: false`), zizmor `artipacked`/`excessive-permissions` |
| F-04 | Needed fixture ignored by `*.jsonl` | resolved | 17b16d7 | `scripts/lint-hygiene.sh` F-04 (no ignored files under `tests/fixtures`) |
| F-05 | `--debug` logged full URL incl. `--params` values | resolved | dfc044f | `tests/secrets.rs` `debug_log_masks_params_and_credentials`; unit `debug_url_masks_params_and_password` |
| F-06 | Unparsable endpoint logged raw; scheme-less `user:pw@host` silently a file | resolved | dfc044f | `tests/secrets.rs` `schemeless_url_is_masked_and_warned` |
| F-07 | Env proxies used silently | resolved (warn + `--noProxy`) | dfc044f | `tests/secrets.rs` `env_proxy_is_visible_and_can_be_disabled` |
| F-08 | `#[derive(Debug)]` on secret-holding structs | resolved | dfc044f | units `client_options_debug_is_redacted`, `debug_never_prints_secrets` (Cli, TlsOptions, EsEndpoint) |
| F-09 | Dispatch input interpolated into shell | resolved | 78c2446 | `scripts/lint-workflows.sh` (no `${{ github.event.inputs` in `run:`), zizmor `template-injection` |
| F-10 | `cargo install cargo-zigbuild` unpinned; Zig fetched unpinned | resolved | 78c2446 | `scripts/lint-workflows.sh` (`cargo install` needs `--locked --version`) |
| F-11 | Tests wrote `test_query_*.json` into the CWD | resolved (code); the two committed junk files are left in place at the owner's request | 17b16d7 | `scripts/check-clean-tree.sh` (tree unchanged after the suites); `scripts/lint-hygiene.sh` F-11 |
| F-12 | compose published 9200 on all interfaces, security off | resolved | 17b16d7 | `scripts/lint-hygiene.sh` F-12 |
| F-13 | Fixed IT passwords in world-readable `/tmp` | resolved (OpenSearch < 2.12 keeps `admin/admin`: its demo config requires it) | 17b16d7 | `scripts/lint-hygiene.sh` F-13; IT runs on all four profiles |
| F-14 | Images pulled by tag | resolved | 17b16d7 | `scripts/lint-hygiene.sh` F-14 (Makefile, Dockerfile, compose, `scripts/it/images.lock`) |

The stray `oom` file: no code path in `src/`, `tests/` or `scripts/` writes it (no such literal in any commit). The committed copy predates the first commit by 6 seconds, so it came from an ad-hoc shell redirect. `scripts/check-clean-tree.sh` catches any test that writes into the tree.

## Decisions and follow-ups

- **Default base.** Since L-808b the default is the glibc + mimalloc build on `dhi.io/static:20250419-glibc-debian13` (`make image`, `scripts/build-image.sh`), chosen by the user for glibc; it has 5 grype-High findings in glibc / libgcc-s with no fix, accepted by the user on 2026-11-01 (L-808d) as known and open until upstream ships fixes; `make scan` ignores exactly those five ids while their fix state is `not-fixed`/`wont-fix` (`docker/grype-ignore.yaml`), so a published fix fails the scan again. The dhi.io Alpine build (`make image-dhi-alpine`, `:<ver>-alpine`) scans clean and stays available. CI needs the repository secrets `DHI_USERNAME`/`DHI_TOKEN` (pull-only token, not committed). Without them, dispatch with `base=alpine`. `make image-dhi-debian` and `make image-public` are kept, but they fail `make scan`.
- **musl vs glibc.** The default binary and image are glibc + mimalloc (L-808b); the release workflow also builds static musl binaries, and the Alpine image is musl. The full integration matrix runs the host (glibc) build; both images were verified with `scripts/test-image.sh` and a dump/restore against OpenSearch 2.19.7 (L-808b for the glibc image).
- **MSRV.** No advisory needed an MSRV bump once the `elasticsearch` dev-dependency was removed. If `time` comes back into the tree, its fix (0.3.47) needs Rust 1.88.
- **Image visibility into the binary's crates.** Building with `cargo auditable` would let grype and trivy see the crates inside the binary. It is not adopted yet; cargo-audit on the committed Cargo.lock covers those crates instead.
