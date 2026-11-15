# Supply-chain controls

How searchdump implements the SUP-* requirements (TEST-REQUIREMENTS.md §7.3).
Each section names the gate that enforces it and how to run it locally.

## cargo-vet (SUP-004, L-201)

- Config: `supply-chain/config.toml`, `audits.toml`, `imports.lock`.
- Imported audit sets: Mozilla, Google, Bytecode Alliance, Embark, ISRG, Zcash.
- Baseline (2026-10-29): 61 crates fully audited by imports, 2 partially, 207 exempted.
  Every exemption carries a note (`L-201 baseline …`). A new crate or a new version
  of an exempted crate fails `cargo vet` until it is audited
  (`cargo vet certify`) or an imported audit covers it.
- Gate: CI job `supply-chain`, step `cargo vet --locked`.
- Local: `cargo vet --locked`; refresh imports with `cargo vet regenerate imports`.
- Shrinking the exemption list is ongoing work: `cargo vet suggest` lists the
  cheapest audits.

## cargo-deny (SUP-005, L-202)

`deny.toml`, checked by `cargo deny check advisories bans licenses sources`
(workflow `security`). The graph is limited to the shipped Linux targets.

- **Sources:** crates.io only; unknown registries and git sources are denied.
- **Duplicates:** `multiple-versions = "deny"`. The skip list has three
  entries, each with the crate that pulls in the old version: base64 0.22,
  hashbrown 0.14 and getrandom 0.2, plus the OpenDAL set (L-809a, see below).
- **Banned crates:** OpenSSL and native-tls, libgit2, curl, other HTTP clients,
  and `libz-sys`.
- **Build scripts** (`[bans.build]`): only the 29 crates listed under
  `allow-build-scripts` may have a `build.rs`. Each has a one-line reason;
  `ring` and `zstd-sys` are the only ones in the shipped (default) build that
  compile native code. `tikv-jemalloc-sys` and `libmimalloc-sys` (L-802a) also
  compile C, but only with the optional allocator features (see "Optional
  allocators" below). Any other
  crate with a build script fails the check until it is reviewed and added.
- **Executables and scripts:** `executables = "deny"` and `interpreted = "deny"`,
  checked in the allowed crates and their dependencies. Maintainer scripts that
  the build never runs are bypassed by exact version: `untrusted` `mk/*`, the
  `ring` perlasm generators, and the `zerocopy` CI scripts. The `ring` build
  only runs perl when built from a git checkout; the crates.io package ships
  pregenerated assembly. `tikv-jemalloc-sys` is bypassed too, but its build
  does run shell: it calls jemalloc's autoconf `configure` and `make`
  (optional feature `jemalloc` only).

Negative check (2026-10-29): adding `libz-sys` and `num-traits` reports
`banned`, two `build-script-not-allowed` errors and `detected-executable-script`.

## Unsafe inventory (SUP-006, L-203)

- `#![forbid(unsafe_code)]` is set in `src/lib.rs` and `src/main.rs`; the crate has no unsafe code.
- `scripts/sup/geiger-gate.sh` runs `cargo geiger` (Linux graph) and compares the result with
  `supply-chain/geiger-baseline.json`. It fails if:
  - our crate stops forbidding unsafe;
  - any crate's used-unsafe count rises;
  - a new crate brings unsafe code;
  - the total rises.

  After a reviewed dependency change, refresh the baseline with `--update`.
- Baseline (2026-10-29): 27,289 used-unsafe items in 111 of 191 crates. The largest counts are
  hashbrown, zlib-rs, tokio, memchr and sonic-rs.
- Known gap: cargo-geiger 0.14's parser cannot read `signal-hook-registry` 1.4.8 (Rust syntax it
  does not support), so that crate is reported as 0.
- CI: the `supply-chain` job, which needs nightly to build cargo-geiger.

## Test-only tools

`tools/edverify` and `tools/edgen` are workspace members, but they are never shipped: `default-members` is the root crate, and the image builds only `searchdump`. In cargo-vet they have `criteria = "safe-to-run"` (`[policy.edgen]`, `[policy.edverify]`), so `rand`, `rand_chacha` and `ppv-lite86` need only the safe-to-run level they already had as dev-dependencies.

## Unused dependencies and enabled features (SUP-007, L-204)

- `cargo machete` runs in the `supply-chain` job on every PR and push. `rust-ini` is in
  `[package.metadata.cargo-machete] ignored`: it is imported as `ini`, which machete does not match to the package
  name. `cargo +nightly udeps --workspace --all-targets --features searchdump-rs/load` runs weekly in
  `security.yml`; it must compile the `it`/`load` tests, otherwise `lipsum` (used only by
  `tests/integration_test.rs`) is reported as unused. L-204 removed `thiserror`, which was declared but never used.
- No `cargo hack` on the box. The crate's own features (`it`, `load`, `chaos`, `fuzz`, and the deprecated aliases
  `large_scale_test` and `manual_test`) gate tests only; none of them changes the shipped binary. The one
  shipped feature is `encrypted-keys` (default; SUP-023).

Every non-default feature enabled in `Cargo.toml` is listed below with its reason.

| Crate | Feature | Why |
|---|---|---|
| tokio | `rt-multi-thread`, `macros` | `#[tokio::main]` multi-thread runtime; `tokio::select!` in the pipeline and signal handling |
| tokio | `fs`, `io-util`, `io-std` | async file input/output; stdin/stdout as `$` endpoints |
| tokio | `sync`, `time` | channels and semaphores between reader and writers; retry backoff and throttling |
| tokio | `signal` | SIGINT/SIGTERM handling (staging-file cleanup) |
| tokio | `process` | `searchdump-multi` awaits its child `searchdump` processes and stops them on Ctrl-C (1.1-8); tokio's own module, no new crate |
| reqwest | `gzip` | transparent decompression of gzip-encoded responses |
| reqwest | `rustls-tls-manual-roots` | rustls only, and we choose the trust roots (`--ca`, native, webpki); no OpenSSL (deny.toml bans it) |
| rustls | `ring`, `std`, `tls12` | crypto provider without aws-lc/cmake; TLS 1.2 for older clusters |
| rustls | `logging` | rustls handshake diagnostics under `RUST_LOG` (header values are marked sensitive; see H-10) |
| pkcs8 (optional) | `encryption`, `pem`, `std` | encrypted PKCS#8 client keys (`--key` with a passphrase); only with the crate feature `encrypted-keys` (default on) |
| clap | `derive`, `wrap_help` | CLI definition; readable `--help` |
| serde | `derive` | config and wire types |
| serde_json | `preserve_order`, `arbitrary_precision` | byte-faithful `_source` (key order, big integers and decimals unchanged) |
| flate2 | `zlib-rs` (no default) | pure-Rust deflate; no `libz-sys` (banned) |
| zstd | no default features | no legacy/zdict builder; the `zstd-sys` C code is on the native allow-list |
| regex | `std`, `unicode-perl`, `unicode-case` (no default) | template/rename patterns with `\w`/`\d` and `(?i)`; the rest of the Unicode tables and the perf features are off |
| tikv-jemallocator (optional) | default | jemalloc as the global allocator; only with the crate feature `jemalloc` (off by default; L-802a) |
| mimalloc (glibc Linux: always; elsewhere optional) | no default features | mimalloc (bundled C, reports v3.3.2 at run time) as the global allocator: the default on `*-linux-gnu` since L-808b (opt out with the feature `system-allocator`); on other targets only with the feature `mimalloc` |

## Build-time code (SUP-009, L-206)

### Inventory

`supply-chain/build-inventory.json` lists every package that runs code at build time: build scripts and proc macros.

- **All of Cargo.lock:** 45 build scripts and 18 proc macros.
- **Shipped graph:** normal and build dependencies of `searchdump` on x86_64/aarch64 Linux, 26 build scripts and 10 proc macros. This is the feature-resolved graph from `cargo tree`; L-213 refined the over-approximated metadata closure, which had given 32/14.

Each entry is pinned as `name@version` and carries a review note.

### Gate

`scripts/sup/build-inventory.py` runs on every PR. It fails when a build script or proc macro is new to the shipped graph, or when such a package changes version. After review, refresh the baseline with `--update`.

### Network-less build

`scripts/sup/offline-build.sh` builds without network access:

1. It vendors the dependencies on the host. This is the only step that downloads anything.
2. It runs `cargo build --offline --locked --release` in `podman run --network none`, with the sources and the vendor directory mounted read-only.
3. Before building, a probe inside the sandbox checks that DNS and TCP really fail.

Verified locally on 2026-10-29 with `rust:1.99-trixie`: the probe showed the network unreachable, and the build succeeded in 2m01s. No build script needs the network.

## Egress-denied test runs (SUP-001, L-701)

`scripts/sup/egress.sh` runs the suites with no way off the box and fails on any attempt to
leave it. Everything runs in one rootless pod created with `podman pod create --network none`
(label `searchdump-it=1`): its network namespace has loopback only. Nothing on the host
changes: no firewall rules, routes or other containers are touched.

* A logging DNS sink (`scripts/sup/egress-sink.py`) listens on 127.0.0.1:53 and [::1]:53 in
  that namespace, answers every query with NXDOMAIN and logs the name. Test processes run
  through `scripts/sup/netns-exec.sh POD CMD`, which enters the namespace (`podman unshare
  nsenter`) with a private mount of `/etc/resolv.conf` pointing at the sink; the node and the
  image container get the same file as a volume, plus a hosts file naming the pod's hostname
  (`--add-host` is ignored with `--network none`).
* The namespace's `Ip:OutNoRoutes` / `Ip6OutNoRoutes` counters count every attempt to reach a
  non-loopback address (TCP, UDP, or a DNS query sent to an outside resolver).

Phases, each counted on its own: `T1-unit`, `T1-contract`, `T1-plant`
(`tests/egress_plant.rs`), `T7-chaos` (the mock-based chaos profile), `node-start` (the
cluster from `scripts/it/up.sh` started inside the pod with `IT_POD`), `T2-it` (the IT suite
against it, default `--profile opensearch-plain`) and `T2-image` (the built
`localhost/searchdump-rs:<version>` image dumping an index to a file and restoring it).
A phase fails when its command fails, when it made a connection attempt off loopback, or
when it asked DNS for any name other than `localhost` and the reserved `*.invalid`, `*.test`,
`*.example` and `*.localhost` names that tests use to provoke resolution errors
(`EGRESS_ALLOW_DNS` overrides that regex). The summary lines are
`EGRESS phase=<name> rc= unexpected_dns= noroute= names=[...] OK|FAIL` in
`$EGRESS_LOG_DIR/egress-summary.log` (default `$IT_DIR/egress/`).

The plant: `EDRS_EGRESS_PLANT=dns` makes `tests/egress_plant.rs` resolve `example.com`,
`EDRS_EGRESS_PLANT=tcp` makes it connect to 192.0.2.1. The test itself passes; the job must
fail. `scripts/sup/egress.sh --selftest` runs both plants and a clean control and passes only
when both plants are caught and the control is clean. Nightly (`nightly.yml`, job `egress`)
runs the self-test, then `--tiers T1,T7,T2`. The job reads the pull-only `DHI_USERNAME`/`DHI_TOKEN` secrets
through the `dhi-pull` environment (zizmor `secrets-outside-env`). Repository-level secrets
stay visible to it until they are moved into that environment.

Not covered: the `--features chaos` toxiproxy tests and cross-engine runs need a second
container or proxy reachable from the namespace; they stay in their own nightly jobs.

## Syscall and file-access tracing (SUP-002, L-702)

`scripts/sup/trace.sh` runs a command matrix under
`strace -ff -y -s 0 -e trace=%file,%network,%process` and checks every traced syscall
against the reviewed allow-list `scripts/sup/trace-allow.txt` with
`scripts/sup/trace-check.py`. The matrix runs against single-node clusters from
`scripts/it/up.sh`:

* `plain` (`opensearch-plain`): dump data, mapping, settings, alias, analyzer and template
  to files; split output (`--fileSize`, `--maxRows`); gzip output; output to stdout; restore
  settings, mapping and data from files; gzip restore; restore from stdin; cluster-to-cluster
  copy; a failing run (missing index).
* `tls` (`opensearch-secure`): dump with `--input-ca` and `--httpAuthFile`, dump with a client
  certificate (`--input-cert`/`--input-key --tlsAuth`), and restore over TLS.

Each run starts from the work directory with `env -i PATH HOME LANG`. `HOME` is a directory
of its own that the allow-list does not cover, so reading a dotfile would show up. The checker
turns each syscall into an entry and rewrites it with placeholders: `@BIN`, `@WORK` (input,
output dir, `.part` staging files, auth file), `@CA`/`@CERT`/`@KEY`/`@KEYENC` and `@PORT`.
The entry kinds are:

* `exec PATH`
* `path PATH`: every path argument of a `%file` syscall, made absolute through the start
  directory or the `-y`-decoded dirfd. Failed probes count too.
* `connect ADDR`: also `sendto`/`sendmsg` with an address.
* `bind ADDR`
* `socket FAMILY/TYPE`
* `process fork`: a clone without `CLONE_THREAD`.
* `syscall NAME`

Each rule is `KIND PATTERN  # reason`, where `*` and `?` are the only wildcards. Entries that
no rule matches are the allow-list diff (`+ KIND VALUE`), and the run fails unless that diff
is empty. A new path, address or child process therefore needs a reviewed change to
`trace-allow.txt`.

What the list allows today:

* Process: the one `execve` of the binary, and threads only.
* Network: TCP to the cluster port on 127.0.0.1 / ::1. Also glibc's `getaddrinfo("localhost")`:
  * the nscd socket probe;
  * a netlink RTM_GETADDR to the kernel;
  * UDP connects to port 0 for RFC 6724 source-address selection. No datagram is sent.
* Files:
  * the user-named files and the work directory;
  * the system CA store locations that rustls-native-certs probes;
  * `/etc/{hosts,resolv.conf,nsswitch.conf,host.conf,gai.conf,localtime}` and a `stat("/")`
    by glibc NSS;
  * dynamic-loader paths;
  * `/proc/self/*` and `/proc/meminfo` (memory guard);
  * `/proc/sys/vm/overcommit_memory` (glibc malloc `check_may_shrink_heap`, system-allocator builds);
  * `/sys/devices/system/node/node*` and `/sys/kernel/mm/transparent_hugepage/enabled`
    (mimalloc init: NUMA node count and THP mode; the default allocator on glibc since L-808b);
  * CPU-count and cgroup v2 `cpu.max`/`memory.max`/`cgroup.controllers` files.

Not on the list, so each of these fails the run: a DNS query, any other address or port, a
listening socket, a child process, `$HOME` and every other path.

`-s 0` keeps socket buffers out of the traces: request bytes, which may carry an
Authorization header, show as `""...`. Pathnames are printed in full regardless.

`scripts/sup/trace.sh --selftest` runs two sets of checks:

* `trace-check.py --selftest`: planted strace lines for each entry kind must be caught and a
  clean trace must pass.
* Live plants against the plain node, each of which must fail while the control passes:
  * the binary started through `sh -c`, which adds an `execve`, a fork and `wait4`;
  * `--input` pointing at a port that is not the cluster's;
  * the control trace checked against the list without its `/proc/meminfo` rule.

Output goes to `${TRACE_LOG_DIR:-$IT_DIR/trace}/`: `trace-summary.log` with
`RUN`/`TRACE case=... diff=N OK|FAIL`/`TRACE_DONE` lines, `observed.tsv` (every rewritten
entry per case), and `<case>/{strace.*,stdout,stderr,check.log}`. The runner needs host
`strace` (6.13 here). Tracing an own child process needs no extra privilege. Nightly
(`nightly.yml`, job `trace`) runs the self-test, then the full matrix.

## Reproducible builds (SUP-010, L-704)

`scripts/sup/repro.sh [image@sha256:…]` builds the release binary twice from the same commit and
fails unless both have the same SHA-256. The default image is the Makefile's pinned builder
(`BUILDER_IMAGE:BUILDER_TAG`, dhi.io Rust on Alpine, by digest); an image given without a digest is
refused. Like `offline-build.sh`, the sources are vendored on the host (`cargo vendor --locked`) and
both builds run in `podman run --network none` with `cargo build --offline --locked --release`.

What the two builds share: the image digest, `SOURCE_DATE_EPOCH` (the commit time),
`CARGO_INCREMENTAL=0`, and `RUSTFLAGS` with `--remap-path-prefix` for the source, vendor and
`CARGO_HOME` directories (mapped to `/build/src`, `/build/vendor`, `/build/cargo-home`).

What differs between them, and must not reach the binary:

| | build a | build b |
|---|---|---|
| user | container root | uid/gid 4242 |
| sources | `/src` | `/home/builder/work/deeper/searchdump` |
| vendor / `CARGO_HOME` / target | `/vendor`, `/cargo-home`, `/target` | `/opt/v2/vendor`, `/var/tmp/ch2`, `/home/builder/tgt` |
| umask, TZ, hostname | 022, UTC, `repro-a` | 077, Asia/Kolkata, `repro-b` |
| start time | t | after build a plus `REPRO_PAUSE` (5 s) |

On a mismatch the script prints the first differing byte offsets and the build-path strings that
differ. `REPRO_OUT=<dir>` keeps both binaries, both build logs and a `SHA256SUMS`; `REPRO_PULL=1`
pulls the image when it is missing.

Result on this box (2026-10-31, commit 4680a58, dhi.io/rust 1.99.0-alpine3.23-dev@sha256:2d438711…):
both binaries `0c5775a3…61c61e8`, 8,419,208 bytes, identical. `strings` finds none of build b's
own paths in its binary, only the remapped `/build/…` prefixes. About 3 minutes per build at
`CARGO_BUILD_JOBS=2`.

CI: the nightly `repro` job runs it with `REPRO_PULL=1` (dhi.io login through the `dhi-pull`
environment, as the egress job does) and uploads `/tmp/edrs-repro/`.

Not covered here:
- **A second machine:** cross-machine bit-for-bit verification is B11.
- **Image builds with `podman build --timestamp`:** both builds would need the dhi.io base and two
  more LTO builds. This is left for the image-hardening work and not checked yet.
- **`cargo auditable`:** the tool is not installed on this box or in the builder image, and
  installing it in the offline sandbox needs a vendored copy. Until it is added, the embedded
  dependency list comes from the SBOM job (SUP-011) instead.

## Release-candidate behaviour diff (SUP-016, L-705)

For every release candidate, `scripts/sup/rc-diff.sh` runs SUP-002 (`trace.sh`) and SUP-001
(`egress.sh --tiers T1,T7,T2`). It condenses the two runs into a **behaviour profile** with
`scripts/sup/rc-profile.py build`, then diffs that profile against the previous release's
profile, which is the highest-versioned `results/profiles/*.json` (or `--baseline`).

The profile is JSON (schema 1) with these parts:
- `trace.entries`: every distinct `kind<TAB>value` from the trace's `observed.tsv`. Kinds are exec, path,
  connect, bind, socket, process and syscall, with the trace's placeholders (`@BIN`, `@WORK`, `@PORT`, …).
- `trace.cases`: the same entries listed per command case, used for context in the report.
- `egress.dns`: every name the processes tried to resolve.
- `egress.phases`: rc, no-route count and unexpected names per egress phase.

Run-specific values are normalised so that two runs of the same code give the same profile:
- staging-file pid/nanos (`.part-*`)
- files inside the system CA directories (these change with the ca-certificates package)
- the host's cgroup path (`/sys/fs/cgroup/*/memory.max`)
- numeric suffixes of pod names

The diff fails (exit 1) on any **addition** that no line in `scripts/sup/rc-signoff.txt` covers.
An addition is one of:
- a new trace entry: a new syscall class, path, socket kind, connect target, exec or child process
- a new DNS name
- an egress phase with more no-route attempts or new unexpected names than before

Removals are listed but pass. A sign-off line is `kind<TAB>value-glob<TAB>reason`, and a
line without a reason is rejected. Each rule covers one release step: once the new baseline
is committed, the file is cleared.

Artefacts in `$RC_DIFF_DIR` (default `$IT_DIR/rc-diff`): `profile.json`, `diff.md`, `diff.json`,
`rc-diff-summary.log`, and the trace/egress logs. `--trace-dir/--egress-dir` builds the profile
from runs that already happened (for example downloaded nightly `trace` and `egress` artefacts).
`--write-baseline` stores the profile as `results/profiles/<release>.json`, which is committed
with the release.

`rc-profile.py --selftest` plants each kind of change and checks the outcome:
- caught: a new syscall, path, exec, connect target and DNS name, and a phase with more no-route attempts and unexpected names
- passes: identical profiles, and a removal
- also checked: a signed-off change passes, a rule of the wrong kind does not cover a change, a rule without a reason is refused, and the normalisation works

CI: the `behaviour-diff` job in `release.yml` (manual dispatch, the same trigger as the release
build) runs the self-test and `rc-diff.sh`, adds `diff.md` to the job summary, and uploads `rc-diff`.

Entries that only some runs of the same code show are listed in `VOLATILE` in `rc-profile.py`,
each with its reason. They are reported as `volatile` and need no sign-off. Today the list has
one entry: `/proc/sys/vm/overcommit_memory`, which glibc reads when a thread arena shrinks.

Baseline `results/profiles/v0.1.0.json` (2026-10-31): the binary code is that of commit fdab304.
The profile comes from one full `rc-diff.sh` run on this box (`trace.sh` OK; `egress.sh` T1, T7, T2
OK with all 7 phases at 0 unexpected queries and 0 no-route attempts). It has 98 trace entries in
20 cases and one DNS name (`no-such-host.invalid`, from a test).

Stability checks:
- A second trace run of the same tree differs only in the volatile entry.
- The L-702 trace of commit 948f73f (an earlier run in another work directory) gives the same profile with 0 changes.

The T2 image phase used the local image built at 16:03 EDT on 2026-10-31, before L-642b. The
image's own process is not traced, and its egress counters were 0.

Limits:
- The profile covers only the trace and egress command matrices. A new code path that those matrices do not run cannot show up in it.
- `@WORK` output paths are part of the profile, so changing the trace matrix itself (new cases, different split counts) shows up as a diff and needs a new baseline.
- Profiles from a different runner image (other glibc, other library paths) can differ in loader paths. Compare candidates against a baseline made on the same runner image.

## SBOM diff per PR (SUP-011, L-207)

`scripts/sup/sbom-pr.sh <base-ref>` checks the base out into a temporary worktree and compares it with HEAD. It builds the following for both sides:

- a CycloneDX SBOM of the Linux graph, with `cargo cyclonedx --target x86_64-unknown-linux-gnu`;
- the build-time inventory.

`scripts/sup/sbom-diff.py` then reports:

- crates added, removed, upgraded or downgraded;
- licence changes;
- new build scripts and proc macros;
- image package changes, when `IMAGE_SBOM_BASE`/`IMAGE_SBOM_HEAD` point to syft SBOMs.

The `sbom-diff` job in `security.yml` runs on every PR. It attaches the diff as an artefact and adds it to the job summary. A non-empty diff fails the job until the PR carries the `sbom-reviewed` label.

Not covered: unsafe deltas. `scripts/sup/geiger-gate.sh` in the `supply-chain` job catches those.

## Native-code allow-list (SUP-021, L-213)

`scripts/sup/native-code-check.py` runs in the `supply-chain` job. It inspects the feature-resolved Linux graph of the shipped binary and flags any crate that has:

- a `links =` key;
- a `cc`, `cmake`, `bindgen` or `pkg-config` build dependency;
- C, C++ or assembly sources.

Any flagged crate that is not on the allow-list fails the job. The allow-list is `ring` (BoringSSL-derived C/asm), `zstd-sys` (vendored libzstd) and, since L-808b, `libmimalloc-sys` (vendored mimalloc C, MIT; the default global allocator on glibc Linux, which puts it in the default graph of the `x86_64`/`aarch64` gnu targets).

The check covers the default feature set. The optional `jemalloc` feature adds
`tikv-jemalloc-sys` (jemalloc 5.3.1, vendored C, BSD-2-Clause), which would be flagged and must
be added to the allow-list before it is turned on by default. `libmimalloc-sys` (vendored
mimalloc C, MIT) was added to the allow-list in L-808b, when mimalloc became the glibc default.

Verified: adding `libz-sys` fails the check. `deny.toml` also bans it, so `cargo deny` fails as well.

## Secret scanning and canaries (SUP-015, L-210)

`scripts/sup/secret-scan.sh` runs three checks:

1. gitleaks 8.28.0 over the whole git history, or only the PR's commits;
2. gitleaks over the tracked files;
3. the `tests/secrets.rs` canary suite.

The `secret-scan` job in `security.yml` runs it on every push and PR.

Results on 2026-10-29:

- 137 commits scanned, no findings.
- Tracked tree clean.
- Canary suite: 3/3 tests pass.
- Negative check: a planted `ghp_…` token is caught, exit 1.

Untracked `target/` holds 44 "private-key" hits. These are test PEMs that dependencies embed in build output. They are not scanned, because `target/` is ignored and never committed.

## Image hardening tests (SUP-017, L-211)

`make test-image` runs `scripts/test-image.sh <image> <variant>`. It checks:

- `--version` and `--help` work;
- bash, the `elasticdump` symlink and the CA bundle are present;
- the image runs as non-root (uid 65532), with working directory `/data`;
- a file-to-file dump into a writable `/data` volume works under `--read-only --cap-drop ALL --security-opt no-new-privileges --network none`;
- the root filesystem cannot be written under `--read-only`;
- the image contains no executable outside the documented set in `docker/image-binaries.<variant>.txt`;
- the base image label is pinned by digest.

The executable baselines were reviewed on 2026-10-29 from images rebuilt at L-211:

| Variant | Executables |
|---|---|
| `dhi-alpine` | 62 |
| `alpine` | 68 |
| `dhi-debian` | 300 |
| `public` | 409 |

All four variants pass. Negative check: testing the old dhi-debian image against the dhi-alpine baseline fails, both on undocumented executables and on a base that is not pinned by digest.

After a reviewed base change, refresh a baseline with `scripts/test-image.sh <image> <variant> --update`.

## Publisher monitoring (SUP-019)

`supply-chain/publishers.json` records the crates.io owners of every dependency (220 crates, 142 distinct
publishers at the 2026-10-30 snapshot, `cargo supply-chain` 0.3.8). The weekly `publishers` job in
`security.yml` re-queries crates.io and runs `scripts/sup/publishers.py diff`; it fails (exit 10) when an
existing crate gains or loses an owner, so a takeover or hand-over gets a human look before the next release.
New or removed crates are listed but do not fail the job; the SBOM diff (SUP-020) covers them. After review,
refresh the snapshot with `cargo supply-chain json > sc.json && scripts/sup/publishers.py snapshot sc.json`.

**Refresh 2026-11-11 (L-810f).** The `publishers` job on main 90d5eb0 (security.yml run 42994732897) failed with exit
10. The snapshot was regenerated from that run's `sc.json` (`cargo supply-chain` 0.3.8): **331 crates, 189 distinct
publishers** (was 220 / 142).
- **Owner changes on existing crates (the alert).** Removals only, no additions; in each case `rust-lang-owner`
  stays as the remaining owner:
  - `flate2`: Byron, joshtriplett removed;
  - `futures-channel`, `futures-core`, `futures-sink`, `futures-task`, `futures-util`: cramertj, taiki-e removed;
  - `pkg-config`: joshtriplett, sdroege removed.
  - This looks like the Rust project moving rust-lang crates to its bot owner, not a takeover. No action beyond
    the refresh.
- **New crates (113).** Mostly OpenDAL 0.59 and its dependencies (L-809a/L-809d):
  - opendal, opendal-core, the opendal service and layer crates: Xuanwo, PsiACE, tisonkun, ClSlaid, oowl, silver-ymz;
  - reqsign-*, backon: Xuanwo, tisonkun;
  - the openssh / openssh-sftp-* crates, ssh_format, tokio-io-utility and a few small helpers (awaitable,
    concurrent_arena, derive_destructure2, vec-strings, asyncband): NobodyXu, jonhoo, silver-ymz;
  - crc-fast (onethumb), quick-xml, rustls-platform-verifier, hybrid-array/cmov/ctutils (RustCrypto).
  - Many of the rest compile only on other platforms (windows-*, wasm-bindgen/js-sys/web-sys, core-foundation /
    security-framework, jni, schannel, r-efi, redox_syscall). They are in the lockfile graph but not in Linux builds.
  - Others are dev/test-only (proptest, lipsum, tempfile).
- **Single-owner crates in the build graph** (a takeover would need one account):
  - crc-fast (onethumb), backon and reqsign-aws-core (Xuanwo);
  - ssh_format_error, derive_destructure2, vec-strings (NobodyXu);
  - simd_cesu8 (seancroach), encode_unicode (tormol), hex (KokaKiwi);
  - mimalloc and libmimalloc-sys (octavonce).
  - They are covered by cargo-vet exemptions and the native-code / geiger gates like every other crate.
- **Gone:** fnv, h2; publishers Byron, cramertj, joshtriplett, olix0r, sdroege.

## Non-hot unsafe trimmed (SUP-023, L-214)

| Item | Decision | Effect |
|---|---|---|
| reqwest `http2` | **Removed.** Elasticsearch and OpenSearch serve HTTP/1.1 only, and Node elasticdump is HTTP/1.1 only, so h2 was only reachable through an h2-capable proxy, where HTTP/1.1 also works. | `h2` and its deps leave the graph (Cargo.lock −29 lines); geiger used-unsafe 27,289 → 27,245, crates with unsafe 111 → 110 |
| pkcs8 `encryption` (aes, scrypt, pbkdf2, sha2, salsa20, cbc) | Behind the crate feature `encrypted-keys`, **on by default** so `--key` + `--pass` keeps working; `--no-default-features` builds leave the stack out and an encrypted key then fails with a message naming the feature and the `openssl pkcs8` conversion. | minimal build: 25,774 used-unsafe across 162 packages (default 27,245 / 186) |
| env_logger `humantime` (jiff) | **Kept.** Dropping it removes the timestamp from every log line, a visible output change. | — |
| indicatif → portable-atomic, console, terminal_size | **Kept.** portable-atomic is a non-optional indicatif dependency; the progress bar is a user-facing feature. | — |

The geiger baseline (`supply-chain/geiger-baseline.json`) was lowered to the new total, so the gate now holds the
reduction.

## Optional allocators (L-802a)

`cargo build --release --features jemalloc` (or `--features mimalloc`) sets one
`#[global_allocator]` in `src/main.rs`; the two features are mutually exclusive.
Since L-808b (user decision) glibc Linux builds (`*-linux-gnu`) use mimalloc without any
feature (a target-specific dependency in Cargo.toml); `--features system-allocator` keeps
glibc's malloc there (the mimalloc C sources are then compiled but not linked). musl builds
keep musl's allocator unless a feature asks for another. New crates, all MIT or MIT/Apache-2.0 at the crate level:
`tikv-jemallocator` 0.7.0 and `tikv-jemalloc-sys` 0.7.1 (jemalloc 5.3.1, BSD-2-Clause
C sources), `mimalloc` 0.1.52 and `libmimalloc-sys` 0.1.49 (bundled mimalloc, v3.3.2 at run time, MIT C
sources). What each gate sees:

- `cargo deny` (all features): licences pass; both `-sys` crates are in
  `allow-build-scripts`, and `tikv-jemalloc-sys` has an exact-version bypass for its
  autoconf scripts (its build runs `configure` and `make`).
- `cargo vet`: exemptions at these exact versions (not audited).
- geiger gate, build-time inventory, native-code check: default features only, so
  unchanged. Turning a feature on by default needs a geiger baseline update, an
  inventory update and a native allow-list entry.

Measurements and the recommendation: `results/local/L-802a.md`.

## OpenDAL storage path (L-809a, L-809d)

`opendal` =0.59.5 (features `opendal`, `opendal-sftp`, on by default) adds S3/B2/SFTP/fs storage.
L-809a used 0.57 under rust-version 1.85; the owner raised rust-version to 1.91 for everything
(L-809d, 2026-11-11), which allowed 0.59 and removed the three advisories 0.57 carried
(RUSTSEC-2026-0194/0195, quick-xml 0.39; RUSTSEC-2026-0385, ctutils 0.4.2). quick-xml is now 0.42
and ctutils 0.4.8; `deny.toml` and cargo audit have no advisory ignores. What each gate sees:

- HTTP: `opendal-http-transport-reqwest` with `rustls-no-provider` (OpenDAL's own `reqwest-rustls-*`
  facade features would also enable rustls' aws-lc provider). Each operator gets our reqwest 0.13
  client through `OperationContext::with_http_transport`, built from the same rustls/ring config as
  cluster connections (`ca_file`, `insecure`). No process-wide default transport is installed, so a
  request cannot go through any other client. One TLS stack, no aws-lc.
- `cargo deny`: licences pass. `rustversion` (runs `$RUSTC --version`, proc-macro via arc-swap in
  the SFTP client) is in `allow-build-scripts`. Duplicates skipped with reasons: reqwest 0.13, the
  RustCrypto 0.11 generation used by reqsign for SigV4 (sha2, hmac, digest, crypto-common,
  block-buffer, const-oid, cpufeatures), getrandom 0.4 and syn 2.
- Native code: `crc-fast` (S3 checksums) is pure Rust with SIMD intrinsics; it ships one C header for
  its optional C ABI (not compiled, no build script) and is on the native-code allow-list for that.
  Its default `ffi` feature exports `extern "C"` CRC functions into the binary (unused). SFTP runs
  the system `ssh` binary as a child process (`openssh` crate).
- `cargo vet`: exemptions at exact versions, notes `L-809a` / `L-809d` (not audited).
- Build-time inventory and geiger baselines updated. Used unsafe rose 27264 -> 35383 in total
  (crc-fast SIMD 2991, parking_lot, triomphe, thin-vec, spin, rustix features, ...). The geiger
  baseline was produced with cargo-geiger on Rust 1.91.1, not nightly.
