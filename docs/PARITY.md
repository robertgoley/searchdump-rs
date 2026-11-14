## 3. Parity matrix

Legend:
* **Status** (branch `rewrite`, after M5; updated 2026-10-28): ✅ done and covered by tests · 🟡 partial · ❌ not done · ➖ N/A or out of scope.
* Before the rewrite most rows were ❌; see `docs/PLAN.md` §3 for the original assessment.
* **Ms**: the milestone that delivers it.
* "Default" is elasticdump's default.

### 3.1 Core, endpoints and types

| elasticdump option | Default | Status | Planned Rust implementation | Ms |
|---|---|---|---|---|
| `--input` | required | ✅ | `Endpoint::parse` → `Es{…}` (http/https), `Stdio` (`$`), `File(path)`. Host-only URL = all indices (`/_search`, `/_mapping`, `/_settings`) | M1 |
| `--output` | required | ✅ | Same enum. ES output via `BulkSink` / `MappingSink` / `SettingsSink` | M1–M3 |
| `--input-index` / `--output-index` | from URL | ✅ | `index[/type]` override; `type` used only for ES < 7 | M1 |
| `--type data` | data | ✅ | keep | – |
| `--type mapping` | – | ✅ | `MappingSource`/`MappingSink` (§6.4) | M3 |
| `--type settings` | – | ✅ | `SettingsSource`/`SettingsSink` (§6.5) | M3 |
| `--type index` | – | ✅ | `GET /{idx\|*}` → one `PUT /{idx}` (settings + typeless mappings + aliases), generated settings stripped, `--skip-existing` | M7 |
| `--type analyzer` | – | ✅ | `{idx}/_settings`; new index → created with the analysis; existing → error / `--skip-existing` / `--closeIndex` (close, PUT, always reopen). No 30 s green wait | M8 |
| `--type alias` | – | ✅ | `GET {idx\|*}/_alias/{alias\|*}`; one atomic `POST /_aliases`; targets follow output-index/prefix/suffix/rename; empty entries skipped; two-segment URL read as `index/alias` with a warning | M8 |
| `--type template` / `component_template` / `index_template` | – | ✅ | `GET _{kind}/{name\|*}` filtered by `--templateRegex` (cluster) and `--filterManaged`; sequential PUTs; component pre-check for `composed_of`; generated dates stripped; cross-engine fields stripped (c2c) or warned (file); ES < 7.8 refused for component/index templates | M9 |
| `--type policy` | – | ✅ | ES `_ilm/policy` / OS `_plugins/_ism/policies` (paged). ISM overwrite via `if_seq_no`/`if_primary_term`. ILM→ISM only with `--convertIlmToIsm` (`--lenientPolicyConversion` drops unsafe parts); ISM→ILM refused; ES OSS builds refused | M10–M12 |
| `--type script` | – | ✅ | `GET _cluster/state/metadata?filter_path=**.stored_scripts` → `{stored_scripts:[…]}`; `POST _scripts/{id}` | M11 |
| `--renamePrefix` / `--renameIndex` / `--renameAliases` / `--renamePatterns` (extension) | – | ✅ | Rename rules for every index-writing type; rename then prefix/suffix; usage error with an explicit output index or file output; names validated | M7–M13 |
| `--overwrite` | false | ✅ | Keep the staged atomic writer. Also applies to split parts | – |
| `--prefix` / `--suffix` | '' | ✅ | Applied to the output index name only | M2 |
| `--help` / `--version` | – | ✅ | Also add elasticdump-style long help text | M1 |
| `--debug` | false | ✅ | Also log each ES request line (method, redacted URL), like Node | M1 |
| `--quiet` | false | ✅ | keep | – |
| `--toLog` | true (false for `$`) | ✅ | Logs always go to stderr, so stdout output is never polluted. Accept the flag; `false` suppresses info logs | M1 |
| `--format human` | '' | ✅ | Pretty-print each doc. Low priority | M5 |

### 3.2 Data retrieval (ES input)

| Option | Default | Status | Planned | Ms |
|---|---|---|---|---|
| `--limit` | 100 | ✅ | Default 100. Page size for ES, batch size for file reads, bulk size for writes | M1 |
| `--size` | -1 | ✅ | Total cap. Shrink the final page's `size`, then stop and clear the scroll/PIT. With slices: a shared atomic budget, truncating the last batch | M2 |
| `--offset` | 0 | ✅ | Client-side skip of the first N docs. **Never** send `from=` with scroll (breaks ES 7+/OS). File input: skip N values | M2 |
| `--searchBody` (+`@file`) | match_all + `stored_fields:["*"]` + `_source:true` | ✅ | Keep JSON and `@file`. Apply Node's default body when none is given. `@` resolves relative to the CWD. `size` is overridden with `limit` (or with `--size` when smaller) | M1 |
| `--searchWithTemplate` | false | ✅ | `POST /_render/template {id,params}` → use `template_output` as the search body | M5 |
| `--searchBodyTemplate` | – | ➖ | Out of scope (JS code). Error | – |
| `--scrollTime` | 10m | ✅ | keep | – |
| `--scrollId` | null | ✅ | Resume: skip the initial search and start with `POST /_search/scroll`. Incompatible with slices | M2 |
| `--scroll-with-post` | false | ✅ | Accepted. Requests stay POST either way (GET with a body breaks some proxies) | M1 |
| `scrollRetryDelay` (undocumented) | 15000 | ✅ | Accepted and ignored. Partial pages are a hard error in Rust | M1 |
| `--searchAfter` | false | ✅ | Without PIT: `search_after` on `{index}/_search`. **Requires a user `sort` with a unique tiebreaker** (error if missing) | M2 |
| `--pit` | false | ✅ | With `--searchAfter` (or `--searchType pit`): flavor-aware PIT (§6.3), reusing the existing shared-PIT slice coordinator | M2 |
| `--pitKeepAlive` | 5m | ✅ | Default 5m | M1 |
| `--esCompress` (responses) | false | ✅ | keep (`Accept-Encoding: gzip`, transparent decode) | – |
| `--support-big-int`, `--big-int-fields` | false | ✅ | Raw passthrough is lossless in both directions (§6.2). Accepted as no-ops | M1 |
| `--parseExtraFields` | '' | ✅ | Extra meta fields copied into the bulk action (§6.6) | M2 |
| `--sourceOnly` | false | ✅ | Write only the raw `_source` bytes | M2 |
| rs-only `--slices`, `--workers`, `--bufferSize`, `--searchType`, `--requestTimeout`, `--username/--password` (= `--input-username/--input-password`; D-4 adds `--output-username/--output-password`), `--caFile`, `--insecure` | – | ✅ | Keep as documented extensions. `--caFile` becomes an alias of `--input-ca` | – |

### 3.3 Data writing (ES output)

| Option | Default | Status | Planned | Ms |
|---|---|---|---|---|
| bulk write | – | ✅ | `BulkSink`: NDJSON builder from raw docs, `PUT {index or host}/_bulk` | M2 |
| `--bulkAction` | index | ✅ | `index` / `create` / `update` (`{"doc":…}`) / `delete` (no body line). Value validated | M2 |
| `--handleVersion` / `--versionType` | false / null | ✅ | Copy `_version` (or `version`) → `version`; pass `version_type` through | M2 |
| `--noRefresh` | false | ✅ (default cadence differs since 1.1) | 1.0: `POST {index}/_refresh` after each bulk call (parity). 1.1: every `--refreshInterval` seconds (default 5) plus once at the end; `--refreshInterval=0` = per batch (parity); with no output index only the written indices are refreshed, never `POST /_refresh`. `--noRefresh` skips it | M2 |
| `--ignore-es-write-errors` | **true** | ✅ (exit status differs) | Failed items are logged to stderr as JSON lines and counted. `false` → fail on the first failed item. Not given: the run continues like Node but exits **3** when any document was not written (L-444, Q3); given explicitly (`true`): exit 0 like Node | M2 |
| `--ignore-errors` | false | ✅ | Batch-level errors (an HTTP failure after retries) are logged and skipped, and the loop continues | M2 |
| `--skip-existing` | false | ✅ | Extension: settings/mapping sinks treat `resource_already_exists_exception` as success | M3 |
| `--esCompress` (requests) | false | ✅ | gzip the bulk body, `Content-Encoding: gzip` | M2 |
| `--params`, `--input-params`, `--output-params` | null | ✅ | JSON object → query string. Input params go on search/scroll, output params on bulk. `params` is the fallback for both and is also used on `_mapping` GET/PUT | M1 |
| `--headers`, `--input-headers`, `--output-headers` | `{"User-Agent":"elasticdump"}` | ✅ | Merge order: defaults → `headers` → direction headers. Applied to every request in that direction | M1 |

### 3.4 Files and compression

| Option | Default | Status | Planned | Ms |
|---|---|---|---|---|
| file / stdin input (`$`) | – | ✅ | `FileSource`: streaming reader of top-level JSON values (NDJSON fast path, concatenated-JSON fallback), batches of `limit` | M1 |
| file / stdout output | – | ✅ | Keep the staged writer, generalised behind `Sink` | – |
| `--fsCompress` | false | ✅ | Write through gzip. Read through gunzip, plus auto-detect the gzip magic `1f 8b` even without the flag (extension; logs a notice) | M4 |
| `--compressionLevel` | -1 (zlib default 6) | ✅ | 0–9; −1 maps to 6 | M4 |
| `--fileSize` | -1 | ✅ | `Splitter` sized in uncompressed **bytes** (Node counts UTF-16 units; documented divergence). Same grammar as the `bytes` module (`b,kb,mb,gb,tb,pb`, base 1024, decimals) | M4 |
| `--maxRows` | -1 | ✅ | `Splitter` by line count | M4 |
| `--compressionType gzip\|zstd` (extension, scope addition) | none | ✅ | zstd is the preferred codec; `--fsCompress` alone = gzip; default uncompressed | M4 |
| zstd write / read | – | ✅ | One complete zstd stream per file or split part (`.zst`); input auto-detected by magic bytes; levels 1–22 (−1 = 3) | M4 |
| split naming | `{base}.split-{n}{ext}[.gz]` | ✅ | Identical naming. Each part staged and committed atomically | M4 |
| reading split sets | n/a in Node | ❌ (optional, not done) | Extension: if `--input` is a directory or glob, read the parts in numeric `split-N` order | M4 (opt) |

### 3.5 Auth and TLS

| Option | Default | Status | Planned | Ms |
|---|---|---|---|---|
| URL basic auth | – | ✅ | keep (percent-decoding, redacted logs) | – |
| `SEARCHDUMP_INPUT_USERNAME/PASSWORD`, `SEARCHDUMP_OUTPUT_…` | – | ✅ | Used when the URL has no credentials | M1 |
| `--httpAuthFile` | null | ✅ | INI `user=`/`password=`. Applies to both endpoints and, when given, wins over the URL, `--{input,output}-username/-password` and env (L-645b, owner decision 2026-11-14; Node uses it only when the URL lacks a username or password). Error `Malformed Auth File`. Warns when world-readable; CRLF trimmed (L-645) | M1 |
| `--tlsAuth` | false | ✅ | Accepted. Rust **always** honours `--ca/--cert/--key` when given; Node silently ignores them without `--tlsAuth` (divergence). Warn when cert flags are set without `--tlsAuth` | M1 |
| `--ca`, `--input-ca`, `--output-ca` | null | ✅ | PEM bundle per direction. The shared `--ca` wins over the direction flag when both are set (Node assignment order) | M1 |
| `--cert` / `--key` (+ input/output) | null | ✅ | PEM client cert chain and key (PKCS#8 / PKCS#1 / SEC1) → rustls client auth | M1 |
| `--pass` (+ input/output) | null | ✅ | Decrypt an encrypted PKCS#8 key (`pkcs8` crate, `encryption` + `pem`). Legacy `Proc-Type: 4,ENCRYPTED` keys are unsupported: clear error with an `openssl pkcs8 -topk8` hint [uncertain whether users rely on them] | M1 |
| insecure | via `NODE_TLS_REJECT_UNAUTHORIZED=0` | ✅ | Keep `--insecure`, add `--input-insecure`/`--output-insecure`. Also honour `NODE_TLS_REJECT_UNAUTHORIZED=0`, with a loud warning | M1 |
| AWS SigV4, socks proxy | – | ➖ | Out of scope. Error | – |

### 3.6 Concurrency, throttling, timeouts, retries

| Option | Default | Status | Planned | Ms |
|---|---|---|---|---|
| `--concurrency` | 1 | ✅ | Max in-flight **write** batches. File sinks are forced to 1 to keep order | M5 |
| `--throttleInterval` | 1 ms | ✅ | Sleep after each batch is enqueued for writing | M5 |
| `--intervalCap` | 5 (dead in Node ≥ 6.119) | ✅ | Rate limiter: at most `intervalCap` request starts per `concurrencyInterval` window (p-queue semantics). **Off unless set explicitly** (Q3) | M5 |
| `--concurrencyInterval` | 5000 ms (dead) | ✅ | Window length for `intervalCap` | M5 |
| `--carryoverConcurrencyCount` | true (dead) | ✅ | If true, requests still in flight at a window boundary count against the next window | M5 |
| `--maxSockets` | ∞ | ✅ | Connection-pool cap plus a global semaphore on in-flight HTTP requests | M5 |
| `--timeout` | null (ms) | ✅ | `--timeout` in **ms** (parity). `--requestTimeout` kept as an alias in seconds. `--timeout` wins if both are set | M1 |
| `--retryAttempts` | 0 | ✅ | See §6.8 | M5 |
| `--retryDelay` | 5000 | ✅ | See §6.8 | M5 |
| `--retryDelayBase`, `--customBackoff` | – | ➖ (S3 only) | Accepted and ignored | – |

### 3.6a Engines and versions tested

See [TESTING-MATRIX.md](../TESTING-MATRIX.md) for the exact images, digests, counts and per-version gaps:
OpenSearch 1.3.20 / 2.x / 3.x, Elasticsearch 7.10.2 (OSS and default builds), 7.17.29, 8.6.1, 8.19.23 and 9.1.2,
each plain and secured, plus ES ↔ OpenSearch file and cluster-to-cluster legs. Minimum supported: ES 7.x,
OpenSearch 1.x.

### 3.7 Explicitly out of scope

The following are accepted with a warning, or rejected, according to the policy below:

* CSV (`csv://`, `--csv*`);
* S3/MinIO (`s3://`, `--s3*`);
* AWS (`--aws*`, `--sessionToken`);
* `--transform`, `--searchBodyTemplate`, `--inputTransport`/`--outputTransport`;
* `searchdump-multi`;
* `--inputSocksProxy/Port` and `--outputSocksProxy/Port`;
* `--heapSnapshot`;
* `--openSearchServerless` and `--force-os-version` (serverless offset/limit mode);
* `--delete` and `--delete-with-routing` (deferred; see §9);
* `--jsonLines` (dead in Node: it exists only in the defaults table).

**Policy:**
* An unknown `--type` → **error** (exit 2) naming it (all 11 Node types are implemented).
* A flag that only tunes an out-of-scope feature (e.g. `--s3Region`, `--csvDelimiter`, `--templateRegex`) → warning, then ignored, so existing wrapper scripts still run.
* A flag that would change the data if silently ignored (`--transform`, `--searchBodyTemplate`, `--inputTransport`, `--outputTransport`, `--delete`, s3/csv URLs, AWS signing, socks proxy) → **error**.

---

