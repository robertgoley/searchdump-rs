// SUP-006: no unsafe Rust in this crate; all unsafe comes from audited deps.
#![forbid(unsafe_code)]

pub mod cli;
pub mod config;
pub mod endpoint;
pub mod envnames;
#[cfg(any(feature = "fuzz", test))]
pub mod fuzz_api;
pub mod http;
pub mod memplan;
pub mod meta;
pub mod output;
pub mod pipeline;
pub mod remote;
pub mod rename;
pub mod retrieval;
pub mod secrets;
pub mod signals;

use std::io::Write;
use std::path::PathBuf;

use anyhow::{Context, Result};
use clap::Parser;
use sonic_rs::json;

use crate::cli::{Cli, DumpType, EffectiveSearch, SearchType};
use crate::endpoint::{Direction, Endpoint};
use crate::pipeline::{DocWindow, FileSink, Sink};

/// A usage error: reported with exit status 2.
#[derive(Debug)]
pub struct UsageError(pub String);

impl std::fmt::Display for UsageError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for UsageError {}

/// L-642: does the dump read and write the same place? Files are compared by canonical
/// path (the output may not exist yet, so its parent is canonicalized); clusters by
/// normalized base URL (scheme, host, port, path) plus index expression. Aliases such as
/// `localhost` vs `127.0.0.1`, or an alias name pointing at the input index, are not resolved.
/// L-642a: the output index is compared as written to, i.e. with `--prefix`/`--suffix`
/// applied (`--input …/p-idx --output …/idx --prefix p-` writes into the input index;
/// `--input …/idx --output …/idx --suffix -copy` does not).
/// Whether a run would read and write the same file or index. The output index is resolved the
/// way documents are written: `--prefix`/`--suffix` around the explicit output index, or, for a
/// host-only output, around each source index name after the `--renamePrefix`/`--renameIndex`
/// rules (`renamer`). A wildcard input on a host-only output writes each match back onto itself
/// unless an affix or rule renames it; with rules, which matches they hit is only known at run
/// time, so such an input is not refused here.
fn same_target(
    input: &Endpoint,
    output: &Endpoint,
    prefix: &str,
    suffix: &str,
    renamer: Option<&rename::IndexRenamer>,
) -> bool {
    fn canon(p: &std::path::Path) -> Option<std::path::PathBuf> {
        if let Ok(c) = p.canonicalize() {
            return Some(c);
        }
        let name = p.file_name()?;
        let parent = match p.parent() {
            Some(d) if !d.as_os_str().is_empty() => d.to_path_buf(),
            _ => std::path::PathBuf::from("."),
        };
        Some(parent.canonicalize().ok()?.join(name))
    }
    match (input, output) {
        (Endpoint::Remote(a), Endpoint::Remote(b)) => a == b,
        (Endpoint::File(a), Endpoint::File(b)) => match (canon(a), canon(b)) {
            (Some(a), Some(b)) => a == b,
            _ => a == b,
        },
        (Endpoint::Es(a), Endpoint::Es(b)) => {
            a.base == b.base
                && match (&a.index, &b.index) {
                    (Some(src), Some(dst)) => *src == format!("{prefix}{dst}{suffix}"),
                    // host to host: every index onto itself unless an affix renames it
                    (None, None) => prefix.is_empty() && suffix.is_empty(),
                    // host-only output: each source index is written to its own name, affixed
                    // and renamed (L-642b)
                    (Some(src), None) => src
                        .split(',')
                        .map(str::trim)
                        .filter(|n| !n.is_empty() && !n.starts_with('-'))
                        .any(|n| {
                            if n.contains(['*', '?']) {
                                renamer.is_none() && prefix.is_empty() && suffix.is_empty()
                            } else {
                                match renamer {
                                    Some(r) => r.target_of(n) == n,
                                    None => format!("{prefix}{n}{suffix}") == n,
                                }
                            }
                        }),
                    (None, Some(_)) => false,
                }
        }
        _ => false,
    }
}

fn usage(msg: impl Into<String>) -> anyhow::Error {
    anyhow::Error::new(UsageError(msg.into()))
}

/// `--parseExtraFields a,_b`: names (leading `_` dropped) must match
/// `^[a-z][a-z0-9_]*$` so they can never inject keys into the bulk action
/// line (INJ-F4). Duplicates are dropped.
fn parse_extra_fields(raw: &str) -> Result<Vec<String>> {
    let mut out: Vec<String> = Vec::new();
    for f in raw.split(',').map(|f| f.trim().trim_start_matches('_')) {
        if f.is_empty() {
            continue;
        }
        let mut chars = f.chars();
        let ok = chars.next().is_some_and(|c| c.is_ascii_lowercase())
            && chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_');
        if !ok {
            return Err(usage(format!(
                "--parseExtraFields: invalid field name {f:?} (allowed: a-z, 0-9 and _, starting with a letter)"
            )));
        }
        if !out.iter().any(|o| o == f) {
            out.push(f.to_string());
        }
    }
    Ok(out)
}

/// The run finished, but documents were not written and the user did not accept that
/// explicitly (TST-BRK-SRV-001, Q3): exit status 3.
#[derive(Debug)]
/// Failed documents of a finished run: `.0` all of them; `.1` those refused by a
/// `cluster_block_exception`, `.2` of those the disk flood-stage block (L-804b, P5).
pub struct WriteFailures(pub u64, pub u64, pub u64);

impl std::fmt::Display for WriteFailures {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{} document(s) were not written (see Failed Writes and the item errors above); the run continued past them. Exit status 3 means \"finished with failed writes\": pass --ignore-es-write-errors explicitly to accept them with status 0 (Node elasticdump's default), or --ignore-es-write-errors false to stop at the first one",
            self.0
        )?;
        let (failed, blocked, disk) = (self.0, self.1, self.2);
        if disk > 0 && disk * 2 > failed {
            write!(
                f,
                ". Most of them ({disk} of {failed}) were refused by the read-only-allow-delete block (cluster_block_exception) that a node sets when its disk passes the flood-stage watermark: free disk space on the cluster (Elasticsearch 7.4+ lifts the block by itself once usage drops; older versions and OpenSearch may need index.blocks.read_only_allow_delete reset to null), then re-run, with a larger --retryAttempts / --retryDelay to ride out a short block"
            )?;
        } else if blocked > 0 && blocked * 2 > failed {
            write!(
                f,
                ". Most of them ({blocked} of {failed}) were refused by an index or cluster block (cluster_block_exception, e.g. index.blocks.write or read_only): lift the block, then re-run"
            )?;
        }
        Ok(())
    }
}

impl std::error::Error for WriteFailures {}

/// Exit status for an error returned by [`run`]: 2 usage, 3 finished with failed writes,
/// 4 transient failure that a rerun can fix (L-804b, [`is_transient`]), 1 anything else. Precedence: a run-level failure (e.g. the target index deleted, or
/// deleted and re-created, mid-restore, TST-BRK-SRV-008) is returned by the writer's drain
/// before [`WriteFailures`] is ever built, so it exits 1 even when documents were also
/// rejected (their item errors are still on stderr): the restore is invalid as a whole,
/// not merely incomplete, and 3 would understate it.
pub fn exit_code(error: &anyhow::Error) -> i32 {
    if error.downcast_ref::<UsageError>().is_some() {
        2
    } else if error.downcast_ref::<WriteFailures>().is_some() {
        3
    } else if let Some(signals::Interrupted(sig)) = error.downcast_ref::<signals::Interrupted>() {
        128 + sig
    } else if let Some(stopped) = error.downcast_ref::<signals::Stopped>() {
        128 + stopped.sig
    } else if is_transient(error) {
        4
    } else {
        1
    }
}

/// L-804b (owner decision 2026-11-14, behaviour change): the run stopped on a condition that
/// may clear by itself and a rerun is safe: a network failure (refused, reset, timeout, cut
/// body; not TLS or a DNS name that does not exist), HTTP 429 or 5xx after the retries, or a
/// lost scroll / point-in-time context. Nothing a rerun would duplicate was committed: file
/// outputs are staged and removed, bulk writes carry `_id` (a run that already wrote
/// documents without `_id` is marked [`http::NotRerunSafe`] and stays 1). A cancellation
/// cascade is not transient.
pub fn is_transient(error: &anyhow::Error) -> bool {
    if error.downcast_ref::<http::NotRerunSafe>().is_some() || http::is_cancelled(error) {
        return false;
    }
    error.downcast_ref::<http::Transient>().is_some()
        || error.downcast_ref::<http::ContextLoss>().is_some()
        || error.downcast_ref::<http::EsHttpError>().is_some_and(|h| {
            (h.status == 429 || (500..600).contains(&h.status)) && !h.is_scroll_context_limit()
        })
}

fn init_logging(cli: &Cli) {
    let default = if cli.debug() {
        "debug"
    } else if cli.quiet() {
        "error"
    } else {
        "warn"
    };
    let _ = env_logger::Builder::from_env(env_logger::Env::default().default_filter_or(default))
        .format(|buf, record| {
            use std::io::Write;
            // every log line is scrubbed of registered secrets (TST-BRK-CLI-007)
            let msg = secrets::scrub(&record.args().to_string());
            let style = buf.default_level_style(record.level());
            writeln!(
                buf,
                "[{} {style}{}{style:#}] {msg}",
                buf.timestamp(),
                record.level()
            )
        })
        .try_init();
}

fn validate(cli: &Cli, input: &Endpoint, output: &Endpoint) -> Result<()> {
    if cli.restart_on_context_loss > 0 {
        let scroll_dump_to_file = matches!(cli.r#type, DumpType::Data)
            && matches!(input, Endpoint::Es(_))
            && matches!(output, Endpoint::File(_))
            && cli.effective_search() == cli::EffectiveSearch::Scroll
            && cli.scroll_id.is_none();
        if !scroll_dump_to_file {
            return Err(usage(
                "--restartOnContextLoss applies only to a scroll data dump from a cluster into a file (no --scrollId, no --searchAfter/--pit, not stdout or a cluster or remote output): only there is nothing committed before a restart",
            ));
        }
    }
    if let Some(extra) = &cli.parse_extra_fields {
        parse_extra_fields(extra)?;
    }
    // Header names/values with CR, LF, NUL or other invisible bytes are a
    // usage error before anything is sent (TST-INJ-011).
    for dir in [endpoint::Direction::Input, endpoint::Direction::Output] {
        config::headers_for(cli, dir)?;
    }
    if cli.file_size.is_some() && cli.max_rows.is_some() {
        return Err(usage("--fileSize and --maxRows cannot be combined"));
    }
    // TST-BRK-CLI-005: directories, symlinks, FIFOs/devices and missing
    // parents are refused before anything is read or sent
    if let Endpoint::File(p) = output {
        output::check_output_path(p).map_err(|e| usage(format!("{e:#}")))?;
    }
    if (cli.file_size.is_some() || cli.max_rows.is_some())
        && !matches!(output, Endpoint::File(_) | Endpoint::Remote(_))
    {
        return Err(usage("--fileSize/--maxRows require a file --output"));
    }
    if !input.is_es() && !output.is_es() && cli.r#type != DumpType::Data {
        return Err(usage(format!(
            "--type {} needs Elasticsearch/OpenSearch on at least one side",
            cli.r#type.name()
        )));
    }
    // compiled here too, so a bad pattern fails before any request
    if let DumpType::Meta(kind) = cli.r#type {
        template_filter(cli, kind)?;
    }
    let renaming = !cli.rename_prefix.is_empty() || !cli.rename_index.is_empty();
    if renaming {
        match output {
            Endpoint::Es(ep) if ep.index.is_some() => {
                return Err(usage(format!(
                    "--renamePrefix/--renameIndex rename source index names and cannot be combined with an explicit output index ('{}'); use a host-only --output",
                    ep.index.as_deref().unwrap_or_default()
                )));
            }
            Endpoint::Es(_) => {}
            _ => {
                return Err(usage(
                    "--renamePrefix/--renameIndex apply when restoring or copying to a cluster, not to file output",
                ));
            }
        }
        renamer(cli)?;
    } else if cli.rename_aliases.unwrap_or(false) || cli.rename_patterns.unwrap_or(false) {
        return Err(usage(
            "--renameAliases/--renamePatterns need --renamePrefix or --renameIndex rules",
        ));
    }
    if cli.scroll_id.is_some()
        && (cli.slices > 1 || cli.effective_search() != EffectiveSearch::Scroll)
    {
        return Err(usage(
            "--scrollId cannot be combined with --slices, --searchAfter or PIT",
        ));
    }
    // L-641: sliced streams interleave nondeterministically, so --offset would skip an
    // arbitrary set of documents rather than "the first N".
    if cli.offset > 0 && cli.slices > 1 {
        return Err(usage(
            "--offset cannot be combined with --slices (sliced results have no stable order to skip into)",
        ));
    }
    if cli.r#type != DumpType::Data && (cli.offset > 0 || cli.size >= 0) {
        log::warn!(
            "--offset/--size are ignored for --type {}",
            cli.r#type.name()
        );
    }
    Ok(())
}

fn file_path(ep: &Endpoint) -> Option<PathBuf> {
    match ep {
        Endpoint::File(p) => Some(p.clone()),
        // L-809a: the object key; split parts are named from it like local files
        Endpoint::Remote(r) => Some(PathBuf::from(&r.key)),
        _ => None,
    }
}

/// File-style input: stdin, a local file or an OpenDAL object (L-809a).
fn input_loc(ep: &Endpoint) -> pipeline::file_source::InputLoc {
    match ep {
        Endpoint::Remote(r) => pipeline::file_source::InputLoc::Remote(r.clone()),
        other => file_path(other).into(),
    }
}

const DEFAULT_MAX_BULK_BYTES: u64 = 100 * 1024 * 1024;

/// `--throttleInterval`: only when given explicitly. Node's 1 ms default
/// would add ~10 s per million documents at --limit 100 for no benefit.
fn throttle(cli: &Cli) -> Option<std::time::Duration> {
    cli.throttle_interval
        .filter(|ms| *ms > 0)
        .map(std::time::Duration::from_millis)
}

fn split_requested(cli: &Cli) -> bool {
    cli.file_size.as_deref().is_some_and(|s| s.trim() != "-1")
        || cli.max_rows.is_some_and(|n| n > 0)
}

/// L-805a: levels that cost far more CPU than they save in size (README "Compression levels"
/// table, L-805). A warning only: the level is still used.
fn slow_level_warning(codec: Option<pipeline::file_sink::Codec>) -> Option<String> {
    use pipeline::file_sink::Codec;
    match codec {
        Some(Codec::Gzip(9)) => Some(
            "--compressionLevel 9 with gzip is slow for no size gain: in the README compression table (L-805) gzip 9 runs at about 120 MB/s per core against about 200 for the default 6, and with the zlib-rs backend writes a 3.5% larger file; the default level (6) is recommended"
                .to_string(),
        ),
        Some(Codec::Zstd(l)) if l >= 19 => Some(format!(
            "--compressionLevel {l} with zstd is very slow for a small size gain: in the README compression table (L-805) zstd 19 runs at about 1.8 MB/s per core (1M docs took 9.5 minutes) against about 100 for zstd 9, which writes a file about 30% larger; consider zstd 9 or the default 3"
        )),
        _ => None,
    }
}

/// File/stdout output options: gzip and splitting.
fn file_options(cli: &Cli, output: &Endpoint) -> Result<pipeline::file_sink::FileOptions> {
    use crate::cli::CompressionType;
    use pipeline::file_sink::Codec;
    use pipeline::file_sink::{FileOptions, SplitBy, parse_size};
    let kind = match (cli.compression_type, cli.fs_compress()) {
        (Some(t), _) => Some(t),
        (None, true) => Some(CompressionType::Gzip),
        (None, false) => None,
    };
    let codec = match (kind, cli.compression_level) {
        (None, Some(_)) => {
            log::warn!(
                "--compressionLevel has no effect without --fsCompress or --compressionType"
            );
            None
        }
        (None, None) => None,
        (Some(CompressionType::Gzip), None | Some(-1)) => Some(Codec::Gzip(6)),
        (Some(CompressionType::Gzip), Some(l @ 0..=9)) => Some(Codec::Gzip(l as u32)),
        (Some(CompressionType::Zstd), None | Some(-1)) => Some(Codec::Zstd(3)),
        (Some(CompressionType::Zstd), Some(l @ 1..=22)) => Some(Codec::Zstd(l)),
        (Some(t), Some(l)) => {
            let range = if t == CompressionType::Gzip {
                "0-9"
            } else {
                "1-22"
            };
            return Err(usage(format!(
                "--compressionLevel {l} is out of range for {t:?} (-1 or {range})"
            )));
        }
    };
    if let Some(msg) = slow_level_warning(codec) {
        log::warn!("{msg}");
    }
    let human = match cli.format.as_deref().map(str::trim) {
        None | Some("") | Some("json") => false,
        Some("human") if split_requested(cli) => {
            return Err(usage(
                "--format human cannot be combined with --fileSize/--maxRows",
            ));
        }
        Some("human") => true,
        Some(other) => {
            return Err(usage(format!(
                "--format must be '' or 'human', got '{other}'"
            )));
        }
    };
    let path = file_path(output);
    if codec.is_some() && path.is_none() {
        log::info!("compression does not apply to stdout output (elasticdump behaviour)");
    }
    let split = match (&cli.file_size, cli.max_rows) {
        (Some(s), _) if s.trim() != "-1" => {
            let bytes = parse_size(s).map_err(|e| usage(format!("--fileSize: {e:#}")))?;
            // TST-BRK-RES-006: below this every document becomes its own file
            if bytes < MIN_FILE_SIZE {
                return Err(usage(format!(
                    "--fileSize {s} is below the 256b minimum: nearly every document would become its own file (use --maxRows 1 if that is really wanted)"
                )));
            }
            Some(SplitBy::Bytes(bytes))
        }
        (_, Some(n)) if n > 0 => Some(SplitBy::Rows(n)),
        _ => None,
    };
    Ok(FileOptions {
        path,
        overwrite: cli.overwrite(),
        source_only: cli.source_only(),
        codec,
        split,
        throttle: throttle(cli),
        human,
        remote: match output {
            Endpoint::Remote(r) => Some(r.clone()),
            _ => None,
        },
    })
}

async fn build_sink(
    cli: &Cli,
    output: &Endpoint,
    retry: crate::http::RetryPolicy,
    gate: crate::http::gate::RequestGate,
) -> Result<Sink> {
    match output {
        Endpoint::Es(ep) => {
            if cli.source_only() {
                log::warn!("--sourceOnly with an Elasticsearch output: documents get new ids");
            }
            if let Some(i) = &ep.index {
                // TST-BRK-IN-012: refuse up front what the engine would refuse per document
                let name = format!("{}{}{}", cli.prefix, i, cli.suffix);
                if let Err(e) = crate::rename::validate_index_name(&name) {
                    return Err(anyhow::Error::new(UsageError(format!(
                        "the --output index '{name}' is not a valid index name: {e} (Elasticsearch and OpenSearch refuse it); nothing was written"
                    ))));
                }
            }
            let client = config::build_client(cli, ep, Direction::Output, gate)?;
            let info = client
                .info(retry, &tokio_util::sync::CancellationToken::new())
                .await?
                .clone();
            let mut meta_fields: Vec<String> = ["routing", "parent", "timestamp", "ttl"]
                .map(String::from)
                .to_vec();
            if let Some(extra) = &cli.parse_extra_fields {
                for f in parse_extra_fields(extra)? {
                    if !meta_fields.contains(&f) {
                        meta_fields.push(f);
                    }
                }
            }
            let max_bulk_bytes = match &cli.max_bulk_bytes {
                Some(raw) => raw
                    .parse::<bytesize::ByteSize>()
                    .map_err(|e| usage(format!("--maxBulkBytes: {e}")))?
                    .as_u64(),
                None => DEFAULT_MAX_BULK_BYTES,
            };
            let opts = pipeline::BulkOptions {
                info,
                index: ep.index.clone(),
                doc_type: ep.doc_type.clone(),
                prefix: cli.prefix.clone(),
                suffix: cli.suffix.clone(),
                action: pipeline::BulkAction::parse(&cli.bulk_action)?,
                handle_version: cli.handle_version(),
                version_type: cli.version_type.clone(),
                meta_fields,
                refresh: !cli.no_refresh(),
                ignore_es_write_errors: cli.ignore_es_write_errors(),
                ignore_errors: cli.ignore_errors(),
                max_bulk_bytes: max_bulk_bytes.max(1) as usize,
                retry,
                rename: renamer(cli)?.map(std::sync::Arc::new),
            };
            Ok(Sink::Bulk(Box::new(
                pipeline::BulkSink::new(client, opts, tokio_util::sync::CancellationToken::new())
                    .with_concurrency(cli.concurrency, throttle(cli)),
            )))
        }
        out => {
            if cli.concurrency > 1 {
                log::info!(
                    "--concurrency applies to Elasticsearch outputs; file output stays sequential to keep order"
                );
            }
            Ok(Sink::File(Box::new(
                FileSink::create(file_options(cli, out)?).await?,
            )))
        }
    }
}

/// `--searchWithTemplate`: render `--searchBody` (`{"id"|"source", "params"}`)
/// with `POST /_render/template` and search with the rendered body.
async fn render_search_template(
    client: &crate::http::EsClient,
    retry: crate::http::RetryPolicy,
    template: &sonic_rs::Value,
) -> Result<sonic_rs::Value> {
    use sonic_rs::JsonValueTrait;
    if template.get("id").is_none() && template.get("source").is_none() {
        return Err(usage(
            "--searchWithTemplate needs a --searchBody with \"id\" (stored template) or \"source\", plus \"params\"",
        ));
    }
    let body: serde_json::Value = serde_json::from_str(&sonic_rs::to_string(template)?)?;
    let body = crate::http::Body::json(&body);
    let body = &body;
    let cancel = tokio_util::sync::CancellationToken::new();
    let resp = crate::http::send_with_retry(
        retry,
        &cancel,
        crate::http::RetryMode::Idempotent,
        "render search template",
        move || {
            client.send(
                reqwest::Method::POST,
                &["_render", "template"],
                &[],
                true,
                body.clone(),
            )
        },
    )
    .await
    .context("--searchWithTemplate: rendering the search template failed")?;
    let v: sonic_rs::Value = sonic_rs::from_slice(&resp)?;
    let out = v
        .get("template_output")
        .cloned()
        .ok_or_else(|| anyhow::anyhow!("_render/template returned no template_output"))?;
    log::debug!("Rendered search template: {}", sonic_rs::to_string(&out)?);
    Ok(out)
}

async fn es_source_options(
    cli: &Cli,
    ep: &endpoint::EsEndpoint,
    retry: crate::http::RetryPolicy,
    gate: crate::http::gate::RequestGate,
) -> Result<retrieval::EsSourceOptions> {
    let client = config::build_client(cli, ep, Direction::Input, gate)?;
    let mut search_type = match cli.effective_search() {
        EffectiveSearch::Scroll => SearchType::Scroll,
        EffectiveSearch::Pit => SearchType::PointInTime,
        EffectiveSearch::SearchAfter => SearchType::SearchAfter,
    };
    // Cluster detection (`GET /`) is only needed for PIT; scroll dumps work
    // the same on every flavor and version.
    let (pit_api, pit_sort) = if search_type == SearchType::PointInTime {
        let info = client
            .info(retry, &tokio_util::sync::CancellationToken::new())
            .await?
            .clone();
        match info.pit_api() {
            Some(_) if cli.slices > 1 && !info.supports_sliced_pit() => {
                log::warn!(
                    "{} {} cannot slice a point-in-time search (needs Elasticsearch >= 7.15); using sliced scroll instead",
                    info.flavor_name(),
                    info.version
                );
                search_type = SearchType::Scroll;
                (crate::http::PitApi::Elastic, json!(["_shard_doc"]))
            }
            Some(api) => {
                let sort = if info.supports_shard_doc_sort() {
                    json!(["_shard_doc"])
                } else {
                    // `_doc` alone loses documents across PIT pages on
                    // OpenSearch; `_id` is the tiebreaker. `unmapped_type`
                    // keeps indices without any mapping searchable.
                    json!([{"_doc": "asc"}, {"_id": {"order": "asc", "unmapped_type": "keyword"}}])
                };
                (api, sort)
            }
            None => {
                // ES < 7.10 and OpenSearch < 2.4 have no PIT: scroll gives the
                // same snapshot-consistent result.
                log::warn!(
                    "{} {}{} does not support point-in-time search (needs Elasticsearch >= 7.10 with X-Pack, or OpenSearch >= 2.4); using scroll instead",
                    info.flavor_name(),
                    info.version,
                    if info.oss { " (OSS build)" } else { "" }
                );
                search_type = SearchType::Scroll;
                (crate::http::PitApi::Elastic, json!(["_shard_doc"]))
            }
        }
    } else {
        (crate::http::PitApi::Elastic, json!(["_shard_doc"]))
    };
    let size = (cli.size >= 0).then(|| (cli.size as u64).saturating_add(cli.offset));
    let search_body = if cli.search_with_template() {
        let template = retrieval::search_body::load_search_body(cli.search_body.as_deref()).await?;
        let rendered = render_search_template(&client, retry, &template).await?;
        retrieval::search_body::finalize_search_body(rendered, cli.limit, size)
    } else {
        retrieval::search_body::prepare_search_body(cli.search_body.as_deref(), cli.limit, size)
            .await?
    };
    if search_type == SearchType::SearchAfter {
        use sonic_rs::{JsonContainerTrait, JsonValueTrait};
        let has_sort = search_body
            .get("sort")
            .is_some_and(|s| s.as_array().is_none_or(|a| !a.is_empty()));
        if !has_sort {
            return Err(usage(
                "--searchAfter without --pit needs a \"sort\" in --searchBody ending in a unique tiebreaker",
            ));
        }
        if cli.slices > 1 {
            return Err(usage(
                "--slices needs scroll or PIT; it cannot be used with --searchAfter alone",
            ));
        }
        log::warn!(
            "--searchAfter without --pit is not snapshot-consistent; concurrent writes may be missed or duplicated"
        );
    }
    Ok(retrieval::EsSourceOptions {
        client,
        index: ep.index.clone(),
        search_body,
        search_type,
        pit_api,
        pit_sort,
        scroll_ttl: cli.scroll.clone(),
        pit_keep_alive: cli.pit_keep_alive.clone(),
        slices: cli.slices,
        workers: cli.workers,
        buffer_size: cli.buffer_size,
        quiet: cli.quiet(),
        retry,
        resume_scroll_id: cli.scroll_id.clone(),
        reconcile_total: cli.size < 0 && cli.offset == 0 && cli.scroll_id.is_none(),
    })
}

/// Register the passwords given as flags or environment variables before
/// anything can log or fail (D-4); URL, auth-file and header secrets follow
/// in [`register_secrets`].
fn register_early_secrets(cli: &Cli) {
    let env = [Direction::Input, Direction::Output]
        .map(|d| crate::envnames::var(&format!("{}_PASSWORD", d.env_prefix())));
    for p in [
        &cli.input_password,
        &cli.output_password,
        &cli.pass,
        &cli.input_pass,
        &cli.output_pass,
    ]
    .into_iter()
    .chain(&env)
    .flatten()
    {
        secrets::register(p);
    }
}

/// Register every credential of this run so logs and errors never show it.
fn register_secrets(cli: &Cli, eps: [&Endpoint; 2], auth_file: Option<&(String, String)>) {
    for ep in eps {
        if let Endpoint::Es(ep) = ep
            && let Some(p) = &ep.password
        {
            secrets::register_basic(ep.username.as_deref(), p);
        }
    }
    if let Some((u, p)) = auth_file {
        secrets::register_basic(Some(u), p);
    }
    for dir in [Direction::Input, Direction::Output] {
        for (k, v) in config::headers_for(cli, dir).unwrap_or_default() {
            secrets::register_header(&k, &v);
        }
        for (k, v) in config::params_for(cli, dir).unwrap_or_default() {
            if secrets::is_sensitive_name(&k) {
                secrets::register(&v);
            }
        }
    }
}

/// Refuse, before anything is written, a cluster copy whose rename rules map
/// several source indices to one target (TST-BRK-CLI-003). Documents read from
/// a file are checked as they arrive (`IndexRenamer::index`).
async fn check_rename_merges(
    cli: &Cli,
    src: &http::EsClient,
    index: Option<&str>,
    retry: http::RetryPolicy,
) -> Result<()> {
    let Some(r) = renamer(cli)? else {
        return Ok(());
    };
    if cli.allow_merge {
        return Ok(());
    }
    let target = index.unwrap_or("_all");
    match meta::http::get_json(src, retry, &[target, "_alias"], "list the source indices").await {
        Ok(v) => {
            let names: Vec<&str> = v
                .as_object()
                .map(|o| o.keys().map(String::as_str).collect())
                .unwrap_or_default();
            r.check_merges(names)
                .map_err(|e| anyhow::Error::new(UsageError(format!("{e:#}. Nothing was written"))))
        }
        Err(e) => {
            log::warn!(
                "could not list the source indices to check the rename rules up front ({e:#}); merges are still refused as documents arrive"
            );
            Ok(())
        }
    }
}

/// `--templateRegex` for template dumps with `--filterSystemTemplates` (the
/// default); a bad or oversized pattern is a usage error (TST-BRK-CLI-004).
fn template_filter(cli: &Cli, kind: meta::MetaKind) -> Result<Option<regex::Regex>> {
    if !matches!(kind, meta::MetaKind::Template(_)) || !cli.filter_system_templates() {
        return Ok(None);
    }
    rename::compile_regex(
        &cli.template_regex,
        "--templateRegex",
        "unsupported pattern (JavaScript look-around and backreferences are not available)",
    )
    .map(Some)
    .map_err(|e| usage(format!("{e:#}")))
}

/// `--renamePrefix`/`--renameIndex` rules (with `--prefix`/`--suffix`), if any.
fn renamer(cli: &Cli) -> Result<Option<rename::IndexRenamer>> {
    if cli.rename_prefix.is_empty() && cli.rename_index.is_empty() {
        return Ok(None);
    }
    rename::IndexRenamer::new(
        &cli.rename_prefix,
        &cli.rename_index,
        &cli.prefix,
        &cli.suffix,
    )
    .map(|r| Some(r.with_allow_merge(cli.allow_merge)))
    .map_err(|e| usage(format!("{e:#}")))
}

fn csv_list(raw: Option<&str>) -> Vec<String> {
    raw.map(|s| {
        s.split(',')
            .map(|p| p.trim().to_string())
            .filter(|p| !p.is_empty())
            .collect()
    })
    .unwrap_or_default()
}

async fn run_meta(
    cli: &Cli,
    kind: meta::MetaKind,
    input: &Endpoint,
    output: &Endpoint,
    retry: crate::http::RetryPolicy,
    gate: crate::http::gate::RequestGate,
) -> Result<pipeline::WriteReport> {
    let cancel = tokio_util::sync::CancellationToken::new();
    let (src_client, src_info, src_index, src_doc_type) = match input {
        Endpoint::Es(ep) => {
            let c = config::build_client(cli, ep, Direction::Input, gate.clone())?;
            let info = c.info(retry, &cancel).await?.clone();
            (Some(c), Some(info), ep.index.clone(), ep.doc_type.clone())
        }
        _ => (None, None, None, None),
    };
    let template_filter = template_filter(cli, kind)?;
    if cli.close_index.unwrap_or(false) && kind != meta::MetaKind::Analyzer {
        log::warn!("--closeIndex only applies to --type analyzer");
    }
    let mut opts = meta::MetaOptions::new(kind, retry);
    opts.skip_existing = cli.skip_existing();
    opts.ignore_errors = cli.ignore_errors();
    opts.exclude_settings = csv_list(cli.exclude_settings.as_deref());
    // Engine-specific settings and fields are stripped automatically only for
    // cluster→cluster copies, where both flavors are known.
    opts.source_info = match (&src_info, output) {
        (Some(s), Endpoint::Es(_)) => Some(s.clone()),
        _ => None,
    };
    opts.object_format = cli.meta_format == "object";
    opts.template_filter = template_filter;
    opts.filter_managed = cli.filter_managed();
    opts.close_index = cli.close_index.unwrap_or(false);
    opts.exclude_template_fields = csv_list(cli.exclude_template_fields.as_deref());
    opts.convert_ilm_to_ism = cli.convert_ilm_to_ism.unwrap_or(false);
    opts.lenient_policy_conversion = cli.lenient_policy_conversion.unwrap_or(false);
    opts.rename_aliases = cli.rename_aliases.unwrap_or(false);
    opts.rename_patterns = cli.rename_patterns.unwrap_or(false);
    if opts.lenient_policy_conversion && !opts.convert_ilm_to_ism {
        log::warn!("--lenientPolicyConversion has no effect without --convertIlmToIsm");
    }
    let src = meta::MetaSource {
        client: src_client,
        info: src_info,
        index: src_index,
        doc_type: src_doc_type,
        file: input_loc(input),
    };
    // read (and validate) the source before touching the target
    let doc = meta::read(&src, &opts).await?;
    let (dst_client, dst_info, dst_ep) = match output {
        Endpoint::Es(ep) => {
            let c = config::build_client(cli, ep, Direction::Output, gate)?;
            let info = c.info(retry, &cancel).await?.clone();
            (Some(c), Some(info), Some(ep))
        }
        _ => (None, None, None),
    };
    let renamer = match renamer(cli)? {
        Some(r) => r,
        None => rename::IndexRenamer::new(&[], &[], &cli.prefix, &cli.suffix)?,
    };
    let dst = meta::MetaTarget {
        client: dst_client,
        info: dst_info,
        index: dst_ep.and_then(|e| e.index.clone()),
        doc_type: dst_ep.and_then(|e| e.doc_type.clone()),
        renamer,
        file: file_options(cli, output)?,
    };
    meta::write(doc, &dst, &opts).await
}

/// `--type alias` with a two-segment `--input` URL and no `--input-index`:
/// elasticdump reads `host/index/alias`; searchdump would read a base
/// path. Follow elasticdump, with a warning.
fn node_alias_url(cli: &Cli, input: &mut Endpoint) {
    let Endpoint::Es(ep) = input else { return };
    if cli.input_index.is_some() {
        return;
    }
    let segs: Vec<String> = ep
        .base
        .path_segments()
        .map(|s| s.filter(|x| !x.is_empty()).map(String::from).collect())
        .unwrap_or_default();
    if segs.len() != 1 || ep.index.is_none() {
        return;
    }
    let index = segs[0].clone();
    let alias = ep.index.take();
    log::warn!(
        "--type alias: reading the --input URL path as <index>/<alias> ('{index}' / '{}') like elasticdump; use --input-index {index}/{} to say so explicitly",
        alias.as_deref().unwrap_or_default(),
        alias.as_deref().unwrap_or_default()
    );
    ep.base.set_path("/");
    ep.index = Some(index);
    ep.doc_type = alias;
}

/// TST-BRK-RES-004: "Too many open files" gets the limit and the flags that drive it.
pub fn emfile_hint(msg: String) -> String {
    if !msg.contains("os error 24") {
        return msg;
    }
    let limit = open_file_limit().map_or_else(|| "?".into(), |n| n.to_string());
    format!(
        "{msg} (the open-file limit, ulimit -n = {limit}, is too low for this run: each of --concurrency, --slices and --maxSockets can hold a connection, and split parts and the input hold files; raise it, e.g. ulimit -n 4096, or lower those flags)"
    )
}

/// Soft `RLIMIT_NOFILE` from /proc/self/limits.
/// Read once at start: when the limit has run out, /proc/self/limits cannot be opened.
fn open_file_limit() -> Option<u64> {
    static LIMIT: std::sync::OnceLock<Option<u64>> = std::sync::OnceLock::new();
    *LIMIT.get_or_init(|| {
        std::fs::read_to_string("/proc/self/limits")
            .ok()
            .and_then(|l| {
                l.lines()
                    .find(|x| x.starts_with("Max open files"))
                    .and_then(|x| x.split_whitespace().nth(3)?.parse().ok())
            })
    })
}

/// The transfer itself (`--type data` or a meta type); the summary is printed by [`run`].
async fn transfer(
    cli: &Cli,
    input: &Endpoint,
    output: &Endpoint,
    retry: crate::http::RetryPolicy,
    gate: crate::http::gate::RequestGate,
    window: DocWindow,
) -> Result<pipeline::WriteReport> {
    let report = match cli.r#type {
        DumpType::Data => {
            // L-650: the guard is held from before the output is staged. Without it, a
            // first Ctrl+C / SIGTERM between staging the output file and the transfer
            // (the input's setup requests) force-quit and left the hidden staging file
            // behind; now it removes it. Setup that waits on a cluster still ends at once.
            let _graceful = signals::graceful();
            let interrupt = signals::token();
            let sink = match output {
                Endpoint::Es(_) => {
                    until_interrupted(&interrupt, build_sink(cli, output, retry, gate.clone()))
                        .await?
                }
                // local only: never cut off half-way through staging its file
                _ => build_sink(cli, output, retry, gate.clone()).await?,
            };
            match input {
                Endpoint::Es(ep) => {
                    let setup = async {
                        let opts = es_source_options(cli, ep, retry, gate.clone()).await?;
                        check_rename_merges(cli, &opts.client, ep.index.as_deref(), retry).await?;
                        Ok(opts)
                    };
                    let opts = match until_interrupted(&interrupt, setup).await {
                        Ok(opts) => opts,
                        Err(e) => {
                            sink.abort().await;
                            return Err(e);
                        }
                    };
                    if let Err(e) = check_cross_engine_target(&sink, &opts, ep, retry).await {
                        sink.abort().await;
                        return Err(e);
                    }
                    // TST-BRK-XV-002: hits without _source stop the dump unless
                    // --allowMissingSource. L-455b (behaviour change): this now holds for a
                    // search body that turns _source off too; `"_source": false` is refused
                    // before the first search, a filter that leaves no _source at the first
                    // such hit. Either error names the search body (as L-455a's stored_fields).
                    if !cli.allow_missing_source
                        && sonic_rs::JsonValueTrait::get(&opts.search_body, "_source")
                            .and_then(sonic_rs::JsonValueTrait::as_bool)
                            == Some(false)
                    {
                        sink.abort().await;
                        anyhow::bail!(
                            "documents cannot be fully dumped: the search body sets \"_source\": false, so every hit would come back without _source (only _id and metadata could be written). Nothing was written; remove \"_source\": false from the search body to keep the documents, or pass --allowMissingSource to dump them without _source"
                        );
                    }
                    // L-455c (behaviour change): a _source filter that excludes every field
                    // would write `{}` for every document; refused up front the same way.
                    let excludes_all = (!cli.allow_missing_source)
                        .then(|| retrieval::source_filter_excludes_all(&opts.search_body))
                        .flatten();
                    if let Some(p) = excludes_all {
                        sink.abort().await;
                        anyhow::bail!(
                            "documents cannot be fully dumped: the search body's _source filter excludes every field (pattern \"{p}\"), so every hit would come back with an empty _source ({{}}) and only _id and metadata could be written. Nothing was written; remove the _source filter from the search body to keep the documents, or pass --allowMissingSource to dump them with an empty _source"
                        );
                    }
                    retrieval::set_require_source(!cli.allow_missing_source);
                    retrieval::set_body_drops_source(retrieval::BodySource::of(&opts.search_body));
                    // L-804b (P3): validate() allows the flag only for scroll dumps into a file
                    let mut restarts_left = cli.restart_on_context_loss;
                    let mut sink = sink;
                    loop {
                        match retrieval::dump_data(opts.clone(), sink, window.clone()).await {
                            Err(e)
                                if restarts_left > 0
                                    && e.downcast_ref::<http::ContextLoss>().is_some()
                                    && !http::is_cancelled(&e)
                                    && signals::received().is_none() =>
                            {
                                restarts_left -= 1;
                                log::warn!(
                                    "{e:#}. Restarting the dump from the start (--restartOnContextLoss; {restarts_left} restart(s) left)"
                                );
                                sink = build_sink(cli, output, retry, gate.clone()).await?;
                            }
                            r => break r?,
                        }
                    }
                }
                inp => {
                    pipeline::file_to_sink(
                        input_loc(inp),
                        cli.limit,
                        (cli.max_doc_mb > 0).then_some(cli.max_doc_mb << 20),
                        window,
                        sink,
                        signals::token().child_token(),
                    )
                    .await?
                }
            }
        }
        DumpType::Meta(kind) => run_meta(cli, kind, input, output, retry, gate).await?,
    };
    Ok(report)
}

/// L-655 (BRK-XV-004): a cluster-to-cluster data copy across engines into a target index
/// that does not exist would create it with dynamic mappings, which silently guess other
/// types for fields the target engine has no type for (`flattened`, `dense_vector`,
/// `flat_object`, `knn_vector`, ...). Refuse that before any document is written, naming
/// the fields; the mapping has to be created on the target first.
async fn check_cross_engine_target(
    sink: &pipeline::Sink,
    src: &retrieval::EsSourceOptions,
    ep: &endpoint::EsEndpoint,
    retry: crate::http::RetryPolicy,
) -> Result<()> {
    let pipeline::Sink::Bulk(bulk) = sink else {
        return Ok(());
    };
    let Some(index) = ep.index.as_deref() else {
        return Ok(());
    };
    // the only source request a scroll copy makes before searching, and only when
    // the copy names an index; a source that hides `GET /` skips the check
    let cancel = tokio_util::sync::CancellationToken::new();
    let src_info = match src.client.info(retry, &cancel).await {
        Ok(i) => i,
        Err(e) => {
            log::debug!("the source engine is unknown ({e:#}); no cross-engine field type check");
            return Ok(());
        }
    };
    let target = bulk.target_info();
    if src_info.flavor == target.flavor {
        return Ok(());
    }
    let Some(missing) = bulk.missing_target().await else {
        return Ok(());
    };
    let what = format!("GET {index}/_mapping (L-655 cross-engine check)");
    let mappings = match meta::http::get_json(&src.client, retry, &[index, "_mapping"], &what).await
    {
        Ok(m) => m,
        Err(e) => {
            log::warn!(
                "the target index '{missing}' does not exist and the source mapping could not be read to check it ({e:#}); it will be created with dynamic mappings"
            );
            return Ok(());
        }
    };
    let mut found = Vec::new();
    if let Some(o) = mappings.as_object() {
        for (src_index, def) in o {
            let m = def.get("mappings").unwrap_or(def);
            found.extend(
                meta::field_types::unportable(m, target)
                    .into_iter()
                    .map(|mut u| {
                        if o.len() > 1 {
                            u.path = format!("{src_index}:{}", u.path);
                        }
                        u
                    }),
            );
        }
    }
    if found.is_empty() {
        return Ok(());
    }
    let list: Vec<String> = found.iter().map(ToString::to_string).collect();
    anyhow::bail!(
        "the target index '{missing}' does not exist, so {} {} would create it with dynamic mappings and guess other types for {} source field(s) it has no type for: {}; nothing was written. Create the index with a converted mapping first (dump --type mapping to a file, change those fields, restore it), then copy the data",
        target.flavor_name(),
        target.version,
        found.len(),
        list.join(", ")
    )
}

/// `fut`, or the interrupted error as soon as a first Ctrl+C / SIGTERM cancels
/// `interrupt` (while a [`signals::graceful`] guard is held).
async fn until_interrupted<T>(
    interrupt: &tokio_util::sync::CancellationToken,
    fut: impl std::future::Future<Output = Result<T>>,
) -> Result<T> {
    tokio::select! {
        r = fut => r,
        _ = interrupt.cancelled() => Err(signals::interrupted_error()),
    }
}

/// Smallest accepted `--fileSize` (TST-BRK-RES-006).
const MIN_FILE_SIZE: u64 = 256;

pub async fn run() -> Result<()> {
    let argv: Vec<String> = std::env::args().collect();
    let (argv, oos_warnings) = cli::prescan_out_of_scope(argv).map_err(usage)?;
    let cli = Cli::parse_from(argv);
    register_early_secrets(&cli);
    init_logging(&cli);
    memplan::set(cli.buffer_size, cli.slices, cli.concurrency, cli.limit);
    if let Some(limit) = open_file_limit() {
        // connections (one per concurrent request or slice, input and output side) plus
        // ~24 for the runtime, logs, input and output files (TST-BRK-RES-004)
        let need = (cli.concurrency as u64)
            .saturating_add(cli.slices.max(1) as u64)
            .saturating_mul(2)
            .saturating_add(24);
        if need > limit {
            log::warn!(
                "--concurrency {} and --slices {} may need ~{need} open files but the limit (ulimit -n) is {limit}; expect 'Too many open files' unless it is raised or those flags lowered",
                cli.concurrency,
                cli.slices
            );
        }
    }
    signals::install();
    for w in &oos_warnings {
        log::warn!("{w}");
    }
    if !cli.quiet() {
        console::set_colors_enabled(true);
    }

    remote::set_config_files(
        cli.input_storage_config.clone().map(PathBuf::from),
        cli.output_storage_config.clone().map(PathBuf::from),
    );
    let mut input = endpoint::parse(&cli.input, Direction::Input, cli.input_index.as_deref())
        .map_err(|e| usage(format!("{e:#}")))?;
    let mut output = endpoint::parse(&cli.output, Direction::Output, cli.output_index.as_deref())
        .map_err(|e| usage(format!("{e:#}")))?;
    if cli.r#type == DumpType::Meta(meta::MetaKind::Alias) {
        node_alias_url(&cli, &mut input);
    }
    validate(&cli, &input, &output)?;
    let target_renamer = renamer(&cli)?;
    if !cli.force()
        && same_target(
            &input,
            &output,
            &cli.prefix,
            &cli.suffix,
            target_renamer.as_ref(),
        )
    {
        let affix = match (
            cli.prefix.is_empty() && cli.suffix.is_empty(),
            target_renamer.is_some(),
        ) {
            (true, false) => "",
            (false, false) => " (after --prefix/--suffix)",
            (true, true) => " (after --renamePrefix/--renameIndex)",
            (false, true) => " (after --renamePrefix/--renameIndex and --prefix/--suffix)",
        };
        return Err(usage(format!(
            "--input and --output name the same file or index{affix}; refusing to read and write it at once (pass --force to do it anyway)"
        )));
    }

    let auth_file = match &cli.http_auth_file {
        Some(p) => Some(endpoint::read_auth_file(p)?),
        None => None,
    };
    if let Endpoint::Es(ep) = &mut input {
        config::resolve_endpoint_auth(&cli, ep, Direction::Input, auth_file.as_ref());
    }
    if let Endpoint::Es(ep) = &mut output {
        config::resolve_endpoint_auth(&cli, ep, Direction::Output, auth_file.as_ref());
    }
    register_secrets(&cli, [&input, &output], auth_file.as_ref());

    log::info!(
        "Starting dump: {} -> {} (type {})",
        endpoint::redacted_endpoint(&cli.input),
        endpoint::redacted_endpoint(&cli.output),
        cli.r#type.name()
    );

    let retry = config::retry_policy(&cli)?;
    let gate = config::request_gate(&cli);
    let window = DocWindow::new(cli.offset, cli.size);

    let report = match transfer(&cli, &input, &output, retry, gate, window).await {
        Ok(report) => report,
        Err(e) => {
            // L-647: a restore stopped by a signal still reports what it wrote
            if let Some(stopped) = e.downcast_ref::<signals::Stopped>()
                && !cli.quiet()
            {
                let mut err = std::io::stderr().lock();
                let _ = writeln!(err, "Total Writes: {}", stopped.written);
                if stopped.overwritten > 0 {
                    let _ = writeln!(
                        err,
                        "Overwritten: {} (documents that replaced one with the same _id; included in Total Writes)",
                        stopped.overwritten
                    );
                }
                if stopped.failed > 0 {
                    let _ = writeln!(err, "Failed Writes: {}", stopped.failed);
                }
            }
            return Err(e);
        }
    };

    if !cli.quiet() {
        let mut err = std::io::stderr().lock();
        let _ = writeln!(err, "Total Writes: {}", report.written);
        if report.overwritten > 0 {
            // TST-BRK-IN-010: duplicate _ids (in the input, or already in the index)
            let _ = writeln!(
                err,
                "Overwritten: {} (documents that replaced one with the same _id; included in Total Writes)",
                report.overwritten
            );
        }
        if report.failed > 0 {
            let _ = writeln!(err, "Failed Writes: {}", report.failed);
        }
        let _ = writeln!(err, "dump complete");
    }
    if report.rejected > 0 {
        // A malformed line means the input framing is broken (INJ-F1):
        // never report success, even with --ignore-errors.
        return Err(anyhow::anyhow!(
            "{} document line(s) were rejected (not exactly one JSON object, or an invalid _index; see the errors above); nothing was written for them",
            report.rejected
        ));
    }
    // Item-level failures (429 storms, flood-stage blocks, mapping errors, conflicts): the
    // run continues past them by default like Node, but only an explicit opt-in turns them
    // into exit status 0 (Q3, safest option).
    let accepted = cli.ignore_es_write_errors == Some(true) || cli.ignore_errors();
    if report.failed > 0 && !accepted {
        return Err(anyhow::Error::new(WriteFailures(
            report.failed,
            report.blocked,
            report.disk_blocked,
        )));
    }
    Ok(())
}

#[cfg(test)]
mod compression_default_tests {
    //! D-2: gzip stays the compression default (`--fsCompress`, Node parity);
    //! zstd is only ever chosen by an explicit `--compressionType zstd`.
    use super::*;
    use crate::pipeline::file_sink::Codec;

    fn codec(extra: &[&str]) -> Option<Codec> {
        let mut v = vec!["searchdump", "--input", "http://localhost:9200/i"];
        v.extend_from_slice(&["--output", "dump.json"]);
        v.extend_from_slice(extra);
        let cli = Cli::try_parse_from(v).unwrap();
        let out = Endpoint::File(std::path::PathBuf::from("dump.json"));
        file_options(&cli, &out).unwrap().codec
    }

    #[test]
    fn no_flag_is_uncompressed() {
        assert_eq!(codec(&[]), None);
        assert_eq!(codec(&["--fsCompress", "false"]), None);
    }

    #[test]
    fn fs_compress_and_level_mean_gzip_never_zstd() {
        assert_eq!(codec(&["--fsCompress"]), Some(Codec::Gzip(6)));
        assert_eq!(codec(&["--fsCompress", "true"]), Some(Codec::Gzip(6)));
        assert_eq!(
            codec(&["--fsCompress", "--compressionLevel", "-1"]),
            Some(Codec::Gzip(6))
        );
        assert_eq!(
            codec(&["--fsCompress", "--compressionLevel", "9"]),
            Some(Codec::Gzip(9))
        );
        assert_eq!(codec(&["--compressionType", "gzip"]), Some(Codec::Gzip(6)));
        // a level alone selects nothing (warned), in particular not zstd
        assert_eq!(codec(&["--compressionLevel", "3"]), None);
    }

    #[test]
    fn zstd_only_when_asked() {
        assert_eq!(codec(&["--compressionType", "zstd"]), Some(Codec::Zstd(3)));
        assert_eq!(
            codec(&["--fsCompress", "--compressionType", "zstd"]),
            Some(Codec::Zstd(3))
        );
    }

    #[test]
    fn slow_levels_warn_others_do_not() {
        let w = |c| slow_level_warning(Some(c));
        assert!(w(Codec::Gzip(9)).unwrap().contains("gzip 9"));
        assert!(
            w(Codec::Zstd(19))
                .unwrap()
                .contains("--compressionLevel 19 with zstd")
        );
        assert!(
            w(Codec::Zstd(22))
                .unwrap()
                .contains("--compressionLevel 22 with zstd")
        );
        for c in [
            Codec::Gzip(1),
            Codec::Gzip(6),
            Codec::Zstd(3),
            Codec::Zstd(9),
            Codec::Zstd(18),
        ] {
            assert!(w(c).is_none(), "{c:?}");
        }
        assert!(slow_level_warning(None).is_none());
        // the levels themselves are unchanged (a warning, not an error)
        assert_eq!(
            codec(&["--fsCompress", "--compressionLevel", "9"]),
            Some(Codec::Gzip(9))
        );
        assert_eq!(
            codec(&["--compressionType", "zstd", "--compressionLevel", "19"]),
            Some(Codec::Zstd(19))
        );
    }

    #[test]
    fn help_says_gzip_default_zstd_opt_in() {
        use clap::CommandFactory;
        let help = Cli::command().render_long_help().to_string();
        // clap wraps long lines: compare with whitespace collapsed
        let help = help.split_whitespace().collect::<Vec<_>>().join(" ");
        assert!(help.contains("--fsCompress"), "{help}");
        assert!(help.contains("off by default"), "{help}");
        assert!(help.contains("only when given explicitly"), "{help}");
    }
}

#[cfg(test)]
mod exit_code_tests {
    //! L-804b (P1, P2, P5): exit status classes.
    use super::*;

    fn http(status: u16) -> anyhow::Error {
        anyhow::Error::new(http::EsHttpError {
            prefix: String::new(),
            operation: "bulk".into(),
            status,
            body: "{}".into(),
            endpoint: String::new(),
        })
    }

    #[test]
    fn transient_failures_exit_4_and_permanent_ones_1() {
        let net = || anyhow::Error::msg(http::Transient("connection refused".into()));
        let cases: Vec<(&str, anyhow::Error, i32)> = vec![
            ("network", net(), 4),
            // P2: the typed cause survives the pipeline's context layers
            (
                "network via output task",
                net()
                    .context("Failed to write batch buffer")
                    .context("Output task failed"),
                4,
            ),
            ("429", http(429), 4),
            ("500", http(500), 4),
            (
                "503 in context",
                http(503).context("Retrieval task failed"),
                4,
            ),
            (
                "context lost",
                http(404).context(http::ContextLoss("scroll context gone".into())),
                4,
            ),
            ("404", http(404), 1),
            // L-809: the same dump would hit the same open-scroll limit again
            (
                "500 scroll context limit",
                anyhow::Error::new(http::EsHttpError {
                    prefix: String::new(),
                    operation: "initial search".into(),
                    status: 500,
                    body: r#"{"error":{"reason":"Trying to create too many scroll contexts. Must be less than or equal to: [500]. This limit can be set by changing the [search.max_open_scroll_context] setting."}}"#.into(),
                    endpoint: String::new(),
                }),
                1,
            ),
            ("400", http(400), 1),
            ("401", http(401), 1),
            ("plain", anyhow::anyhow!("mapping refused"), 1),
            (
                "auto ids already written",
                http(429).context(http::NotRerunSafe("duplicates".into())),
                1,
            ),
            (
                "cancelled",
                anyhow::Error::new(http::DumpCancelled).context(http::ContextLoss("x".into())),
                1,
            ),
            ("usage", usage("bad flag"), 2),
            (
                "write failures",
                anyhow::Error::new(WriteFailures(3, 0, 0)),
                3,
            ),
        ];
        for (name, e, want) in cases {
            assert_eq!(exit_code(&e), want, "{name}: {e:#}");
        }
        // the marker only adds a type: the message reads as before
        assert_eq!(
            format!("{:#}", net().context("Output task failed")),
            "Output task failed: connection refused"
        );
    }

    #[test]
    fn exit_3_summary_names_a_dominant_block() {
        let plain = WriteFailures(4, 1, 1).to_string();
        assert!(!plain.contains("Most of them"), "{plain}");
        let disk = WriteFailures(4, 3, 3).to_string();
        assert!(
            disk.contains("Most of them (3 of 4)")
                && disk.contains("flood-stage watermark")
                && disk.contains("free disk space")
                && disk.contains("--retryAttempts"),
            "{disk}"
        );
        let block = WriteFailures(4, 3, 0).to_string();
        assert!(
            block.contains("index or cluster block") && !block.contains("disk"),
            "{block}"
        );
    }

    #[test]
    fn disk_blocks_are_recognised_in_every_version_wording() {
        for reason in [
            "index [i] blocked by: [FORBIDDEN/12/index read-only / allow delete (api)];",
            "index [i] blocked by: [TOO_MANY_REQUESTS/12/disk usage exceeded flood-stage watermark, index has read-only-allow-delete block];",
        ] {
            assert!(pipeline::bulk::is_disk_block(reason), "{reason}");
        }
        assert!(!pipeline::bulk::is_disk_block(
            "index [i] blocked by: [FORBIDDEN/8/index write (api)];"
        ));
    }
}
