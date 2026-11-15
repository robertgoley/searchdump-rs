# Differences caused by Node elasticdump being incomplete or buggy

This file lists only the places where searchdump behaves differently **because Node elasticdump
(6.125.1, `lib/…` paths below) is incomplete, buggy, or has dead behaviour**. Two other lists are separate:

* intentional design choices (zstd, staged split files, `--filterManaged`, …): README,
  "Intentional differences from Node elasticdump";
* every change to what gets loaded (stripped settings, conversions, renames): TESTING-MATRIX.md §3.

"File-compatible" means a file written by one tool restores with the other.

| # | Area | Node behaviour (source) | searchdump behaviour | File-compatible | Test coverage |
|---|---|---|---|---|---|
| 1 | PIT sort on OpenSearch | `searchBody.sort = ['_shard_doc']` for every engine (`lib/transports/__es__/_data.js:39`). OpenSearch rejects `_shard_doc` ("No mapping found for [_shard_doc]"), and a `_doc`-only sort was measured to lose documents across PIT pages | OpenSearch: `[{"_doc":"asc"},{"_id":{"order":"asc","unmapped_type":"keyword"}}]`; ES ≥ 7.12: `_shard_doc`; ES 7.10/7.11: `_doc` + `_id` | n/a (search only) | `engine::pit_request_shape_matches_engine`, `smoke::pit_dump_returns_all_docs_unsliced_and_sliced` (all profiles) |
| 2 | PIT on clusters without it | Opens a PIT and fails (ES OSS builds, OpenSearch < 2.4); sliced PIT fails on ES < 7.15 ("`slice` cannot be used outside of a scroll context") | Warns and falls back to scroll (sliced scroll for slices); same documents | n/a | `engine::pit_request_shape_matches_engine`, `data::exact_size_with_scroll_and_sliced_pit` on es710 / os13 |
| 3 | Dead throttling flags | `--intervalCap`, `--concurrencyInterval`, `--carryoverConcurrencyCount` are parsed and documented but have done nothing since 6.119.0 (p-queue removed) | Implemented with p-queue semantics, **only when set explicitly** | n/a | unit `http::gate::interval_cap_admits_cap_per_window`; IT `concurrency::interval_cap_and_throttle_timing` |
| 4 | `--offset` with scroll | Sends `from=` on the scroll request (`_data.js:108`); ES 7+/OpenSearch reject it | Skips the first N documents client-side | n/a | IT `smoke::size_and_offset`, contract `file_to_file_…_with_offset_and_size` |
| 5 | Client certificates | `--ca/--cert/--key` silently ignored unless `--tlsAuth` (`lib/transports/elasticsearch.js:102`) | Always honoured; warns when `--tlsAuth` is missing | n/a | `tls_auth` suite (secure profiles) |
| 6 | Hidden retries | `requestretry` with `maxAttempts: retryAttempts \|\| 5` (`elasticsearch.js:92`): `--retryAttempts 0` still retries 4×; strategy `HTTPOrNetworkError` (`:94`) never retries 429 | `--retryAttempts 0` = Node's implicit 4 retries unless `--noImplicitRetry`; 429 and 408 are retried; scroll continuations only when provably not applied | n/a | unit `http::retry::idempotent_requests_retry_network_5xx_429_408`, `scroll_continuations_retry_only_when_provably_not_applied` |
| 7 | OpenSearch policy restore never works | `async.forEachOf(data, …)` over the dump object (`_policy.js:47`): for an ISM dump (`{"policies":[…],"total_policies":N}`) the "policy names" are `policies` and `total_policies`; no `if_seq_no`/`if_primary_term`, so overwriting an existing ISM policy (409) is impossible | Iterates `policies[]`, strips `policy_id`/`last_updated_time`/`schema_version`, creates or overwrites with the sequence-number check; a second 409 is reported as a concurrent change | yes (same dump shape) | IT `meta_types::policy_round_trip_same_engine` (asserts Node's restore leaves the policy missing); contract `ism_overwrite_uses_sequence_numbers` |
| 8 | Policy list not paged | One `GET _plugins/_ism/policies/` (`_policy.js:11`): OpenSearch returns its default page (20) | Pages with `size=1000&from=N` until all are read | yes | contract `ism_list_dump_pages_through_all_policies`; IT dumps 25 policies |
| 9 | Alias host-only input | URL `${host}/${index}/_alias/…` with no index → `GET /undefined/_alias/*` (`_alias.js:12`) | `GET /*/_alias/*` | yes | contract `host_only_index_and_alias_dumps_use_wildcards_and_node_alias_url` |
| 10 | Alias restore aborts on empty lists | `cb(new Error('no aliases detected'))` for any index without aliases (`_alias.js:48`), failing the whole restore | Skips such entries | yes | IT `alias_round_trip_filters_prefix_and_node_forms` (wildcard dumps include alias-less indices) |
| 11 | Alias restore ignores output index / prefix / suffix | Uses the index names from the file as-is (`_alias.js:52`) | Targets follow `--output-index` (single source only), `--prefix`/`--suffix` and rename rules | yes | contract `alias_restore_is_one_atomic_post_with_prefixed_targets`; IT prefix restore |
| 12 | Index / settings host-only input | `GET ${base.url}/` with no index = `GET /` (`_setting.js:12`): writes the cluster banner as the "index" | `GET /*` | yes | contract `host_only_index_and_alias_dumps_use_wildcards_…`; IT wildcard dump |
| 13 | Generated settings not stripped for wildcard dumps | `settingsExclude` applied only to `output[this.base.index]` (`_setting.js:25`), so `uuid`, `creation_date`, `version`, `provided_name` survive for `*`/pattern dumps and the restore fails | Stripped for every index in the dump | yes | IT `index_round_trip_wildcard_dump_and_node_interop` |
| 14 | Analyzer restore | `PUT` the index first (fails if it exists; `_analyzer.js:78`), waits for `green` (`:95`, 30 s timeout; never green with replicas on one node), then close → PUT → open for every index in the file against the **one** output index; a failed PUT leaves the index **closed** (`:50`) | New index: created with the analysis, no green wait. Existing index: error, `--skip-existing`, or `--closeIndex` (close → PUT → always reopen, also on failure, Ctrl+C or SIGTERM (exit 130 / 143); before L-649 a signal exited at once and left it closed) | yes | IT `analyzer_create_close_index_and_failure_reopens`; contract `analyzer_close_index_reopens_even_when_the_update_fails`; chaos `l649_signal_while_the_analyzer_index_is_closed_reopens_it` |
| 15 | Default `--templateRegex` | `^(metrics\|logs\|.+_audit_log\|.+-index-template\|\..+)$` (`bin/elasticdump:41`) misses the ES 8/9 built-ins (`*@template`, `*@mappings`, APM/OTel …); the help text documents a different default, `((metrics\|logs\|\..+)(-.+)?)` (`lib/help.txt:346`) | Same regex kept for parity, plus `--filterManaged` (default on) dropping `_meta.managed: true` templates/policies; help text shows the real default | yes | IT `managed_templates_and_policies_are_filtered_on_elasticsearch` |
| 16 | Unordered template restores | `async.forEachOf` PUTs all templates in parallel (`_template.js:107`); an index template can arrive before its component templates | Sequential; `composed_of` components are checked first with "restore component templates first (--type component_template)" | yes | IT `templates_round_trip_order_and_node_fixtures`; contract `template_skip_existing_uses_create_and_index_template_needs_components` |
| 17 | Wrong-type files | A dump of one `--type` restored with another is sent anyway (e.g. an alias dump PUT as settings) | Shape check: "this input looks like a --type X dump …; use --type X" | yes | contract `unknown_type_and_file_to_file_metadata_are_usage_errors`; IT templates test |
| 18 | Parallel script restore | `async.each` (`_script.js:64`) | Sequential, `--skip-existing` honoured | yes | IT `scripts_round_trip_search_template_and_node` |
| 19 | ISM → ES / ILM → OpenSearch | Sends the other engine's body to the wrong API and fails with a server error | Clear refusal; ILM → ISM only with `--convertIlmToIsm` | n/a | IT `cross::*policy*` |
| 20 | Typed 6.x template/mapping files into 7+/OpenSearch | Sent as-is; rejected by typeless clusters | Single mapping type unwrapped with an info log | yes | IT templates test (Node `template_6x.json` fixture) |
| 21 | `--httpAuthFile` values | The `ini` parser cuts a value at an unquoted `#` or `;` (`password=a#b` → `a`), so such a password fails with 401; a CRLF file is handled (`lib/add-auth.js`) | Values are literal (`#`, `;`, `\` kept; quotes and surrounding spaces removed as in Node); a trailing CR/LF is removed from every credential source; a world-readable file is warned about (L-645) | yes | unit `endpoint::tests::auth_values_keep_special_characters`; contract `secrets::credential_flags_pick_their_endpoint_and_never_leak` |
| 22 | Mistyped or scheme-less cluster URLs | `isUrl` is a case-sensitive `indexOf('http://') === 0 \|\| indexOf('https://') === 0` (`lib/is-url.js`, `lib/ioHelper.js`); everything else is a file, so `HTTP://host` is a file too and `--output user:pw@host:9200` writes a file whose name holds the password; `http:/host`, `htp://host` become files or missing-file errors | Mistyped schemes (`http:/`, `http:`, `http//`, unknown `x://`, surrounding whitespace) and scheme-less outputs with `user:password@` are usage errors (exit 2) that never quote the value; scheme-less `host:port` / `user:pw@host` files warn (masked); `./` forces a file; `HTTP://` is a cluster (L-646) | yes | unit `endpoint::redaction_tests::mistyped_and_scheme_less_urls`; contract `cli_contract::l646_scheme_less_and_mistyped_urls` |
| 23 | Ctrl+C / SIGTERM during a restore into a cluster | `lib/shutdown.js` only sets a flag; `lib/processor.js:145` then waits for queued writes (`flusher.onIdle()`) and returns normally, so the run prints `Total Writes: N` and **`dump complete`** and **exits 0**, like a finished dump; no resume point is printed (only the per-batch `sent 10 objects, N offset` log lines hint at one). Checked 2026-10-31 with 6.125.1, file -> 200 ms `_bulk` mock, SIGINT after 2.5 s: in-flight bulks finished, 120 written, exit 0 | Same wait for in-flight bulks, then `Total Writes` and **exit 130 / 143** with `dump interrupted by user (SIGINT) …; to resume, rerun the same command with --offset N` (L-647); scripts can tell an interrupted restore from a complete one | n/a | chaos `l647_sigint_during_file_restore_reports_the_resume_offset`, `l647_sigint_during_cluster_copy_reports_the_resume_offset` |
| 24 | A second Ctrl+C | `lib/shutdown.js` only sets a flag, so every further SIGINT / SIGTERM is ignored too: a run hung in cleanup or in a request (no timeout by default) can only be stopped with `kill -9`, which reports nothing | The second signal force-quits at once (130 / 143) and names what it leaves behind: an `--output` that was not committed (its staging files are named again by the next run) and an index `--closeIndex` may have left closed, with the `POST /{index}/_open` to run (L-649) | n/a | chaos `l649_second_sigint_during_the_reopen_names_the_closed_index`, `l649_second_sigint_during_dump_cleanup_commits_nothing` |
| 25 | Refresh with no output index (1.1-2) | `reindex()` posts `${base.url}/_refresh` after every batch (`lib/transports/__es__/_data.js:267`, `:311`); with no index in the output URL `base.url` is the cluster, so every index in the cluster is refreshed after every batch | Refreshes only the indices the run wrote (after prefix/suffix/rename), `POST /a,b/_refresh?ignore_unavailable=true&allow_no_indices=true`, split under ~3 KB per URL; nothing when nothing was written | n/a (same data visible after the run) | unit `bulk::tests::refresh_targets_are_the_written_indices_never_the_cluster`, `refresh_index_lists_are_split_under_the_url_budget`; contract `restore_without_output_index_refreshes_only_written_indices` |
| 26 | Refresh after every batch (1.1-1) | `reindex()` after **every** bulk batch (`lib/transports/__es__/_data.js:267`), so the engine builds a new segment per batch (`--limit` docs, 100 by default); no final refresh beyond the last batch's | Refreshes at most every `--refreshInterval` seconds (default 5, clock starts at the first batch) plus once at the end of the run (also with `--concurrency 1`); `--refreshInterval=0` = Node's per-batch refresh; `--noRefresh` wins | n/a (same data visible after the run) | unit `bulk::tests::refresh_policy_cadence`, `refresh_state_is_shared_and_the_clock_starts_at_the_first_batch`, `cli::tests::refresh_interval_flag_and_policy`; contract `refresh_cadence_no_refresh_and_refresh_failure_is_a_warning`, `refresh_interval_refreshes_during_long_runs`; goldens `data-restore`/`bulk-gzip` pinned to `--refreshInterval=0` |
| 27 | Scroll `_doc` sort (1.1-3) | Sets `sort: ['_doc']` on the request **options** object passed to `request` (`lib/transports/__es__/_data.js:115`), not in the search body, so no sort is ever sent (dead code; the scroll runs in score order, `_score: 1` on every hit) | Default: same (no sort; identical dump lines). Opt-in `--scrollDocSort` sends `"sort":["_doc"]` for unscored scroll bodies without a sort, drops the returned `sort` member from each hit and keeps `"_score": null` | yes (restores ignore `_score`/`sort`) | unit `search_body::tests::scroll_doc_sort_is_added_only_where_scores_do_not_matter`, `extract::tests::scroll_doc_sort_member_is_stripped_from_hits`, `cli::tests::scroll_doc_sort_is_opt_in`; contract `scroll_doc_sort_is_opt_in_and_strips_the_hit_sort` |

Found during this implementation (not in the original list): #2, #14 (closed-index on failure, many-to-one
application), #18, #19, #20. The Node interop tests run when Node elasticdump is installed at
`/tmp/ed/node_modules/.bin/elasticdump`; they also assert Node's broken ISM restore (#7) so a fixed Node release
would be noticed.

`it::node_interop::node_interop_every_type` (L-433) runs all 11 types in both directions (Node dump → Rust
restore, Rust dump → Node restore) on OpenSearch 2.19.7 and Elasticsearch 8.19.23: 22 directions per engine.
Node's dumps are committed as `tests/fixtures/node/<engine>-<version>/<type>.json` (unique prefix replaced by
`__U__`; re-record with `IT_RECORD_NODE_FIXTURES=1`), so the Node → Rust half also runs without Node
(`IT_NODE=0` forces that path). One shape difference is normalised by the test: for a single **named** ISM
policy, Node writes the raw `GET _plugins/_ism/policies/<id>` response while Rust always writes the list
shape `{"policies":[…],"total_policies":N}`; Rust restores both.

Further interop findings (L-434..L-437, `it::node_interop`): gzip files interoperate both ways. Node cannot
read Rust's zstd files: with `--fsCompress` it exits 1, and **without** it Node exits 0 having imported 0
documents (a silent no-op on Node's side). Node with `--support-big-int` writes integers ≥ 1e21 in exponent
form (`1.2345678901234567890123456789e+29`), which is value-equal but not byte-equal; Rust keeps the original
digits. Split parts: `--fileSize` counts UTF-8 bytes in Rust and JavaScript string length in Node, so non-ASCII
data splits into more Rust parts (7 vs 5 in the test). Node's own bigint fixtures round-trip byte for byte.

Exit status for failed writes (L-444, Q3): Node exits 0 when bulk items fail (default
`--ignore-es-write-errors true`). searchdump continues past failed items the same way but exits **3**
("finished with failed writes") unless `--ignore-es-write-errors` or `--ignore-errors` is given explicitly;
`--ignore-es-write-errors false` stops at the first failed item with status 1, as in Node.

Escaping corpus (L-511, TST-INJ-014; IT `node_interop::xv_escaping_corpus_round_trips_byte_for_byte`, OpenSearch
2.19.7 and Elasticsearch 8.19.23, 88 documents into an `enabled: false` index): searchdump returns every
`_source` byte for byte (multiset digest equal) and every `_id` decodes to the string sent; the engines themselves
store a lone surrogate escape inside an `_id` (`"\ud800"`) as U+FFFD. Node 6.125.1 changes all 88 sources on the way
file -> engine -> file because it parses and re-serialises them. Most changes are only re-escaping (`\/` -> `/`,
`\u2028` -> raw U+2028, surrogate pairs -> raw astral characters, key order), but these change the data:

| Input value | After the Node round trip |
|---|---|
| `1e400` | `null` |
| `-18446744073709551617` | `-18446744073709552000` (rounded) |
| `18446744073709551617` | `"18446744073709551617"` (a string) |
| `0.1000000000000000055511151231257827` | `0.1` |
| `-0`, `-0.0` | `0` |
| `1.0E+2` | `100` |
| lone surrogate escape `"\ud800"` (value or key) | U+FFFD or the character dropped |

## 1.1 changes with no Node difference

These 1.1 roadmap items change only searchdump internals or searchdump-only extensions; Node elasticdump has no
equivalent, so there is no compatibility difference to record in the table above.

* **1.1-4 slice cap / `--sliceField`**: `--slices` is a searchdump extension; Node never slices. The
  `--sliceCap=auto` default caps only on Elasticsearch 7.10 (1.1-4a) and adds a `GET /` request to sliced scroll
  dumps, which Node cannot run.
* **1.1-9 compression thread**: internal; the gzip bytes are identical, and zstd is a searchdump extension.
* **1.1-6 encode/send overlap**: same requests in the same order as Node at `--concurrency 1`; only the client-side timing changes.
* **1.1-8 searchdump-multi Ctrl-C**: Node `multielasticdump` forks children too; how they are stopped on Ctrl-C is not part of the dump format or CLI (README "Bulk dump and load").
* **1.1-7 child counts**: searchdump-multi reads Node's `Total Writes: N` line too; `--statsFile` is a hidden searchdump flag that is never passed to a Node binary.
