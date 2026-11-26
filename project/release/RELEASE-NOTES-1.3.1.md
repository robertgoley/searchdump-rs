# searchdump-rs 1.3.1 release notes

Status: **released 2026-11-25** (tag `v1.3.1`). Security patch on top of 1.3.0; command-line behaviour, sizing and
output formats are unchanged. The full list is in [`CHANGELOG.md`](../../CHANGELOG.md) under **1.3.1**; image CVE
details are in [`SECURITY-CVE.md`](../../SECURITY-CVE.md#openssl-359-l-831).

## Disclaimer

searchdump-rs is provided **as is, without warranty of any kind**, as set out in its
[MIT license](../../LICENSE). See [Disclaimer](../../README.md#disclaimer). Try it on a non-production cluster first
and verify every backup independently (restore it, compare counts) before relying on it.

## Upgrade

- Binaries: replace the 1.3.0 tarball with `searchdump-rs-1.3.1-<target>.tar.gz` (checksums and provenance as before).
- Images: `ghcr.io/robertgoley/searchdump-rs:1.3.1` (= `:1.3.1-dhi-debian` = `:latest`), or the other types
  (`:1.3.1-dhi-alpine`, `:1.3.1-dhi-static`, ...). SFTP (`sftp://`) works as in 1.3.0 in every image except
  `dhi-static`.

## What changed

- **Rust dependencies:** `url` 2.5.9 fixes RUSTSEC-2026-0592 (not reachable from searchdump, but `cargo audit` flagged
  it); TLS patch bumps (`rustls-webpki` 0.103.17, `webpki-roots` 1.0.12); `base64` 0.23.1.
- **Default image (`dhi-debian`):** new base digest with glibc deb13u5 and Kerberos deb13u2 (17 High ids fixed), and
  openssl +dhi2 from Docker Hardened Images with backports for 8 OpenSSL ids. Remaining accepted: CVE-2026-84786 in
  OpenSSL (DANE clients only; not used by ssh or searchdump) plus the base-OS and OpenSSH ids already accepted in
  1.2/1.3.
- **`-dhi-alpine` image:** new base digest; five OpenSSL ids in libssl3/libcrypto3 3.5.9-r3 are accepted until Alpine
  ships a fixed build.
- **CI:** dhi.io bases are refreshed and verified by a CI job (`security`, `base_refresh=true`), including an
  `sftp://` round trip through each image's own OpenSSH client.
