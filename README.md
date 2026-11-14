# searchdump-rs

Back up, restore and copy **Elasticsearch and OpenSearch** data, mappings, settings, templates, policies and more,
between clusters, files, object storage and SFTP. Two binaries:

- **`searchdump`**: one index or one item type per run (the counterpart of Node `elasticdump`);
- **`searchdump-multi`**: many indices at once, in parallel, to or from a dump directory (the counterpart of Node
  `multielasticdump`), plus whole-directory copies between stores.

Worked dump and restore examples (minimum resources, fastest, large indices, TLS, object storage,
cluster-to-cluster copy, and `searchdump-multi`) for Kubernetes, Docker/Podman and bash are in
[`examples/`](examples/).

## Where it comes from

- **Code derivative of [elasticdump-rs](https://github.com/tenyears-tech/elasticdump-rs)** by TenYears
  (Copyright (c) 2025 TenYears, MIT License). This repository started as a fork of it; its retrieval and staged-output
  code is still here, and its MIT license and copyright notice are kept in [`LICENSE`](LICENSE). Most of the current
  code (types, restore, cross-engine handling, `searchdump-multi`, storage backends, the test and supply-chain
  tooling) was written in this fork. The project was named elasticdump-rs-fork until 2026-11-11.
- **Spiritual and compatibility derivative of Node
  [elasticdump](https://github.com/elasticsearch-dump/elasticsearch-dump)** (elasticsearch-dump, Apache-2.0). No Node
  code is included; its behaviour and defaults were studied from its source so that the **CLI flags and file formats
  stay compatible**: existing elasticdump scripts and dump files keep working, at much higher speed (see
  [Performance](#performance)). Known, deliberate differences are listed below and in
  [`NODE-DIFFERENCES.md`](NODE-DIFFERENCES.md).

## Disclaimer

searchdump-rs is provided **as is, without warranty of any kind**, as set out in its [MIT license](LICENSE) (the
same warranty disclaimer as the elasticdump-rs code it derives from; Node elasticdump, whose behaviour it follows, has
the equivalent disclaimer in Apache-2.0, sections 7 and 8). The authors accept no liability for any damage from its
use, including data loss, data corruption, downtime or service disruption, in production or anywhere else.

It has been stable and fast in our testing (see [Testing](#testing)), but no test suite can cover every edge case
across cluster versions, configurations, plugins and storage backends, and beta releases in particular may still
contain bugs. Before you rely on it:

- try it in a non-production environment first;
- verify every backup independently: restore it somewhere, then compare document counts (and spot-check documents,
  mappings and settings) against the source;
- keep your existing backup method until those checks pass for your own data.

## What it does

| Direction | `--type data` | `--type mapping` | `--type settings` |
|---|---|---|---|
| cluster → file / stdout | ✅ scroll, search_after, PIT (sliced) | ✅ | ✅ (elasticdump's string-literal format) |
| file / stdin → cluster | ✅ `_bulk` | ✅ (typed → typeless unwrap) | ✅ (creates the index) |
| cluster → cluster | ✅ | ✅ | ✅ (engine-specific settings auto-stripped) |
| file → file | ✅ (normalise, offset/size, sourceOnly) | – | – |

"File" also means `s3://`, `b2://`, `sftp://` and `fs://` locations (see
[Object storage and SFTP](#object-storage-and-sftp-opendal)). More types: analyzer, alias, legacy/component/index
templates, ILM/ISM policies (with ILM → ISM conversion) and stored scripts ([Types](#types)).

Plus: basic auth (URL, `--httpAuthFile`, `SEARCHDUMP_{INPUT,OUTPUT}_{USERNAME,PASSWORD}`), TLS with custom CA,
client certificates (encrypted PKCS#8 keys via `--pass`) and insecure mode per direction; gzip **and zstd**
files; file splitting; write concurrency, throttling and rate limiting; Node-style retries plus 429.

Tested against **OpenSearch 1.3, 2.19 and 3.9** and **Elasticsearch 7.10, 7.17, 8.6, 8.19 and 9.1**, plain and
secured (basic auth + TLS), across engines (ES ↔ OpenSearch, files and cluster to cluster), and against Node
elasticdump 6.125.1 for file-format interop ([`TESTING-MATRIX.md`](TESTING-MATRIX.md)).

## Install and build

From source (Rust 1.91 or newer; `rust-toolchain.toml` pins 1.91.1, the CI toolchain):

```bash
git clone https://github.com/robertgoley/searchdump-rs.git && cd searchdump-rs
cargo build --release            # fat LTO; needs a few GB of RAM
# binaries: target/release/searchdump and target/release/searchdump-multi
cargo build --profile it         # lighter optimised build (no LTO), used by the test harness
```

Container image (podman or docker; details in [Container image](#container-image)):

```bash
podman login dhi.io                         # pull login for the hardened default bases
scripts/build-image.sh                      # default: dhi.io Debian 13, amd64 -> localhost/searchdump-rs:<version>, :latest
scripts/build-image.sh --type dhi-alpine    # :<version>-alpine (musl); also dhi-static, public, alpine
podman run --rm -v "$PWD:/data" localhost/searchdump-rs --input=https://os:9200/logs --output=/data/logs.json
```

The image's entrypoint is `searchdump`; `searchdump-multi` is next to it. Every image type also has compatibility
symlinks in `/usr/local/bin`: `elasticdump` and `elasticdump-rs` point at `searchdump`, and `multielasticdump` points
at `searchdump-multi`, so commands written for the old names keep working (`podman run --entrypoint multielasticdump
…`, or `command: [multielasticdump]` in Kubernetes). Neither binary changes behaviour based on the name it was started
under.

Allocator: builds for glibc Linux (`*-linux-gnu`, the preferred release artifacts and the default container image)
use mimalloc as the global allocator; musl builds (`*-linux-musl`, the Alpine image) keep musl's own allocator.
Features: `--features system-allocator` builds glibc Linux with glibc's malloc (mimalloc's C code is still compiled
but not linked in), `--features jemalloc` uses jemalloc on any target, `--features mimalloc` forces mimalloc on any
target (also musl). `jemalloc` and `mimalloc` are mutually exclusive, and `system-allocator` excludes both. The
storage backends are the default features `opendal` and `opendal-sftp`.
Release builds (`.github/workflows/release.yml`, `cargo zigbuild`): `x86_64`/`aarch64-unknown-linux-gnu` against
glibc 2.28 (preferred), plus static `x86_64`/`aarch64-unknown-linux-musl`.
Each archive `searchdump-rs-<version>-<target>.tar.gz` holds a directory of the same name with `searchdump`,
`searchdump-multi`, the `elasticdump`, `elasticdump-rs` and `multielasticdump` symlinks, `LICENSE`,
`THIRD-PARTY-NOTICES.md`, `README.md` and `CHANGELOG.md` (see [Releases](#releases)); the container images carry
`LICENSE` and `THIRD-PARTY-NOTICES.md` in `/usr/share/doc/searchdump-rs/`.

### Renamed from elasticdump-rs (2026-11-11)

| Before | Now |
|---|---|
| crate `elasticdump-rs`, library `elasticdump_rs` | `searchdump-rs`, `searchdump_rs` |
| binary `elasticdump-rs` | `searchdump` |
| binary `multielasticdump` | `searchdump-multi` |
| `ELASTICDUMP_*` environment variables | `SEARCHDUMP_*` (the old names still work, deprecated, with a warning) |
| image `localhost/elasticdump-rs` | `localhost/searchdump-rs` |

The `searchdump-multi` manifest keeps its file name `multielasticdump.json`, so existing dump directories load and
`--resume` as before.

## Quick usage

### searchdump

Replace `elasticdump` with `searchdump`; the flags are spelled the same (`--input-index`, `--fsCompress`,
`--ignore-es-write-errors`, booleans with or without `=true`). A full backup/restore:

```bash
# backup
for t in settings mapping data; do
  searchdump --input=https://os:9200/logs --output=logs.$t.json --type=$t --ca=ca.pem
done
# restore (order matters: settings create the index, then mapping, then data)
for t in settings mapping data; do
  searchdump --input=logs.$t.json --output=https://os:9200/logs --type=$t --ca=ca.pem
done
```

(credentials from `SEARCHDUMP_INPUT_USERNAME`/`_PASSWORD`, `SEARCHDUMP_OUTPUT_*` or `--httpAuthFile`.) Other common forms:

```bash
searchdump --input=http://os:9200/src --output=http://os:9200/dst --limit 1000 --concurrency 4  # copy
searchdump --input=http://os:9200/src --output=dump.json --fileSize=500mb --fsCompress           # gzip parts
searchdump --input=http://os:9200/src --output=dump.json --compressionType zstd                 # zstd (opt-in)
searchdump --input=dump.json.zst --output=http://os:9200/dst                                     # auto-detected
searchdump --input=http://os:9200/src --output=$ --searchBody=@query.json | jq .                # stdout
searchdump --input=http://os:9200/src --output=d.json --searchAfter --pit --slices 4            # sliced PIT
searchdump --input=http://os:9200 --output=all.mapping.json --type=mapping                      # all indices
searchdump --input=all.mapping.json --output=http://other:9200 --type=mapping --prefix=copy_    # restore each
searchdump --input=http://os:9200/src --output=s3://backups/src.json --outputStorageConfig=s3.ini # to S3
```

Restoring split dumps takes one invocation per part (as in Node): `for f in dump.split-*.json.gz; do … --input=$f …; done`;
`searchdump-multi` finds and loads the parts itself.

### searchdump-multi

```bash
# every logs-* index, one file set per index, zstd, 4 at a time
searchdump-multi --input=https://src:9200 --output=/data/logs --match='^logs-' --compression=zstd3 --parallel=4
# load it all back
searchdump-multi --direction=load --input=/data/logs --output=https://dst:9200 --parallel=4
# dump straight to S3 (staged locally, uploaded when every index succeeded)
searchdump-multi --input=https://src:9200 --output=s3://backups/logs --outputStorageConfig=s3.ini
# copy a dump directory between stores, byte for byte, no cluster involved
searchdump-multi --direction=copy --input=s3://backups/logs --output=sftp://backup@vault:22/srv/logs \
  --inputStorageConfig=s3.ini
# a dump that Node's multielasticdump 6.x can restore
searchdump-multi --input=https://src:9200 --output=/data/node --nodeCompat
```

Details: [Bulk dump and load](#bulk-dump-and-load-searchdump-multi).

Worked scripts and manifests for these flows, plus minimum-resource, fastest and very-large-index
cases, are in [`examples/`](examples/README.md).

## Compatibility with Node elasticdump

- Same flag names and spellings, same NDJSON data format and metadata file formats; dumps made by either tool load
  with the other (checked against elasticdump 6.125.1, including digest-equal dumps).
- `searchdump-multi --nodeCompat` writes the layout Node's `multielasticdump` loads; `searchdump-multi --direction=load`
  reads Node-made dump directories.
- Not supported (clear error instead of silent difference): JavaScript `--transform` and other JS hooks, AWS SigV4
  signing and OpenSearch Serverless, CSV, SOCKS5, `--delete`; Node's `--s3*` flags warn and do nothing (use
  `s3://` URLs). See [`FEATURES-TODO.md`](FEATURES-TODO.md).
- Deliberate differences (safer defaults, stricter errors, numeric precision above 2^53 kept, gzip/zstd, sliced
  retrieval): [Intentional differences](#intentional-differences-from-node-elasticdump) and
  [`NODE-DIFFERENCES.md`](NODE-DIFFERENCES.md).

## Flags

`searchdump --help` lists everything. Groups:

* **Endpoints/types:** `--input`, `--output` (URL, file, `$`, or a storage URL `s3://`, `b2://`, `sftp://`, `fs://`;
  see "Object storage and SFTP"), `--inputStorageConfig`/`--outputStorageConfig`, `--input-index`/`--output-index` (`index[/type]`),
  `--type data|mapping|settings|index|analyzer|alias|template|component_template|index_template|policy|script`,
  `--overwrite`, `--prefix`/`--suffix`, `--quiet`, `--debug`, `--format human`.
* **Retrieval:** `--limit` (default 100), `--offset`, `--size`, `--searchBody` (JSON or `@file`),
  `--searchWithTemplate`, `--scrollTime`, `--scrollId`, `--searchAfter`, `--pit`, `--pitKeepAlive`,
  `--sourceOnly`, `--esCompress`, `--headers`/`--params` (+ `--input-*`/`--output-*`).
* **Writing:** `--bulkAction index|create|update|delete`, `--handleVersion`, `--versionType`, `--noRefresh`,
  `--ignore-errors`, `--ignore-es-write-errors` (default: continue past failed items), `--parseExtraFields`.
* **Files:** `--fsCompress`, `--compressionType gzip|zstd`, `--compressionLevel`, `--fileSize`, `--maxRows`.
  `--compressionType` wins over `--fsCompress` when both are given. `--offset` cannot be combined with `--slices`
  (sliced results interleave, so there is no stable "first N" to skip); `--skip-existing` (metadata restores) and
  `--overwrite` (file output) are independent.
* **Throughput:** `--concurrency`, `--throttleInterval`, `--intervalCap`, `--concurrencyInterval`,
  `--carryoverConcurrencyCount`, `--maxSockets` (a hard cap on requests in flight, and so on connections; it
  wins over a higher `--concurrency`), `--timeout` (ms of server *silence*, as in Node's `request`;
  a slow but progressing response is not cut off; overrides `--idleTimeout`).
* **Retries:** `--retryAttempts` (0 = Node's 4 implicit retries), `--retryDelay` (5000 ms).
* **Trusted roots:** `--ca`/`--input-ca`/`--output-ca` when given; otherwise the system store (honouring
  `SSL_CERT_FILE`/`SSL_CERT_DIR`). When that store has no usable certificate, the bundled webpki (Mozilla)
  roots are used; if `SSL_CERT_FILE`/`SSL_CERT_DIR` was set, this fallback is logged as a warning naming it.
* **Redirects** are never followed (Node follows them): a 3xx answer stops the run and names the target, so
  credentials and documents only go to the cluster you named. Use the final URL directly.
* **Auth/TLS:** `--httpAuthFile`, `--input-username`/`--input-password`, `--output-username`/`--output-password`
  (see [Credentials](#credentials)), `--tlsAuth`, `--ca`/`--cert`/`--key`/`--pass` and their `--input-*`/`--output-*` forms.
* **Metadata:** `--templateRegex`, `--filterSystemTemplates` (Node), and the extensions `--filterManaged`
  (default true), `--closeIndex`, `--excludeTemplateFields`, `--convertIlmToIsm`, `--lenientPolicyConversion`,
  `--metaFormat string|object`, `--renamePrefix`, `--renameIndex`, `--renameAliases`, `--renamePatterns`.

Rust-only extensions (all opt-in): `--compressionType zstd`, `--slices`, `--workers`, `--bufferSize`,
`--searchType scroll|pit`, `--skip-existing`, `--force` (allow the same file/index as input and output; the output index is compared after `--prefix`/`--suffix`, so `--input …/idx --output …/idx --suffix=-copy` is not refused, `--input …/p-idx --output …/idx --prefix p-` is; a host-only `--output` writes each input index to its own name after `--renamePrefix`/`--renameIndex` and the affixes, so `--input …/idx --output …` is refused unless an affix or rename rule changes the name, and a wildcard input is refused only with neither), `--excludeSettings`, `--maxBulkBytes`, `--retryOn`,
`--retryBackoff`, `--noImplicitRetry`, `--insecure`/`--input-insecure`/`--output-insecure`, `--username`/`--password`
(aliases of `--input-username`/`--input-password`), `--caFile`, `--requestTimeout` (seconds; a deadline for the whole request, body included), `--idleTimeout` (seconds of server silence before a request fails or is retried; default 300, 0 = never; unlike Node, a stalled server cannot hang a run), `--maxResponseMb` (largest response body read into memory, default 256, 0 = unlimited; a bigger page fails with a hint to lower `--limit` instead of an OOM kill), `--allowMerge` (let `--renamePrefix`/`--renameIndex` rename several source indices to one target; refused by default, before anything is read for a cluster source, and before the batch is sent for a file source), `--maxDocMb` (largest single document read from a file or stdin, default 100 like `http.max_content_length`, 0 = unlimited; a bigger one stops the run naming its position instead of being buffered whole), `--allowMissingSource` (dump hits that carry no `_source`: indices whose mapping has `_source.enabled: false`, or a `--searchBody` that turns `_source` off. By default the first hit without `_source` stops the dump before anything of that page is written, and the error names the cause; a search body with `"_source": false` is refused before the first search, naming the search body and suggesting to remove it or pass this flag. **Behaviour change (L-455b):** `"_source": false` used to be allowed without the flag. `stored_fields` without `_source` is refused the same way (L-455a). `"_source": []` is not refused: on OpenSearch 1.3-3.x and Elasticsearch 7.17-9.1 it returns the whole `_source`. **Behaviour change (L-455c):** an empty `_source` (`{}`) is refused like a missing one. A filter that excludes every field (`{"excludes":["*"]}`, which returns `{}` for every hit) is refused before the first search. Otherwise the first hit whose `_source` is `{}` stops the dump, and the error names the document's `_index` and `_id` and the likely cause: the search body's `_source` filter, or a document that is really empty. **A document that is genuinely empty (`{}`) in the source index also needs `--allowMissingSource`.** These dumps used to write `{}`), `--noProxy` (ignore `HTTP(S)_PROXY`/`ALL_PROXY`; when an
environment proxy is used, a warning names it), `--restartOnContextLoss N` (L-804b; a scroll data dump from a cluster into a file restarts from the start, rewriting the file, up to N times when the scroll context is lost, e.g. after a node restart; default 0. A point-in-time dump, `--searchAfter --pit`, retries a failed page instead and does not need it).

Input safety: every hit is written as exactly one NDJSON line (pretty-printed `_source` is flattened),
a bulk input line that is not exactly one JSON object, has duplicate top-level keys, a non-object
`_source`, a non-string `_id`/`routing`/`parent`, a non-integer `version` or an invalid index name
(`*`, `?`, `,`, `_all`, `.`, `..`, leading `-_+`, upper case, > 255 bytes) is rejected on its own and
makes the run exit 1. `--headers` with CR/LF/NUL is a usage error.

Exit status: 0 success; 1 error (takes precedence over 3: e.g. a target index deleted, or deleted and re-created, mid-restore exits 1 even when documents were also rejected); 2 usage error; **3 finished, but documents were not written**; **4 transient failure, safe to rerun** (**behaviour change, L-804b**; these exited 1 before: the network, HTTP 429 or 5xx, or a lost scroll context still failed after the retries, and nothing was written that a rerun of the same command would duplicate. A restore that already sent documents without `_id` exits 1, because a rerun would index them twice. TLS certificate or protocol errors, refused credentials, mapping errors and other permanent failures stay 1. `searchdump-multi` keeps its own codes); 130 / 143 stopped by SIGINT / SIGTERM (both stop in order: the scroll or PIT is closed and staged output removed; a second signal force-quits)
(`Failed Writes` > 0: bulk item errors such as 429 storms, flood-stage or write blocks, mapping errors,
conflicts; when most failed items are `cluster_block_exception`s the summary says so, and for a disk flood-stage block suggests freeing disk and rerunning with a higher `--retryAttempts`). Like Node, the run continues past failed items by default, but unlike Node it does not exit 0
then; pass `--ignore-es-write-errors` (or `--ignore-errors`) explicitly to accept failed writes with status 0,
or `--ignore-es-write-errors false` to stop at the first one with status 1. `--debug` masks query values
except a small allow-list (`scroll`, `size`, ...).

Metadata types: `index`, `analyzer`, `alias`, `template`, `component_template`, `index_template`, `policy`
and `script` (see [Types](#types)). Index renaming: `--renamePrefix`, `--renameIndex`, `--renameAliases`,
`--renamePatterns` (see [Renaming indices](#renaming-indices)).

Out of scope (rejected or ignored with a warning, see `docs/PARITY.md` §3.7): CSV, S3/MinIO, AWS signing,
transforms, `--delete`. `searchdump-multi` is a separate, smaller tool (see [Bulk dump and load](#bulk-dump-and-load-searchdump-multi)).

Where this tool differs from Node because Node is incomplete or buggy, see [NODE-DIFFERENCES.md](NODE-DIFFERENCES.md);
for every place the tool changes what is loaded, and for the tested versions, see
[TESTING-MATRIX.md](TESTING-MATRIX.md). Security scanning: [SECURITY-CVE.md](SECURITY-CVE.md).

## Intentional differences from Node elasticdump

* **zstd**: `--compressionType zstd` writes `.zst` files/parts that Node cannot read. Default output stays uncompressed;
  `--fsCompress` keeps meaning gzip, and gzip stays the default codec (D-2); zstd is used only when asked for.
* **Compressed input is auto-detected** (gzip or zstd magic bytes) with or without `--fsCompress`. Decoding streams
  (memory stays bounded by `--maxDocMb`, also for 1000:1 and 30000:1 bombs); zstd frames that declare a window over
  128 MiB (`zstd --long=28` or more) are refused with a hint. Truncated input, trailing garbage and corrupt frames stop
  the run with exit 1 and nothing committed.
* **`--fileSize` counts UTF-8 bytes**; Node counts UTF-16 code units. Identical part names for ASCII data
  (verified against Node), slightly different boundaries for non-ASCII text.
* **Object key order is kept as the server sent it.** Node re-serialises parsed JSON, and JavaScript lists
  integer-like keys (`"0"`, `"15"`) first in numeric order; metadata lines with such keys are therefore
  equal in content but not byte-equal (found by the L-502 Node comparison; no other byte difference).
* **Split parts are staged and committed together** at the end of a successful run; a failed run (including a full disk) leaves nothing and an existing destination untouched. With `--overwrite` the files being replaced are moved aside first and restored if any part cannot be committed; older parts beyond the new set are left in place with a warning.
  Existing `*.split-*` files need `--overwrite`.
* **Certificate flags always apply**; Node ignores `--ca/--cert/--key` unless `--tlsAuth` is set.
* **Retries also cover HTTP 429**, and bulk items rejected with 429/503 are retried individually.
* **PIT is engine-aware**: Elasticsearch 7.10+ uses `POST /{index}/_pit` and the `_shard_doc` tiebreaker;
  OpenSearch uses `/_search/point_in_time` and `_doc` + `_id` (a `_doc`-only sort was verified to lose documents).
* **Scroll + `--offset`** skips client-side; Node sends `from=`, which ES 7+/OpenSearch reject.
* **`--scrollId` ids are never printed** (Node's `--debug` shows them, which its resume recipe relies on); resume
  a stopped file dump by rerunning it with `--overwrite`, or pass the id of a scroll you opened yourself (L-650).
* **`--throttleInterval`** only applies when given (Node's 1 ms default would add ~10 s per million docs).
* **`--intervalCap`/`--concurrencyInterval`/`--carryoverConcurrencyCount`** work (p-queue semantics) when set;
  they have been no-ops in Node since 6.119.
* **`--format human`** writes pretty-printed JSON (restorable), not Node's `util.inspect` output; it cannot be
  combined with splitting.
* **`--searchWithTemplate`** renders `{"id"|"source", "params"}` via `_render/template`, then searches normally.
* **Big integers and unicode** are preserved byte-for-byte in both directions (no `--support-big-int` needed).
* **Restored documents are sent as written in the file.** Node re-serialises every `_source` and `_id` (escape forms
  such as `\u00e9` become `é`, `1.0` becomes `1`, `1e21` becomes `1e+21`, integer-like keys move first, integers
  beyond 2^53 are rounded or turned into strings); the Rust tool passes the JSON text through unchanged. A numeric `_id`/`routing`/`parent` in the file becomes a string of exactly its text (`-0` → `"-0"`, `1e3` → `"1e3"`, `0.0` → `"0.0"`), where Node sends `"0"`, `"1000"`, `"0"`; a literal that is not a JSON number (`00`, `+1`, `NaN`) rejects that document (L-629a). Node also sends
  each bulk batch in reverse file order, so when one batch holds two documents with the same `_id` the earlier line
  wins in Node and the later line wins here (file order). Verified on 10,000 random documents by
  `scripts/node/bulk-diff.py` (L-506): every other byte difference is one of these.
* **Bulk uses POST**; `/index/type` URLs are not parsed (use `--output-index idx/type` for ES < 7).
* **Endpoints that look like mistyped URLs are refused** (exit 2, L-646); Node reads or writes them as files.
  Only `http://` and `https://` are clusters, in any case (`HTTP://` is a cluster here, a file in Node); everything else is a file path, but
  `http:/host`, `http:host`, `http//host`, an unsupported scheme such as `htp://`, and a URL with leading or trailing
  whitespace are usage errors, and a scheme-less `--output user:password@host:9200` is refused because it would write
  the password into a file name. A scheme-less `host:port` or `user:pw@host` that is used as a file prints a warning
  (password masked). Start the path with `./` when such a name really is a file. No message quotes a password.
* **`--username`/`--password` apply to the input only** (they are aliases of `--input-username`/`--input-password`;
  use `--output-username`/`--output-password` for the output).
* **`--httpAuthFile` values are taken literally:** `#`, `;` and `\` inside a value are kept (Node's INI parser cuts
  the password at `#`/`;`); surrounding spaces and quotes are removed as in Node.
* **`--httpAuthFile` wins over URL credentials and the credential flags** (L-645b). Node uses the auth file only when
  the URL lacks a username or password; searchdump uses it whenever it is given and warns about what it replaced.
* **Metadata types** fix several Node bugs (listed in [NODE-DIFFERENCES.md](NODE-DIFFERENCES.md)); intentional
  choices: `--filterManaged` (default on), `--output-index`/`--prefix`/`--suffix` apply to alias targets,
  cluster-level types ignore the output index (warning), the source is read before the target is contacted,
  `--closeIndex` is opt-in, analyzer restores do not wait for green.
* **PIT falls back to scroll** on clusters without it (ES OSS builds, OpenSearch < 2.4) and sliced PIT falls back
  to sliced scroll on ES < 7.15, with a warning; Node fails.
* **Cluster→cluster settings** drop engine-specific keys (ES `index.lifecycle` and
  `index.routing.allocation.include._tier_preference` going to OpenSearch; OpenSearch `index.replication.type`,
  `plugins.*`, `opendistro.*` going to ES) with a warning. File restores keep them (a file does not say which
  engine wrote it) but warn with the exact `--excludeSettings=` to use; Elasticsearch rejects OpenSearch keys
  outright (the error carries the same hint), while OpenSearch would otherwise store ES keys silently.

## Types

| `--type` | Dump (cluster → file) | Restore (file/cluster → cluster) |
|---|---|---|
| `data`, `mapping`, `settings` | as before | as before |
| `index` | `GET /{index}` (host-only: `GET /*`): settings + mappings + aliases, generated settings stripped | one `PUT /{index}` per index (`--skip-existing` skips existing ones); `data_stream` indices are skipped with a warning |
| `analyzer` | `{index}/_settings` (the analysis block is what matters) | new index: created with the analysis; existing index: error, `--skip-existing`, or `--closeIndex` (close → update → always reopen; the index is **offline** meanwhile; Ctrl+C / SIGTERM while it is closed abandons the update, reopens it and exits 130 / 143; a second Ctrl+C before the reopen finished force-quits and names the index that may still be closed, with the `POST /{index}/_open` to run) |
| `alias` | `GET {index\|*}/_alias/{alias\|*}` (`--input-index idx/alias*` filters) | one atomic `POST /_aliases`; targets follow `--output-index`/`--prefix`/`--suffix`/rename rules |
| `template` | legacy `_template` | `PUT _template/{name}` (deprecated on ES ≥ 8, warned once) |
| `component_template` | `_component_template` | `PUT _component_template/{name}` |
| `index_template` | `_index_template` | `PUT _index_template/{name}`; missing `composed_of` components fail with "restore component templates first" |
| `policy` | ES: `_ilm/policy`; OpenSearch: `_plugins/_ism/policies` (paged) | same engine only, unless `--convertIlmToIsm` (ES → OpenSearch); OpenSearch overwrites use `if_seq_no`/`if_primary_term` |
| `script` | stored scripts from `_cluster/state/metadata` (needs the `monitor` cluster privilege) | `POST _scripts/{id}` |

Files keep elasticdump's format (one JSON string literal per file, mappings as an object), so Node and this tool
read each other's dumps. A file dumped with one `--type` and restored with another fails with
"this input looks like a --type X dump".

**Full backup and restore order** (each step its own command, all with the same index pattern):
`component_template` → `index_template` → `template` → `policy` → `script` → `index` (or `settings` + `mapping`)
→ `alias` → `data`. Templates and policies first so new indices pick them up; aliases after the indices exist.
The `full_backup_and_restore_in_documented_order` integration test runs exactly this order on every profile.

**Filters.** `--templateRegex` (default elasticdump's `^(metrics|logs|.+_audit_log|.+-index-template|\..+)$`,
cluster dumps only, `--filterSystemTemplates=false` disables it) and `--filterManaged` (default on; drops
templates and ILM policies with `_meta.managed: true`, for cluster and file input). Elasticsearch 8/9 ship dozens of
managed built-ins (`*@template`, `*@mappings`, `*@settings`, `logs`, `metrics`, `synthetics`, APM/OTel templates,
`7-days-default` … ILM policies) that the Node regex misses; ES 7.17 marks fewer of them (`ilm-history`,
`synthetics`, the `logs-*`/`metrics-*` component templates and the default ILM policies).

**Cross-engine templates.** Cluster → cluster strips the other engine's fields with a warning
(ES: `allow_auto_create`, `ignore_missing_component_templates`, `deprecated`, `data_stream_options`,
`template.lifecycle`, `data_stream.{hidden,allow_custom_routing,failure_store}`; OpenSearch:
`data_stream.timestamp_field`, `context`) and the settings keys the target rejects. From a file they are only
warned about, with the exact `--excludeTemplateFields=` to use.

### ILM → ISM conversion (`--convertIlmToIsm`)

Off by default: restoring or copying ILM policies into OpenSearch fails with an error naming the flag, and
ISM → ILM is always refused. With the flag, each ILM policy becomes one ISM policy with one state per phase
(`hot` → `warm` → `cold` → `delete`, in that order; a non-zero first `min_age` adds a `pending` state). Every
dropped or approximated feature is logged as a warning. Unsafe features fail the policy unless
`--lenientPolicyConversion` is given, which drops them with a `DROPPED …` warning.

| ILM | ISM | Notes |
|---|---|---|
| phase `min_age` | transition `min_index_age` (after a rollover: `min_rollover_age`) | ILM counts from rollover, ISM from index creation unless `min_rollover_age` |
| `set_priority` | `index_priority` | `priority: null` dropped |
| `rollover` `max_age`/`max_docs`/`max_size`/`max_primary_shard_size` | `min_index_age`/`min_doc_count`/`min_size`/`min_primary_shard_size` | ISM needs `plugins.index_state_management.rollover_alias` on the index (warned) |
| `allocate` | `replica_count` + `allocation` (`require`/`include`/`exclude`, `wait_for: false`) | |
| `forcemerge` | `force_merge` | |
| `shrink` | `shrink` (`num_new_shards` / `max_shard_size`) | OpenSearch 1.x has no ISM `shrink`: refused there unless `--lenientPolicyConversion` |
| `readonly` | `read_only` | |
| `delete` | `delete` | `delete_searchable_snapshot` dropped (warned) |
| `migrate`, implicit tier migration | — | dropped with a warning (OpenSearch has no data tiers) |
| `_meta.description` (a string) | `description` | other `_meta` keys (nested ones too), a non-string `description` and `deprecated` dropped (warned) |
| `frozen` phase, `searchable_snapshot`, `downsample`, `unfollow`, `freeze`, `total_shards_per_node`, `index_codec`, `allow_write_after_shrink: true`, `max_primary_shard_docs`, `min_*` rollover conditions, a `rollover` / `forcemerge` / `shrink` without its size, unknown phases, actions, action options or phase keys | — | **refused** unless `--lenientPolicyConversion` |
| `wait_for_snapshot` (any phase) | — | **refused even with `--lenientPolicyConversion`**, naming the policy and phase: ISM has no equivalent, and dropping it would delete the indices without waiting for the snapshot (L-651a) |
| a `min_age` that is not a whole-number duration (`7`, `"soon"`, `null`, `1.5h`, which OpenSearch rejects) | — | **refused even with `--lenientPolicyConversion`**: read as 0 it would start the next phase (e.g. `delete`) at once (L-651) |

Converted policies are not attached to indices (ILM attaches through `index.lifecycle.name`, ISM through
`ism_template` or `POST _plugins/_ism/add/<index>`); the tool says so after converting. Tested on OpenSearch 1.3,
2.x and 3.x, from ES 7.17, 8.6, 8.19 and 9.1 exports and cluster to cluster. A policy with no convertible phase
(empty `phases`, `frozen` only) is refused even with `--lenientPolicyConversion`. Property oracle (L-507): 150 random
ILM policies × strict/lenient on OpenSearch 1.3.20, 2.19.7 and 3.9.0; every converted policy was accepted.

A set of policies is all or nothing at conversion: every policy is converted and checked before the first one is
written, so one refused policy writes none of them (`--ignore-errors` skips it and writes the rest). If the cluster
itself then rejects a policy partway through the set, the run is rolled back before it exits (L-651a): policies this
run created are deleted, policies it overwrote get their previous body back (newest first; nothing it did not write is
touched, and policies after the rejected one are never sent). The error names the rejected policy, what was rolled back
and whether the rollback succeeded; if a delete or restore fails, it names exactly which policies could not be put back
and how they were left. `--ignore-errors` skips the rejected policy and keeps the rest (no rollback). Live-tested on
OpenSearch 1.3.20, 2.19.7 and 3.9.0 with an unknown ISM action injected third of four. The first policy writes on a fresh OpenSearch
cluster can race another client creating the ISM config index; that rejection is retried, not read as an existing policy. L-651: a proptest over every phase × every ILM action (and unknown ones) × convertible and unconvertible
options (4,096 cases per run, 300,000 once) checks against an independent model that exactly the unsupported parts
are refused (strict) or dropped one `DROPPED` line each (lenient) and that the result is a well-formed ISM policy;
live, OpenSearch 2.x and 1.3.20 accept every one of the 322 converted policies of the exhaustive set.

Odd but valid policies (L-652): phases in any JSON order become states in lifecycle order with forward-only
transitions; a missing `min_age` means no delay; a `min_age` or rollover `max_age` keeps its value and unit as
written (`0ms`, `30s`, `90m`, `12h`, `1d`, `1000d`, `500micros`, `7nanos`; a zero age in any unit gives an
unconditional transition); a delete-only policy, a rollover-only hot phase, nested `_meta` and unicode policy names
(`politique-é-日本`) convert. Live, OpenSearch 1.3.20, 2.19.7 and 3.9.0 accept all 42 such policies and return the
states, transitions and units unchanged.

Elasticsearch built-in policies (L-653): with `--filterManaged false`, every built-in ILM policy of ES 7.10.2,
7.17.29, 8.6.1, 8.19.23 and 9.1.2 (6 to 46 per version: `logs`, `metrics`, `synthetics`, `*-days-default`,
`*@lifecycle`, `ilm-history-ilm-policy`, `slm-history-ilm-policy`, `watch-history-ilm-policy*`, APM, Fleet,
`.deprecation-indexing-ilm-policy`, `.monitoring-8-ilm-policy`) converts strictly, with nothing dropped, ids with a
leading `.` or an `@` kept as they are. Each one marked `_meta.managed: true` is warned as "Elasticsearch-managed
(built-in) policy: on OpenSearch it is an ordinary policy that nothing manages or updates". With the default
`--filterManaged` those are skipped and named instead (an info line, shown with `--debug`; only the lower
`Total Writes` shows it otherwise); ES 7.10 marks none of its built-ins managed, and
`.monitoring-8-ilm-policy` is not marked on 8.x/9.x, so those are always restored. OpenSearch 1.x has no ISM
`shrink`, so there the `*-days-default` / `*-days@lifecycle` built-ins (warm-phase shrink; 5 on 7.17/8.6, 10 on
8.19/9.1) are refused before any write, naming policy, phase and action, and `--lenientPolicyConversion` drops only
the shrink. Live, OpenSearch 1.3.20, 2.19.7 and 3.9.0 accept the 7.17.29, 8.19.23 and 9.1.2 sets. ISM → ILM (OpenSearch policies to Elasticsearch) is refused
before any write, with or without `--convertIlmToIsm`, `--lenientPolicyConversion` or `--ignore-errors`; the
error names the refused policies (the first 10, then "and N more") and says nothing was written.

OpenSearch reporting Elasticsearch 7.10.2 (L-654): with `compatibility.override_main_response_version: true`
(dynamic on OpenSearch 1.x and 2.x, rejected by 3.x), `GET /` reports `7.10.2` with no `version.distribution` and no
`build_flavor` (the tagline stays OpenSearch's). That `GET /`, or a `7.10.2` with neither field that a proxy
rewrote, is followed by `GET _nodes?filter_path=nodes.*.version,nodes.*.build_flavor`: OpenSearch nodes give the
real version (the oldest node counts) and the run continues as that OpenSearch with a warning; nodes with a
`build_flavor` are Elasticsearch. If `_nodes` does not answer, the OpenSearch tagline still means the OpenSearch 1.x
feature set (warned); with no tagline either the run fails before any dump request, naming the setting, instead of
guessing Elasticsearch. Live on OpenSearch 1.3.20 and 2.19.7 with the setting on, a `--searchType pit` dump detects the real
version and never uses the Elasticsearch PIT (1.3.20 falls back to scroll; 2.19.7 uses the OpenSearch PIT).

Field types without an equivalent (L-655): restoring a mapping or index (`--type mapping`/`index`) checks every
field (`properties`, nested objects, multi-fields, `runtime` and `dynamic_templates`) against the target's engine
and version before any write. Types the target has no handler for, for example `flattened`, `dense_vector`,
`semantic_text` or `histogram` on OpenSearch and `flat_object`, `knn_vector`, `semantic` or `xy_shape` on
Elasticsearch, X-Pack types on the Elasticsearch OSS build, or a type newer than the target, are refused with one
error that lists each field as `path` (type, why, closest type) and says nothing was written; the mapping is not
converted. With `--ignore-errors` only the indices with such fields are skipped (counted as failed). A
`sparse_vector` is told apart by its definition (OpenSearch's has `method`). A cluster-to-cluster data copy between
engines whose target index does not exist is refused the same way before any read or write, because the target
would create it with dynamic mappings and guess other types; create the index from a converted mapping first.
That copy asks the source for its engine (`GET /`) before searching; a source that does not answer skips the check.
Plugin and unknown types are left to the server. Not checked: data restored from a file (its types are unknown),
an existing target index, and index templates. The API-071 expectation table is unchanged (its fixture has none of
these types). Live, the refusals match what the server rejects on OpenSearch 1.3.20, 2.19.7, 3.9.0 and
Elasticsearch 7.10.2 (OSS and default), 7.17.29, 8.6.1, 8.19.23 and 9.1.2.

Features the target does not have (L-656): restoring templates (`--type template`/`index_template`/
`component_template`), settings or indices checks them against the target before any write. A composable template
with `data_stream` on the Elasticsearch OSS build (data streams are X-Pack) or before 7.9, and settings the target
rejects as unknown (`index.lifecycle.*` on OpenSearch, which attaches ISM policies with
`index.plugins.index_state_management.policy_id`; `index.lifecycle.*`, `index.frozen` and `index.xpack.*` on the
OSS build) are refused with one error naming each item, the feature, the reason and the exclusion that drops it
(`--excludeTemplateFields=data_stream`, `--excludeSettings=...`); nothing is written. With `--ignore-errors` only
those items are skipped (counted as failed). Templates are all prepared before the first PUT. ILM policies are
refused on the OSS build and on OpenSearch (use `--convertIlmToIsm`), `--searchType pit` falls back to scroll with a
warning where there is no PIT, and credentials sent to the OSS build (which has no security) give a warning, since
it does not check them. Writing to a data stream with the default `--bulkAction index` fails per document; the
first such failure prints a hint to use `--bulkAction create`. Checked live on Elasticsearch 7.10.2 (OSS and
default), 7.17.29, 8.6.1, 8.19.23 and 9.1.2, and OpenSearch 1.3.20, 2.19.7 and 3.9.0.

## Bulk dump and load (searchdump-multi)

`searchdump-multi` dumps every index whose name matches a regex to a directory, one file set per index, and loads
such a directory back. It runs one `searchdump` per index and item type (found next to its own executable, then
`$SEARCHDUMP_BIN`, then `PATH`), so all flags, retries and credential handling are the main tool's. It is not a
copy of Node's `multielasticdump`; it follows `scripts/oslogs.py` and writes the same file names, so either tool can
load the other's dumps.

```sh
# Every logs-* index, broken out per application into DIR/app/, DIR/web/, ... (zstd level 3)
searchdump-multi --input=https://src:9200 --output=/data/logs \
  --match='^logs-(?P<group>[a-z]+)-' --layout=group --compression=zstd3 --parallel=4

# Data streams logs-* with the templates and policies they need, data in 1 GB parts
searchdump-multi --input=https://src:9200 --output=/data/ds --matchType=datastream --match='^logs-' \
  --types=data,component_template,index_template,policy --fileSize=1gb

# Load it all back (any layout; uses /data/logs/multielasticdump.json when present)
searchdump-multi --direction=load --input=/data/logs --output=https://dst:9200 --parallel=4
```

- Selection: `--match` (unanchored regex, default all), `--exclude`. Indices starting with `.` are left out unless
  `--includeHidden` is set or `--match` starts with `^\.`. Closed indices are skipped on dump.
  `--matchType index,alias,datastream` (default `index`, any combination) says what `--match` is tested against:
  `alias` selects every index behind a matching alias (and takes the `--layout group` capture from the alias name);
  `datastream` dumps each matching data stream as one unit and leaves its backing `.ds-*` indices out.
- Layout: `flat` (`DIR/INDEX.*`, as `oslogs.py`), `index` (`DIR/INDEX/`) or `group` (`DIR/GROUP/`, from the `group`
  named capture of `--match` or its first capture group).
- Files per index: `INDEX.settings.json`, optional `INDEX.analyzer.json`, `INDEX.mapping.json`, optional
  `INDEX.alias.json`, and `INDEX.data.json.gz` / `.zst` / `.json` (`--compression gzip|zstd1|zstd3|none`, default
  gzip). `--fileSize=1gb` or `--maxRows=N` splits each data file into `INDEX.data.split-0.json.gz`, `-1`, ... (as
  `searchdump` names parts); load finds the parts, loads them in order and checks their summed record count.
  A data stream gets `STREAM.datastream.json` (its `GET _data_stream` description) and its data file.
- Cluster-wide types, dumped once per run into `DIR/_cluster/TYPE.json`: `policy`, `component_template`,
  `index_template`, `template`, `script`. Load applies them before any index, in that order (policies before the
  templates that name them, component templates before the index templates built from them).
- `--types` picks items (dump default `settings,mapping,data`; load default every per-index type plus every
  cluster-wide file present, missing files skipped; `all` selects everything); `--ignoreType` removes some.
  Node's flag names work too: `--ignoreData`, `--ignoreMapping`, `--ignoreSettings`, `--ignoreTemplate`,
  `--ignoreAlias`, `--ignoreAnalyzer` (bare or `=true` removes the type, `=false` adds it), `--includeType`
  (instead of `--types`), and `--ignoreIndex` / `--ignoreChildError` (accepted; Node's `index` type is our
  settings + mapping, and one failed index never stops the others here). Node ignores analyzer and alias unless
  told otherwise, so pass `--ignoreAlias=false` to a Node load of our alias files.
- Dump checks: the record count of each data file (or the parts together) against the source `_count` (an empty
  file for a non-empty index fails it). `multielasticdump.json` records each index's directory, types, data file or
  parts, records and status, and each cluster-wide file; `--resume` skips indices already done.
- Load (as `oslogs.py restore`): an index or data stream that already exists fails (or is skipped with
  `--skipExisting`, which also creates cluster-wide items only where missing); settings first, with `--lifecycle drop`
  (default: `--excludeSettings=index.lifecycle,index.routing.allocation.include._tier_preference`, also applied to
  templates) or `convert` (`--convertIlmToIsm` for policies, which does not rewrite `index.lifecycle` settings, so use
  `drop` for OpenSearch); the analyzer only when settings were not loaded (the settings already carry
  `index.analysis`); then the mapping (PUT `_mapping` directly if that run fails), aliases, data, and a `_refresh` +
  `_count` check against the file's record count. Data streams load with `--bulkAction=create` into the stream name,
  which needs its index template on the target (dump it with `--types=data,index_template,component_template`).
  `--prefix`/`--suffix` rename targets (indices and streams, not cluster-wide items).
- `--parallel` (default 2) indices at a time, started in name order (`--order desc` reverses it); `--interval=MS` (default 0) is the least time between starting one
  index and the next, as Node's `--interval`.
- Data runs get `--limit=5000 --scrollTime=10m --retryAttempts=5 --retryDelay=2000 --timeout=300000 --maxResponseMb=0`
  (`--noDefaultDataFlags` drops them); anything after `--` is passed to every data run, and a flag given there replaces the default of the same name (`-- --limit=200`). Credentials:
  `SEARCHDUMP_INPUT_USERNAME`/`_PASSWORD` and `SEARCHDUMP_OUTPUT_USERNAME`/`_PASSWORD`, or `--httpAuthFile`.
- Per-run logs go to `--logDir` (default a temp dir; one log per data part); a failure prints the last lines.
  `--dryRun` lists what would run. Exit status: 0 all done, 1 any item failed or nothing matched, 2 usage error,
  130 interrupted.
- Not in this tool (Node `multielasticdump`): `--customMatcher` and `--transform` (JavaScript), `--delete`.
  Cluster-wide items are not filtered by `--match`.
- Node compatibility (`--nodeCompat`, dump only): writes the layout Node's `multielasticdump` 6.x loads with its
  defaults: `DIR/INDEX.json.gz` (data), `DIR/INDEX.settings.json.gz`, `INDEX.mapping.json.gz` and
  `INDEX.template.json.gz` (all legacy `_template`s, per index, as Node does), all gzipped. The directory holds only
  those files, because Node treats every entry in it as an index; the manifest and any other cluster-wide types go to
  the sibling `DIR.meta/` (or `--metaDir`). Default types are `settings,mapping,template,data` (Node's load needs all
  four unless it gets `--ignoreX=true`). Refused with it: `--fileSize`/`--maxRows` (Node 6.x cannot load split
  parts), zstd, layouts other than `flat`, and `--matchType datastream` (Node loads data with the `index` op).
  Restore with Node's `multielasticdump --direction=load --input=DIR --output=URL`, or with this tool, which also
  loads `DIR.meta/_cluster/` first. This tool's load reads Node-made dumps too (both namings, plain or gzipped
  metadata). Tested against the real elasticdump 6.125.1 npm package both ways.
- Test: `scripts/it/multielasticdump-roundtrip.sh` (two in-memory fake nodes, no containers; set `NODE_ED_BIN` to the
  `node_modules/.bin` of an elasticdump 6.x install to add the Node checks).

## Object storage and SFTP (OpenDAL)

Besides Elasticsearch/OpenSearch URLs, `$` and plain paths, `--input` and `--output` accept storage URLs. They go
through [OpenDAL](https://opendal.apache.org) 0.59; plain paths keep the native file code and do not change.

| URL | Store |
|---|---|
| `s3://BUCKET/KEY` | S3 and S3-compatible servers (MinIO, RustFS, ...) |
| `b2://BUCKET/KEY` | Backblaze B2 (native API) |
| `sftp://USER@HOST:PORT/ABSOLUTE/PATH` | SFTP over the system `ssh` client |
| `fs:///PATH` or `file:///PATH` | local files through OpenDAL (mostly for testing) |

`http(s)://` is still a cluster and a bare path is still a native file. Compression (`--compressionType`, `.gz`/`.zst`
names) and splitting (`--fileSize`, `--maxRows`, parts `KEY.split-0.json.gz`, ...) work as for files, in both
directions; S3 and B2 writes are multipart uploads (`chunk_mb`, default 16, `concurrent`, default 4).

```sh
# dump to MinIO with zstd in 1 GB parts; credentials from the INI file
searchdump --input=https://src:9200/logs --output='s3://backups/logs/logs.json?endpoint=https://minio:9000' \
  --outputStorageConfig=/etc/searchdump/s3.ini --compressionType=zstd --fileSize=1gb

# restore from SFTP
searchdump --input=sftp://backup@vault:22/srv/dumps/logs.json --output=https://dst:9200/logs

# copy a file between stores (no cluster): s3 -> sftp
searchdump --inputStorageConfig=s3.ini --input=s3://backups/logs/logs.json \
  --output=sftp://backup@vault:22/srv/dumps/logs.json
```

**Secrets never go in the URL.** A URL with a password (`user:pw@`) or a secret query parameter
(`secret_access_key`, `session_token`, `sse_c_key`, `application_key`) is refused with a usage error that says where to
put it. Each setting is looked up in this order, first match wins:

1. a URL query parameter (non-secret settings only): `?endpoint=...&region=...`;
2. the INI file given with `--inputStorageConfig` / `--outputStorageConfig` (or `SEARCHDUMP_INPUT_STORAGE_CONFIG` /
   `SEARCHDUMP_OUTPUT_STORAGE_CONFIG`), the storage counterpart of `--httpAuthFile`: top-level `key = value` lines,
   overridden by a `[s3]` / `[b2]` / `[sftp]` section and then by a `[s3.BUCKET]` / `[sftp.HOST]` section. Unknown
   keys get a warning;
3. the environment: `SEARCHDUMP_<SCHEME>_<SETTING>` (`SEARCHDUMP_S3_ENDPOINT`, `SEARCHDUMP_SFTP_KEY`, ...), then
   the standard names: `AWS_ACCESS_KEY_ID`, `AWS_SECRET_ACCESS_KEY`, `AWS_SESSION_TOKEN`, `AWS_ENDPOINT_URL_S3` /
   `AWS_ENDPOINT_URL`, `AWS_REGION` / `AWS_DEFAULT_REGION`, `B2_APPLICATION_KEY_ID`, `B2_APPLICATION_KEY`.

```ini
# s3.ini (chmod 600)
endpoint = https://minio.example:9000
region = us-east-1
key = 
secret = 
ca_file = /etc/searchdump/minio-ca.pem
sse_c_key_file = /etc/searchdump/sse-c.key
```

Settings (INI aliases in brackets):

- `s3`: `endpoint`, `region` (default `us-east-1`), `access_key_id` (`key`), `secret_access_key` (`secret`),
  `session_token` (`token`), `virtual_host_style` (default path style), `allow_anonymous` (unsigned requests),
  `disable_config_load` (ignore `~/.aws`), `storage_class`, `sse_c_algorithm`, `sse_c_key`, `sse_c_key_file`,
  `ca_file`, `insecure`, `chunk_mb`, `concurrent`.
- `b2`: `application_key_id` (`key`), `application_key` (`secret`), `bucket_id` (B2 needs the bucket id as well as
  the name), `chunk_mb`, `concurrent`.
- `sftp`: `key` (`key_file`; a private key file, otherwise ssh-agent / `~/.ssh` defaults), `user` (or `USER@` in the
  URL), `known_hosts` = `strict` (default; the host must already be in `~/.ssh/known_hosts`), `add` (trust a new host
  once and record it) or `accept` (trust any host key; avoid).

Every secret (keys, tokens, the SSE-C key) is masked in logs, `--debug` output and errors, like the cluster
passwords, and any storage URL that is printed has its userinfo password and secret query values masked. An INI file or SSE-C key file readable by other users is used
with a warning.

**SSE-C** (S3 server-side encryption with a customer key; MinIO and RustFS support it): set `sse_c_key_file` (a file
with the 32 raw key bytes or their base64) or `sse_c_key` (base64, INI or env only); `sse_c_algorithm` defaults to
`AES256`. It is applied with OpenDAL's `server_side_encryption_with_customer_key`. The server does the encryption:
the key is sent with every request (so use `https://`; MinIO refuses SSE-C over plain HTTP), the server does not
store it, and the object cannot be read without the same key (a read without it or with another key fails). Losing
the key loses the data. Reads need the same setting as the write.

Other behaviour:

- `--overwrite` is needed to replace an existing object, as for files. A split set on a store is not atomic: parts
  appear one by one, and a failed run deletes the parts it wrote (an interrupted one may leave some).
- `sftp://` needs the `ssh` client on `PATH` (OpenSSH; the server side can be any SFTP server, tested with OpenSSH
  and Dropbear + OpenSSH's `sftp-server`). A connection failure hints at the usual causes (host key, user, key).
- Requests time out after 300 s of no progress and are retried 5 times with jittered backoff. TLS uses the same
  rustls setup as cluster connections (`ca_file`, `insecure` per store).
- Cargo features `opendal` (S3, B2, fs) and `opendal-sftp` are on by default; a build without them refuses storage
  URLs with a clear error.

**`sftp://` in the container images (L-810i, unreleased; 1.0.0 images lack it).** OpenDAL's SFTP backend runs the
system OpenSSH `ssh` client (Dropbear's client is not compatible). Since L-810i every image built with a package
manager installs it: Debian-based images (`dhi-debian` default, `public`) get `openssh-client` (no recommends,
`ssh-keysign` without its setuid bit), Alpine-based images (`dhi-alpine`, `alpine`) get `openssh-client-default`.
The COPY-only `dhi-static` image still has no `ssh`, so `sftp://` does not work there. Give the key with
`key_file` in a `--inputStorageConfig`/`--outputStorageConfig` file (mounted read-only and readable by uid 65532)
and pre-seed `known_hosts` (the image has no `~/.ssh`); `known_hosts = accept` works but trusts any host key. In
1.0.0 images SFTP works only with the host binaries and release archives on systems that have OpenSSH.

### searchdump-multi with storage

- `--direction dump --output=s3://BUCKET/PREFIX` dumps to a local staging directory (`--stagingDir`, default a temp
  dir) and uploads the finished tree only when every item succeeded; otherwise the staging directory is kept and the
  error says how to `--resume` with `--stagingDir`.
- `--direction load --input=sftp://...` downloads the tree to the staging directory first, then loads as usual.
- `--direction copy --input=SRC --output=DST` copies a whole dump directory between any two of local paths, `s3://`,
  `b2://`, `sftp://` and `fs://` byte for byte (format, compression, split parts, manifest and `_cluster/` as they
  are), checking each object's size; existing targets are refused unless `--overwrite`. No cluster is involved, and
  layout is not converted.
- `--inputStorageConfig` / `--outputStorageConfig` as above; `--metaDir` is refused with storage URLs.

Tests: unit tests in `src/remote.rs`, fs round trips in the lib tests, and container scripts in
`scripts/opendal-tests/` (`run-all.sh`; S3 on MinIO and RustFS with SSE-C, SFTP on OpenSSH and Dropbear, cross-store
and OpenSearch runs; see TESTING-COMPLETED.md). B2 is only unit tested (no mock exists); `b2-real.sh` is for a real
account.

## Renaming indices

`--renamePrefix old:new` (repeatable) and `--renameIndex '<regex>:<replacement>'` (repeatable, Rust `regex`
syntax, `$1`/`${name}` captures; write `${1}abc` when letters follow) rename every index a run writes, for
`data`, `mapping`, `settings`, `index`, `analyzer` and `alias` (the indices the aliases point at). The first
matching rule wins (`--renamePrefix` rules are tried first); unmatched names stay as they are.

* Order: rename first, then `--prefix`/`--suffix`.
* With an explicit single `--output-index` (or an index in the output URL) rename rules are a usage error, as is a
  file output.
* `--renameAliases` also renames alias names (alias, index and template types) with the same rules.
* `--renamePatterns` also renames template `index_patterns` (and legacy `template`) and ISM `ism_template`
  patterns; without it they are left alone.
* Each rename is logged once (`Renaming index 'a' -> 'b' (rule)`); invalid results (uppercase, illegal
  characters, leading `-_+`, `.`/`..`, > 255 bytes) fail before anything is written.

```bash
# dump every logstash-* index, restore it as logstash-srctag-*
searchdump --input http://es:9200/logstash-* --output idx.json --type index
searchdump --input http://es:9200/logstash-* --output data.json
searchdump --input idx.json  --output http://os:9200 --type index --renamePrefix logstash-:logstash-srctag- --renameAliases
searchdump --input data.json --output http://os:9200 --renamePrefix logstash-:logstash-srctag-
```

## Container image

`Dockerfile` is a multi-stage build: a Rust builder (release profile, `CARGO_BUILD_JOBS=2` by default
because the fat-LTO link peaks at ~1.5-2 GB) and one of two runtime stages. Builder and runtime always use the
same libc. Every build uses the same Dockerfile through the `BUILDER_IMAGE`/`BUILDER_TAG`/`BASE_IMAGE`/`BASE_TAG`
build args and a `--target` (bases are pinned by multi-arch index digest in the Makefile):

| Make target (`--type`) | Tag | Builder | Runtime (`--target`) | libc / allocator | Size | HIGH/CRITICAL CVEs (2026-11-01) |
|---|---|---|---|---|---|---|
| `make image` (`dhi-debian`, **default**, L-810c) | `:<ver>`, `:latest` | `dhi.io/rust:1.99.1-debian13-dev` | `dhi.io/debian-base:trixie-debian13-dev` (`runtime-shell`) | glibc 2.41 / mimalloc | 90 MB | 19 won't-fix High CVEs (18 grype High, 1 trivy-only) accepted for this image (L-810e, [SECURITY-CVE.md](SECURITY-CVE.md#dhiio-debian-13--dev-make-image-the-default-since-l-810c)); apt CVE-2026-109518 fixed by the 2026-11-08 base |
| `make image-dhi-static` (`dhi-static`; default until L-810c) | `:<ver>-static` | `dhi.io/rust:1.99.1-debian13-dev` | `dhi.io/static:20250419-glibc-debian13` (`runtime`, COPY-only, no shell) | glibc 2.41 / mimalloc | 24 MB | grype 5 High (glibc, libgcc-s; won't-fix / not-fixed upstream), trivy 0 |
| `make image-dhi-alpine` (`dhi-alpine`) | `:<ver>-alpine` | `dhi.io/rust:1.99.0-alpine3.23-dev` | `dhi.io/alpine-base:3.24-alpine3.24-dev` + apk `bash ca-certificates libgcc` (`runtime-shell`) | musl / musl | 21 MB | **0** (0 at any severity) |
| `make image-alpine` (`alpine`, public bases, not default) | `:<ver>-alpine-public` | `docker.io/library/rust:1.99.1-alpine3.24` | `docker.io/library/alpine:3.24.3` + apk `bash ca-certificates libgcc` (`runtime-shell`) | musl / musl | 20 MB | 0 (2026-10-29) |
| `make image-public` (`public`, public bases, not default) | `:<ver>-debian-public` | `docker.io/library/rust:1.99-trixie` | `docker.io/library/debian:trixie-slim` (`runtime-shell`) | glibc 2.41 / mimalloc | 90 MB | 54 High matches (grype) / 43 (trivy), none fixable (2026-10-29) |

Since L-810c (owner decision 2026-11-11) the default is the dhi.io Debian 13 image with a shell (`debian-base -dev`),
glibc + mimalloc, built for linux/amd64: every default build uses dhi.io bases for both the builder and the runtime.
It is larger than the shell-less `static` image and carries more (unfixable) CVEs, but it can run wrapper scripts
inside the container. The `static` image (the default from L-808b to L-810c) is still built as `dhi-static`: the
smallest (16 MB base: glibc, libgcc_s, the CA bundle, tzdata and a `nonroot` 65532 user; no shell, no package manager),
with the fewest CVEs (grype 5 High / 24 distinct against 14 / 46 for `debian-base:trixie-debian13` and 46 / 71 for the
`-dev` base); the comparison is in `results/local/L-808b.md` and [SECURITY-CVE.md](SECURITY-CVE.md). The Alpine
variant (`:<ver>-alpine`, bash, musl's allocator, 0 CVEs) is the smallest image with a shell. The two public-base
types (`public`, `alpine`) need no dhi.io login and are kept for that, but they are not the default. On Alpine runtimes the Dockerfile
runs `apk upgrade`, then adds `bash`, `ca-certificates` and `libgcc`. Alpine-packaged rustc links `libgcc_s`
dynamically.

**Image name:** set the full repository and tag of the built image with `scripts/build-image.sh --imagename registry.example.com/team/searchdump-rs:1.2.3`, or with the `SEARCHDUMP_IMAGE` environment variable, which also works for every `make image*` target (`SEARCHDUMP_IMAGE=registry.example.com/team/searchdump-rs:1.2.3 make image-dhi-debian`). It replaces the default tags; the script adds `:<version>` to a bare repository name.

**Prerequisites:** podman (or docker with the buildx plugin). No Rust is needed on the host: crates are vendored inside a container from the pinned builder image (`VENDOR_ON_HOST=1` uses host cargo >= 1.91 instead). `scripts/build-image.sh` checks these and the dhi.io login up front and stops with a clear message. `dhi.io` (Docker Hardened Images) needs a pull login: `podman login dhi.io` or `docker login dhi.io` with a Docker
account. Anonymous pulls return `authentication required`. The login stays in the container tool's auth file and
never enters a build layer.

**Offline build (L-808i).** Crates are fetched first and the image build runs offline: `scripts/build-image.sh` and
every `make image*` target first run `scripts/vendor-crates.sh`, which runs `cargo vendor --locked` inside a throwaway
container from the pinned builder image (repo mounted at `/src`, files stay owned by you) into the gitignored `.vendor/`
(about 270 MB, refreshed on each build; `rm -rf .vendor` removes it). The builder copies it in and runs
`cargo build --locked --offline` from the vendored sources, in a `RUN --network=none` step, so the build needs no DNS or
crates.io access. The host needs no Rust (`VENDOR_ON_HOST=1` uses host cargo instead). Behind a proxy, `HTTPS_PROXY`,
`HTTP_PROXY`, `NO_PROXY` and `CARGO_HTTP_PROXY` are passed into the vendoring container; the container tool still
pulls the base images, and the Alpine stages still run `apk`, with its normal network.

Every image contains `/usr/local/bin/searchdump` and `searchdump-multi` plus the `elasticdump`, `elasticdump-rs`
and `multielasticdump` symlinks, runs as uid
65532 (non-root), has a CA bundle (`SSL_CERT_FILE=/etc/ssl/certs/ca-certificates.crt`), uses
`/data` as its writable working directory, and carries OCI labels (version, revision, created,
base name).

**`sftp://` in the container images (L-810i, unreleased; 1.0.0 images lack it).** OpenDAL's SFTP backend runs the
system OpenSSH `ssh` client (Dropbear's client is not compatible). Since L-810i every image built with a package
manager installs it: Debian-based images (`dhi-debian` default, `public`) get `openssh-client` (no recommends,
`ssh-keysign` without its setuid bit), Alpine-based images (`dhi-alpine`, `alpine`) get `openssh-client-default`.
The COPY-only `dhi-static` image still has no `ssh`, so `sftp://` does not work there. Give the key with
`key_file` in a `--inputStorageConfig`/`--outputStorageConfig` file (mounted read-only and readable by uid 65532)
and pre-seed `known_hosts` (the image has no `~/.ssh`); `known_hosts = accept` works but trusts any host key. In
1.0.0 images SFTP works only with the host binaries and release archives on systems that have OpenSSH.

### `scripts/build-image.sh`

Builds any image type locally (nothing is pushed unless `--push` is given):

```bash
scripts/build-image.sh                              # default dhi-debian, amd64: searchdump-rs:<ver>, :latest
scripts/build-image.sh --type dhi-static            # shell-less dhi.io/static: searchdump-rs:<ver>-static
scripts/build-image.sh --type dhi-alpine            # Alpine/musl, musl allocator: searchdump-rs:<ver>-alpine
scripts/build-image.sh --type public                # public Debian bases, no login (not default): <ver>-debian-public
scripts/build-image.sh --platform both              # linux/amd64 + linux/arm64 manifest list (podman)
scripts/build-image.sh --platform linux/arm64 --tag registry.example.com/team/searchdump-rs:1.0.0-arm64
scripts/build-image.sh --tag registry.example.com/team/searchdump-rs:1.0.0 --push
scripts/build-image.sh --dry-run --type dhi-alpine  # print the podman commands only
scripts/build-image.sh --help
```

Options: `--type dhi-debian|dhi-static|dhi-alpine|public|alpine` (default `dhi-debian`; the older
`--variant glibc|alpine` still works and means `dhi-static` / `dhi-alpine`), `--platform linux/amd64|linux/arm64|both`
(default `linux/amd64`; the Makefile targets take `PLATFORM=`),
`--tag REF` (repeatable; replaces the default tags), `--name REPO` (default `localhost/searchdump-rs`), `--jobs N`
(`CARGO_BUILD_JOBS`, default 2), `--push`, `--dry-run`. The base pins are read from the Makefile. A foreign platform
runs the Rust build under emulation, so the host needs qemu-user-static (binfmt_misc) and it is slow; `both` builds a
podman manifest list. `--push` uses only the container tool's existing registry login (`podman login <registry>`);
the script never reads, takes or stores credentials. `CONTAINER=docker` works for single-platform builds.

```bash
make image                 # default: dhi.io Debian 13 -dev runtime with a shell, amd64 (after `podman login dhi.io`)
make image-dhi-debian      # the same as `make image`
make image-dhi-static      # dhi.io static glibc, no shell (:<ver>-static, after the login)
make image-dhi-alpine      # dhi.io Alpine/musl (after the login)
make image-public          # public Debian 13 bases, no login (not default)
make image-alpine          # public Alpine/musl bases, no login (not default)
make test-image            # --version, --help, non-root, compat symlinks (each run by name), CA bundle,
                           # --read-only, executables, base digest (default IMAGE_VARIANT=dhi-debian)
make test-image IMAGE_VARIANT=dhi-alpine   # the same for :<ver>-alpine (variants: dhi-debian, dhi-static,
                                           # dhi-alpine, public, alpine)
make sbom scan             # syft SBOM (CycloneDX + SPDX), grype + trivy (fail on HIGH/CRITICAL) of IMAGE_VARIANT
make image REGISTRY=registry.example.com/team IMAGE_NAME=searchdump IMAGE_TAG=1.0.0

# podman
podman run --rm -v "$PWD/dumps:/data:Z" localhost/searchdump-rs:1.0.0 \
  --input=http://es:9200/my_index --output=/data/my_index.json --type=data
# docker (same image, Node-style entrypoint name)
docker run --rm -v "$PWD/dumps:/data" --entrypoint elasticdump localhost/searchdump-rs:1.0.0 \
  --input=/data/my_index.json --output=http://es:9200/my_index --type=data
```

Nothing in the Makefile pushes; tag and push yourself if you want the image in a registry.
Mounted host directories must be writable by uid 65532 (or use `--user`).

## Releases

Releases are built by [`.github/workflows/release.yml`](.github/workflows/release.yml) when a version tag is pushed.
Nothing is published automatically: the workflow creates a **draft** GitHub release, which a maintainer reviews and
publishes by hand.

To cut a release:

1. Set `version` in `Cargo.toml` (for example `1.0.0-beta1`) and add a `## 1.0.0-beta1` section to
   [`CHANGELOG.md`](CHANGELOG.md); merge to `main`.
2. Tag that commit and push the tag: `git tag -s v1.0.0-beta1 && git push origin v1.0.0-beta1`
   (a manual run of the workflow with an existing tag does the same). The tag must match the `Cargo.toml` version.
3. Wait for the workflow, review the draft on the Releases page, then press **Publish**.

The workflow runs the RC behaviour-diff gate (`scripts/sup/rc-diff.sh`; unsigned behaviour changes fail it) before
anything is pushed or drafted. The draft contains:

- `searchdump-rs-<version>-<target>.tar.gz` for x86_64 and aarch64, glibc (`*-linux-gnu`, glibc 2.28+, mimalloc) and
  musl (`*-linux-musl`, static): `searchdump`, `searchdump-multi`, the `elasticdump`, `elasticdump-rs` and
  `multielasticdump` symlinks, `LICENSE`, `THIRD-PARTY-NOTICES.md`, `README.md`, `CHANGELOG.md`, the whole
  `examples/` tree, and `scripts/perf` (the release-eval harness);
- `searchdump-rs-<version>.cdx.json`, a CycloneDX SBOM of the Rust dependencies;
- a `.sha256` file per asset and a combined `SHA256SUMS`;
- release notes from the version's `CHANGELOG.md` section (GitHub's generated notes if there is none), plus the image
  tags.

Every archive, the SBOM and every pushed image have a GitHub build-provenance attestation:
`gh attestation verify searchdump-rs-1.0.0-x86_64-unknown-linux-gnu.tar.gz --repo robertgoley/searchdump-rs`.

Container images: every image type is built, tested with `scripts/test-image.sh` and pushed to
`ghcr.io/robertgoley/searchdump-rs:<version>-<type>` (`dhi-debian`, `dhi-static`, `dhi-alpine`, `public`, `alpine`;
linux/amd64). The default type, `dhi-debian`, is also tagged `<version>` and `latest` for a final release; a
pre-release (a version with a hyphen, such as `1.0.0-beta1`) is marked as a pre-release on GitHub and only moves the
channel tag (`beta`), never `latest`. A manual run can turn the image push off (`push_images`), or only for the
dhi.io-based types (`push_dhi_images`). Docker Hardened Images are Apache-2.0 (DHI Community), and Docker's terms allow
redistributing them as part of your own software, which is what these images do.

## Reliability

* Partial search results (`timed_out`, failed shards) are errors, not silently written.
* Retries: transport errors and 5xx/429 are retried at `--retryDelay`; scroll continuations only on 429
  (re-sending one after a network error could skip a page). Bulk writes with explicit ids are idempotent;
  a 20-run fault-injection suite (connections cut mid-response) restores exact copies.
* File output is staged, fsynced and committed atomically (hard link without `--overwrite`, so a file created
  meanwhile is never clobbered). `--output` must be a regular file path: a directory, a symbolic link (the link and
  its target are both left alone; pass the target path to write there), a FIFO or device (`/dev/stdout`: use
  `--output $`) or a missing parent directory is a usage error (exit 2) before anything is read. With
  `--overwrite`, the replaced file's mode is kept.
  Symlinked directories in the parent path are followed (`linkdir/out.json` with `linkdir -> /real/dir` writes
  `/real/dir/out.json`); only the final file itself may not be a link. A dangling parent link is a usage error
  that names the link and its missing target (L-644a).
* A run that is killed outright (`kill -9`, the OOM killer, a power cut) cannot clean up, but it never leaves a
  partial file at the output path: the output (each split part) is absent, the old file, or complete. A split set
  killed mid-commit can be torn, but the parts not yet moved into place are still staged whole
  (`.d.split-N.json.part-PID-TS`) and files being replaced with `--overwrite` were moved aside whole
  (`.d.split-N.json.edrs-replaced-PID`). These hidden files are **left in place, never deleted**: the next run to
  the same output prints a warning naming each one with its size (D-5); check and remove them yourself.
  `--overwrite` replaces only the output itself (and its split parts), never these files; with `--overwrite` the
  warning says so (L-805a). A killed
  restore never changes its input file; the cluster got the first documents of the file, without gaps (with the
  default `--concurrency`). Checked by `kill -9` at random points, 50 runs each for dump, split and restore (L-648).
* A second Ctrl+C (or SIGTERM) while the first one is still being cleaned up force-quits at once (130 / 143). It
  never commits output: an unfinished `--output` stays uncommitted, the exit message says so, and its hidden staging
  files are named by the next run. Anything else it could not clean up (an index `--closeIndex` closed) is named
  too (L-649).
* Ctrl+C clears the scroll / closes the PIT (ES input pipelines).
* Ctrl+C (or SIGTERM) during a restore into a cluster (file -> cluster or cluster -> cluster) stops reading,
  lets the bulk requests already sent finish (none is cut off), prints `Total Writes` and exits 130 / 143 with
  the `--offset` to resume from, e.g. `rerun the same command with --offset 1230` (plus `--size N` when the run
  had a `--size`). Every input document before that offset was sent (failed ones are counted in `Failed Writes`
  and not retried). The offset is exact for a file input; for a cluster input only while the source index is
  unchanged and its search order is stable (a sort in `--searchBody`). A sliced dump (`--slices` > 1) has no
  single order: it says so and is simply rerun. File output still removes its staged files (L-647).
* Resuming a stopped run (L-650, checked end to end with random INT / TERM / `kill -9` and digest-equal results):
  * **Restore into a cluster:** rerun with the printed `--offset` (and `--size`). Chained over several stops it
    is exact, with any `--concurrency` (bulk requests finishing out of order included): every document is
    written once, none is skipped, and the result equals an uninterrupted run (by `_id`, so a rerun after
    `kill -9` overwrites rather than duplicates; with `--offset` picked by hand after `kill -9`, count from what
    the cluster has).
  * **Dump to a file (or split set):** rerun the same command with `--overwrite`. Without it the rerun refuses
    (`already exists, quitting`). A stopped run leaves the earlier output and no staging files; a killed one
    may leave hidden files, which the rerun names. A Ctrl+C during the setup, before any document is read, now
    also removes the staged output (before L-650 it force-quit and left the `.part` file behind); the same holds
    for a metadata dump (`--type mapping`, `settings`, ...) to a file (L-650a). A signal that comes after the
    commit, while the process is still exiting, still exits 130 / 143, with the complete new output.
  * **`--scrollId`** continues the scroll *from where the cluster's cursor is*: pages already fetched, by an
    earlier request or a killed run, are not returned again. It is meant for a scroll you opened yourself (keep
    the first page from your own `_search?scroll=` request, then pass its `_scroll_id`); this tool never prints
    scroll ids (redaction). A scroll cleared by Ctrl+C, expired after `--scrollTime`, or lost with its node
    cannot be resumed: the run exits 1 with `--scrollId: the cluster no longer has this scroll ...`, commits
    nothing, and says to rerun without `--scrollId`.

## Security notes

* Passwords never appear in logs, `--debug` output, error messages or panic reports, in any form (as given,
  percent-encoded, JSON-escaped, or inside the `Authorization: Basic` token), even when a server echoes the request
  back. Prefer `--httpAuthFile` or the environment variables so they also stay out of shell history and `ps`.
  Values shorter than 4 characters are only masked inside the Basic token (masking them everywhere would mangle
  messages).

### Credentials

Each direction resolves its basic-auth username and password on its own (L-645). Highest first:

1. `--httpAuthFile` (INI with `user=` and `password=`, used for both directions). **When it is given, its pair
   wins over every other source** in both directions, including flags and URL credentials (L-645b, owner decision
   2026-11-14; a behaviour change from 1.0.0, where the auth file only filled in missing credentials). A warning
   names the sources it replaced (never their values).
2. `--input-username`/`--input-password` (aliases `--username`/`--password`) and
   `--output-username`/`--output-password`; each flag overrides one field and they never apply to the other
   direction.
3. `user:password@` in the `--input`/`--output` URL (percent-decoded).
4. `SEARCHDUMP_{INPUT,OUTPUT}_{USERNAME,PASSWORD}` (when its `_USERNAME` is set), used only when 2 and 3 together
   do not give both a username and a password; a flag still wins over them.

`searchdump-multi` uses the same order for its own cluster requests (the auth file over URL userinfo over env) and
passes `--httpAuthFile` on to every child run.

* A trailing CR/LF is removed from every value (an auth file saved with CRLF line ends, `$(cat file)`), so it never
  turns into a confusing 401. An empty password is sent as is (`user:`).
* In a URL, `#`, `?`, `/` and `@` inside the password must be percent-encoded (`%23`, `%3F`, `%2F`, `%40`); `:`,
  spaces and unicode work as they are. An unencoded `#`/`?`/`/` is a usage error (exit 2) that says so without
  quoting the password, instead of a dump to the wrong host; or pass the password with `--input-password` /
  `--output-password`, where any character works.
* A `--httpAuthFile` readable by every user on the machine is used, with a warning to `chmod 600` it.
* `--insecure` / `NODE_TLS_REJECT_UNAUTHORIZED=0` disable certificate verification and print a warning.
* Encrypted client keys: PKCS#8 only (convert legacy keys with `openssl pkcs8 -topk8`).

## Memory

Peak RSS ≈ `(bufferSize + slices + concurrency) × limit × avg_doc_size × ~3 + ~15 MB` (**changed in L-802, from
× 1.5 + 10 MB, pending lead review**: the old estimate was exceeded by up to 127% by concurrent restores and sliced
copies at 1M documents; `results/local/L-802.json`). Measured with glibc's malloc before L-808b (Elasticsearch 7.17, ~1 KB docs): 24 MB dumping at
`--limit 5000`, 30–80 MB sliced (`--slices 2`–`16`), 180–330 MB restoring at `--limit 5000 --concurrency 4`–`8`,
85–160 MB copying. Peak RSS grows slowly with the number of documents in concurrent restores and copies (restore
`--concurrency 8 --limit 5000`: 196 MB at 100k docs, 268 MB at 1M, 346 MB at 10M with `MALLOC_ARENA_MAX=2`), and
stops growing on long runs when glibc is limited to two malloc arenas (`MALLOC_ARENA_MAX=2`). With glibc's default
arenas the same 10M-doc restore reached 537 MB and was still growing slowly, so set `MALLOC_ARENA_MAX=2` for long
concurrent restores in tight memory (builds with glibc's malloc, `--features system-allocator`; restores and copies ran at the same speed within 10%, sliced
dumps within this box's noise; musl/Alpine builds were not measured). File-to-file runs stay at 14–40 MB for ~1 KB
docs at any size up to 10M docs.
Allocators (L-808b): glibc Linux builds (the release `*-linux-gnu` binaries and the default container image) use
mimalloc; musl builds (the `*-linux-musl` binaries and the `:<ver>-alpine` image) use musl's own allocator. The
glibc-arena figures above (`MALLOC_ARENA_MAX=2`) and the `MALLOC_*` tuning below apply only to builds with glibc's malloc
(`cargo build --features system-allocator`); mimalloc and musl ignore those variables. Measured with mimalloc (L-802a,
`results/local/L-802a.md`, confirmed for the default build in `results/local/L-808b.md`; ~1 KB docs,
`--concurrency 8 --limit 5000`): peak RSS 312 / 335 / 364 MB restoring at 100k / 1M / 10M docs, 278 / 388 / 419 MB
copying, 68 / 82 / 131 MB dumping and 176 / 148 / 212 MB for `--slices 4`, at the same speed as glibc's malloc. That is
below glibc's default arenas on long concurrent restores and copies (520 / 684 MB at 10M) and above them on dumps;
the × 3 estimate above still covers every mimalloc run except the 10M copy (≈ 7% over, inside the 25% tolerance of
`scripts/load/memcheck.py`).
musl builds use the least memory (154 / 222 / 18 / 34–44 MB restore / copy / dump / sliced at 10M) with plain dumps
15–20% slower. With `--features system-allocator`, `MALLOC_MMAP_THRESHOLD_=131072` gives about the same footprint
as musl (158 / 226 / 20 / 40 MB at 10M, flat from 1M to 10M; glibc default 520 / 684 / 75 / 165 MB,
`MALLOC_ARENA_MAX=2` 318 / 432 / 50 / 65 MB) with the same dump slowdown (137–140k docs/s against 162–178k), and
`MALLOC_ARENA_MAX=2` stops the slow growth of long concurrent restores. `--features jemalloc` ran at the same speed.
When the first batch shows that the estimate (still computed as × 1.5 + 10 MB in `src/memplan.rs` until the lead
reviews the change above) exceeds the memory available (the cgroup's `memory.max` or
`MemAvailable`, whichever is smaller), a warning names the flags to lower; under real cgroup caps of 64–256 MB every
run inside the estimate completed (`scripts/res/low-memory.sh`, `results/local/L-635.json`).

## Performance

On the recorded runs below, searchdump was **1.30× to 6.69× faster** than Node elasticdump 6.125.1
(wall-clock), depending on the scenario. The largest published speed-up is a gzip dump of 1M documents on
Elasticsearch 7.17 (**6.69×**); on OpenSearch 2.19.7 at 100k documents the gzip dump is **5.4×**. Copy on the
single-node ES 7.17 setup was **1.30×** — engine-bound there (searchdump used about half a core).

**Legend:** "× faster" is Node wall time divided by searchdump wall time (higher means searchdump is faster).
The same direction is used in every table.

**Fast settings.** Defaults match Node (`--limit 100`, concurrency 1). For throughput, a typical starting point
is `--limit 5000 --concurrency 4` (the restore and copy rows below). For scroll dumps, add `--slices` equal to the
primary-shard count; PIT (`--searchAfter --pit`) tolerates more slices than shards. See the tuning note at the end
of this section.

All numbers come from recorded runs in this repository; follow the links for the raw data. Absolute figures
depend on the cluster, network and disk; the ratios to Node elasticdump 6.125.1 on the same host are the portable
result.

**1M documents, Elasticsearch 7.17** (about 1 KB per doc, 1.02 GB NDJSON, 2-shard index, 8 shared vCPUs; medians of
3 runs; `scripts/load/nodecmp.py`, [`results/local/L-808.md`](results/local/L-808.md)).
Source: 2026-11-01, L-808 (`27d3b7a`), pre-rename build.

| Scenario | Node s | searchdump s | × faster | CPU-s Node / searchdump | RSS MB Node / searchdump |
|---|---|---|---|---|---|
| dump, defaults (`--limit 100`) | 25.04 | 15.38 | 1.63× | 19.57 / 5.50 | 309 / 44 |
| dump `--limit 5000` | 13.82 | 6.86 | 2.01× | 15.50 / 4.02 | 354 / 72 |
| dump `--limit 5000 --fsCompress` | 42.63 | 6.37 | 6.69× | 49.66 / 9.10 | 1,416 / 77 |
| dump, searchdump `--slices 2` (= shards; Node unsliced) | 13.59 | 3.88 | 3.51× | 15.64 / 3.96 | 356 / 98 |
| dump, searchdump PIT `--slices 8` (Node unsliced) | 13.59 | 2.11 | 6.44× | 15.64 / 4.62 | 356 / 216 |
| restore `--limit 5000 --concurrency 4 --noRefresh` | 33.22 | 17.66 | 1.88× | 35.97 / 8.37 | 623 / 250 |
| copy, same flags | 23.00 | 17.64 | 1.30× | 24.89 / 9.87 | 583 / 347 |

Node's output and searchdump's are digest-equal in every dump scenario. Restore and copy are bound by the single
engine node here (searchdump used about half a core); the CPU column (2.5–5.5× less) is the better guide.

**Sequential speed suite, Elasticsearch 9.1.2 with TLS and basic auth** (100k `logs` docs, `--limit 2000`, 1 GB
engine heap, medians of 3 runs after a warm-up; `scripts/perf/run-suite.sh`, run
[`results/perf/20261109T011630-1176214`](results/perf/20261109T011630-1176214/summary.md)).
Source: 2026-11-09, pre-rename build at `5c76605`. Speed-up values are Node seconds ÷ searchdump seconds from the
same published medians (the original report used searchdump/Node, lower-is-better).

| Case | searchdump s | Node s | × faster |
|---|---|---|---|
| dump, plain | 0.803 | 1.607 | 2.00× |
| dump, gzip | 0.704 | 4.517 | 6.42× |
| dump, gzip in parts | 0.804 | 2.614 | 3.25× |
| restore, plain | 4.818 | 5.924 | 1.23× |
| restore, gzip parts | 5.225 | 8.038 | 1.54× |
| copy | 4.821 | 5.926 | 1.23× |
| mapping / settings dump | 0.102 | 0.302 | 2.96× |
| multi dump, `--parallel 1` / 4 / 8 | 1.104 / 0.403 / 0.303 | 5.321 / 1.713 / 1.319 | 4.82× / 4.25× / 4.35× |
| multi restore, `--parallel 1` / 4 / 8 | 7.224 / 2.009 / 1.311 | 12.849 / 3.732 / 2.636 | 1.78× / 1.86× / 2.01× |

In the same run Node changed integers above 2^53 in the edge-case dump and exceeded the 1 GB memory cap restoring it;
searchdump kept the values. Other engines' runs are under [`results/perf/`](results/perf/).

**100k documents, OpenSearch 2.19.7** (512 MB heap, ≈ 42 MB of data; `scripts/benchmark/quick-vs-node.py`, medians of 3
runs, L-808; raw: [`docs/bench/l808-opensearch-2.19.7-100k.json`](docs/bench/l808-opensearch-2.19.7-100k.json)).
Source: 2026-11-01, L-808, pre-rename release profile at `e9d8cc5`.

| Scenario | Node 6.125.1 | searchdump | × faster | CPU-s Node / searchdump |
|---|---|---|---|---|
| dump, defaults (`--limit 100`) | 2.85 s | 1.30 s | 2.2× | 2.07 / 0.32 |
| dump `--limit 5000` | 1.00 s | 0.41 s | 2.4× | 1.22 / 0.19 |
| dump `--limit 5000 --fsCompress` | 3.32 s | 0.62 s | 5.4× | 3.41 / 0.74 |
| dump `--limit 5000 --compressionType zstd` | – | 0.36 s | – | – / 0.27 |
| restore `--limit 5000 --concurrency 4 --noRefresh` | 2.29 s | 0.76 s | 3.0× | 2.55 / 0.44 |
| copy `--limit 5000 --concurrency 4 --noRefresh` | 1.77 s | 0.92 s | 1.9× | 1.96 / 0.54 |

**Compression levels** at 1M docs (1.01 GB of NDJSON, edgen `logs` profile, Elasticsearch 7.17; medians of 3 runs,
zstd 19 one run; [`results/local/L-805.md`](results/local/L-805.md)).
Source: 2026-11-01, L-805 (`18c4334`), pre-rename build. Per-core speed is the uncompressed MB divided by
the CPU-s the codec adds over an uncompressed file-to-file run:

| `--compressionType` / `--compressionLevel` | gzip 1 | gzip 6 (default) | gzip 9 | zstd 1 | zstd 3 (default) | zstd 9 | zstd 19 |
|---|---|---|---|---|---|---|---|
| ratio | 5.2 | 9.9 | 9.6 | 8.2 | 8.0 | 10.5 | 13.6 |
| size vs gzip 6 | +90% | – | +3.5% | +21% | +25% | −5% | −27% |
| MB/s per core | ~1,000 | ~200 | ~120 | ~700–1,200 | ~530–580 | ~100 | ~1.8 |

A 1M-doc dump (about 7 s, bound by the cluster) took no longer with gzip 1/6 or zstd 1/3; gzip 9 and zstd 9 added
30–40%, and zstd 19 took 9.5 minutes. With the zlib-rs backend, gzip level 9 writes a slightly *larger* file than level
6, so it isn't worth using; `--compressionLevel 9` with gzip and levels 19 and up with zstd print a warning.

Tuning: sliced retrieval (`--slices`) is what uses more cores. With scroll, match `--slices` to the primary-shard
count (more slices than shards adds a per-document slice filter); PIT (`--searchAfter --pit`) tolerates more slices
than shards. Keep `--workers` ≥ `--slices`. Sliced output does not keep a global document order.
`scripts/benchmark/compare-with-elasticdump.sh` (from upstream elasticdump-rs) compares both tools on your own cluster.

## Testing

To run everything on a local server in one go (host prep, pinned images, release build,
integration matrix, load matrix, extra checks and the speed suite), run
`./start-local-testing.sh` as a normal user with rootless Podman logged in to dhi.io.
`--dry-run` prints the plan, `--steps` picks steps, `--update-baselines` records the first
speed baselines, and logs plus a summary go to `results/local-testing/<timestamp>/`.

The [sequential speed suite](scripts/perf/README.md) benchmarks dump/restore, copy,
codecs/splits, metadata and searchdump-multi parallelism against every pinned engine,
with correctness gates, per-engine baselines and Node 6.125.1 comparisons:
`./scripts/perf/run-suite.sh --dry-run` (plan) or `./scripts/perf/run-suite.sh` (rootless Podman).
Real-engine performance runs and initial baselines are pending.

Tests are split into tiers (`.config/nextest.toml`; each tier writes JUnit to `target/nextest/<tier>/junit.xml`):

```bash
cargo nextest run --workspace --profile unit       # T0 unit tests (all crates)
cargo nextest run --workspace --profile contract   # T1 CLI contract / injection / secrets / tool tests (mocks only)
cargo nextest run --workspace --profile it --features searchdump-rs/it      # T2, needs scripts/it/up.sh
cargo nextest run --workspace --profile load --features searchdump-rs/load  # T3 live long tests
scripts/load/run.sh L0 dump-restore-logs           # load harness (scripts/load/README.md)
cargo test --lib --bins --test cli_contract        # plain cargo test still works (T0 + part of T1)
scripts/it/run.sh all                  # podman: builds first, then every profile below
scripts/it/run.sh opensearch-plain     # one profile: opensearch-{plain,secure}, elasticsearch-{plain,secure}
OS_TAG=3 scripts/it/run.sh opensearch  # both OpenSearch variants against 3.x (default OS_TAG=2)
scripts/it/run.sh elasticsearch8       # both ES variants against the pinned 8.6.1 regression tag
scripts/it/run.sh cross                # ES <-> OpenSearch cluster-to-cluster (two nodes at once)
```

One engine-aware suite runs against every profile; expectations that differ per engine (PIT request shape and
tiebreaker, default settings keys, typeless APIs) are asserted per engine rather than duplicated.

| Profile | Image | Notes |
|---|---|---|
| `opensearch-plain` / `-secure` | `opensearchproject/opensearch:$OS_TAG` | secure = security plugin, basic auth, TLS, client-cert (PKI) auth |
| `elasticsearch8-plain` / `-secure` | `bitnamilegacy/elasticsearch:8.6.1-debian-11-r0` (`ES8_TAG`) | same setup as below; regression target for ES 8 |
| `elasticsearch-plain` / `-secure` | `bitnamilegacy/elasticsearch:9.1.2-debian-12-r0` (`ES_TAG`, latest tag) | bitnami env (`ELASTICSEARCH_HEAP_SIZE`, `ELASTICSEARCH_ENABLE_SECURITY`, `ELASTICSEARCH_ENABLE_REST_TLS`, PEM certs from a generated CA), data dir owned by the image's non-root UID 1001 |
| `es717-plain` / `-secure` | `docker.elastic.co/elasticsearch/elasticsearch:7.17.29` (official) | latest 7.17 patch; bitnamilegacy stops at 7.17.26 |
| `es710-plain` | `bitnamilegacy/elasticsearch:7.10.2` | the OSS build (no X-Pack: no PIT, ILM, security) |
| `es710-secure` | `docker.elastic.co/elasticsearch/elasticsearch:7.10.2` (official) | default distribution: security, PIT, ILM |
| `es819-plain` / `-secure` | `docker.elastic.co/elasticsearch/elasticsearch:8.19.23` (official) | bitnamilegacy stops at 8.18.0 |
| `os13-plain` / `-secure` | `opensearchproject/opensearch:1.3.20` | demo security config (`admin`/`admin`) |
| `cross` | one plain node of each | needs `CROSS_MIN_MB` (default 2200) MB free, otherwise SKIP |

`scripts/it/run.sh matrix` runs the four older/extra versions (plain and secure, one container at a time);
`run.sh es717` etc. run one version. Exact images, digests and the latest counts are in
[TESTING-MATRIX.md](TESTING-MATRIX.md). Minimum supported: Elasticsearch 7.x, OpenSearch 1.x (ES 6.8 is out of
scope; its typed paths remain unit- and mock-tested only).

The secured Elasticsearch profile uses basic auth over HTTPS and verifies client certificates
(`client_authentication: optional`; a rogue certificate is rejected), but cannot authenticate *by* certificate:
the PKI realm is a paid X-Pack feature with no bitnami switch, so the client-certificate tests send basic auth plus
the certificate there. Certificate-only auth is covered by the OpenSearch secure profile.

Cross-engine file dump/restore runs one container at a time: each plain profile exports a zstd dump plus the
expected result (one directory per engine version, e.g. `elasticsearch-8.19.23`), and the other engine restores every
such export later in the same `all` run. Cluster-to-cluster needs both nodes at
once (256m heaps, lean JVM flags, ~1.3 GB RSS together plus headroom); on hosts without that much free memory it
is skipped with a message instead of risking the OOM killer.

Heaps default to 512m (`OS_HEAP`; `ES_HEAP` defaults to the same). Elasticsearch 9 at 512m is ~950 MB RSS; on
hosts with ~1.5 GB free use `ES_HEAP=320m IT_LEAN_JVM=1` (smaller code cache, direct memory and thread stacks).

`scripts/it/run.sh` compiles everything before starting a container (an optimised build next to a running
node can exhaust memory on small hosts) and always removes its containers and data directories. The Node interop tests
run when `/tmp/ed/node_modules/.bin/elasticdump` exists.

## Acknowledgements

- [elasticdump-rs](https://github.com/tenyears-tech/elasticdump-rs) by TenYears (MIT), the Rust project this one was
  forked from and is a code derivative of. Its license and copyright notice are in [`LICENSE`](LICENSE).
- [elasticsearch-dump](https://github.com/elasticsearch-dump/elasticsearch-dump) (Node elasticdump, Apache-2.0),
  whose CLI, file formats and behaviour this tool follows. No Node code is included; twelve test fixtures are reused
  unchanged under Apache-2.0 ([`tests/fixtures/node`](tests/fixtures/node/SOURCE.md)).

## License

MIT, see [`LICENSE`](LICENSE) (the upstream elasticdump-rs license, Copyright (c) 2025 TenYears, which also covers
this derivative). Third-party material (the reused test fixtures, the bundled mimalloc and jemalloc C code, the Rust
crates) and its licenses are listed in [`THIRD-PARTY-NOTICES.md`](THIRD-PARTY-NOTICES.md); regenerate the crate list
with `scripts/third-party-notices.py` after a dependency change.

## Trademarks

Elasticsearch is a trademark of Elasticsearch B.V., registered in the U.S. and in other countries. OpenSearch is a
trademark of LF Projects, LLC. searchdump-rs is an independent project and is not affiliated with, endorsed by or
sponsored by Elasticsearch B.V., LF Projects, LLC, or the elasticsearch-dump project.
