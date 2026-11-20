# Examples

Worked dump and restore commands for [searchdump-rs](../README.md) 1.3.0. Start with the
[safe simple](#safe-simple-recommended) cases: they need no sizing flags. The other cases use
`--serverLimits` (new in 1.2.0; drop it, and set the sizes by hand, to run them with a 1.1.0 or
1.0.0 binary or image); the 1.1.0 behaviours they rely on or mention are marked (see
[New in 1.1.0](#new-in-110)).

These are starting points, not a production runbook. Adjust URLs, index names, resource
limits and page sizes for your cluster, and test outside production first.

**Disclaimer:** searchdump-rs is provided as is, without warranty of any kind. Read the
[Disclaimer](../README.md#disclaimer) and the [MIT license](../LICENSE) before you rely on
a backup. Verify every dump by restoring it and comparing document counts (and spot-checking
documents, mappings and settings) against the source.

Credentials belong in `SEARCHDUMP_INPUT_USERNAME` / `SEARCHDUMP_INPUT_PASSWORD`,
`SEARCHDUMP_OUTPUT_*`, `--httpAuthFile`, or `--inputStorageConfig` / `--outputStorageConfig`
(or the `SEARCHDUMP_*_STORAGE_CONFIG` environment variables). Since 1.1.0 an
`--httpAuthFile` **wins** over every other credential source for both directions, so use it only
when both sides share one user. Never put a password or access
key in a URL. The templates in this directory leave secret values blank; copy them, fill in a
private file (`chmod 600`), and do not commit the filled copy.

The default container image in the Kubernetes and Docker/Podman examples is the project's
dhi.io Debian image with a shell:

- published: `ghcr.io/robertgoley/searchdump-rs:1.3.0-dhi-debian` (published with `v1.3.0`;
  a `:1.2.0-*` tag also runs these examples; a `:1.1.0-*` tag works only without `--serverLimits`)
- local `make image` / `scripts/build-image.sh`: `localhost/searchdump-rs:1.3.0` (and
  `:latest`)

The image entrypoint is `searchdump`; `searchdump-multi` is next to it. It runs as uid 65532,
and `/data` is the writable working directory. The shell-less `dhi-static` image is not used
here, because several examples run a short `sh` loop (settings, then mapping, then data).
`sftp://` does not work inside the published 1.0.0 images (no `ssh` client); use `s3://`,
`b2://`, `fs://`, or the host binaries for SFTP. 1.1.0 images
(`make image`, `make image-public`, `make image-alpine`, `make image-dhi-alpine`) include the
OpenSSH client, so `sftp://` works there; `dhi-static` still has none. Give the key with
`key_file` in a storage-config INI mounted read-only and readable by uid 65532, and pre-seed
`known_hosts` (`known_hosts = accept` trusts any host key). See the
[container image](../README.md#container-image) section of the README.

## Layout

| Directory | What it is |
|---|---|
| [`bash/`](bash/) | Runnable scripts that call `searchdump` / `searchdump-multi` on the host (`PATH`, or `SEARCHDUMP_BIN` / `SEARCHDUMP_MULTI_BIN`) |
| [`docker-podman/`](docker-podman/) | The same cases, run with `podman` (default) or `docker` (`CONTAINER_ENGINE=docker`) |
| [`kubernetes/`](kubernetes/) | Job manifests, plus a shared PVC for the dump directory. Secrets are referenced, not filled in |

Each case has a dump and a restore (cluster-to-cluster copy is a single run). Shared templates:
[`http-auth.ini.example`](http-auth.ini.example), [`storage-config.ini.example`](storage-config.ini.example).

## Exit status and reruns

| Status | `searchdump` means | What to do |
|---|---|---|
| 0 | done | |
| 1 | permanent failure (bad URL or credentials, TLS, mapping errors, ...) | fix the cause first |
| 2 | usage error; **since 1.2** also `--serverLimits=check` when a value you set is above the cluster's limits | fix the command line, or lower the value |
| 3 | finished, but some documents failed to write | read the item errors; rerun only after fixing them |
| 4 | **since 1.1.0:** a transient failure after all retries (network, HTTP 429/5xx, lost scroll context) where nothing was written that a rerun would duplicate | **safe to rerun** as is, after a pause |
| 130 / 143 | interrupted (SIGINT / SIGTERM) | rerun |

1.0.0 reports transient failures as 1. The bash and Docker/Podman dump and cluster-copy scripts
wrap each `searchdump` run in `rerun_on_4` (`_common.sh`): on exit 4 they run the step again `RERUNS` times (default 1)
after `RERUN_DELAY` seconds (default 30). `very-large-index-dump.yaml` does the same with a
Kubernetes `podFailurePolicy` (retry only exit 4). `searchdump-multi` keeps its own codes (0 all
done, 1 an item failed or nothing matched, 2 usage, 130 interrupted); rerun it with `--resume`.

## Safe simple (recommended)

Since 1.2, `searchdump` and `searchdump-multi` read the cluster's limits once at the start (heap,
circuit breakers, indexing pressure, `http.max_content_length`, thread pools) and size every page,
bulk and, for `searchdump-multi`, how many indices run at once from them (`--serverLimits=auto`,
the default). The `safe-simple-*` cases pass **no sizing flags at all** and work on a 256 MB-heap
node and on a large cluster alike:

| Case | bash | Docker/Podman | Kubernetes |
|---|---|---|---|
| dump one index | [`safe-simple-dump.sh`](bash/safe-simple-dump.sh) | [`safe-simple-dump.sh`](docker-podman/safe-simple-dump.sh) | [`safe-simple-dump.yaml`](kubernetes/safe-simple-dump.yaml) |
| restore one index | [`safe-simple-restore.sh`](bash/safe-simple-restore.sh) | [`safe-simple-restore.sh`](docker-podman/safe-simple-restore.sh) | [`safe-simple-restore.yaml`](kubernetes/safe-simple-restore.yaml) |
| dump many (`searchdump-multi`) | [`safe-simple-multi-dump.sh`](bash/safe-simple-multi-dump.sh) | [`safe-simple-multi-dump.sh`](docker-podman/safe-simple-multi-dump.sh) | [`safe-simple-multi-dump.yaml`](kubernetes/safe-simple-multi-dump.yaml) |
| restore many | [`safe-simple-multi-restore.sh`](bash/safe-simple-multi-restore.sh) | [`safe-simple-multi-restore.sh`](docker-podman/safe-simple-multi-restore.sh) | [`safe-simple-multi-restore.yaml`](kubernetes/safe-simple-multi-restore.yaml) |

Each one first runs a preflight (`_preflight.sh`, or an init container in Kubernetes):
`--serverLimits=check` prints the limits it read and the sizes it will use, then exits **0** (go)
or **2** (a value you set is above what the cluster can take, or a usage error such as a bad
flag; the message above says which); the script stops on 2 before any data moves. `check` never
opens the output, so the preflight can name `--output=/dev/null`. `SERVER_LIMITS=min` keeps every page within half of what `auto` would use (a fragile
or shared cluster); `SERVER_LIMITS=off` reproduces the 1.1.0 sizes exactly and sends no probe.

When the limits cannot be read, the run never fails because of it; it falls back and says why in
one log line:

| Situation | What it uses |
|---|---|
| no `monitor` privilege (HTTP 401/403 on `_nodes/stats`) | the 1.1.0 sizes (`fallback-403`) |
| OpenSearch Serverless (`*.aoss.amazonaws.com`) or no node stats API (404/410/400/501) | the 1.1.0 sizes (`fallback-serverless`) |
| node stats slower than 5 s | the 1.1.0 sizes (`fallback-timeout`) |
| fields hidden by a filter or proxy | the 1.1.0 sizes (`fallback-hidden`) |
| AWS OpenSearch Service (`*.es.amazonaws.com`, `*.aos.*.on.aws`) | the probe as usual, and bulks never above 10 MiB, even on a fallback |

**When to override.** Set a size yourself only when you know the cluster better than its stats do
(a dedicated, idle cluster where you want bigger pages, or a proxy in front with a lower body
limit). A flag you pass always wins; with `auto`/`min` a value above the cluster's limits prints a
warning (and `check` exits 2). The `minimum-resources-*`, `multi-index-*` and `fastest-*` cases
below keep a commented manual-override block for that.

## Cases and tradeoffs

### Minimum resources

Small pages, one writer at a time, low memory and CPU. Use this on a laptop, a small CI job, or
a Bitnami / constrained Elasticsearch or OpenSearch node.

- `searchdump` already defaults to `--limit=100` and `--concurrency=1`. The examples also set
  `--workers=1`, a modest `--maxResponseMb`, and (on restore) `--concurrency=1`.
- **`searchdump-multi` 1.0.0 default data flags are too heavy for a small heap.** In 1.0.0,
  every data child gets `--limit=5000`, `--maxResponseMb=0` (no response-size cap) and
  `--scrollTime=10m`, from `DATA_FLAGS` in `src/bin/searchdump-multi.rs`. A single 5000-document
  page can take the node down during a **dump**, even with one thread and one shard: on
  2026-11-14 a 256 MB-heap Elasticsearch 7.10.2 or 8.6.1 (bitnamilegacy) died with
  `OutOfMemoryError`, and 8.19.23 answered HTTP 429 (`circuit_breaking_exception`) until the
  retries ran out. With 1.0.0 pass `-- --limit=500 --maxResponseMb=64` (a flag after `--`
  replaces the default of the same name) or `--noDefaultDataFlags` and set `--limit` yourself.
  Also use `--parallel=1`. The examples pass these overrides, so they are safe on both versions.
- **On a `searchdump-multi` load, also cap the bulk size:** `-- --maxBulkBytes=16mb`. The default
  100 MB bulk of 5000 large documents was refused with HTTP 429 (`indexing_pressure`, 10 % of
  the heap) even by a 1 GB heap. The multi-index restore examples pass `MAX_BULK_BYTES`
  (default `16mb`).
- **1.1.0 (item 1.1-10) changes these defaults:** data children page 1000 documents (fewer when
  the index's documents average over 16 KB stored, about 16 MB per page), keep the 256 MB response
  cap, and split loads into 16 MB bulks. `scripts/load/lowheap.py` checks dump and load with these
  defaults on 256 MB and 384 MB heaps (ES 7.10.2, 8.6.1, 8.19.23, 9.1.2).
- **1.2.0 (`--serverLimits`, default `auto`)** sizes pages and bulks from the node itself, so none of
  the overrides above are needed: `lowheap.py` passes every 256 MB and 384 MB cell with `auto` on
  ES 7.10.2, 8.6.1, 8.19.23, 9.1.2 and OpenSearch 1.3, 2 and 3 (results/local/L2-006.md). Use
  `--serverLimits=min` for a node that is also busy with other work.

Gzip (or zstd at `--compressionLevel=1`) is fine here; the point is the small page size, not the codec.

### Fastest dump (and a matching restore)

Higher `--limit`, more `--workers`, and, when the engine supports it, sliced PIT
(`--searchAfter --pit --slices=N`). The fast examples write zstd at level 1
(`--compressionType=zstd --compressionLevel=1`), which is much faster to write than gzip-6 and
still compact. (searchdump's own defaults are gzip when compression is on, and zstd level 3
when only `--compressionType=zstd` is given.) `--esCompress` asks the cluster for HTTP
compression on responses.

Set `--slices` to the **primary shard count** of the index, not higher. On Elasticsearch 7.10
and earlier (and some OpenSearch builds) too many slices fall back to `_id` slicing and can
be slower than an unsliced scroll. 1.1.0 caps `--slices` to the primary-shard count on
Elasticsearch 7.10 by default (`--sliceCap=auto`); on other engines, and with 1.0.0, pick `N`
yourself.

On restore and on cluster-to-cluster copy, raise `--concurrency` (concurrent write batches)
and consider `--noRefresh` so the cluster is not asked to refresh after every bulk batch.
1.1.0 already refreshes at most every 5 seconds plus once at the end (`--refreshInterval`), so
`--noRefresh` matters most with 1.0.0, which refreshes after every batch.

Faster also means heavier: more client CPU and memory, and more load on the cluster.

### Very large index

Split the data file with `--fileSize` (for example `1gb`) or `--maxRows`, and give the scroll
a longer `--scrollTime` (for example `30m`) so a slow page does not expire the context. Since
1.1.0, `--restartOnContextLoss=N` (set `RESTART_ON_CONTEXT_LOSS=N` in the very-large-index
examples) restarts a scroll dump into a file from the start, up to N times, when the cluster
loses the scroll context; the parts written so far are discarded and rewritten (checked
2026-11-14: a context deleted mid-dump of 20,000 documents into 11 `--fileSize` parts ended with
20,000 unique documents). It is refused for `--searchAfter --pit`, `--scrollId` and non-file
outputs, and 1.0.0 does not accept it, so the examples leave it off by default.
`searchdump-multi` records finished indices in `multielasticdump.json` and `--resume` skips
those whose files are still present. A single `searchdump` run has no `--resume`; if it fails
mid-index you usually start that index again. A failed split run may delete the parts it
wrote; an interrupted one can leave some, so check the dump directory before you retry
(`--overwrite` replaces existing parts).

Restoring a split dump with `searchdump` takes one invocation per part, in order, as in Node.
`searchdump-multi --direction=load` finds the parts and loads them in order itself.

### Secure TLS and auth

Use `--httpAuthFile` or the `SEARCHDUMP_*` environment variables (since 1.1.0 the auth
file wins when both are set, with a warning naming the replaced sources), plus `--ca` / `--cert` /
`--key` (or the `--input-*` / `--output-*` forms) for a private CA and optional client
certificates. Do not use `--insecure` in these examples.

`searchdump-multi` has `--httpAuthFile` and `--insecure`, but no `--ca` / `--cert` / `--key`.
Its own cluster calls (listing indices, counts) use the process's default trust store, which
honours `SSL_CERT_FILE` / `SSL_CERT_DIR` via rustls-native-certs. For a private CA, point
`SSL_CERT_FILE` at a bundle that includes it, or install the CA in the system store. mTLS
for `searchdump-multi` is not fully wired (1.0.0 and 1.1.0) (a `-- --cert= --key=` after `--` reaches
only data children, not the parent or settings/mapping runs). Use `searchdump` when you need
client certificates.

### Object storage

Dump to or restore from `s3://bucket/key` (MinIO and other S3-compatible endpoints work the
same). Put `endpoint`, `region`, `key` and `secret` in an INI file passed as
`--outputStorageConfig` / `--inputStorageConfig`, or in `SEARCHDUMP_S3_*` / `AWS_*`. Never
put keys in the URL; the tool refuses a URL that contains a secret. `--overwrite` is required
to replace an existing object. The same pattern works with `b2://`. See
[Object storage and SFTP](../README.md#object-storage-and-sftp-opendal).

### Cluster-to-cluster copy

One `searchdump` process per item type (`settings`, `mapping`, then `data`) from
`--input=https://src:9200/index` to `--output=https://dst:9200/index`. Use
`SEARCHDUMP_INPUT_*` and `SEARCHDUMP_OUTPUT_*` when the users differ. `--httpAuthFile` gives
one user to both sides; since 1.1.0 it wins over the `SEARCHDUMP_*` variables and the
`--input-*` / `--output-*` user flags, so leave it unset when the users differ (the copy
scripts print a note when both are set). This is not
`searchdump-multi --direction=copy`, which copies a dump **directory** between stores and
does not talk to a cluster.

### Multi-index (`searchdump-multi`)

`--match` selects indices, `--parallel` runs several at a time, `--compression` sets the data
codec (`gzip`, `zstd1`, `zstd3`, `none`). Load with `--direction=load`. Add `--resume` on a
retry, and `--skipExisting` on load if some target indices already exist. On a small-heap
source with 1.0.0, pass `-- --limit=500` as in [Minimum resources](#minimum-resources), and on a
small-heap target `-- --maxBulkBytes=16mb` (both are the 1.1.0 behaviour already).

## New in 1.1.0

In 1.1.0 binaries, release archives and images; not in 1.0.0 (see
[`CHANGELOG.md`](../CHANGELOG.md) and the
[1.1.0 release notes](../project/release/RELEASE-NOTES-1.1.md)).

- **Exit status 4** for transient failures that are safe to rerun (see
  [Exit status and reruns](#exit-status-and-reruns)). Scripts that test for exactly 1 should
  also accept 4.
- **`--httpAuthFile` wins** over URL credentials, the `--input-*` / `--output-*` user flags and
  `SEARCHDUMP_*` for both directions.
- **`--restartOnContextLoss N`** for scroll dumps into a file (opt-in in the examples through
  `RESTART_ON_CONTEXT_LOSS`).
- **OpenSSH client in the images** (not `dhi-static`), so `sftp://` works inside them; the
  Debian images carry no user-admin or init-script tools.
- **Small-heap-safe `searchdump-multi` data defaults** (1.1-10, see
  [Minimum resources](#minimum-resources)).
- **Refresh at most every 5 s** (`--refreshInterval`), **`--sliceCap`** on Elasticsearch 7.10,
  opt-in **`--scrollDocSort`**, and **`searchdump-multi --verifyCounts`**; the examples do not
  need them.

## How to run them

**Bash.** Install the [host binaries](../README.md#install-and-build) (or a release archive)
so `searchdump` and `searchdump-multi` are on `PATH`. From the repo root:

```bash
export ES_URL=https://localhost:9200 INDEX=logs
# export SEARCHDUMP_INPUT_USERNAME SEARCHDUMP_INPUT_PASSWORD   # or HTTP_AUTH_FILE=/path/http-auth.ini
examples/bash/minimum-resources-dump.sh
examples/bash/minimum-resources-restore.sh
```

Every bash and Docker/Podman script starts with `set -euo pipefail` and is configured through
the environment (see comments at the top of each `_common.sh`).

**Docker / Podman.** Default engine is `podman`; use `CONTAINER_ENGINE=docker` if you prefer
Docker. The default `--network=host` is convenient on Linux so `ES_URL=http://127.0.0.1:9200`
reaches a cluster on the host. The dump directory is mounted at `/data`. Uid 65532 must be
able to write it (the scripts create the directory and explain the permission options).

```bash
export ES_URL=http://127.0.0.1:9200 INDEX=logs
# SEARCHDUMP_IMAGE=localhost/searchdump-rs:1.3.0
examples/docker-podman/minimum-resources-dump.sh
```

**Kubernetes.** Create the PVC and the Secrets the Jobs refer to, then apply one Job at a
time. Details are in [`kubernetes/README.md`](kubernetes/README.md).

## Further ideas

Not given as full files, but useful once the starters fit:

- A filtered dump with `--searchBody=@query.json` (or `--searchWithTemplate`)
- Data streams: `searchdump-multi --matchType=datastream --types=data,component_template,index_template,policy`
- ILM to ISM on load: `searchdump-multi --lifecycle=convert` (see the README before using this on OpenSearch)
- Node-compatible layout: `searchdump-multi --nodeCompat`
- A Kubernetes `CronJob` around any of the dump Jobs, for a scheduled backup
- `searchdump-multi --direction=copy` to move an existing dump directory between `s3://`, `b2://`, `sftp://` (host binaries) or a local path
