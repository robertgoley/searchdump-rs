//! searchdump-multi: dump or load many indices at once with searchdump.
//!
//! Each selected index gets its own settings, mapping, optional alias and data files, in the
//! layout that `scripts/oslogs.py` writes (`INDEX.settings.json`, `INDEX.mapping.json`,
//! `INDEX.data.json.gz` / `.zst`), so a directory written by either tool loads with either.
//! Indices are selected by `--match` (and `--exclude`) regexes and can be broken out into
//! per-index or per-group subdirectories (`--layout`). The per-index work runs as child
//! `searchdump` processes, at most `--parallel` indices at a time; one index failing does not
//! stop the others. A manifest (`multielasticdump.json`) records what was dumped.
//!
//! `--matchType` also selects indices by alias or whole data streams; cluster-wide templates,
//! policies and stored scripts go to `DIR/_cluster/` and load before any index; `--fileSize` /
//! `--maxRows` split data files into parts that load in order.
//!
//! `--nodeCompat` writes the file names Node's `multielasticdump` 6.x reads (`INDEX.json.gz`,
//! `INDEX.settings.json.gz`, ...) with the manifest and `_cluster/` moved to `DIR.meta` (`--metaDir`),
//! because Node treats every entry in the directory as an index. Load accepts both namings.
//!
//! Credentials: use `SEARCHDUMP_INPUT_*` / `SEARCHDUMP_OUTPUT_*` environment variables or
//! `--httpAuthFile`; children inherit them. `user:pass@` in a URL works but puts the password on
//! every child's command line (visible in `ps`). When `--httpAuthFile` is given it wins over URL
//! userinfo and env (L-645b).
#![forbid(unsafe_code)]

use std::collections::BTreeMap;
use std::fs;
use std::io::{BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use anyhow::{Context, Result, anyhow, bail};
use clap::{Parser, ValueEnum};
use regex::Regex;
use reqwest::Method;
use searchdump_rs::endpoint::read_auth_file;
use searchdump_rs::http::gate::RequestGate;
use searchdump_rs::http::retry::error_chain;
use searchdump_rs::http::tls::TlsOptions;
use searchdump_rs::http::{Body, ClientOptions, EsClient};
use serde::{Deserialize, Serialize};
use tokio_util::sync::CancellationToken;
use url::Url;

pub const MANIFEST: &str = "multielasticdump.json";

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum Direction {
    Dump,
    Load,
    /// Directory -> directory between stores (local, s3://, b2://, sftp://, fs://), byte for
    /// byte: compression, split parts, manifest and cluster files are kept (L-809a)
    Copy,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Layout {
    /// DIR/INDEX.* (oslogs.py layout)
    Flat,
    /// DIR/INDEX/INDEX.*
    Index,
    /// DIR/GROUP/INDEX.*, GROUP from the `group` named capture of --match (or its first group)
    Group,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Compression {
    Gzip,
    Zstd1,
    Zstd3,
    None,
}

impl Compression {
    pub fn suffix(self) -> &'static str {
        match self {
            Compression::Gzip => "json.gz",
            Compression::Zstd1 | Compression::Zstd3 => "json.zst",
            Compression::None => "json",
        }
    }
    fn flags(self) -> &'static [&'static str] {
        match self {
            Compression::Gzip => &["--fsCompress"],
            Compression::Zstd1 => &["--compressionType=zstd", "--compressionLevel=1"],
            Compression::Zstd3 => &["--compressionType=zstd", "--compressionLevel=3"],
            Compression::None => &[],
        }
    }
    /// The codec of an existing data file, from its name.
    pub fn of_file(name: &str) -> Option<Compression> {
        if name.ends_with(".data.json.gz") {
            Some(Compression::Gzip)
        } else if name.ends_with(".data.json.zst") {
            Some(Compression::Zstd3)
        } else if name.ends_with(".data.json") {
            Some(Compression::None)
        } else {
            None
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum Lifecycle {
    /// --excludeSettings=index.lifecycle,index.routing.allocation.include._tier_preference
    Drop,
    /// --convertIlmToIsm (translates ILM policy documents only; index.lifecycle settings are
    /// not rewritten, so an OpenSearch target rejects them: use drop there)
    Convert,
}

/// Default run-control flags for data runs (from scripts/oslogs.py). 1.2 (ROADMAP-1.2 section 6):
/// page and bulk sizes are no longer fixed here; they come from [`sizing`] per index
/// (`--serverLimits`), and `--serverLimits=off` reproduces the 1.1.0 values exactly.
const DATA_FLAGS: &[&str] = &[
    "--scrollTime=10m",
    "--retryAttempts=5",
    "--retryDelay=2000",
    "--timeout=300000",
];

/// 1.2 server-aware sizing for searchdump-multi (ROADMAP-1.2 sections 3-6). The parent probes
/// the cluster once (node stats, thread pools) and reads every index's primary docs and stored
/// bytes in one `_stats?level=indices` call, then gives each child explicit
/// `--limit=N --maxBulkBytes=M --serverLimits=off` (children never probe again).
pub mod sizing {
    use searchdump_rs::limits::{self, Budget, Limits, Mode};

    /// The `off` profile: the 1.1.0 (1.1-10) fixed data defaults.
    pub const OFF_LIMIT: u64 = 1000;
    /// The `off` profile bulk size (1.1-10).
    pub const OFF_BULK: u64 = 16 << 20;
    /// 1.1-10: stored (primary, compressed) bytes one dump page holds in the `off` profile.
    /// `_source` in the response is usually 1-4x the stored size, so a page stays well under the
    /// ~80 MB that a 256 MB heap survived in the 1.1-10 runs.
    pub const PAGE_STORE_BUDGET: u64 = 16 << 20;

    /// 1.1-10, the `off` profile: documents per dump page for an index whose primary documents
    /// average `avg_doc_bytes` stored bytes: 1000 for small documents, fewer for large ones so a
    /// page holds about [`PAGE_STORE_BUDGET`], never less than 1 (identical to 1.1.0).
    pub fn page_limit_for(avg_doc_bytes: u64) -> u64 {
        if avg_doc_bytes == 0 {
            return OFF_LIMIT;
        }
        (PAGE_STORE_BUDGET / avg_doc_bytes).clamp(1, OFF_LIMIT)
    }

    /// One run's sizing decision.
    #[derive(Debug, Clone)]
    pub struct Plan {
        pub mode: Mode,
        /// `--noDefaultDataFlags`: no sizing flags at all (only `--serverLimits=off`).
        pub no_defaults: bool,
        pub limits: Option<Limits>,
        pub budget: Option<Budget>,
        /// `--parallel` as asked (the upper bound).
        pub parallel: usize,
        /// Workers the budget fits at the largest index's page (reported; the byte semaphore
        /// enforces it at run time, so small indices still run `parallel` at once).
        pub fit_parallel: usize,
        /// Largest page response in bytes after the worker-first policy.
        pub page_cap: u64,
        /// Bulk bytes per load worker.
        pub bulk_bytes: u64,
    }

    /// One index's sizes.
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct IndexSize {
        pub limit: u64,
        pub max_bulk_bytes: u64,
        /// `default` | `large-docs` | `budget` | `off` | `fallback`
        pub reason: &'static str,
        /// Estimated bytes of one page in flight (for the byte semaphore; 0 = unknown).
        pub page_bytes: u64,
    }

    /// (docs, stored bytes) of an index's primaries.
    pub type Stats = (u64, u64);

    fn avg(s: Option<Stats>) -> Option<u64> {
        s.and_then(|(d, b)| (d > 0 && b > 0).then(|| b / d))
    }

    /// Estimated response bytes of a page of `limit` documents of an index.
    pub fn page_estimate(limit: u64, s: Option<Stats>) -> u64 {
        match s {
            Some((d, _)) if avg(s).is_some() => {
                limit.min(d.max(1)) * avg(s).unwrap_or(0) * limits::EXPANSION
            }
            _ => 0,
        }
    }

    impl Plan {
        /// Build the plan from the probe (or none for `off` / `--noDefaultDataFlags`) and the
        /// selected indices' stats. Worker-first (ROADMAP-1.2 4.1): if `parallel` pages of the
        /// largest index do not fit the in-flight budget, fewer workers are assumed first
        /// (down to 2, or 1 when 1 was asked), and only then the page shrinks.
        pub fn new(
            mode: Mode,
            no_defaults: bool,
            limits: Option<Limits>,
            parallel: usize,
            stats: &[Option<Stats>],
        ) -> Self {
            let budget = limits
                .as_ref()
                .and_then(|l| limits::budget(l, mode, 256 << 20));
            let mut plan = Plan {
                mode,
                no_defaults,
                limits,
                budget,
                parallel,
                fit_parallel: parallel,
                page_cap: 0,
                bulk_bytes: OFF_BULK,
            };
            let Some(b) = budget else {
                // a fallback on AWS OpenSearch Service still keeps bulks at the 10 MiB floor
                if plan.limits.as_ref().is_some_and(|l| l.aws) {
                    plan.bulk_bytes = limits::AWS_PAYLOAD_FLOOR;
                }
                return plan;
            };
            let biggest = stats
                .iter()
                .map(|s| page_estimate(limits::MULTI_MAX_PAGE, *s).min(b.page_bytes))
                .max()
                .unwrap_or(0)
                .max(1);
            let floor = parallel.min(2);
            let mut p = parallel.max(1);
            while p > floor && (p as u64) * biggest > b.inflight_bytes {
                p -= 1;
            }
            plan.fit_parallel = p;
            plan.page_cap = b.page_bytes.min(b.inflight_bytes / p.max(1) as u64);
            plan.bulk_bytes = limits::bulk_bytes(&b, parallel.max(1) as u64).min(100 << 20);
            plan
        }

        /// Sizes for a dump of one index.
        pub fn dump(&self, s: Option<Stats>) -> IndexSize {
            match self.budget {
                None => {
                    let limit = avg(s).map_or(OFF_LIMIT, page_limit_for);
                    IndexSize {
                        limit,
                        max_bulk_bytes: OFF_BULK,
                        reason: if self.limits.is_some() {
                            "fallback"
                        } else {
                            "off"
                        },
                        page_bytes: 0,
                    }
                }
                Some(b) => {
                    let limit = limits::page_docs(self.page_cap, avg(s), limits::MULTI_MAX_PAGE);
                    let reason = if limit >= limits::MULTI_MAX_PAGE {
                        "default"
                    } else if self.page_cap < b.page_bytes {
                        "budget"
                    } else {
                        "large-docs"
                    };
                    IndexSize {
                        limit,
                        max_bulk_bytes: self.bulk_bytes,
                        reason,
                        page_bytes: page_estimate(limit, s),
                    }
                }
            }
        }

        /// Sizes for a load (the file batch and the bulk bytes; the bulk bytes are the memory
        /// control on the target).
        pub fn load(&self) -> IndexSize {
            match self.budget {
                None => IndexSize {
                    limit: OFF_LIMIT,
                    max_bulk_bytes: self.bulk_bytes,
                    reason: if self.limits.is_some() {
                        "fallback"
                    } else {
                        "off"
                    },
                    page_bytes: 0,
                },
                Some(_) => IndexSize {
                    limit: if self.bulk_bytes >= 64 << 20 {
                        limits::MULTI_MAX_PAGE
                    } else {
                        OFF_LIMIT
                    },
                    max_bulk_bytes: self.bulk_bytes,
                    reason: if self.bulk_bytes < OFF_BULK {
                        "budget"
                    } else {
                        "default"
                    },
                    page_bytes: 0,
                },
            }
        }

        /// The child flags for `size` (before the user's flags after `--`, which win).
        pub fn flags(&self, size: &IndexSize, child_knows_server_limits: bool) -> Vec<String> {
            let mut v = vec![];
            if !self.no_defaults {
                v.push(format!("--limit={}", size.limit));
                v.push(if size.max_bulk_bytes == OFF_BULK {
                    "--maxBulkBytes=16mb".to_string()
                } else {
                    format!("--maxBulkBytes={}kb", (size.max_bulk_bytes >> 10).max(1))
                });
            }
            if child_knows_server_limits {
                v.push("--serverLimits=off".to_string());
            }
            v
        }

        /// The one "server limits:" line.
        pub fn line(&self) -> String {
            let head = match &self.limits {
                None if self.no_defaults => {
                    return "server limits: not used (--noDefaultDataFlags) [off]".into();
                }
                None => {
                    return format!(
                        "server limits: not probed -> fixed 1.1 defaults [{}]",
                        self.mode.as_str()
                    );
                }
                Some(l) => limits::describe(l),
            };
            match self.budget {
                None => format!("server limits: {head} [{}]", self.mode.as_str()),
                Some(b) => format!(
                    "server limits: {head} -> page budget {} MB, in flight {} MB, bulk {} KB per worker, parallel {}{} [{}]",
                    self.page_cap >> 20,
                    b.inflight_bytes >> 20,
                    self.bulk_bytes >> 10,
                    self.parallel,
                    if self.fit_parallel < self.parallel {
                        format!(
                            " (the largest index fits {} at its full page; smaller ones run {} at once)",
                            self.fit_parallel, self.parallel
                        )
                    } else {
                        String::new()
                    },
                    self.mode.as_str()
                ),
            }
        }
    }
}

/// The flag name of an argument (`--limit=5` / `--limit` -> `--limit`), or None for a value.
fn flag_name(arg: &str) -> Option<&str> {
    if !arg.starts_with("--") || arg == "--" {
        return None;
    }
    Some(arg.split_once('=').map_or(arg, |(n, _)| n))
}

/// Default data flags followed by the arguments after `--`. A flag given after `--` replaces
/// the default of the same name (`-- --limit=200` runs with `--limit=200`, not both), so
/// every default can be overridden without `--noDefaultDataFlags`.
fn merge_data_flags(defaults: &[&str], extra: &[String]) -> Vec<String> {
    let given: std::collections::HashSet<&str> =
        extra.iter().filter_map(|a| flag_name(a)).collect();
    defaults
        .iter()
        .filter(|d| flag_name(d).is_none_or(|n| !given.contains(n)))
        .map(|d| d.to_string())
        .chain(extra.iter().cloned())
        .collect()
}

/// [`merge_data_flags`] for computed (owned) defaults: `extra` (the run's data flags, ending in
/// the user's flags after `--`) replaces a default of the same name.
fn merge_data_flags_owned(defaults: &[String], extra: &[String]) -> Vec<String> {
    let d: Vec<&str> = defaults.iter().map(String::as_str).collect();
    merge_data_flags(&d, extra)
}

/// 1.2: the per-index note when an index gets a page below the 1.1 default (large documents
/// or the budget), and a warning when the user's `-- --limit` is above the safe page.
fn note_size(ctx: &Ctx, index: &str, st: Option<sizing::Stats>, size: &sizing::IndexSize) {
    let user_limit = ctx.args.extra.iter().find_map(|a| {
        a.strip_prefix("--limit=")
            .and_then(|v| v.parse::<u64>().ok())
    });
    let avg_kb = st
        .and_then(|(d, b)| (d > 0).then(|| (b / d).div_ceil(1024)))
        .unwrap_or(0);
    match user_limit {
        Some(u) if ctx.plan().budget.is_some() && u > size.limit => eprintln!(
            "warning: {index}: -- --limit={u} is above the page the cluster's limits allow ({} documents of ~{avg_kb} KB stored); kept because you chose it",
            size.limit
        ),
        Some(_) => {}
        None if ctx.plan().no_defaults => {}
        None if size.limit < sizing::OFF_LIMIT => eprintln!(
            "note: {index}: documents average {avg_kb} KB stored; dumping {} per page instead of {} ({}; give -- --limit=N to choose)",
            size.limit,
            if ctx.plan().budget.is_some() {
                limits_max_page()
            } else {
                sizing::OFF_LIMIT
            },
            size.reason
        ),
        None => {}
    }
}

fn limits_max_page() -> u64 {
    searchdump_rs::limits::MULTI_MAX_PAGE
}

/// The manifest's per-index `sizing` object.
fn sizing_json(size: &sizing::IndexSize, user: &[String]) -> serde_json::Value {
    let user_limit = user.iter().any(|a| flag_name(a) == Some("--limit"));
    serde_json::json!({
        "limit": size.limit,
        "max_bulk_bytes": size.max_bulk_bytes,
        "reason": if user_limit { "user" } else { size.reason },
    })
}

/// Per-index item types, in the order they are dumped and loaded.
pub const TYPE_ORDER: &[&str] = &["settings", "analyzer", "mapping", "alias", "data"];

/// Cluster-scoped types, dumped once per run into `DIR/_cluster/TYPE.json` and loaded before
/// any index, in this order (policies before the templates that name them, component templates
/// before the index templates composed of them).
pub const CLUSTER_TYPES: &[&str] = &[
    "policy",
    "component_template",
    "index_template",
    "template",
    "script",
];

/// Subdirectory of the dump root for cluster-scoped files. Index names cannot start with `_`,
/// so it never collides with an index directory.
pub const CLUSTER_DIR: &str = "_cluster";

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum Order {
    Asc,
    Desc,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum MatchType {
    /// --match is tested against index names
    Index,
    /// --match is tested against alias names; every index behind a matching alias is selected
    Alias,
    /// --match is tested against data stream names; each matching stream is dumped as one unit
    Datastream,
}

#[derive(Parser, Debug, Clone)]
#[command(
    name = "searchdump-multi",
    version,
    about = "Dump or load many indices at once with searchdump, selected by regex",
    after_help = "Examples:\n  \
      # every index matching logs-*, gzip, one set of files per index (oslogs.py layout)\n  \
      SEARCHDUMP_INPUT_USERNAME=admin SEARCHDUMP_INPUT_PASSWORD=... \\\n    \
      searchdump-multi --direction dump --input https://os:9200 --output /backup --match '^logs-'\n  \
      # break out by month: /backup/2026.10/logs-app-2026.10.31.*\n  \
      searchdump-multi --input https://os:9200 --output /backup --layout group \\\n    \
      --match '^logs-.*-(?P<group>\\d{4}\\.\\d{2})\\.\\d{2}$'\n  \
      # data streams plus the templates and policies they need, 1 GB data parts\n  \
      searchdump-multi --input https://es:9200 --output /backup --matchType datastream \\\n    \
      --match '^logs-' --types data,index_template,component_template,policy --fileSize 1gb\n  \
      # load everything back (SEARCHDUMP_OUTPUT_* credentials), 4 indices at a time\n  \
      searchdump-multi --direction load --input /backup --output https://os2:9200 --parallel 4\n\n\
      Extra arguments after `--` are passed to every searchdump data run.\n\n\
      Provided as is, without warranty (MIT license). Verify every backup by restoring it and\n\
      comparing document counts, and test outside production first."
)]
pub struct Args {
    /// dump: cluster -> directory; load: directory -> cluster; copy: directory -> directory
    #[arg(long, value_enum, default_value = "dump")]
    pub direction: Direction,
    /// dump: cluster base URL (https://host:9200); load/copy: dump directory (a local path or
    /// s3://bucket/prefix, b2://bucket/prefix, sftp://user@host/path, fs:///path)
    #[arg(long)]
    pub input: String,
    /// dump/copy: output directory (local path or storage URL as for --input); load: cluster
    /// base URL
    #[arg(long)]
    pub output: String,
    /// Storage settings and credentials for a storage-URL --input (INI, as searchdump
    /// --inputStorageConfig; default $SEARCHDUMP_INPUT_STORAGE_CONFIG)
    #[arg(long = "inputStorageConfig")]
    pub input_storage_config: Option<PathBuf>,
    /// The same for a storage-URL --output (default $SEARCHDUMP_OUTPUT_STORAGE_CONFIG)
    #[arg(long = "outputStorageConfig")]
    pub output_storage_config: Option<PathBuf>,
    /// Local directory a dump to (or load from) a storage URL is staged in. Default: a new
    /// directory under $TMPDIR, removed after a successful upload / load. Needs room for the
    /// whole dump. Kept after a failed dump, so the run can be finished with --resume
    #[arg(long = "stagingDir")]
    pub staging_dir: Option<PathBuf>,
    /// copy, and the upload of a staged dump: replace objects that already exist at the
    /// destination (otherwise any existing one stops the copy before anything is written)
    #[arg(long)]
    pub overwrite: bool,
    /// Regex a name must match (unanchored; use ^...$ for whole names)
    #[arg(long = "match", default_value = "^.*$")]
    pub match_re: String,
    /// What --match is tested against (dump), comma separated: index, alias, datastream
    #[arg(
        long = "matchType",
        value_enum,
        value_delimiter = ',',
        default_value = "index"
    )]
    pub match_type: Vec<MatchType>,
    /// Regex of names to leave out (tested against the same names as --match)
    #[arg(long)]
    pub exclude: Option<String>,
    /// Also select hidden/system indices (names starting with '.'); off by default
    #[arg(long = "includeHidden", alias = "include-hidden")]
    pub include_hidden: bool,
    /// Directory layout for each index's files (dump); load finds files in any layout
    #[arg(long, value_enum, default_value = "flat")]
    pub layout: Layout,
    /// Item types, comma separated. Per index: settings, analyzer, mapping, alias, data.
    /// Cluster-wide, once per run: policy, component_template, index_template, template,
    /// script. `all` selects every type.
    /// Default: dump settings,mapping,data (as oslogs.py); load settings,mapping,alias,data
    /// plus every cluster-wide file present (a missing file just skips that step)
    #[arg(long, value_delimiter = ',')]
    pub types: Vec<String>,
    /// Types to skip (Node multielasticdump --ignoreType), comma separated
    #[arg(long = "ignoreType", value_delimiter = ',')]
    pub ignore_type: Vec<String>,
    /// Node compatibility: `--ignoreData[=true|false]` (true removes the type, false adds it)
    #[arg(long = "ignoreData", num_args = 0..=1, default_missing_value = "true", value_parser = clap::builder::BoolishValueParser::new(), hide_short_help = true)]
    pub ignore_data: Option<bool>,
    /// Node compatibility: `--ignoreMapping[=true|false]` (true removes the type, false adds it)
    #[arg(long = "ignoreMapping", num_args = 0..=1, default_missing_value = "true", value_parser = clap::builder::BoolishValueParser::new(), hide_short_help = true)]
    pub ignore_mapping: Option<bool>,
    /// Node compatibility: `--ignoreSettings[=true|false]` (true removes the type, false adds it)
    #[arg(long = "ignoreSettings", num_args = 0..=1, default_missing_value = "true", value_parser = clap::builder::BoolishValueParser::new(), hide_short_help = true)]
    pub ignore_settings: Option<bool>,
    /// Node compatibility: `--ignoreTemplate[=true|false]` (true removes the type, false adds it)
    #[arg(long = "ignoreTemplate", num_args = 0..=1, default_missing_value = "true", value_parser = clap::builder::BoolishValueParser::new(), hide_short_help = true)]
    pub ignore_template: Option<bool>,
    /// Node compatibility: `--ignoreAlias[=true|false]` (true removes the type, false adds it)
    #[arg(long = "ignoreAlias", num_args = 0..=1, default_missing_value = "true", value_parser = clap::builder::BoolishValueParser::new(), hide_short_help = true)]
    pub ignore_alias: Option<bool>,
    /// Node compatibility: `--ignoreAnalyzer[=true|false]` (true removes the type, false adds it)
    #[arg(long = "ignoreAnalyzer", num_args = 0..=1, default_missing_value = "true", value_parser = clap::builder::BoolishValueParser::new(), hide_short_help = true)]
    pub ignore_analyzer: Option<bool>,
    /// Node compatibility: `--ignoreIndex[=true|false]` (true removes the type, false adds it)
    #[arg(long = "ignoreIndex", num_args = 0..=1, default_missing_value = "true", value_parser = clap::builder::BoolishValueParser::new(), hide_short_help = true)]
    pub ignore_index: Option<bool>,
    /// Node compatibility, accepted and ignored: one index failing never stops the others here
    #[arg(long = "ignoreChildError", num_args = 0..=1, default_missing_value = "true", value_parser = clap::builder::BoolishValueParser::new(), hide_short_help = true)]
    pub ignore_child_error: Option<bool>,
    /// Node compatibility: process only these types (as --types; `index` means settings,mapping)
    #[arg(long = "includeType", value_delimiter = ',', conflicts_with = "types")]
    pub include_type: Vec<String>,
    /// How many indices to process at once
    #[arg(long, default_value_t = 2)]
    pub parallel: usize,
    /// Order in which indices are started, by name (Node --order)
    #[arg(long, value_enum, default_value = "asc")]
    pub order: Order,
    /// Minimum milliseconds between starting one index and the next (Node --interval)
    #[arg(long, default_value_t = 0)]
    pub interval: u64,
    /// Data-file compression (dump). Load detects it from each file name
    #[arg(long, value_enum, default_value = "gzip")]
    pub compression: Compression,
    /// Dump: split each data file into parts of about this size (elasticdump --fileSize,
    /// e.g. 500mb, 2gb). Parts are INDEX.data.split-N.json[.gz|.zst]
    #[arg(long = "fileSize", conflicts_with = "max_rows")]
    pub file_size: Option<String>,
    /// Dump: split each data file into parts of at most this many records (--maxRows)
    #[arg(long = "maxRows")]
    pub max_rows: Option<u64>,
    /// Lifecycle settings on load: drop them, or convert ILM to ISM
    #[arg(long, value_enum, default_value = "drop")]
    pub lifecycle: Lifecycle,
    /// Load: prepend this to every target index name
    #[arg(long, default_value = "")]
    pub prefix: String,
    /// Load: append this to every target index name
    #[arg(long, default_value = "")]
    pub suffix: String,
    /// Load: skip an index that already exists on the target instead of failing it
    /// (cluster-wide items are created only if missing)
    #[arg(long = "skipExisting", alias = "skip-existing")]
    pub skip_existing: bool,
    /// Dump: skip indices the manifest already records as done (files still present)
    #[arg(long)]
    pub resume: bool,
    /// Skip TLS certificate verification (also passed to searchdump)
    #[arg(long)]
    pub insecure: bool,
    /// INI file with user= and password= for the cluster (also passed to searchdump)
    #[arg(long = "httpAuthFile")]
    pub http_auth_file: Option<String>,
    /// searchdump binary (default: next to this binary, then $SEARCHDUMP_BIN, then PATH)
    #[arg(long = "elasticdumpBin")]
    pub elasticdump_bin: Option<PathBuf>,
    /// Count the records of every data file by reading it back (and check the count the
    /// searchdump run reported), instead of using that count
    #[arg(long = "verifyCounts")]
    pub verify_counts: bool,
    /// Where each child run's output is logged (default: a new temp directory)
    #[arg(long = "logDir")]
    pub log_dir: Option<PathBuf>,
    /// Do not add the data-run defaults (--scrollTime=10m ... and the sizes); implies
    /// --serverLimits=off for the data flags
    #[arg(long = "noDefaultDataFlags")]
    pub no_default_data_flags: bool,
    /// 1.2: size pages, bulks and workers from the cluster's limits: off (1.1 fixed sizes), auto
    /// (default: probe once, page up to 5000 small documents, tighten for small heaps), min
    /// (half of auto's budget), check (print the limits and the per-index sizes, exit 0; exit 2
    /// when a flag after -- exceeds them)
    #[arg(long = "serverLimits", value_parser = searchdump_rs::limits::parse_mode)]
    pub server_limits: Option<searchdump_rs::limits::Mode>,
    /// Print the selected indices and the files they map to, then exit
    #[arg(long = "dryRun", alias = "dry-run")]
    pub dry_run: bool,
    /// Dump in Node multielasticdump's layout, so `multielasticdump --direction=load` of Node
    /// elasticdump 6.x restores it: flat, data as INDEX.json[.gz], parts as
    /// INDEX.split-N.json[.gz], every file gzipped or none (gzip or none only), one
    /// INDEX.template.json per index, and the manifest and other cluster-wide files kept out
    /// of the directory (in --metaDir). Load reads Node-named directories without it.
    #[arg(long = "nodeCompat", alias = "node-compat")]
    pub node_compat: bool,
    /// Where the manifest and cluster-wide files (`_cluster/`) go. Default: the dump
    /// directory, or DIR.meta beside it with --nodeCompat. Load looks in DIR, then DIR.meta
    #[arg(long = "metaDir")]
    pub meta_dir: Option<PathBuf>,
    /// Extra searchdump arguments for every data run; a flag given here replaces the default
    /// data flag of the same name (e.g. `-- --limit=200`)
    #[arg(last = true)]
    pub extra: Vec<String>,
}

impl Args {
    /// Whether data files are written in parts (`--fileSize`/`--maxRows` here or after `--`).
    pub fn splits(&self) -> bool {
        self.file_size.is_some()
            || self.max_rows.is_some()
            || self.extra.iter().any(|a| {
                ["--fileSize", "--maxRows"]
                    .iter()
                    .any(|f| a == f || a.starts_with(&format!("{f}=")))
            })
    }
}

// ----------------------------------------------------------------------------- selection

/// Name selection: `--match`, `--exclude`, hidden indices, and the break-out group.
pub struct Selector {
    include: Regex,
    exclude: Option<Regex>,
    include_hidden: bool,
}

impl Selector {
    pub fn new(include: &str, exclude: Option<&str>, include_hidden: bool) -> Result<Self> {
        let include =
            Regex::new(include).with_context(|| format!("invalid --match regex '{include}'"))?;
        let exclude = exclude
            .map(|e| Regex::new(e).with_context(|| format!("invalid --exclude regex '{e}'")))
            .transpose()?;
        Ok(Self {
            include,
            exclude,
            include_hidden,
        })
    }

    pub fn selects(&self, name: &str) -> bool {
        // Hidden/system indices only with --includeHidden or a --match that starts with a literal dot.
        let p = self.include.as_str();
        let explicit_dot = p.starts_with("^\\.") || p.starts_with("\\.");
        if name.starts_with('.') && !self.include_hidden && !explicit_dot {
            return false;
        }
        self.include.is_match(name) && !self.exclude.as_ref().is_some_and(|e| e.is_match(name))
    }

    pub fn has_group(&self) -> bool {
        self.include.captures_len() > 1
    }

    /// The break-out group: the `group` named capture, else the first capture group.
    pub fn group(&self, name: &str) -> Option<String> {
        let caps = self.include.captures(name)?;
        let m = caps.name("group").or_else(|| caps.get(1))?;
        Some(sanitize_dir(m.as_str()))
    }
}

/// What one dump job covers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TargetKind {
    Index,
    DataStream,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Target {
    /// Index or data stream name.
    pub name: String,
    pub kind: TargetKind,
    /// The name --match was tested against (the alias for --matchType alias); the group comes
    /// from it.
    pub matched: String,
}

/// The dump jobs for a cluster's indices, aliases and data streams. Backing indices of a
/// selected data stream are left to the stream; an index reached twice is dumped once.
pub fn select_targets(
    sel: &Selector,
    match_types: &[MatchType],
    indices: &[String],
    aliases: &[(String, String)],
    streams: &[(String, Vec<String>)],
) -> Vec<Target> {
    let mut out: Vec<Target> = Vec::new();
    let mut seen = std::collections::BTreeSet::new();
    let mut backing = std::collections::BTreeSet::new();
    if match_types.contains(&MatchType::Datastream) {
        for (name, members) in streams {
            if sel.selects(name) && seen.insert(name.clone()) {
                backing.extend(members.iter().cloned());
                out.push(Target {
                    name: name.clone(),
                    kind: TargetKind::DataStream,
                    matched: name.clone(),
                });
            }
        }
    }
    let open: std::collections::BTreeSet<&str> = indices.iter().map(String::as_str).collect();
    let mut add = |index: &str, matched: &str, out: &mut Vec<Target>| {
        if !backing.contains(index) && open.contains(index) && seen.insert(index.to_string()) {
            out.push(Target {
                name: index.to_string(),
                kind: TargetKind::Index,
                matched: matched.to_string(),
            });
        }
    };
    if match_types.contains(&MatchType::Index) {
        for i in indices.iter().filter(|i| sel.selects(i)) {
            add(i, i, &mut out);
        }
    }
    if match_types.contains(&MatchType::Alias) {
        for (alias, index) in aliases.iter().filter(|(a, _)| sel.selects(a)) {
            add(index, alias, &mut out);
        }
    }
    out.sort_by(|a, b| a.name.cmp(&b.name));
    out
}

/// A group value as one safe directory name.
pub fn sanitize_dir(raw: &str) -> String {
    let s: String = raw
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.' | '+' | '@') {
                c
            } else {
                '_'
            }
        })
        .collect();
    match s.as_str() {
        "" => "_".into(),
        "." | ".." => s.replace('.', "_"),
        _ => s,
    }
}

/// Index names are used as file names: refuse anything that could leave the directory.
pub fn check_index_name(name: &str) -> Result<()> {
    if name.is_empty()
        || name == "."
        || name == ".."
        || name.contains(['/', '\\', '\0', '\n', '\r'])
        || name.starts_with('-')
    {
        bail!("refusing unsafe index name {name:?}");
    }
    Ok(())
}

/// The directory (relative to the dump root) that holds one index's files. `matched` is the
/// name the group capture is taken from (the index itself, or the alias that selected it).
pub fn rel_dir(layout: Layout, sel: &Selector, index: &str, matched: &str) -> Result<PathBuf> {
    Ok(match layout {
        Layout::Flat => PathBuf::new(),
        Layout::Index => PathBuf::from(sanitize_dir(index)),
        Layout::Group => PathBuf::from(sel.group(matched).ok_or_else(|| {
            anyhow!("--layout group: --match has no capture group for {matched}")
        })?),
    })
}

pub fn item_file(index: &str, kind: &str, compression: Compression) -> String {
    if kind == "data" {
        format!("{index}.data.{}", compression.suffix())
    } else {
        format!("{index}.{kind}.json")
    }
}

/// File names in Node multielasticdump's layout: `INDEX.json[.gz]` for data and
/// `INDEX.TYPE.json[.gz]` for the rest, every file gzipped or none.
pub fn node_item_file(index: &str, kind: &str, compression: Compression) -> String {
    let gz = if compression == Compression::Gzip {
        ".gz"
    } else {
        ""
    };
    if kind == "data" {
        format!("{index}.json{gz}")
    } else {
        format!("{index}.{kind}.json{gz}")
    }
}

/// The file for one item in either naming.
pub fn item_file_for(node: bool, index: &str, kind: &str, compression: Compression) -> String {
    if node {
        node_item_file(index, kind, compression)
    } else {
        item_file(index, kind, compression)
    }
}

/// Name suffixes of the non-data files of an index, in either naming (`.index` is Node's
/// `--type=index` file).
const META_SUFFIXES: &[&str] = &[
    "settings",
    "analyzer",
    "mapping",
    "alias",
    "template",
    "index",
    "datastream",
];

/// What a data-file name says: the index, the codec, and the part number of a split dump.
/// searchdump names: `INDEX.data.json[.gz|.zst]`, `INDEX.data.split-N.json[...]`;
/// Node names: `INDEX.json[.gz]`, `INDEX.split-N.json[.gz]`.
pub fn parse_data_file(name: &str) -> Option<(String, Compression, Option<u64>)> {
    if name == MANIFEST {
        return None;
    }
    let (stem, c) = if let Some(s) = name.strip_suffix(".json.gz") {
        (s, Compression::Gzip)
    } else if let Some(s) = name.strip_suffix(".json.zst") {
        (s, Compression::Zstd3)
    } else if let Some(s) = name.strip_suffix(".json") {
        (s, Compression::None)
    } else {
        return None;
    };
    if let Some(index) = stem.strip_suffix(".data") {
        return (!index.is_empty()).then(|| (index.to_string(), c, None));
    }
    if let Some((head, n)) = stem.rsplit_once(".split-") {
        if !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit()) {
            let head = head.strip_suffix(".data").unwrap_or(head);
            let part: u64 = n.parse().ok()?;
            return (!head.is_empty()).then(|| (head.to_string(), c, Some(part)));
        }
        return None;
    }
    if stem.is_empty()
        || META_SUFFIXES
            .iter()
            .any(|k| stem.ends_with(&format!(".{k}")))
    {
        return None;
    }
    Some((stem.to_string(), c, None))
}

/// Whether a data-file name is in searchdump naming (`.data.` before the extension).
fn rs_named(name: &str) -> bool {
    name.contains(".data.json") || name.contains(".data.split-")
}

/// An index's non-data file in either form (`INDEX.KIND.json`, then `.json.gz`), if it is
/// there and not empty; the flag says it is gzipped.
pub fn meta_file(dir: &Path, index: &str, kind: &str) -> Option<(PathBuf, bool)> {
    [("json", false), ("json.gz", true)]
        .into_iter()
        .map(|(ext, gz)| (dir.join(format!("{index}.{kind}.{ext}")), gz))
        .find(|(p, _)| nonempty(p))
}

/// A text file, gunzipped when its name ends in `.gz`.
fn read_text(p: &Path) -> Result<String> {
    let f = fs::File::open(p).with_context(|| format!("open {}", p.display()))?;
    let mut s = String::new();
    if p.extension().is_some_and(|e| e == "gz") {
        flate2::read::MultiGzDecoder::new(f).read_to_string(&mut s)
    } else {
        std::io::BufReader::new(f).read_to_string(&mut s)
    }
    .with_context(|| format!("read {}", p.display()))?;
    Ok(s)
}

/// A split dump's part files for one index in `dir`, in part order. Errors on a gap or on parts
/// in more than one codec.
pub fn data_parts(dir: &Path, index: &str) -> Result<Vec<PathBuf>> {
    let mut parts: Vec<(u64, Compression, PathBuf)> = Vec::new();
    for ent in fs::read_dir(dir).with_context(|| format!("read {}", dir.display()))? {
        let ent = ent?;
        let name = ent.file_name().to_string_lossy().into_owned();
        if let Some((i, c, Some(n))) = parse_data_file(&name)
            && i == index
            && ent.file_type()?.is_file()
        {
            parts.push((n, c, ent.path()));
        }
    }
    parts.sort_by_key(|p| p.0);
    if let Some(w) = parts.windows(2).find(|w| w[0].0 == w[1].0) {
        bail!(
            "{index}: data part {} twice ({} and {})",
            w[0].0,
            w[0].2.display(),
            w[1].2.display()
        );
    }
    if parts.windows(2).any(|w| w[0].1.suffix() != w[1].1.suffix()) {
        bail!(
            "{index}: data parts in more than one codec in {}",
            dir.display()
        );
    }
    for (want, (n, _, _)) in parts.iter().enumerate() {
        if *n != want as u64 {
            bail!("{index}: data part {want} is missing in {}", dir.display());
        }
    }
    Ok(parts.into_iter().map(|p| p.2).collect())
}

/// Per-index and cluster-wide types to process, in processing order.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Types {
    pub index: Vec<&'static str>,
    pub cluster: Vec<&'static str>,
}

/// Node multielasticdump's per-type flags folded into `--types`/`--ignoreType` lists:
/// `--includeType` replaces the type list, `--ignoreX=true` removes X, `--ignoreX=false` adds it
/// (Node ignores analyzer, alias and index unless told otherwise). Node's `index` type (the
/// index create body) is our settings + mapping.
pub fn node_type_flags(args: &Args, default_types: &[String]) -> (Vec<String>, Vec<String>) {
    let expand = |t: &str| -> Vec<String> {
        match t.trim() {
            "index" => vec!["settings".into(), "mapping".into()],
            t => vec![t.to_string()],
        }
    };
    let mut want: Vec<String> = if !args.include_type.is_empty() {
        args.include_type.iter().flat_map(|t| expand(t)).collect()
    } else if !args.types.is_empty() {
        args.types.clone()
    } else {
        default_types.to_vec()
    };
    // Ignoring Node's `index` run leaves settings and mapping alone, so it is a no-op here.
    let mut ignore: Vec<String> = args
        .ignore_type
        .iter()
        .filter(|t| t.trim() != "index")
        .cloned()
        .collect();
    let flags = [
        ("data", args.ignore_data),
        ("mapping", args.ignore_mapping),
        ("settings", args.ignore_settings),
        ("template", args.ignore_template),
        ("alias", args.ignore_alias),
        ("analyzer", args.ignore_analyzer),
    ];
    for (t, v) in flags {
        match v {
            Some(true) => ignore.push(t.to_string()),
            Some(false) if !want.iter().any(|w| w.trim() == t || w.trim() == "all") => {
                want.push(t.to_string())
            }
            _ => {}
        }
    }
    // --ignoreIndex: Node's separate index-create run; settings and mapping already create it.
    if args.ignore_index == Some(false) {
        for t in ["settings", "mapping"] {
            if !want.iter().any(|w| w.trim() == t || w.trim() == "all") {
                want.push(t.to_string());
            }
        }
    }
    (want, ignore)
}

pub fn resolve_types(types: &[String], ignore: &[String]) -> Result<Types> {
    let known = |t: &str| TYPE_ORDER.contains(&t) || CLUSTER_TYPES.contains(&t);
    let in_types = types.iter().map(|t| (t, true));
    for (t, allow_all) in in_types.chain(ignore.iter().map(|t| (t, false))) {
        let t = t.trim();
        if !(known(t) || allow_all && t == "all") {
            bail!(
                "unknown type '{t}' (supported: {}, {}, all)",
                TYPE_ORDER.join(", "),
                CLUSTER_TYPES.join(", ")
            );
        }
    }
    let all = types.iter().any(|x| x.trim() == "all");
    let wanted = |t: &&str| {
        (all || types.iter().any(|x| x.trim() == *t)) && !ignore.iter().any(|x| x.trim() == *t)
    };
    let out = Types {
        index: TYPE_ORDER.iter().copied().filter(wanted).collect(),
        cluster: CLUSTER_TYPES.iter().copied().filter(wanted).collect(),
    };
    if out.index.is_empty() && out.cluster.is_empty() {
        bail!("no types left to process");
    }
    Ok(out)
}

// ----------------------------------------------------------------------------- manifest

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct IndexEntry {
    /// Directory relative to the dump root ("" for flat).
    pub dir: String,
    /// "datastream" for a data stream; absent for an index.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kind: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub group: Option<String>,
    pub types: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub data_file: Option<String>,
    /// Split dumps: the part files, in load order.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub data_parts: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub records: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_count: Option<u64>,
    pub status: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    /// 1.2: the data run's page / bulk sizes and why (`default|large-docs|budget|off|fallback|user`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sizing: Option<serde_json::Value>,
}

/// One cluster-scoped file (`_cluster/TYPE.json`).
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct ClusterEntry {
    pub file: String,
    pub status: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Manifest {
    pub tool: String,
    pub version: String,
    #[serde(default)]
    pub source: String,
    #[serde(default)]
    pub layout: Option<Layout>,
    #[serde(default)]
    pub compression: Option<Compression>,
    /// Written with --nodeCompat (Node multielasticdump file names).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub node_compat: bool,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub cluster: BTreeMap<String, ClusterEntry>,
    pub indices: BTreeMap<String, IndexEntry>,
    /// 1.2: the run's `--serverLimits` mode, source, node limits and budget.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub server_limits: Option<serde_json::Value>,
}

impl Manifest {
    pub fn load(root: &Path) -> Result<Option<Manifest>> {
        let p = root.join(MANIFEST);
        if !p.exists() {
            return Ok(None);
        }
        let text = fs::read_to_string(&p).with_context(|| format!("read {}", p.display()))?;
        Ok(Some(
            serde_json::from_str(&text).with_context(|| format!("parse {}", p.display()))?,
        ))
    }

    pub fn save(&self, root: &Path) -> Result<()> {
        let tmp = root.join(format!(".{MANIFEST}.tmp"));
        fs::write(&tmp, serde_json::to_vec_pretty(self)?)?;
        fs::rename(&tmp, root.join(MANIFEST))?;
        Ok(())
    }
}

// ----------------------------------------------------------------------------- discovery (load)

#[derive(Debug, Clone, PartialEq)]
pub struct Found {
    pub index: String,
    pub dir: PathBuf,
    /// Data files in load order: one file, or the parts of a split dump; empty for none.
    pub data: Vec<PathBuf>,
    pub records_hint: Option<u64>,
    /// A data stream (written with `--bulkAction=create` to the stream name).
    pub datastream: bool,
}

/// Indices in a dump directory: from the manifest (in `meta`) when there is one, else every
/// `INDEX.settings.json` / `INDEX.mapping.json` / `INDEX.data*.json*` / `INDEX.datastream.json`
/// up to two levels down (the `_cluster` directory excepted). Node multielasticdump names
/// (`INDEX.json.gz`, `INDEX.settings.json.gz`, `INDEX.split-N.json.gz`) are found too; a
/// Node-named data file counts only next to a settings or mapping file of the same index.
pub fn discover(root: &Path, meta: &Path) -> Result<Vec<Found>> {
    let mut out: BTreeMap<String, Found> = BTreeMap::new();
    let mut has_meta = std::collections::BTreeSet::new();
    if let Some(m) = Manifest::load(meta)? {
        for (name, e) in &m.indices {
            if e.status != "done" {
                eprintln!(
                    "note: {name} is recorded as {} in the manifest; skipping it",
                    e.status
                );
                continue;
            }
            check_index_name(name)?;
            let dir = root.join(&e.dir);
            let data: Vec<PathBuf> = if e.data_parts.is_empty() {
                e.data_file
                    .iter()
                    .map(|f| dir.join(f))
                    .filter(|p| p.is_file())
                    .collect()
            } else {
                let parts: Vec<PathBuf> = e.data_parts.iter().map(|f| dir.join(f)).collect();
                if let Some(p) = parts.iter().find(|p| !p.is_file()) {
                    bail!("{name}: data part {} is missing", p.display());
                }
                parts
            };
            out.insert(
                name.clone(),
                Found {
                    index: name.clone(),
                    dir,
                    data,
                    records_hint: e.records,
                    datastream: e.kind.as_deref() == Some("datastream"),
                },
            );
        }
        return Ok(out.into_values().collect());
    }
    let mut dirs = vec![(root.to_path_buf(), 0)];
    while let Some((d, depth)) = dirs.pop() {
        for ent in fs::read_dir(&d).with_context(|| format!("read {}", d.display()))? {
            let ent = ent?;
            let path = ent.path();
            let name = ent.file_name().to_string_lossy().into_owned();
            if ent.file_type()?.is_dir() {
                if depth < 2 && !(depth == 0 && name == CLUSTER_DIR) {
                    dirs.push((path, depth + 1));
                }
                continue;
            }
            let data = parse_data_file(&name);
            let marker = [
                "settings",
                "analyzer",
                "mapping",
                "alias",
                "template",
                "datastream",
            ]
            .iter()
            .flat_map(|k| [format!(".{k}.json"), format!(".{k}.json.gz")])
            .find_map(|suffix| name.strip_suffix(&suffix).map(|i| (i.to_string(), suffix)));
            let index = marker
                .as_ref()
                .map(|m| m.0.clone())
                .or_else(|| data.as_ref().map(|d| d.0.clone()));
            let Some(index) = index else { continue };
            if check_index_name(&index).is_err() {
                continue;
            }
            let key = format!("{}\0{index}", d.display());
            if marker
                .as_ref()
                .is_some_and(|m| m.1.starts_with(".settings.") || m.1.starts_with(".mapping."))
            {
                has_meta.insert(key.clone());
            }
            let entry = out.entry(key).or_insert_with(|| Found {
                index: index.clone(),
                dir: d.clone(),
                data: Vec::new(),
                records_hint: None,
                datastream: false,
            });
            if name.ends_with(".datastream.json") {
                entry.datastream = true;
            }
            if let Some((_, _, part)) = data
                && (fs::metadata(&path)?.len() > 0 || part.is_some())
            {
                entry.data.push(path);
            }
        }
    }
    // A Node-style `NAME.json` alone is just some JSON file, not an index.
    out.retain(|k, f| {
        has_meta.contains(k)
            || f.datastream
            || f.data.is_empty()
            || f.data.iter().any(|p| rs_named(&file_name(p)))
    });
    // Settle each index's data files: one single file, or a gap-free run of parts.
    for f in out.values_mut() {
        let (parts, singles): (Vec<PathBuf>, Vec<PathBuf>) = std::mem::take(&mut f.data)
            .into_iter()
            .partition(|p| is_part(p));
        match (singles.len(), parts.is_empty()) {
            (0, _) => {
                f.data = if parts.is_empty() {
                    parts
                } else {
                    data_parts(&f.dir, &f.index)?
                }
            }
            (1, true) => f.data = singles,
            (1, false) => bail!(
                "{}: both a single data file ({}) and split parts",
                f.index,
                singles[0].display()
            ),
            _ => bail!(
                "{}: more than one data file ({} and {})",
                f.index,
                singles[0].display(),
                singles[1].display()
            ),
        }
    }
    let mut seen: BTreeMap<String, PathBuf> = BTreeMap::new();
    let mut found: Vec<Found> = out.into_values().collect();
    found.sort_by(|a, b| a.index.cmp(&b.index));
    for f in &found {
        if let Some(prev) = seen.insert(f.index.clone(), f.dir.clone()) {
            bail!(
                "index {} appears in both {} and {}",
                f.index,
                prev.display(),
                f.dir.display()
            );
        }
    }
    Ok(found)
}

fn is_part(p: &Path) -> bool {
    parse_data_file(&p.file_name().unwrap_or_default().to_string_lossy())
        .is_some_and(|d| d.2.is_some())
}

// ----------------------------------------------------------------------------- data files

/// Records (lines) in a data file, decompressing by its name.
pub fn count_records(path: &Path) -> Result<u64> {
    let f = fs::File::open(path).with_context(|| format!("open {}", path.display()))?;
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let mut r: Box<dyn Read> = match parse_data_file(&name).map(|d| d.1) {
        Some(Compression::Gzip) => Box::new(flate2::read::MultiGzDecoder::new(f)),
        Some(Compression::Zstd1 | Compression::Zstd3) => {
            Box::new(zstd::stream::read::Decoder::new(f)?)
        }
        _ => Box::new(f),
    };
    let mut buf = vec![0u8; 1 << 20];
    let mut n = 0u64;
    loop {
        let k = r
            .read(&mut buf)
            .with_context(|| format!("read {}", path.display()))?;
        if k == 0 {
            return Ok(n);
        }
        n += memchr::memchr_iter(b'\n', &buf[..k]).count() as u64;
    }
}

/// elasticdump writes settings and mappings as JSON, sometimes as a JSON string of JSON.
pub fn mappings_body(text: &str) -> Result<serde_json::Value> {
    let mut doc: serde_json::Value = serde_json::from_str(text.trim())?;
    if let serde_json::Value::String(s) = &doc {
        doc = serde_json::from_str(s)?;
    }
    let obj = doc
        .as_object()
        .filter(|o| !o.is_empty())
        .ok_or_else(|| anyhow!("mapping dump is empty"))?;
    let (_, body) = obj.iter().next().unwrap();
    Ok(body
        .get("mappings")
        .cloned()
        .unwrap_or_else(|| body.clone()))
}

// ----------------------------------------------------------------------------- cluster

/// Base URL without credentials, plus credentials: `--httpAuthFile` wins (L-645b), then URL
/// userinfo, then env.
pub fn split_url(
    raw: &str,
    env_prefix: &str,
    auth_file: Option<&(String, String)>,
    env: &dyn Fn(&str) -> Option<String>,
) -> Result<(Url, Option<String>, Option<String>)> {
    let mut url = Url::parse(raw).with_context(|| {
        format!(
            "invalid URL '{}'",
            searchdump_rs::endpoint::redacted_endpoint(raw)
        )
    })?;
    if !matches!(url.scheme(), "http" | "https") {
        bail!("URL must start with http:// or https://");
    }
    let dec = |s: &str| {
        percent_encoding::percent_decode_str(s)
            .decode_utf8_lossy()
            .into_owned()
    };
    let mut user = (!url.username().is_empty()).then(|| dec(url.username()));
    let mut pass = url.password().map(dec);
    let _ = url.set_username("");
    let _ = url.set_password(None);
    if !url.path().ends_with('/') {
        let p = format!("{}/", url.path());
        url.set_path(&p);
    }
    if let Some((u, p)) = auth_file {
        // L-645b (owner decision 2026-11-14): the auth file wins over the URL and env
        user = Some(u.trim_end_matches(['\r', '\n']).to_string());
        pass = Some(p.trim_end_matches(['\r', '\n']).to_string());
    } else if (user.is_none() || pass.is_none())
        && let Some(u) = env(&format!("{env_prefix}_USERNAME"))
    {
        user = Some(u.trim_end_matches(['\r', '\n']).to_string());
        pass = env(&format!("{env_prefix}_PASSWORD"))
            .map(|p| p.trim_end_matches(['\r', '\n']).to_string());
    }
    Ok((url, user, pass))
}

pub struct Cluster {
    client: EsClient,
    /// What children get as --input/--output (as the user gave it, without a trailing slash).
    child_base: String,
}

impl Cluster {
    pub fn new(raw: &str, env_prefix: &str, args: &Args) -> Result<Self> {
        let auth = args
            .http_auth_file
            .as_deref()
            .map(read_auth_file)
            .transpose()?;
        let (base, username, password) = split_url(raw, env_prefix, auth.as_ref(), &|k| {
            searchdump_rs::envnames::var(k)
        })?;
        let opts = ClientOptions {
            base,
            username,
            password,
            headers: vec![],
            params: vec![],
            tls: TlsOptions {
                ca: None,
                cert: None,
                key: None,
                pass: None,
                insecure: args.insecure,
            },
            timeout: Some(Duration::from_secs(300)),
            idle_timeout: None,
            compress: false,
            max_sockets: None,
            debug: false,
            no_proxy: false,
        };
        Ok(Self {
            client: EsClient::new(&opts, RequestGate::new(None, None))?,
            child_base: raw.trim_end_matches('/').to_string(),
        })
    }

    pub fn index_url(&self, index: &str) -> String {
        let enc = percent_encoding::utf8_percent_encode(index, percent_encoding::NON_ALPHANUMERIC)
            .to_string();
        format!("{}/{enc}", self.child_base)
    }

    async fn call(
        &self,
        method: Method,
        segs: &[&str],
        query: &[(&str, &str)],
        body: Body,
    ) -> Result<(u16, Vec<u8>)> {
        let resp = self
            .client
            .send(method.clone(), segs, query, false, body)
            .await
            .map_err(|e| anyhow!("{method} /{}: {}", segs.join("/"), error_chain(&e)))?;
        let status = resp.status().as_u16();
        let bytes = resp.bytes().await?.to_vec();
        Ok((status, bytes))
    }

    async fn ok_json(
        &self,
        method: Method,
        segs: &[&str],
        query: &[(&str, &str)],
        body: Body,
    ) -> Result<serde_json::Value> {
        let (status, bytes) = self.call(method.clone(), segs, query, body).await?;
        if !(200..300).contains(&status) {
            let detail = String::from_utf8_lossy(&bytes[..bytes.len().min(2000)]).into_owned();
            let hint = if status == 401 {
                " (credentials rejected: check SEARCHDUMP_*_USERNAME/PASSWORD or --httpAuthFile)"
            } else {
                ""
            };
            bail!(
                "{method} /{} -> HTTP {status}: {detail}{hint}",
                segs.join("/")
            );
        }
        if bytes.is_empty() {
            return Ok(serde_json::Value::Null);
        }
        Ok(serde_json::from_slice(&bytes)?)
    }

    /// Open indices (closed ones cannot be dumped), sorted by name.
    pub async fn list_indices(&self) -> Result<Vec<String>> {
        let v = self
            .ok_json(
                Method::GET,
                &["_cat", "indices"],
                &[
                    ("format", "json"),
                    ("h", "index,status"),
                    ("expand_wildcards", "all"),
                    ("s", "index"),
                ],
                Body::Empty,
            )
            .await?;
        let mut out = vec![];
        for row in v
            .as_array()
            .ok_or_else(|| anyhow!("unexpected _cat/indices response"))?
        {
            let name = row
                .get("index")
                .and_then(|x| x.as_str())
                .unwrap_or_default();
            let status = row.get("status").and_then(|x| x.as_str()).unwrap_or("open");
            if name.is_empty() {
                continue;
            }
            if status != "open" {
                eprintln!("note: skipping closed index {name}");
                continue;
            }
            out.push(name.to_string());
        }
        out.sort();
        Ok(out)
    }

    /// (alias, index) pairs, sorted.
    pub async fn list_aliases(&self) -> Result<Vec<(String, String)>> {
        let v = self
            .ok_json(
                Method::GET,
                &["_cat", "aliases"],
                &[("format", "json"), ("h", "alias,index")],
                Body::Empty,
            )
            .await?;
        let mut out: Vec<(String, String)> = v
            .as_array()
            .ok_or_else(|| anyhow!("unexpected _cat/aliases response"))?
            .iter()
            .filter_map(|row| {
                let a = row.get("alias")?.as_str()?;
                let i = row.get("index")?.as_str()?;
                Some((a.to_string(), i.to_string()))
            })
            .collect();
        out.sort();
        Ok(out)
    }

    /// Data streams and their backing indices, sorted by name.
    pub async fn list_data_streams(&self) -> Result<Vec<(String, Vec<String>)>> {
        let v = self
            .ok_json(Method::GET, &["_data_stream"], &[], Body::Empty)
            .await
            .context("--matchType datastream: listing data streams (needs Elasticsearch 7.9+ or OpenSearch 1.0+)")?;
        let mut out: Vec<(String, Vec<String>)> = v
            .get("data_streams")
            .and_then(|d| d.as_array())
            .ok_or_else(|| anyhow!("unexpected _data_stream response"))?
            .iter()
            .filter_map(|ds| {
                let name = ds.get("name")?.as_str()?.to_string();
                let members = ds
                    .get("indices")
                    .and_then(|i| i.as_array())
                    .map(|a| {
                        a.iter()
                            .filter_map(|i| i.get("index_name")?.as_str().map(String::from))
                            .collect()
                    })
                    .unwrap_or_default();
                Some((name, members))
            })
            .collect();
        out.sort();
        Ok(out)
    }

    /// One data stream's description (`GET _data_stream/NAME`), or None if there is none.
    pub async fn data_stream(&self, name: &str) -> Result<Option<serde_json::Value>> {
        let (status, bytes) = self
            .call(Method::GET, &["_data_stream", name], &[], Body::Empty)
            .await?;
        match status {
            200..=299 => {
                let v: serde_json::Value = serde_json::from_slice(&bytes)?;
                Ok(v.get("data_streams")
                    .and_then(|d| d.as_array())
                    .and_then(|a| {
                        a.iter()
                            .find(|d| d.get("name").and_then(|n| n.as_str()) == Some(name))
                    })
                    .cloned())
            }
            404 => Ok(None),
            s => bail!(
                "GET /_data_stream/{name} -> HTTP {s}: {}",
                String::from_utf8_lossy(&bytes[..bytes.len().min(500)])
            ),
        }
    }

    pub async fn exists(&self, index: &str) -> Result<bool> {
        let (status, bytes) = self
            .call(Method::GET, &[index, "_settings"], &[], Body::Empty)
            .await?;
        match status {
            200..=299 => Ok(true),
            404 => Ok(false),
            401 => bail!("GET /{index}/_settings -> HTTP 401: credentials rejected"),
            s => bail!(
                "GET /{index}/_settings -> HTTP {s}: {}",
                String::from_utf8_lossy(&bytes[..bytes.len().min(500)])
            ),
        }
    }

    pub async fn count(&self, index: &str, refresh: bool) -> Result<u64> {
        if refresh {
            self.ok_json(Method::POST, &[index, "_refresh"], &[], Body::Empty)
                .await?;
        }
        let v = self
            .ok_json(Method::GET, &[index, "_count"], &[], Body::Empty)
            .await?;
        v.get("count")
            .and_then(|c| c.as_u64())
            .ok_or_else(|| anyhow!("no count in /{index}/_count response"))
    }

    /// 1.2: primary docs and stored bytes of every open index in ONE request
    /// (`_stats/docs,store?level=indices`), replacing 1.1-10's per-index `_stats` call.
    pub async fn index_stats(&self) -> Result<BTreeMap<String, sizing::Stats>> {
        let v = self
            .ok_json(
                Method::GET,
                &["_stats", "docs,store"],
                &[
                    ("level", "indices"),
                    ("expand_wildcards", "open,hidden"),
                    (
                        "filter_path",
                        "indices.*.primaries.docs.count,indices.*.primaries.store.size_in_bytes",
                    ),
                ],
                Body::Empty,
            )
            .await?;
        let mut out = BTreeMap::new();
        if let Some(m) = v.get("indices").and_then(|i| i.as_object()) {
            for (name, s) in m {
                let p = &s["primaries"];
                out.insert(
                    name.clone(),
                    (
                        p["docs"]["count"].as_u64().unwrap_or(0),
                        p["store"]["size_in_bytes"].as_u64().unwrap_or(0),
                    ),
                );
            }
        }
        Ok(out)
    }

    /// The stats of one target (a data stream: its backing indices summed), for a target the
    /// one `index_stats` call did not name.
    pub async fn target_stats(&self, name: &str) -> Option<sizing::Stats> {
        let v = self
            .ok_json(
                Method::GET,
                &[name, "_stats", "docs,store"],
                &[("filter_path", "_all.primaries")],
                Body::Empty,
            )
            .await
            .ok()?;
        let p = &v["_all"]["primaries"];
        Some((
            p["docs"]["count"].as_u64().unwrap_or(0),
            p["store"]["size_in_bytes"].as_u64().unwrap_or(0),
        ))
    }

    /// 1.2: the node probe (once per run, by the parent).
    pub async fn probe_limits(&self) -> searchdump_rs::limits::Limits {
        let host = Url::parse(&self.child_base)
            .ok()
            .and_then(|u| u.host_str().map(String::from))
            .unwrap_or_default();
        searchdump_rs::limits::probe(
            &self.client,
            &host,
            None,
            searchdump_rs::limits::Need {
                max_content_length: true,
                thread_pools: true,
            },
        )
        .await
    }

    pub async fn put_mapping(&self, index: &str, mappings: &serde_json::Value) -> Result<()> {
        self.ok_json(Method::PUT, &[index, "_mapping"], &[], Body::json(mappings))
            .await
            .map(|_| ())
    }
}

// ----------------------------------------------------------------------------- child runs

/// 1.1-7: where a child's record count came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CountSource {
    /// The JSON the child wrote through `--statsFile`.
    StatsFile,
    /// Its `Total Writes: N` log line (searchdump and Node elasticdump print it).
    LogLine,
}

/// 1.1-7: what a finished child reported. `written` is `None` when it reported nothing usable
/// (the caller then counts the data file itself).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ChildStats {
    pub written: Option<u64>,
    pub failed: u64,
    pub source: Option<CountSource>,
}

impl ChildStats {
    /// Documents the child read from its input: written plus failed (a load compares this with
    /// the target's `_count`, so documents the child skipped still show as a mismatch).
    pub fn docs_read(&self) -> Option<u64> {
        self.written.map(|w| w + self.failed)
    }
}

/// `--statsFile` JSON of a finished run (see searchdump's `write_stats_file`).
pub fn parse_stats_file(text: &str) -> Option<ChildStats> {
    let v: serde_json::Value = serde_json::from_str(text).ok()?;
    if v.get("complete").and_then(|c| c.as_bool()) != Some(true) {
        return None;
    }
    Some(ChildStats {
        written: Some(v.get("written")?.as_u64()?),
        failed: v.get("failed").and_then(|f| f.as_u64()).unwrap_or(0),
        source: Some(CountSource::StatsFile),
    })
}

/// The last `Total Writes: N` (and `Failed Writes: N`) of a child's log. searchdump prints
/// `Total Writes: N`; Node elasticdump prints `<date> | Total Writes: N`.
pub fn parse_total_writes(log: &str) -> Option<ChildStats> {
    fn number_after(line: &str, key: &str) -> Option<u64> {
        let rest = &line[line.rfind(key)? + key.len()..];
        let digits: String = rest
            .trim_start()
            .chars()
            .take_while(|c| c.is_ascii_digit())
            .collect();
        digits.parse().ok()
    }
    let mut written = None;
    let mut failed = 0;
    for line in log.lines() {
        if let Some(n) = number_after(line, "Total Writes:") {
            written = Some(n);
            failed = 0;
        } else if let Some(n) = number_after(line, "Failed Writes:") {
            failed = n;
        }
    }
    Some(ChildStats {
        written: Some(written?),
        failed,
        source: Some(CountSource::LogLine),
    })
}

/// The last `max` bytes of a file, as text (a child's log can be long; the counts are at its end).
fn read_tail(path: &Path, max: u64) -> Option<String> {
    use std::io::{Seek, SeekFrom};
    let mut f = fs::File::open(path).ok()?;
    let len = f.metadata().ok()?.len();
    f.seek(SeekFrom::Start(len.saturating_sub(max))).ok()?;
    let mut buf = Vec::new();
    f.read_to_end(&mut buf).ok()?;
    Some(String::from_utf8_lossy(&buf).into_owned())
}

/// 1.1-7: the counts of a child that exited 0: its `--statsFile`, else its log line, else none.
pub fn child_stats(stats_file: Option<&Path>, log: &Path) -> ChildStats {
    if let Some(p) = stats_file
        && let Ok(text) = fs::read_to_string(p)
        && let Some(s) = parse_stats_file(&text)
    {
        return s;
    }
    read_tail(log, 64 * 1024)
        .and_then(|t| parse_total_writes(&t))
        .unwrap_or_default()
}

/// 1.1-7: whether `bin` is searchdump (which takes `--statsFile`), not Node elasticdump (which
/// would get an option it does not know). The `elasticdump`/`elasticdump-rs` compatibility
/// names are symlinks to searchdump, so the name is checked after resolving links.
pub fn takes_stats_file(bin: &Path) -> bool {
    let real = fs::canonicalize(bin).unwrap_or_else(|_| bin.to_path_buf());
    real.file_name()
        .and_then(|n| n.to_str())
        .is_some_and(|n| n.starts_with("searchdump"))
}

/// 1.1-7: whether this `bin` build accepts `--statsFile` (searchdump 1.1+). A searchdump 1.0.x
/// given with `--elasticdump-bin` has the name but not the option, and would fail every data
/// run on it. clap rejects an unknown option before it acts on `--help`, so this exits 0 only
/// when the option is known. Run once at startup.
fn accepts_server_limits(bin: &Path) -> bool {
    std::process::Command::new(bin)
        .arg("--serverLimits=off")
        .arg("--help")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|s| s.success())
}

fn accepts_stats_file(bin: &Path) -> bool {
    std::process::Command::new(bin)
        .arg("--statsFile=/dev/null")
        .arg("--help")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|s| s.success())
}

/// 1.1-7: `SEARCHDUMP_MULTI_DEBUG=1` prints where each record count came from.
fn debug_counts() -> bool {
    std::env::var_os("SEARCHDUMP_MULTI_DEBUG").is_some_and(|v| v != "0" && !v.is_empty())
}

/// 1.1-8: how long a child gets after SIGTERM (on Ctrl-C) before it is killed.
pub const CHILD_GRACE: Duration = Duration::from_secs(5);

pub struct Runner {
    pub bin: PathBuf,
    pub log_dir: PathBuf,
    pub common: Vec<String>,
    pub data_flags: Vec<String>,
    /// Set on Ctrl-C: no new step starts.
    pub cancelled: Arc<AtomicBool>,
    /// 1.1-8: cancelled on Ctrl-C together with `cancelled`; running children are stopped
    /// (SIGTERM, then SIGKILL after `grace`).
    pub cancel: CancellationToken,
    pub grace: Duration,
    /// 1.1-7: data runs get `--statsFile` (the binary is searchdump, see [`takes_stats_file`]).
    pub stats: bool,
    /// 1.2: data runs get `--serverLimits=off` (the child is searchdump 1.2+; the parent sized
    /// them already).
    pub server_limits: bool,
}

impl Runner {
    /// Run searchdump once; output goes to LOGDIR/INDEX.KIND.log.
    pub async fn run(
        &self,
        index: &str,
        kind: &str,
        input: &str,
        output: &str,
        extra: &[String],
    ) -> Result<ChildStats> {
        self.run_as(index, kind, kind, input, output, extra).await
    }

    /// Like `run`, logging to LOGDIR/INDEX.LABEL.log (one log per data part).
    ///
    /// 1.1-8: the child is a `tokio::process` child, awaited without holding a runtime thread.
    /// On cancel it gets SIGTERM, then SIGKILL after `grace`; a child still running when its
    /// future is dropped is killed (`kill_on_drop`).
    ///
    /// 1.1-7: returns the counts the child reported ([`child_stats`]).
    pub async fn run_as(
        &self,
        index: &str,
        kind: &str,
        label: &str,
        input: &str,
        output: &str,
        extra: &[String],
    ) -> Result<ChildStats> {
        if self.cancelled.load(Ordering::SeqCst) || self.cancel.is_cancelled() {
            bail!("interrupted");
        }
        let log_path = self.log_dir.join(format!("{index}.{label}.log"));
        let log = fs::File::create(&log_path)
            .with_context(|| format!("create {}", log_path.display()))?;
        let stats_path = (self.stats && kind == "data")
            .then(|| self.log_dir.join(format!("{index}.{label}.stats.json")));
        if let Some(p) = &stats_path {
            // never read the counts of an earlier run
            let _ = fs::remove_file(p);
        }
        let mut child = {
            let mut cmd = tokio::process::Command::new(&self.bin);
            cmd.arg(format!("--input={input}"))
                .arg(format!("--output={output}"))
                .arg(format!("--type={kind}"))
                .arg("--overwrite")
                .args(&self.common)
                .args(extra)
                .args(
                    stats_path
                        .iter()
                        .map(|p| format!("--statsFile={}", p.display())),
                )
                .stdin(Stdio::null())
                .stdout(log.try_clone()?)
                .stderr(log)
                .kill_on_drop(true);
            // 1.1-8: its own process group, so a terminal Ctrl-C reaches only this process.
            // The child then gets exactly one signal (our SIGTERM below); a SIGINT from the
            // terminal followed by our SIGTERM would be its second signal, a force-quit that
            // skips finishing in-flight requests and cleaning up staged output.
            #[cfg(unix)]
            cmd.process_group(0);
            cmd.spawn()
                .with_context(|| format!("could not start {}", self.bin.display()))?
        };
        let waited = tokio::select! {
            s = child.wait() => Some(s),
            _ = self.cancel.cancelled() => None,
        };
        let status = match waited {
            Some(s) => s.with_context(|| format!("wait for {}", self.bin.display()))?,
            None => {
                let how = stop_child(&mut child, self.grace).await;
                eprintln!(
                    "interrupted: stopped searchdump --type={kind} for {index} ({how}); log: {}",
                    log_path.display()
                );
                bail!(
                    "interrupted: searchdump --type={kind} was stopped ({how}); last lines of {}:\n{}",
                    log_path.display(),
                    tail(&log_path, 15)
                );
            }
        };
        if !status.success() {
            bail!(
                "searchdump --type={kind} failed ({status}); last lines of {}:\n{}",
                log_path.display(),
                tail(&log_path, 15)
            );
        }
        Ok(child_stats(stats_path.as_deref(), &log_path))
    }
}

/// 1.1-8: stop a running child: SIGTERM (searchdump then finishes its in-flight requests and
/// exits), SIGKILL when it is still running after `grace`. Says what happened.
async fn stop_child(child: &mut tokio::process::Child, grace: Duration) -> String {
    if let Ok(Some(status)) = child.try_wait() {
        return format!("it had already exited, {status}");
    }
    if let Some(pid) = child.id() {
        send_term(pid).await;
    }
    match tokio::time::timeout(grace, child.wait()).await {
        Ok(Ok(status)) => format!("SIGTERM, {status}"),
        Ok(Err(e)) => format!("SIGTERM, then waiting failed: {e}"),
        Err(_) => {
            let _ = child.start_kill();
            match child.wait().await {
                Ok(status) => format!("SIGKILL after {grace:?} without exiting, {status}"),
                Err(e) => format!("SIGKILL after {grace:?}, then waiting failed: {e}"),
            }
        }
    }
}

/// SIGTERM to `pid` through `kill(1)`: this binary forbids unsafe code, so no `libc::kill`.
/// Elsewhere there is no SIGTERM; the child is killed once the grace period ends.
async fn send_term(pid: u32) {
    #[cfg(unix)]
    {
        let _ = tokio::process::Command::new("kill")
            .arg("-TERM")
            .arg(pid.to_string())
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .await;
    }
    #[cfg(not(unix))]
    let _ = pid;
}

fn tail(path: &Path, n: usize) -> String {
    let Ok(f) = fs::File::open(path) else {
        return String::new();
    };
    let lines: Vec<String> = BufReader::new(f).lines().map_while(Result::ok).collect();
    lines[lines.len().saturating_sub(n)..]
        .iter()
        .map(|l| format!("    {l}"))
        .collect::<Vec<_>>()
        .join("\n")
}

fn find_bin(explicit: Option<&Path>) -> PathBuf {
    if let Some(p) = explicit {
        return p.to_path_buf();
    }
    if let Some(p) = std::env::current_exe()
        .ok()
        .and_then(|e| e.parent().map(|d| d.join("searchdump")))
        .filter(|p| p.is_file())
    {
        return p;
    }
    if let Some(p) = searchdump_rs::envnames::var("SEARCHDUMP_BIN") {
        return PathBuf::from(p);
    }
    PathBuf::from("searchdump")
}

fn nonempty(p: &Path) -> bool {
    fs::metadata(p)
        .map(|m| m.is_file() && m.len() > 0)
        .unwrap_or(false)
}

// ----------------------------------------------------------------------------- dump / load

#[derive(Debug)]
pub enum Outcome {
    Done(String),
    Skipped(String),
    Failed(String),
}

struct Ctx {
    args: Args,
    /// Where the manifest and `_cluster/` live (the dump directory unless --metaDir or
    /// --nodeCompat put them elsewhere).
    meta: PathBuf,
    types: Types,
    sel: Selector,
    runner: Runner,
    pacer: Pacer,
    /// 1.2: the sizing plan (set by [`plan_sizes`] before any index starts).
    plan: std::sync::OnceLock<sizing::Plan>,
    /// 1.2: primary (docs, stored bytes) per selected index, from one stats call.
    index_stats: std::sync::OnceLock<BTreeMap<String, sizing::Stats>>,
}

impl Ctx {
    fn plan(&self) -> &sizing::Plan {
        self.plan.get_or_init(|| {
            sizing::Plan::new(
                searchdump_rs::limits::Mode::Off,
                self.args.no_default_data_flags,
                None,
                self.args.parallel,
                &[],
            )
        })
    }

    fn stats_of(&self, index: &str) -> Option<sizing::Stats> {
        self.index_stats.get().and_then(|m| m.get(index).copied())
    }
}

/// 1.2: probe the cluster once, read every target's stats in one call, and set the plan
/// (ROADMAP-1.2 section 4). `targets` are the indices to dump (empty for a load). Returns the
/// exit code to stop with for `--serverLimits=check` (0, or 2 when a flag after `--` exceeds
/// the limits) and the manifest's `server_limits` object.
async fn plan_sizes(
    ctx: &Ctx,
    cluster: &Cluster,
    targets: &[String],
) -> (Option<i32>, serde_json::Value) {
    use searchdump_rs::limits::{self, Mode};
    let no_defaults = ctx.args.no_default_data_flags && ctx.args.server_limits.is_none();
    let mode = ctx.args.server_limits.unwrap_or(if no_defaults {
        Mode::Off
    } else {
        limits::DEFAULT_MODE
    });
    let probed = if mode == Mode::Off {
        None
    } else {
        Some(cluster.probe_limits().await)
    };
    let user_limit = ctx
        .args
        .extra
        .iter()
        .any(|a| flag_name(a) == Some("--limit"));
    let mut stats = BTreeMap::new();
    let skip_stats = targets.is_empty() || no_defaults || (mode == Mode::Off && user_limit);
    if !skip_stats {
        match cluster.index_stats().await {
            Ok(m) => stats = m,
            Err(e) => eprintln!("note: index stats in one call failed ({e:#}); asking per index"),
        }
        for t in targets {
            if !stats.contains_key(t)
                && let Some(s) = cluster.target_stats(t).await
            {
                stats.insert(t.clone(), s);
            }
        }
    }
    let per: Vec<Option<sizing::Stats>> = targets.iter().map(|t| stats.get(t).copied()).collect();
    let plan = sizing::Plan::new(mode, no_defaults, probed, ctx.args.parallel, &per);
    eprintln!("{}", plan.line());
    let json = match &plan.limits {
        Some(l) => limits::to_json(l, mode, plan.budget.as_ref()),
        None => serde_json::json!({ "mode": mode.as_str(), "source": "off" }),
    };
    let mut json = json;
    if let Some(o) = json.as_object_mut() {
        o.insert("parallel".into(), ctx.args.parallel.into());
        o.insert("fit_parallel".into(), plan.fit_parallel.into());
        o.insert("bulk_bytes_per_worker".into(), plan.bulk_bytes.into());
    }
    let mut stop = None;
    if mode == Mode::Check {
        let mut too_big = vec![];
        let user_limit_v = ctx.args.extra.iter().find_map(|a| {
            a.strip_prefix("--limit=")
                .and_then(|v| v.parse::<u64>().ok())
        });
        let user_bulk = ctx.args.extra.iter().find_map(|a| {
            a.strip_prefix("--maxBulkBytes=")
                .and_then(|v| searchdump_rs::pipeline::file_sink::parse_size(v).ok())
        });
        if targets.is_empty() {
            let l = plan.load();
            println!(
                "load: --limit={} --maxBulkBytes={} KB per worker ({}), parallel {}",
                l.limit,
                l.max_bulk_bytes >> 10,
                l.reason,
                ctx.args.parallel
            );
            if plan.budget.is_some()
                && let Some(u) = user_bulk
                && u > l.max_bulk_bytes
            {
                too_big.push(format!("--maxBulkBytes={u} > {}", l.max_bulk_bytes));
            }
        }
        for (t, st) in targets.iter().zip(&per) {
            let d = plan.dump(*st);
            let avg = st.and_then(|(n, b)| (n > 0).then(|| b / n)).unwrap_or(0);
            println!(
                "  {t}: --limit={} ({}, ~{} KB stored per document, ~{} MB per page)",
                d.limit,
                d.reason,
                avg.div_ceil(1024),
                d.page_bytes >> 20
            );
            if plan.budget.is_some()
                && let Some(u) = user_limit_v
                && u > d.limit
            {
                too_big.push(format!("{t}: --limit={u} > {}", d.limit));
            }
        }
        if too_big.is_empty() {
            println!("server limits: check passed");
            stop = Some(0);
        } else {
            eprintln!(
                "server limits: flags after -- exceed the limits: {}",
                too_big.join("; ")
            );
            stop = Some(2);
        }
    }
    let _ = ctx.plan.set(plan);
    let _ = ctx.index_stats.set(stats);
    (stop, json)
}

/// `--interval`: a minimum gap between job starts, shared by all workers.
pub struct Pacer {
    gap: Duration,
    next: tokio::sync::Mutex<Option<tokio::time::Instant>>,
}

impl Pacer {
    pub fn new(gap: Duration) -> Self {
        Self {
            gap,
            next: tokio::sync::Mutex::new(None),
        }
    }

    pub async fn wait(&self) {
        if self.gap.is_zero() {
            return;
        }
        let mut next = self.next.lock().await;
        if let Some(at) = *next {
            tokio::time::sleep_until(at).await;
        }
        *next = Some(tokio::time::Instant::now() + self.gap);
    }
}

const LIFECYCLE_DROP: &str =
    "--excludeSettings=index.lifecycle,index.routing.allocation.include._tier_preference";

fn strings(flags: &[&str]) -> Vec<String> {
    flags.iter().map(|s| s.to_string()).collect()
}

/// Remove this index's earlier data files (single or split) before a new data dump, so stale
/// parts from a larger earlier dump are not counted or loaded. Only files named exactly as
/// this tool names them are touched.
fn remove_data_files(dir: &Path, index: &str) -> Result<()> {
    let Ok(rd) = fs::read_dir(dir) else {
        return Ok(());
    };
    for ent in rd {
        let ent = ent?;
        let name = ent.file_name().to_string_lossy().into_owned();
        if parse_data_file(&name).is_some_and(|d| d.0 == index) && ent.file_type()?.is_file() {
            fs::remove_file(ent.path())
                .with_context(|| format!("remove stale {}", ent.path().display()))?;
        }
    }
    Ok(())
}

fn file_name(p: &Path) -> String {
    p.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default()
}

async fn dump_one(
    ctx: &Ctx,
    cluster: &Cluster,
    root: &Path,
    manifest: &Mutex<Manifest>,
    target: &Target,
) -> Outcome {
    ctx.pacer.wait().await;
    let index = target.name.as_str();
    let ds = target.kind == TargetKind::DataStream;
    let kinds: Vec<&str> = ctx
        .types
        .index
        .iter()
        .copied()
        .filter(|k| !ds || *k == "data")
        .collect();
    if kinds.is_empty() {
        return Outcome::Skipped(
            "data stream: only its data is dumped (its settings and mappings come from its index template); add data to --types".into(),
        );
    }
    let rel = match rel_dir(ctx.args.layout, &ctx.sel, index, &target.matched) {
        Ok(r) => r,
        Err(e) => return Outcome::Failed(format!("{e:#}")),
    };
    let dir = root.join(&rel);
    let comp = ctx.args.compression;
    let split = ctx.args.splits();
    let node = ctx.args.node_compat;
    let mut entry = IndexEntry {
        dir: rel.to_string_lossy().into_owned(),
        kind: ds.then(|| "datastream".to_string()),
        group: (ctx.args.layout == Layout::Group).then(|| rel.to_string_lossy().into_owned()),
        types: kinds.iter().map(|t| t.to_string()).collect(),
        status: "running".into(),
        ..Default::default()
    };
    let res: Result<String> = async {
        // 1.1-7: what the data run reported (its record count, unless --verifyCounts)
        let mut data_stats: Option<ChildStats> = None;
        fs::create_dir_all(&dir).with_context(|| format!("create {}", dir.display()))?;
        let source = cluster.index_url(index);
        entry.source_count = cluster.count(index, false).await.ok();
        if ds {
            let info = cluster
                .data_stream(index)
                .await?
                .ok_or_else(|| anyhow!("data stream {index} disappeared"))?;
            let marker = dir.join(format!("{index}.datastream.json"));
            fs::write(&marker, serde_json::to_vec_pretty(&info)?)
                .with_context(|| format!("write {}", marker.display()))?;
        }
        for kind in &kinds {
            let (out, extra) = if *kind == "data" {
                remove_data_files(&dir, index)?;
                let mut extra = strings(comp.flags());
                if let Some(s) = &ctx.args.file_size {
                    extra.push(format!("--fileSize={s}"));
                }
                if let Some(n) = ctx.args.max_rows {
                    extra.push(format!("--maxRows={n}"));
                }
                // 1.2: this index's sizes from the parent's plan (one stats call for all
                // indices, no per-index probe); the user's flags after `--` still win.
                let st = ctx.stats_of(index);
                let size = ctx.plan().dump(st);
                note_size(ctx, index, st, &size);
                entry.sizing = Some(sizing_json(&size, &ctx.args.extra));
                extra.extend(merge_data_flags_owned(
                    &ctx.plan().flags(&size, ctx.runner.server_limits),
                    &ctx.runner.data_flags,
                ));
                // searchdump adds the codec extension to each part itself.
                let name = if split {
                    item_file_for(node, index, "data", Compression::None)
                } else {
                    item_file_for(node, index, "data", comp)
                };
                (dir.join(name), extra)
            } else if node {
                // Node gzips every file with --fsCompress, not only the data.
                (
                    dir.join(node_item_file(index, kind, comp)),
                    strings(comp.flags()),
                )
            } else {
                (dir.join(item_file(index, kind, comp)), vec![])
            };
            let o = out.to_string_lossy().into_owned();
            // Node's per-index template file holds the cluster's legacy templates.
            let input = if *kind == "template" {
                cluster.child_base.clone()
            } else {
                source.clone()
            };
            let runner = &ctx.runner;
            let stats = runner.run(index, kind, &input, &o, &extra).await?;
            if *kind == "data" {
                data_stats = Some(stats);
            }
            if !matches!(*kind, "data" | "analyzer" | "template") && !nonempty(&out) {
                bail!("{kind} export is empty");
            }
        }
        let mut summary = String::new();
        if kinds.contains(&"data") {
            let files: Vec<PathBuf> = if split {
                data_parts(&dir, index)?
            } else {
                let f = dir.join(item_file_for(node, index, "data", comp));
                if f.exists() { vec![f] } else { vec![] }
            };
            let reported = data_stats.and_then(|s| s.written.zip(s.source));
            let records = match reported {
                // nothing written (no data file) needs no reading either
                _ if files.is_empty() => 0,
                Some((n, source)) if !ctx.args.verify_counts => {
                    if debug_counts() {
                        eprintln!("debug: {index}: {n} records ({source:?})");
                    }
                    n
                }
                _ => {
                    let mut scanned = 0;
                    for f in &files {
                        scanned += count_records(f)?;
                    }
                    if let Some((n, source)) = reported
                        && n != scanned
                    {
                        bail!(
                            "the data files hold {scanned} records but the searchdump run reported {n} ({source:?})"
                        );
                    }
                    if debug_counts() {
                        eprintln!("debug: {index}: {scanned} records (read back)");
                    }
                    scanned
                }
            };
            if split {
                entry.data_parts = files.iter().map(|f| file_name(f)).collect();
            } else {
                entry.data_file = Some(item_file_for(node, index, "data", comp));
            }
            entry.records = Some(records);
            if let Some(src) = entry.source_count {
                if records == 0 && src > 0 {
                    bail!("data export is empty but the index has {src} documents");
                }
                if records != src {
                    eprintln!("warning: {index}: {records} records written, the index reported {src} before the dump (still being written to?)");
                }
            }
            summary = if split {
                format!(" ({records} records in {} parts)", files.len())
            } else {
                format!(" ({records} records)")
            };
        }
        if ds {
            summary.push_str(" [data stream]");
        }
        Ok(format!("{}{summary}", dir.display()))
    }
    .await;
    let outcome = match res {
        Ok(s) => {
            entry.status = "done".into();
            Outcome::Done(s)
        }
        Err(e) => {
            entry.status = "failed".into();
            entry.error = Some(format!("{e:#}"));
            Outcome::Failed(format!("{e:#}"))
        }
    };
    let mut m = manifest.lock().unwrap();
    m.indices.insert(index.to_string(), entry);
    if let Err(e) = m.save(&ctx.meta) {
        eprintln!("warning: could not write the manifest: {e:#}");
    }
    outcome
}

/// Flags for a cluster-scoped run.
fn cluster_flags(args: &Args, kind: &str, load: bool) -> Vec<String> {
    let mut v = vec![];
    if load {
        match (kind, args.lifecycle) {
            ("policy", Lifecycle::Convert) => v.push("--convertIlmToIsm".to_string()),
            ("template" | "index_template" | "component_template", Lifecycle::Drop) => {
                v.push(LIFECYCLE_DROP.to_string())
            }
            _ => {}
        }
        if args.skip_existing {
            v.push("--skip-existing".to_string());
        }
    }
    v
}

/// Dump every selected cluster-scoped type once, to `ROOT/_cluster/TYPE.json`.
async fn dump_cluster(
    ctx: &Ctx,
    cluster: &Cluster,
    manifest: &Mutex<Manifest>,
) -> Vec<(String, Outcome)> {
    let dir = ctx.meta.join(CLUSTER_DIR);
    let mut out = vec![];
    for kind in &ctx.types.cluster {
        let name = format!("{CLUSTER_DIR}/{kind}");
        let file = dir.join(format!("{kind}.json"));
        let res =
            match fs::create_dir_all(&dir).with_context(|| format!("create {}", dir.display())) {
                Ok(()) => {
                    let o = file.to_string_lossy().into_owned();
                    ctx.runner
                        .run(CLUSTER_DIR, kind, &cluster.child_base, &o, &[])
                        .await
                }
                Err(e) => Err(e),
            };
        let entry = ClusterEntry {
            file: format!("{kind}.json"),
            status: if res.is_ok() { "done" } else { "failed" }.into(),
            error: res.as_ref().err().map(|e| format!("{e:#}")),
        };
        out.push((
            name,
            match res {
                Ok(_) => Outcome::Done(file.display().to_string()),
                Err(e) => Outcome::Failed(format!("{e:#}")),
            },
        ));
        let mut m = manifest.lock().unwrap();
        m.cluster.insert(kind.to_string(), entry);
        if let Err(e) = m.save(&ctx.meta) {
            eprintln!("warning: could not write the manifest: {e:#}");
        }
    }
    out
}

/// Load the cluster-scoped files present under `ROOT/_cluster`, in CLUSTER_TYPES order.
async fn load_cluster(ctx: &Ctx, cluster: &Cluster) -> Vec<(String, Outcome)> {
    let dir = ctx.meta.join(CLUSTER_DIR);
    let recorded = Manifest::load(&ctx.meta).ok().flatten().map(|m| m.cluster);
    let mut out = vec![];
    for kind in &ctx.types.cluster {
        let name = format!("{CLUSTER_DIR}/{kind}");
        if recorded
            .as_ref()
            .and_then(|c| c.get(*kind))
            .is_some_and(|e| e.status != "done")
        {
            out.push((
                name,
                Outcome::Skipped("recorded as failed in the manifest".into()),
            ));
            continue;
        }
        let file = dir.join(format!("{kind}.json"));
        if !nonempty(&file) {
            if !ctx.args.types.is_empty() {
                out.push((name, Outcome::Skipped(format!("no {}", file.display()))));
            }
            continue;
        }
        let input = file.to_string_lossy().into_owned();
        let extra = cluster_flags(&ctx.args, kind, true);
        let r = ctx
            .runner
            .run(CLUSTER_DIR, kind, &input, &cluster.child_base, &extra)
            .await;
        out.push((
            name,
            match r {
                Ok(_) => Outcome::Done(format!("from {}", file.display())),
                Err(e) => Outcome::Failed(format!("{e:#}")),
            },
        ));
    }
    out
}

async fn load_one(ctx: &Ctx, cluster: &Cluster, f: &Found) -> Outcome {
    ctx.pacer.wait().await;
    let target = format!("{}{}{}", ctx.args.prefix, f.index, ctx.args.suffix);
    let res: Result<Option<String>> = async {
        check_index_name(&target)?;
        if cluster.exists(&target).await? {
            if ctx.args.skip_existing {
                return Ok(None);
            }
            bail!("{target} already exists on the target (use --skipExisting to skip it)");
        }
        let url = cluster.index_url(&target);
        let mut records = None;
        let mut settings_applied = false;
        let kinds: Vec<&str> = ctx
            .types
            .index
            .iter()
            .copied()
            .filter(|k| !f.datastream || *k == "data")
            .collect();
        // A Node-layout dump carries the legacy templates next to each index; load them
        // first, as Node does (the same templates again for each index is harmless).
        if ctx.types.cluster.contains(&"template")
            && !f.datastream
            && let Some((p, gz)) = meta_file(&f.dir, &f.index, "template")
        {
            let mut extra = cluster_flags(&ctx.args, "template", true);
            if gz {
                extra.push("--fsCompress".into());
            }
            let input = p.to_string_lossy().into_owned();
            let runner = &ctx.runner;
            runner
                .run(&f.index, "template", &input, &cluster.child_base, &extra)
                .await?;
        }
        for kind in kinds {
            if kind == "data" {
                if f.data.is_empty() {
                    let meta_only =
                        f.datastream || meta_file(&f.dir, &f.index, "settings").is_some();
                    if f.records_hint.unwrap_or(0) == 0 && meta_only {
                        eprintln!(
                            "note: {}: no data file (empty index); loading metadata only",
                            f.index
                        );
                        continue;
                    }
                    bail!("missing data file for {} in {}", f.index, f.dir.display());
                }
                let mut total = 0;
                for (n, part) in f.data.iter().enumerate() {
                    let c = parse_data_file(&file_name(part)).map_or(Compression::None, |d| d.1);
                    let mut extra = strings(c.flags());
                    let size = ctx.plan().load();
                    extra.extend(merge_data_flags_owned(
                        &ctx.plan().flags(&size, ctx.runner.server_limits),
                        &ctx.runner.data_flags,
                    ));
                    if f.datastream && !extra.iter().any(|a| a.starts_with("--bulkAction")) {
                        // A data stream only accepts create.
                        extra.push("--bulkAction=create".into());
                    }
                    let label = if f.data.len() > 1 {
                        format!("data.part-{n}")
                    } else {
                        "data".into()
                    };
                    let input = part.to_string_lossy().into_owned();
                    let runner = &ctx.runner;
                    let stats = runner
                        .run_as(&f.index, "data", &label, &input, &url, &extra)
                        .await
                        .map_err(|e| with_part(e, f.data.len(), n, part))?;
                    // 1.1-7: the documents the run read (written + failed), else read the part
                    total += match stats.docs_read() {
                        Some(read) if !ctx.args.verify_counts => read,
                        reported => {
                            let scanned = count_records(part)?;
                            if let Some(read) = reported
                                && read != scanned
                            {
                                bail!(
                                    "{} holds {scanned} records but the searchdump run read {read}",
                                    part.display()
                                );
                            }
                            scanned
                        }
                    };
                }
                records = Some(total);
                continue;
            }
            if kind == "analyzer" && settings_applied {
                // The settings file already carries index.analysis.
                continue;
            }
            let Some((p, gz)) = meta_file(&f.dir, &f.index, kind) else {
                if matches!(kind, "alias" | "analyzer") {
                    continue;
                }
                bail!(
                    "missing {kind} file {}",
                    f.dir
                        .join(item_file(&f.index, kind, Compression::None))
                        .display()
                );
            };
            let mut extra = if kind == "settings" {
                match ctx.args.lifecycle {
                    Lifecycle::Drop => vec![LIFECYCLE_DROP.to_string()],
                    Lifecycle::Convert => vec!["--convertIlmToIsm".into()],
                }
            } else {
                vec![]
            };
            if gz {
                extra.push("--fsCompress".into());
            }
            let input = p.to_string_lossy().into_owned();
            let runner = &ctx.runner;
            let r = runner.run(&f.index, kind, &input, &url, &extra).await;
            match r {
                Ok(_) => settings_applied |= kind == "settings",
                Err(e) if kind == "mapping" => {
                    eprintln!(
                        "note: {target}: mapping via searchdump failed, applying it directly"
                    );
                    let text = read_text(&p)?;
                    cluster
                        .put_mapping(&target, &mappings_body(&text)?)
                        .await
                        .with_context(|| format!("{e:#}"))?;
                }
                Err(e) => return Err(e),
            }
        }
        if let Some(want) = records {
            let got = cluster.count(&target, true).await?;
            if got != want {
                bail!("restored {target} has {got} documents, the backup has {want} records");
            }
            let parts = if f.data.len() > 1 {
                format!(" from {} parts", f.data.len())
            } else {
                String::new()
            };
            return Ok(Some(format!("{target} docs={got}{parts}")));
        }
        Ok(Some(target.clone()))
    }
    .await;
    match res {
        Ok(Some(s)) => Outcome::Done(s),
        Ok(None) => Outcome::Skipped(format!("{target} already exists")),
        Err(e) => Outcome::Failed(format!("{e:#}")),
    }
}

/// The part a failed data load step was reading, when the index has more than one. A single data
/// file adds no context: an empty one printed as "FAILED  IX: : interrupted: ..." (1.1-item-checks,
/// check 4).
fn with_part(e: anyhow::Error, parts: usize, n: usize, part: &Path) -> anyhow::Error {
    if parts > 1 {
        e.context(format!("part {n} ({})", part.display()))
    } else {
        e
    }
}

/// Where the manifest and `_cluster/` go: --metaDir; else for a dump the directory itself, or
/// `DIR.meta` beside it with --nodeCompat (Node takes every entry in DIR for an index); for a
/// load the directory when it holds a manifest or `_cluster/`, else `DIR.meta` when that exists.
pub fn meta_dir(args: &Args) -> PathBuf {
    if let Some(d) = &args.meta_dir {
        return d.clone();
    }
    let dir = match args.direction {
        Direction::Dump => &args.output,
        Direction::Load | Direction::Copy => &args.input,
    };
    let root = PathBuf::from(dir);
    let beside = PathBuf::from(format!("{}.meta", dir.trim_end_matches('/')));
    match args.direction {
        Direction::Dump if args.node_compat => beside,
        Direction::Dump => root,
        Direction::Load | Direction::Copy => {
            if root.join(MANIFEST).exists() || root.join(CLUSTER_DIR).is_dir() || !beside.is_dir() {
                root
            } else {
                beside
            }
        }
    }
}

/// L-809a: storage-URL directories. `copy` copies a dump directory between stores; a dump to
/// a storage URL is written to a local staging directory and uploaded when every index
/// succeeded; a load from one is downloaded to a staging directory first.
#[cfg_attr(not(feature = "opendal"), allow(unused_variables, unused_mut))]
async fn run(mut args: Args) -> Result<i32> {
    use searchdump_rs::remote;
    if args.parallel == 0 {
        bail!("--parallel must be at least 1");
    }
    let in_cfg = args.input_storage_config.clone().or_else(|| {
        searchdump_rs::envnames::var_os("SEARCHDUMP_INPUT_STORAGE_CONFIG").map(PathBuf::from)
    });
    let out_cfg = args.output_storage_config.clone().or_else(|| {
        searchdump_rs::envnames::var_os("SEARCHDUMP_OUTPUT_STORAGE_CONFIG").map(PathBuf::from)
    });
    let is_cluster = |u: &str| {
        let l = u.to_ascii_lowercase();
        l.starts_with("http://") || l.starts_with("https://")
    };
    match args.direction {
        Direction::Copy => {
            if is_cluster(&args.input) || is_cluster(&args.output) {
                bail!(
                    "--direction copy copies a dump directory between stores; use dump or load for a cluster"
                );
            }
            if args.node_compat || args.file_size.is_some() || args.max_rows.is_some() {
                bail!(
                    "--direction copy copies the files as they are (format, compression and split parts kept); --nodeCompat, --fileSize and --maxRows do not apply"
                );
            }
            #[cfg(feature = "opendal")]
            {
                let src = remote::parse_dir(&args.input, in_cfg.as_deref())?;
                let dst = remote::parse_dir(&args.output, out_cfg.as_deref())?;
                if src == dst {
                    bail!("--input and --output are the same directory");
                }
                let r = remote::copy_tree(&src, &dst, args.overwrite, args.parallel).await?;
                println!(
                    "copied {} files ({} bytes) from {src} to {dst}",
                    r.files, r.bytes
                );
                Ok(0)
            }
            #[cfg(not(feature = "opendal"))]
            bail!("--direction copy needs a build with the `opendal` feature");
        }
        Direction::Dump if remote::is_storage_url(&args.output) => {
            #[cfg(feature = "opendal")]
            {
                let dst = remote::parse_dir(&args.output, out_cfg.as_deref())?;
                let (staging, temp) = staging_dir(&args)?;
                args.output = staging.display().to_string();
                if args.meta_dir.is_some() {
                    bail!("--metaDir cannot be combined with a storage-URL --output");
                }
                if args.dry_run {
                    return run_local(args).await;
                }
                eprintln!(
                    "dumping to the staging directory {} first",
                    staging.display()
                );
                let code = run_local(args.clone()).await?;
                if code != 0 {
                    eprintln!(
                        "not uploaded to {dst}: some steps failed. The dump is kept in {}; finish it with --stagingDir {} --resume (same arguments), which uploads it",
                        staging.display(),
                        staging.display()
                    );
                    return Ok(code);
                }
                let src = remote::parse_dir(&staging.display().to_string(), None)?;
                let r = remote::copy_tree(&src, &dst, args.overwrite, args.parallel)
                    .await
                    .with_context(|| {
                        format!(
                            "uploading the dump failed; it is kept in {} (rerun with --stagingDir {} --resume, or copy it with --direction copy)",
                            staging.display(),
                            staging.display()
                        )
                    })?;
                println!("uploaded {} files ({} bytes) to {dst}", r.files, r.bytes);
                if temp {
                    let _ = fs::remove_dir_all(&staging);
                } else {
                    eprintln!("the staged copy stays in {}", staging.display());
                }
                Ok(0)
            }
            #[cfg(not(feature = "opendal"))]
            bail!("a storage-URL --output needs a build with the `opendal` feature");
        }
        Direction::Load if remote::is_storage_url(&args.input) => {
            #[cfg(feature = "opendal")]
            {
                let src = remote::parse_dir(&args.input, in_cfg.as_deref())?;
                let (staging, temp) = staging_dir(&args)?;
                if args.meta_dir.is_some() {
                    bail!("--metaDir cannot be combined with a storage-URL --input");
                }
                let dst = remote::parse_dir(&staging.display().to_string(), None)?;
                let r = remote::copy_tree(&src, &dst, true, args.parallel).await?;
                eprintln!(
                    "downloaded {} files ({} bytes) from {src} to {}",
                    r.files,
                    r.bytes,
                    staging.display()
                );
                args.input = staging.display().to_string();
                let code = run_local(args).await;
                if temp {
                    let _ = fs::remove_dir_all(&staging);
                }
                code
            }
            #[cfg(not(feature = "opendal"))]
            bail!("a storage-URL --input needs a build with the `opendal` feature");
        }
        Direction::Dump if remote::is_storage_url(&args.input) => bail!(
            "--direction dump reads a cluster; to copy a dump directory between stores use --direction copy"
        ),
        Direction::Load if remote::is_storage_url(&args.output) => bail!(
            "--direction load writes into a cluster; to copy a dump directory between stores use --direction copy"
        ),
        _ => run_local(args).await,
    }
}

/// `--stagingDir`, or a new temp directory (`true`: made here, removed when done).
#[cfg(feature = "opendal")]
fn staging_dir(args: &Args) -> Result<(PathBuf, bool)> {
    match &args.staging_dir {
        Some(d) => {
            fs::create_dir_all(d).with_context(|| format!("create {}", d.display()))?;
            Ok((d.clone(), false))
        }
        None => {
            let d = std::env::temp_dir().join(format!(
                "searchdump-multi-staging-{}-{}",
                std::process::id(),
                chrono_stamp()
            ));
            fs::create_dir_all(&d).with_context(|| format!("create {}", d.display()))?;
            Ok((d, true))
        }
    }
}

async fn run_local(args: Args) -> Result<i32> {
    if args.direction == Direction::Load && (args.file_size.is_some() || args.max_rows.is_some()) {
        bail!("--fileSize/--maxRows apply to dumps; load finds split parts on its own");
    }
    if args.node_compat {
        if args.direction == Direction::Load {
            bail!("--nodeCompat applies to dumps; load reads Node-named directories on its own");
        }
        if args.layout != Layout::Flat {
            bail!("--nodeCompat needs --layout flat (Node reads one directory, no subdirectories)");
        }
        if !matches!(args.compression, Compression::Gzip | Compression::None) {
            bail!("--nodeCompat: Node reads gzip or uncompressed files only, not zstd");
        }
        if args.splits() {
            bail!(
                "--nodeCompat cannot split data: Node multielasticdump 6.x cannot load split parts (it opens INDEX.json or 'undefined' for them); dump without --nodeCompat, or without --fileSize/--maxRows"
            );
        }
        if args.match_type.contains(&MatchType::Datastream) {
            bail!(
                "--nodeCompat: Node multielasticdump cannot restore data streams (its data runs use the index op); dump them without --nodeCompat"
            );
        }
    }
    let mut default_types: Vec<String> = match args.direction {
        // Node dumps (and by default loads) the legacy templates with every index.
        Direction::Dump if args.node_compat => vec!["settings", "mapping", "template", "data"],
        Direction::Dump => vec!["settings", "mapping", "data"],
        Direction::Load => vec!["settings", "analyzer", "mapping", "alias", "data"],
        Direction::Copy => unreachable!("copy is handled before run_local"),
    }
    .into_iter()
    .map(String::from)
    .collect();
    if args.direction == Direction::Load {
        default_types.extend(CLUSTER_TYPES.iter().map(|s| s.to_string()));
    }
    let (want, ignore) = node_type_flags(&args, &default_types);
    let types = resolve_types(&want, &ignore)?;
    let mut types = types;
    if args.node_compat {
        if let Some(i) = types.cluster.iter().position(|k| *k == "template") {
            // Node keeps the legacy templates in INDEX.template.json, loaded before the index.
            types.cluster.remove(i);
            types.index.insert(0, "template");
        } else {
            eprintln!(
                "warning: no template type: Node's multielasticdump load stops on the missing INDEX.template.json unless run with --ignoreTemplate=true"
            );
        }
        for k in ["settings", "mapping", "data"] {
            if !types.index.contains(&k) {
                eprintln!(
                    "warning: no {k} type: Node's multielasticdump load stops on the missing files unless run with --ignore{}=true",
                    match k {
                        "settings" => "Settings",
                        "mapping" => "Mapping",
                        _ => "Data",
                    }
                );
            }
        }
    }
    let sel = Selector::new(&args.match_re, args.exclude.as_deref(), args.include_hidden)?;
    if args.layout == Layout::Group && !sel.has_group() {
        bail!("--layout group needs a capture group in --match, e.g. '^logs-(?P<group>[a-z]+)-'");
    }
    let creds_in = |u: &str| {
        Url::parse(u)
            .map(|u| u.password().is_some())
            .unwrap_or(false)
    };
    if creds_in(&args.input) || creds_in(&args.output) {
        eprintln!(
            "warning: a password in the URL is passed to every searchdump run and shows in `ps`; prefer SEARCHDUMP_*_PASSWORD or --httpAuthFile"
        );
    }
    let log_dir = match &args.log_dir {
        Some(d) => d.clone(),
        None => std::env::temp_dir().join(format!(
            "searchdump-multi-{}-{}",
            std::process::id(),
            chrono_stamp()
        )),
    };
    let mut common = vec![];
    if args.insecure {
        common.push("--insecure".to_string());
    }
    if let Some(f) = &args.http_auth_file {
        common.push(format!("--httpAuthFile={f}"));
    }
    let data_flags = merge_data_flags(
        if args.no_default_data_flags {
            &[]
        } else {
            DATA_FLAGS
        },
        &args.extra,
    );
    let cancelled = Arc::new(AtomicBool::new(false));
    let cancel = CancellationToken::new();
    {
        let c = cancelled.clone();
        let token = cancel.clone();
        tokio::spawn(async move {
            if tokio::signal::ctrl_c().await.is_ok() {
                eprintln!(
                    "interrupted: stopping running steps (SIGTERM, SIGKILL after {}s), starting no new ones",
                    CHILD_GRACE.as_secs()
                );
                c.store(true, Ordering::SeqCst);
                token.cancel();
            }
        });
    }
    let bin = find_bin(args.elasticdump_bin.as_deref());
    let stats = takes_stats_file(&bin) && accepts_stats_file(&bin);
    let server_limits = takes_stats_file(&bin) && accepts_server_limits(&bin);
    let runner = Runner {
        bin,
        log_dir: log_dir.clone(),
        common,
        data_flags,
        cancelled: cancelled.clone(),
        cancel: cancel.clone(),
        grace: CHILD_GRACE,
        stats,
        server_limits,
    };
    let pacer = Pacer::new(Duration::from_millis(args.interval));
    let meta = meta_dir(&args);
    let ctx = Arc::new(Ctx {
        meta,
        args,
        types,
        sel,
        runner,
        pacer,
        plan: Default::default(),
        index_stats: Default::default(),
    });
    let started = Instant::now();
    let sem = Arc::new(tokio::sync::Semaphore::new(ctx.args.parallel));
    let mut set = tokio::task::JoinSet::new();
    // Cluster-scoped results, reported with the per-index ones.
    let mut early: Vec<(String, Outcome)> = vec![];

    match ctx.args.direction {
        Direction::Dump => {
            let cluster = Arc::new(Cluster::new(
                &ctx.args.input,
                "SEARCHDUMP_INPUT",
                &ctx.args,
            )?);
            let mt = &ctx.args.match_type;
            let mut selected = vec![];
            let mut total = 0;
            if !ctx.types.index.is_empty() {
                let indices = cluster.list_indices().await?;
                let aliases = if mt.contains(&MatchType::Alias) {
                    cluster.list_aliases().await?
                } else {
                    vec![]
                };
                let streams = if mt.contains(&MatchType::Datastream) {
                    cluster.list_data_streams().await?
                } else {
                    vec![]
                };
                total = indices.len() + streams.len();
                selected = select_targets(&ctx.sel, mt, &indices, &aliases, &streams);
                if ctx.args.order == Order::Desc {
                    selected.reverse();
                }
                for t in &selected {
                    check_index_name(&t.name)?;
                }
                if selected.iter().any(|t| t.kind == TargetKind::DataStream)
                    && !ctx.types.cluster.contains(&"index_template")
                {
                    eprintln!(
                        "warning: a data stream loads only where a matching index template exists; add index_template (and component_template) to --types to dump them too"
                    );
                }
            }
            let root = PathBuf::from(&ctx.args.output);
            if ctx.args.dry_run {
                if !ctx.types.cluster.is_empty() {
                    println!("cluster-wide:");
                    for k in &ctx.types.cluster {
                        println!(
                            "  {}",
                            ctx.meta
                                .join(CLUSTER_DIR)
                                .join(format!("{k}.json"))
                                .display()
                        );
                    }
                }
                println!(
                    "{} of {total} indices and data streams selected:",
                    selected.len()
                );
                let split = ctx.args.splits();
                for t in &selected {
                    let rel = rel_dir(ctx.args.layout, &ctx.sel, &t.name, &t.matched)?;
                    let d = root.join(&rel);
                    let mut files: Vec<String> = vec![];
                    if t.kind == TargetKind::DataStream {
                        files.push(
                            d.join(format!("{}.datastream.json", t.name))
                                .display()
                                .to_string(),
                        );
                    }
                    for k in ctx
                        .types
                        .index
                        .iter()
                        .filter(|k| t.kind == TargetKind::Index || **k == "data")
                    {
                        let node = ctx.args.node_compat;
                        let f = if *k == "data" && split {
                            if node {
                                node_item_file(
                                    &format!("{}.split-N", t.name),
                                    k,
                                    ctx.args.compression,
                                )
                            } else {
                                format!("{}.data.split-N.{}", t.name, ctx.args.compression.suffix())
                            }
                        } else {
                            item_file_for(node, &t.name, k, ctx.args.compression)
                        };
                        files.push(d.join(f).display().to_string());
                    }
                    let via = if t.matched != t.name {
                        format!(" (via alias {})", t.matched)
                    } else {
                        String::new()
                    };
                    let ds = if t.kind == TargetKind::DataStream {
                        " [data stream]"
                    } else {
                        ""
                    };
                    println!("  {}{ds}{via}\n    {}", t.name, files.join("\n    "));
                }
                return Ok(0);
            }
            if selected.is_empty() && ctx.types.cluster.is_empty() {
                eprintln!(
                    "nothing matches --match '{}' (of {total} indices and data streams)",
                    ctx.args.match_re,
                );
                return Ok(1);
            }
            let names: Vec<String> = selected.iter().map(|t| t.name.clone()).collect();
            let (stop, limits_json) = plan_sizes(&ctx, &cluster, &names).await;
            if let Some(code) = stop {
                return Ok(code);
            }
            fs::create_dir_all(&root).with_context(|| format!("create {}", root.display()))?;
            fs::create_dir_all(&ctx.meta)
                .with_context(|| format!("create {}", ctx.meta.display()))?;
            fs::create_dir_all(&log_dir)?;
            let mut m = Manifest::load(&ctx.meta)?.unwrap_or_default();
            if m.node_compat != ctx.args.node_compat && !m.indices.is_empty() {
                bail!(
                    "{} already holds a dump {} --nodeCompat; use another directory",
                    root.display(),
                    if m.node_compat {
                        "made with"
                    } else {
                        "made without"
                    }
                );
            }
            m.node_compat = ctx.args.node_compat;
            if let Some(prev) = m.compression.filter(|c| *c != ctx.args.compression)
                && !m.indices.is_empty()
            {
                eprintln!(
                    "warning: this directory already holds a {prev:?} dump; new indices use {:?}",
                    ctx.args.compression
                );
            }
            m.tool = "searchdump-multi".into();
            m.version = env!("CARGO_PKG_VERSION").into();
            m.source = searchdump_rs::endpoint::redacted_endpoint(&ctx.args.input);
            m.layout = Some(ctx.args.layout);
            m.compression = Some(ctx.args.compression);
            m.server_limits = Some(limits_json);
            let mut todo = vec![];
            for t in selected {
                let done = m.indices.get(&t.name).is_some_and(|e| {
                    let d = root.join(&e.dir);
                    e.status == "done"
                        && e.types.iter().all(|k| match k.as_str() {
                            "data" if !e.data_parts.is_empty() => {
                                e.data_parts.iter().all(|p| d.join(p).is_file())
                            }
                            "data" => {
                                e.records == Some(0)
                                    || e.data_file.as_ref().is_some_and(|f| nonempty(&d.join(f)))
                            }
                            "analyzer" | "template" => d
                                .join(item_file_for(
                                    m.node_compat,
                                    &t.name,
                                    k,
                                    m.compression.unwrap_or(Compression::None),
                                ))
                                .is_file(),
                            k => meta_file(&d, &t.name, k).is_some(),
                        })
                });
                if ctx.args.resume && done {
                    println!("skip {} (already dumped)", t.name);
                    continue;
                }
                todo.push(t);
            }
            m.save(&ctx.meta)?;
            let manifest = Arc::new(Mutex::new(m));
            if ctx.meta != root {
                println!("manifest and cluster-wide files in {}", ctx.meta.display());
            }
            if !ctx.types.cluster.is_empty() {
                println!(
                    "dumping cluster-wide {} to {}",
                    ctx.types.cluster.join(", "),
                    ctx.meta.join(CLUSTER_DIR).display()
                );
                early = dump_cluster(&ctx, &cluster, &manifest).await;
            }
            println!(
                "dumping {} indices/data streams to {} ({} at a time, logs in {})",
                todo.len(),
                root.display(),
                ctx.args.parallel,
                log_dir.display()
            );
            // 1.2: a byte budget next to the --parallel bound: a worker holds its index's
            // estimated page bytes (KiB permits), so large-document indices run fewer at once
            // while small ones still run --parallel at once.
            let bytes = ctx.plan().budget.map(|b| {
                let total = (b.inflight_bytes >> 10).clamp(1, u32::MAX as u64 / 2) as u32;
                (Arc::new(tokio::sync::Semaphore::new(total as usize)), total)
            });
            for t in todo {
                let (ctx, cluster, root, manifest, sem, bytes) = (
                    ctx.clone(),
                    cluster.clone(),
                    root.clone(),
                    manifest.clone(),
                    sem.clone(),
                    bytes.clone(),
                );
                set.spawn(async move {
                    let _p = sem.acquire().await;
                    let _b = match &bytes {
                        Some((s, total)) => {
                            let need = ctx.plan().dump(ctx.stats_of(&t.name)).page_bytes >> 10;
                            s.clone()
                                .acquire_many_owned(need.clamp(1, *total as u64) as u32)
                                .await
                                .ok()
                        }
                        None => None,
                    };
                    let o = dump_one(&ctx, &cluster, &root, &manifest, &t).await;
                    (t.name, o)
                });
            }
        }
        Direction::Load | Direction::Copy => {
            let root = PathBuf::from(&ctx.args.input);
            if !root.is_dir() {
                bail!("dump directory does not exist: {}", root.display());
            }
            let mut found: Vec<Found> = if ctx.types.index.is_empty() {
                vec![]
            } else {
                discover(&root, &ctx.meta)?
                    .into_iter()
                    .filter(|f| ctx.sel.selects(&f.index))
                    .collect()
            };
            if ctx.args.order == Order::Desc {
                found.reverse();
            }
            let cluster_files: Vec<&str> = ctx
                .types
                .cluster
                .iter()
                .copied()
                .filter(|k| nonempty(&ctx.meta.join(CLUSTER_DIR).join(format!("{k}.json"))))
                .collect();
            if ctx.args.dry_run {
                if !cluster_files.is_empty() {
                    println!("cluster-wide, loaded first: {}", cluster_files.join(", "));
                }
                println!("{} indices/data streams to load:", found.len());
                for f in &found {
                    let parts = if f.data.len() > 1 {
                        format!(", {} data parts", f.data.len())
                    } else {
                        String::new()
                    };
                    let ds = if f.datastream { " [data stream]" } else { "" };
                    println!(
                        "  {}{ds} -> {}{}{} (from {}{parts})",
                        f.index,
                        ctx.args.prefix,
                        f.index,
                        ctx.args.suffix,
                        f.dir.display()
                    );
                }
                return Ok(0);
            }
            if found.is_empty() && cluster_files.is_empty() {
                eprintln!(
                    "nothing in {} matches --match '{}'",
                    root.display(),
                    ctx.args.match_re
                );
                return Ok(1);
            }
            let cluster = Arc::new(Cluster::new(
                &ctx.args.output,
                "SEARCHDUMP_OUTPUT",
                &ctx.args,
            )?);
            if ctx.types.index.contains(&"data") {
                let (stop, _) = plan_sizes(&ctx, &cluster, &[]).await;
                if let Some(code) = stop {
                    return Ok(code);
                }
            }
            fs::create_dir_all(&log_dir)?;
            if !ctx.types.cluster.is_empty() {
                early = load_cluster(&ctx, &cluster).await;
            }
            println!(
                "loading {} indices/data streams from {} ({} at a time, logs in {})",
                found.len(),
                root.display(),
                ctx.args.parallel,
                log_dir.display()
            );
            for f in found {
                let (ctx, cluster, sem) = (ctx.clone(), cluster.clone(), sem.clone());
                set.spawn(async move {
                    let _p = sem.acquire().await;
                    let o = load_one(&ctx, &cluster, &f).await;
                    (f.index, o)
                });
            }
        }
    }

    let (mut ok, mut skipped, mut failed) = (0, 0, vec![]);
    let mut report = |index: String, outcome: Outcome| {
        match outcome {
            Outcome::Done(s) => {
                ok += 1;
                println!("ok      {index}: {s}");
            }
            Outcome::Skipped(s) => {
                skipped += 1;
                println!("skipped {index}: {s}");
            }
            Outcome::Failed(s) => {
                eprintln!("FAILED  {index}: {s}");
                failed.push(index);
            }
        }
        let _ = std::io::stdout().flush();
    };
    for (index, outcome) in early {
        report(index, outcome);
    }
    while let Some(r) = set.join_next().await {
        let (index, outcome) = r?;
        report(index, outcome);
    }
    failed.sort();
    println!(
        "\n{ok} done, {skipped} skipped, {} failed in {:.0?}{}",
        failed.len(),
        started.elapsed(),
        if failed.is_empty() {
            String::new()
        } else {
            format!(": {}", failed.join(", "))
        }
    );
    if cancelled.load(Ordering::SeqCst) {
        return Ok(130);
    }
    Ok(if failed.is_empty() { 0 } else { 1 })
}

fn chrono_stamp() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn main() {
    let args = Args::parse();
    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("tokio runtime");
    let code = match rt.block_on(run(args)) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("searchdump-multi: {e:#}");
            2
        }
    };
    std::process::exit(code);
}

#[cfg(test)]
mod tests {
    #[test]
    fn extra_flags_override_default_data_flags() {
        let extra: Vec<String> = ["--limit=200", "--scrollTime", "1m", "--noRefresh"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        let got = super::merge_data_flags(super::DATA_FLAGS, &extra);
        assert!(!got.iter().any(|a| a == "--scrollTime=10m"), "{got:?}");
        assert_eq!(got.iter().filter(|a| a.starts_with("--limit")).count(), 1);
        assert!(got.contains(&"--retryAttempts=5".to_string()));
        assert!(got.ends_with(&extra));
        // no extras: the defaults unchanged
        assert_eq!(
            super::merge_data_flags(super::DATA_FLAGS, &[]),
            super::strings(super::DATA_FLAGS)
        );
        assert!(super::merge_data_flags(&[], &[]).is_empty());
        // computed sizes: the user's --limit replaces the plan's
        let sized = super::merge_data_flags_owned(
            &[
                "--limit=5000".to_string(),
                "--maxBulkBytes=16mb".to_string(),
            ],
            &got,
        );
        assert!(!sized.contains(&"--limit=5000".to_string()), "{sized:?}");
        assert!(
            sized.contains(&"--maxBulkBytes=16mb".to_string()),
            "{sized:?}"
        );
    }

    /// 1.2: the run-control flags are the 1.1.0 ones minus the sizes (which moved to `sizing`).
    #[test]
    fn run_control_flags_unchanged() {
        assert_eq!(
            super::strings(super::DATA_FLAGS),
            [
                "--scrollTime=10m",
                "--retryAttempts=5",
                "--retryDelay=2000",
                "--timeout=300000"
            ]
        );
    }

    /// 1.2: `--serverLimits=off` (and an unprobed / fallback run) gives every child exactly the
    /// 1.1.0 data flags: `--limit=1000` (fewer for large documents, the 1.1-10 rule) and
    /// `--maxBulkBytes=16mb`; `--noDefaultDataFlags` gives no sizes at all.
    #[test]
    fn server_limits_off_reproduces_1_1_0_flags() {
        use super::sizing::Plan;
        use searchdump_rs::limits::{Limits, Mode, Source};
        let v110: Vec<String> = [
            "--limit=1000",
            "--scrollTime=10m",
            "--retryAttempts=5",
            "--retryDelay=2000",
            "--timeout=300000",
            "--maxBulkBytes=16mb",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect();
        let off = Plan::new(Mode::Off, false, None, 2, &[Some((6250, 6250 * 500))]);
        let mut flags = off.flags(&off.dump(Some((6250, 6250 * 500))), true);
        flags.extend(super::merge_data_flags(super::DATA_FLAGS, &[]));
        let mut want = v110.clone();
        want.push("--serverLimits=off".into());
        let (mut a, mut b) = (flags.clone(), want.clone());
        a.sort();
        b.sort();
        assert_eq!(a, b);
        // an old child (no --serverLimits) gets exactly the 1.1.0 set
        let mut old = off.flags(&off.dump(None), false);
        old.extend(super::merge_data_flags(super::DATA_FLAGS, &[]));
        old.sort();
        let mut v = v110.clone();
        v.sort();
        assert_eq!(old, v);
        // large documents: the 1.1-10 page
        assert_eq!(off.dump(Some((10, 10 * 80 * 1024))).limit, 204);
        assert_eq!(off.load().limit, 1000);
        assert_eq!(off.load().max_bulk_bytes, 16 << 20);
        // a fallback probe (403, serverless, ...) uses the same values
        let fb = Plan::new(
            Mode::Auto,
            false,
            Some(Limits::fallback(Source::Fallback403, "403")),
            2,
            &[],
        );
        assert_eq!(fb.dump(Some((10, 10 * 80 * 1024))).limit, 204);
        assert_eq!(fb.dump(None).reason, "fallback");
        assert_eq!(
            fb.flags(&fb.load(), true),
            ["--limit=1000", "--maxBulkBytes=16mb", "--serverLimits=off"]
        );
        // a fallback on AWS keeps bulks at the 10 MiB floor
        let mut aws = Limits::fallback(Source::Fallback403, "403");
        aws.aws = true;
        let fa = Plan::new(Mode::Auto, false, Some(aws), 2, &[]);
        assert_eq!(
            fa.load().max_bulk_bytes,
            searchdump_rs::limits::AWS_PAYLOAD_FLOOR
        );
        assert_eq!(
            fa.flags(&fa.load(), false),
            ["--limit=1000", "--maxBulkBytes=10240kb"]
        );
        // --noDefaultDataFlags: no sizes
        let nd = Plan::new(Mode::Off, true, None, 8, &[]);
        assert_eq!(nd.flags(&nd.dump(None), true), ["--serverLimits=off"]);
        assert!(nd.flags(&nd.dump(None), false).is_empty());
    }

    /// 1.1-10 (the `off` profile): page size from the average stored document size.
    #[test]
    fn page_limit_shrinks_for_large_documents() {
        use super::sizing::page_limit_for as f;
        assert_eq!(f(0), 1000); // unknown
        assert_eq!(f(500), 1000); // log lines
        assert_eq!(f(12 * 1024), 1000); // PMC articles (~12 KB stored)
        assert_eq!(f(16 * 1024), 1000);
        assert_eq!(f(80 * 1024), 204); // ~80 KB documents
        assert_eq!(f(1 << 20), 16);
        assert_eq!(f(10 << 20), 1);
        assert_eq!(f(u64::MAX), 1);
    }

    /// 1.2 auto: pages grow to 5000 small documents on a roomy heap, shrink for large documents
    /// and small heaps; worker-first keeps at least 2 workers; min is tighter than auto.
    #[test]
    fn auto_plan_sizes_from_fixture_limits() {
        use super::sizing::Plan;
        use searchdump_rs::limits::{Limits, Mode, Node, Source};
        let node = |heap: u64| Node {
            name: "n".into(),
            data: true,
            heap_max: heap,
            parent_limit: Some(heap * 95 / 100),
            parent_estimated: Some(heap * 40 / 100),
            request_limit: Some(heap * 60 / 100),
            indexing_pressure: Some(heap / 10),
            max_content_length: Some(100 << 20),
            search_pool: Some(13),
            ..Default::default()
        };
        let lim = |heap| Limits {
            source: Source::Probed,
            nodes: vec![node(heap)],
            aws: false,
            reason: String::new(),
        };
        let small = Some((100_000u64, 100_000u64 * 400)); // log lines
        let big = Some((10_000u64, 10_000u64 * 80 * 1024)); // ~80 KB documents
        let roomy = Plan::new(Mode::Auto, false, Some(lim(1 << 30)), 8, &[small, big]);
        assert_eq!(roomy.dump(small).limit, 5000);
        assert_eq!(roomy.dump(small).reason, "default");
        let b = roomy.dump(big);
        assert!(b.limit < 1000 && b.limit >= 1, "{b:?}");
        let tight = Plan::new(Mode::Auto, false, Some(lim(256 << 20)), 8, &[small, big]);
        assert!(tight.dump(big).limit <= roomy.dump(big).limit);
        assert!(tight.fit_parallel >= 2 && tight.fit_parallel <= 8);
        assert!(tight.bulk_bytes <= roomy.bulk_bytes);
        let min = Plan::new(Mode::Min, false, Some(lim(256 << 20)), 8, &[small, big]);
        assert!(min.dump(big).limit <= tight.dump(big).limit);
        assert!(min.bulk_bytes <= tight.bulk_bytes);
        // the flags carry the sizes and stop the child probing again
        let f = roomy.flags(&roomy.dump(small), true);
        assert_eq!(f[0], "--limit=5000");
        assert!(
            f[1].starts_with("--maxBulkBytes=") && f[1].ends_with("kb"),
            "{f:?}"
        );
        assert_eq!(f[2], "--serverLimits=off");
        // parallel 1 stays 1
        assert_eq!(
            Plan::new(Mode::Auto, false, Some(lim(256 << 20)), 1, &[big]).fit_parallel,
            1
        );
    }

    use super::*;

    #[test]
    fn selection_include_exclude_hidden() {
        let s = Selector::new("^logs-", Some("-debug-"), false).unwrap();
        assert!(s.selects("logs-app-2026.10.01"));
        assert!(!s.selects("logs-debug-2026.10.01"));
        assert!(!s.selects("metrics-1"));
        let all = Selector::new("^.*$", None, false).unwrap();
        assert!(all.selects("a"));
        assert!(
            !all.selects(".kibana"),
            "hidden indices need --includeHidden or an explicit match"
        );
        assert!(
            Selector::new("^.*$", None, true)
                .unwrap()
                .selects(".kibana")
        );
        assert!(
            Selector::new("^\\.opendistro", None, false)
                .unwrap()
                .selects(".opendistro-ism-config")
        );
        assert!(Selector::new("(", None, false).is_err());
    }

    #[test]
    fn group_breakout() {
        let s = Selector::new(r"^logs-.*-(?P<group>\d{4}\.\d{2})\.\d{2}$", None, false).unwrap();
        assert!(s.has_group());
        assert_eq!(s.group("logs-app-2026.10.31").as_deref(), Some("2026.10"));
        assert_eq!(
            rel_dir(
                Layout::Group,
                &s,
                "logs-app-2026.10.31",
                "logs-app-2026.10.31"
            )
            .unwrap(),
            PathBuf::from("2026.10")
        );
        let first = Selector::new(r"^([a-z]+)-", None, false).unwrap();
        assert_eq!(first.group("web-1").as_deref(), Some("web"));
        let none = Selector::new("^x", None, false).unwrap();
        assert!(!none.has_group());
        assert_eq!(
            rel_dir(Layout::Flat, &none, "x1", "x1").unwrap(),
            PathBuf::new()
        );
        assert_eq!(
            rel_dir(Layout::Index, &none, "x1", "x1").unwrap(),
            PathBuf::from("x1")
        );
        assert_eq!(sanitize_dir("a/b"), "a_b");
        assert_eq!(sanitize_dir(".."), "__");
        assert_eq!(sanitize_dir(""), "_");
    }

    #[test]
    fn names_and_types() {
        assert_eq!(
            item_file("i", "settings", Compression::Gzip),
            "i.settings.json"
        );
        assert_eq!(item_file("i", "data", Compression::Gzip), "i.data.json.gz");
        assert_eq!(
            item_file("i", "data", Compression::Zstd1),
            "i.data.json.zst"
        );
        assert_eq!(item_file("i", "data", Compression::None), "i.data.json");
        assert_eq!(
            Compression::of_file("x.data.json.zst"),
            Some(Compression::Zstd3)
        );
        assert_eq!(Compression::of_file("x.mapping.json"), None);
        let t = resolve_types(
            &[
                "data".into(),
                "settings".into(),
                "alias".into(),
                "mapping".into(),
            ],
            &["alias".into()],
        )
        .unwrap();
        assert_eq!(t.index, vec!["settings", "mapping", "data"]);
        assert!(t.cluster.is_empty());
        let t = resolve_types(
            &[
                "template".into(),
                "data".into(),
                "policy".into(),
                "index_template".into(),
            ],
            &[],
        )
        .unwrap();
        assert_eq!(t.index, vec!["data"]);
        assert_eq!(t.cluster, vec!["policy", "index_template", "template"]);
        let t = resolve_types(&["all".into()], &["script".into(), "analyzer".into()]).unwrap();
        assert_eq!(t.index, vec!["settings", "mapping", "alias", "data"]);
        assert_eq!(
            t.cluster,
            vec!["policy", "component_template", "index_template", "template"]
        );
        assert!(resolve_types(&["data".into()], &["all".into()]).is_err());
        assert!(resolve_types(&["bogus".into()], &[]).is_err());
        assert!(resolve_types(&["data".into()], &["data".into()]).is_err());
        for bad in ["", ".", "..", "a/b", "a\\b", "-x", "a\nb"] {
            assert!(check_index_name(bad).is_err(), "{bad:?}");
        }
        check_index_name(".ds-logs-000001").unwrap();
    }

    #[test]
    fn mappings_body_shapes() {
        let plain = r#"{"idx":{"mappings":{"properties":{"a":{"type":"keyword"}}}}}"#;
        assert_eq!(
            mappings_body(plain).unwrap()["properties"]["a"]["type"],
            "keyword"
        );
        let stringified = serde_json::to_string(plain).unwrap();
        assert_eq!(
            mappings_body(&stringified).unwrap()["properties"]["a"]["type"],
            "keyword"
        );
        assert!(mappings_body("{}").is_err());
    }

    #[test]
    fn credentials_order_and_redaction() {
        let env = |k: &str| match k {
            "SEARCHDUMP_INPUT_USERNAME" => Some("envu".to_string()),
            "SEARCHDUMP_INPUT_PASSWORD" => Some("envp\n".to_string()),
            _ => None,
        };
        let (u, user, pass) = split_url("https://h:9200", "SEARCHDUMP_INPUT", None, &env).unwrap();
        assert_eq!(u.as_str(), "https://h:9200/");
        assert_eq!(
            (user.as_deref(), pass.as_deref()),
            (Some("envu"), Some("envp"))
        );
        let (u, user, pass) = split_url(
            "https://a:b%40c@h:9200/base",
            "SEARCHDUMP_INPUT",
            None,
            &env,
        )
        .unwrap();
        assert_eq!(u.as_str(), "https://h:9200/base/");
        assert_eq!((user.as_deref(), pass.as_deref()), (Some("a"), Some("b@c")));
        let file = ("fu".to_string(), "fp".to_string());
        let (_, user, _) =
            split_url("https://h:9200", "SEARCHDUMP_INPUT", Some(&file), &env).unwrap();
        assert_eq!(user.as_deref(), Some("fu"));
        // L-645b: the auth file also beats URL userinfo
        let (u, user, pass) =
            split_url("https://a:b@h:9200", "SEARCHDUMP_INPUT", Some(&file), &env).unwrap();
        assert_eq!(u.as_str(), "https://h:9200/");
        assert_eq!((user.as_deref(), pass.as_deref()), (Some("fu"), Some("fp")));
        assert!(split_url("ftp://h", "SEARCHDUMP_INPUT", None, &env).is_err());
    }

    #[test]
    fn record_counts_all_codecs() {
        let d = tempfile::tempdir().unwrap();
        let body = b"{\"a\":1}\n{\"a\":2}\n{\"a\":3}\n";
        let plain = d.path().join("i.data.json");
        fs::write(&plain, body).unwrap();
        let gz = d.path().join("i.data.json.gz");
        let mut e = flate2::write::GzEncoder::new(
            fs::File::create(&gz).unwrap(),
            flate2::Compression::default(),
        );
        e.write_all(body).unwrap();
        e.finish().unwrap();
        let zst = d.path().join("j.data.json.zst");
        fs::write(&zst, zstd::stream::encode_all(&body[..], 3).unwrap()).unwrap();
        for p in [&plain, &gz, &zst] {
            assert_eq!(count_records(p).unwrap(), 3, "{}", p.display());
        }
    }

    #[test]
    fn discover_layouts_and_manifest() {
        let d = tempfile::tempdir().unwrap();
        let r = d.path();
        // oslogs.py flat files
        for f in ["a.settings.json", "a.mapping.json"] {
            fs::write(r.join(f), "{}").unwrap();
        }
        fs::write(r.join("a.data.json.gz"), b"x").unwrap();
        // grouped
        fs::create_dir_all(r.join("2026.10")).unwrap();
        for f in ["b.settings.json", "b.mapping.json", "b.data.json.zst"] {
            fs::write(r.join("2026.10").join(f), "{}").unwrap();
        }
        fs::write(r.join("notes.txt"), "x").unwrap();
        let found = discover(r, r).unwrap();
        let names: Vec<_> = found.iter().map(|f| f.index.as_str()).collect();
        assert_eq!(names, vec!["a", "b"]);
        assert_eq!(found[1].dir, r.join("2026.10"));
        assert!(found[1].data[0].ends_with("b.data.json.zst"));
        // duplicate across directories is an error
        fs::create_dir_all(r.join("dup")).unwrap();
        fs::write(r.join("dup").join("a.settings.json"), "{}").unwrap();
        assert!(discover(r, r).is_err());
        fs::remove_dir_all(r.join("dup")).unwrap();
        // two data files for one index is an error
        fs::write(r.join("a.data.json.zst"), b"x").unwrap();
        assert!(discover(r, r).is_err());
        fs::remove_file(r.join("a.data.json.zst")).unwrap();
        // a manifest wins and failed entries are skipped
        let mut m = Manifest {
            tool: "searchdump-multi".into(),
            version: "t".into(),
            ..Default::default()
        };
        m.indices.insert(
            "a".into(),
            IndexEntry {
                dir: "".into(),
                types: vec!["data".into()],
                data_file: Some("a.data.json.gz".into()),
                records: Some(1),
                status: "done".into(),
                ..Default::default()
            },
        );
        m.indices.insert(
            "c".into(),
            IndexEntry {
                status: "failed".into(),
                ..Default::default()
            },
        );
        m.save(r).unwrap();
        let found = discover(r, r).unwrap();
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].index, "a");
        assert_eq!(found[0].records_hint, Some(1));
    }

    #[test]
    fn targets_by_index_alias_and_datastream() {
        let indices: Vec<String> = [
            "logs-a",
            "logs-b",
            "other",
            ".ds-web-2026.11.01-000001",
            ".ds-web-2026.11.02-000002",
        ]
        .map(String::from)
        .to_vec();
        let aliases = vec![
            ("logs".to_string(), "logs-a".to_string()),
            ("logs".to_string(), "logs-b".to_string()),
            ("ali-other".to_string(), "other".to_string()),
            ("gone".to_string(), "closed-idx".to_string()),
        ];
        let streams = vec![(
            "web".to_string(),
            vec![
                ".ds-web-2026.11.01-000001".to_string(),
                ".ds-web-2026.11.02-000002".to_string(),
            ],
        )];
        let names = |t: &[Target]| t.iter().map(|t| t.name.clone()).collect::<Vec<_>>();
        let all = Selector::new("^.*$", None, true).unwrap();
        // index matching with hidden indices still leaves stream backing indices alone
        let t = select_targets(
            &all,
            &[MatchType::Index, MatchType::Datastream],
            &indices,
            &aliases,
            &streams,
        );
        assert_eq!(names(&t), vec!["logs-a", "logs-b", "other", "web"]);
        assert_eq!(t[3].kind, TargetKind::DataStream);
        // alias matching selects the indices behind the alias, grouped by the alias name
        let s = Selector::new("^(?P<group>ali)-|^logs$", None, false).unwrap();
        let t = select_targets(&s, &[MatchType::Alias], &indices, &aliases, &streams);
        assert_eq!(names(&t), vec!["logs-a", "logs-b", "other"]);
        assert_eq!(t[2].matched, "ali-other");
        assert_eq!(
            rel_dir(Layout::Group, &s, &t[2].name, &t[2].matched).unwrap(),
            PathBuf::from("ali")
        );
        // index and alias together dump an index once
        let s = Selector::new("logs", None, false).unwrap();
        let t = select_targets(
            &s,
            &[MatchType::Index, MatchType::Alias],
            &indices,
            &aliases,
            &streams,
        );
        assert_eq!(names(&t), vec!["logs-a", "logs-b"]);
        // without datastream, backing indices are ordinary hidden indices
        let t = select_targets(&all, &[MatchType::Index], &indices, &aliases, &streams);
        assert_eq!(t.len(), 5);
    }

    #[test]
    fn split_part_names_and_discovery() {
        assert_eq!(
            parse_data_file("i.data.split-12.json.gz"),
            Some(("i".into(), Compression::Gzip, Some(12)))
        );
        assert_eq!(
            parse_data_file("i.x.data.split-0.json"),
            Some(("i.x".into(), Compression::None, Some(0)))
        );
        assert_eq!(
            parse_data_file("i.data.json.zst"),
            Some(("i".into(), Compression::Zstd3, None))
        );
        assert_eq!(parse_data_file("i.data.split-x.json"), None);
        assert_eq!(parse_data_file("i.mapping.json"), None);
        let d = tempfile::tempdir().unwrap();
        let r = d.path();
        for n in [0, 1, 2, 10] {
            fs::write(r.join(format!("p.data.split-{n}.json")), "{}\n").unwrap();
        }
        assert!(data_parts(r, "p").is_err(), "gap 3..9 must be an error");
        for n in 3..10 {
            fs::write(r.join(format!("p.data.split-{n}.json")), "{}\n").unwrap();
        }
        let parts = data_parts(r, "p").unwrap();
        assert_eq!(parts.len(), 11);
        assert!(
            parts[10].ends_with("p.data.split-10.json"),
            "numeric, not lexical, order"
        );
        fs::write(r.join("p.settings.json"), "{}").unwrap();
        // cluster-scoped files are not indices; a data stream marker is recognised
        fs::create_dir_all(r.join(CLUSTER_DIR)).unwrap();
        fs::write(r.join(CLUSTER_DIR).join("template.json"), "{}").unwrap();
        fs::write(r.join("web.datastream.json"), "{}").unwrap();
        fs::write(r.join("web.data.json.gz"), "x").unwrap();
        let found = discover(r, r).unwrap();
        assert_eq!(found.len(), 2);
        assert_eq!(found[0].index, "p");
        assert_eq!(found[0].data.len(), 11);
        assert!(!found[0].datastream);
        assert_eq!(found[1].index, "web");
        assert!(found[1].datastream);
        // a single data file next to parts is ambiguous
        fs::write(r.join("p.data.json"), "{}\n").unwrap();
        assert!(discover(r, r).is_err());
        fs::remove_file(r.join("p.data.json")).unwrap();
        // parts in two codecs are an error
        fs::write(r.join("p.data.split-11.json.gz"), "x").unwrap();
        assert!(discover(r, r).is_err());
        fs::remove_file(r.join("p.data.split-11.json.gz")).unwrap();
        // stale-file cleanup only touches this index's data files
        remove_data_files(r, "p").unwrap();
        assert!(data_parts(r, "p").unwrap().is_empty());
        assert!(r.join("p.settings.json").exists());
        assert!(r.join("web.data.json.gz").exists());
    }

    #[test]
    fn split_flag_detection_and_cluster_flags() {
        let parse = |a: &[&str]| {
            Args::try_parse_from(
                [
                    "searchdump-multi",
                    "--input",
                    "http://h:9200",
                    "--output",
                    "/tmp/x",
                ]
                .iter()
                .chain(a),
            )
            .unwrap()
        };
        assert!(!parse(&[]).splits());
        assert!(parse(&["--fileSize", "1gb"]).splits());
        assert!(parse(&["--maxRows", "10"]).splits());
        assert!(parse(&["--", "--maxRows=10"]).splits());
        assert!(
            Args::try_parse_from([
                "m",
                "--input",
                "a",
                "--output",
                "b",
                "--fileSize",
                "1gb",
                "--maxRows",
                "2"
            ])
            .is_err()
        );
        let a = parse(&["--matchType", "alias,datastream", "--interval", "250"]);
        assert_eq!(a.match_type, vec![MatchType::Alias, MatchType::Datastream]);
        assert_eq!(a.interval, 250);
        let mut a = parse(&["--skipExisting"]);
        assert_eq!(
            cluster_flags(&a, "index_template", true),
            vec![LIFECYCLE_DROP.to_string(), "--skip-existing".to_string()]
        );
        assert!(cluster_flags(&a, "index_template", false).is_empty());
        a.lifecycle = Lifecycle::Convert;
        assert_eq!(
            cluster_flags(&a, "policy", true),
            vec![
                "--convertIlmToIsm".to_string(),
                "--skip-existing".to_string()
            ]
        );
    }

    #[tokio::test(start_paused = true)]
    async fn interval_spaces_out_starts() {
        let p = Arc::new(Pacer::new(Duration::from_millis(500)));
        let t0 = tokio::time::Instant::now();
        let mut set = tokio::task::JoinSet::new();
        for _ in 0..3 {
            let p = p.clone();
            set.spawn(async move {
                p.wait().await;
                tokio::time::Instant::now()
            });
        }
        let mut at: Vec<Duration> = vec![];
        while let Some(t) = set.join_next().await {
            at.push(t.unwrap() - t0);
        }
        at.sort();
        assert_eq!(at[0], Duration::ZERO);
        assert!(at[1] >= Duration::from_millis(500), "{at:?}");
        assert!(at[2] >= Duration::from_millis(1000), "{at:?}");
        let none = Pacer::new(Duration::ZERO);
        none.wait().await;
        none.wait().await;
    }

    #[test]
    fn node_ignore_flags() {
        let p = |extra: &[&str]| {
            let mut v = vec![
                "searchdump-multi",
                "--input=http://h:9200",
                "--output=/tmp/x",
            ];
            v.extend_from_slice(extra);
            let a = Args::try_parse_from(v).unwrap();
            let d: Vec<String> = ["settings", "mapping", "data"].map(String::from).to_vec();
            let (w, i) = node_type_flags(&a, &d);
            resolve_types(&w, &i).unwrap()
        };
        assert_eq!(p(&[]).index, ["settings", "mapping", "data"]);
        assert_eq!(p(&["--ignoreData"]).index, ["settings", "mapping"]);
        assert_eq!(p(&["--ignoreData=true"]).index, ["settings", "mapping"]);
        assert_eq!(
            p(&["--ignoreMapping=false"]).index,
            ["settings", "mapping", "data"]
        );
        assert_eq!(
            p(&[
                "--ignoreAlias=false",
                "--ignoreAnalyzer=false",
                "--ignoreSettings=true"
            ])
            .index,
            ["analyzer", "mapping", "alias", "data"]
        );
        let t = p(&["--ignoreTemplate=false"]);
        assert_eq!(t.cluster, ["template"]);
        assert!(p(&["--ignoreTemplate"]).cluster.is_empty());
        assert_eq!(
            p(&["--includeType=index,data"]).index,
            ["settings", "mapping", "data"]
        );
        assert_eq!(p(&["--includeType=data,alias"]).index, ["alias", "data"]);
        assert_eq!(
            p(&["--ignoreType=index,data"]).index,
            ["settings", "mapping"]
        );
        assert_eq!(
            p(&["--ignoreIndex=false", "--types=data"]).index,
            ["settings", "mapping", "data"]
        );
        assert_eq!(
            p(&["--ignoreChildError=true", "--ignoreIndex"]).index,
            ["settings", "mapping", "data"]
        );
        assert!(
            Args::try_parse_from([
                "m",
                "--input=a",
                "--output=b",
                "--types=data",
                "--includeType=data"
            ])
            .is_err()
        );
        assert!(
            Args::try_parse_from(["m", "--input=a", "--output=b", "--ignoreData=maybe"]).is_err()
        );
    }

    #[test]
    fn node_names_and_meta_dir() {
        assert_eq!(node_item_file("i", "data", Compression::Gzip), "i.json.gz");
        assert_eq!(
            node_item_file("i", "settings", Compression::Gzip),
            "i.settings.json.gz"
        );
        assert_eq!(
            node_item_file("i", "template", Compression::None),
            "i.template.json"
        );
        assert_eq!(
            parse_data_file("i.json.gz"),
            Some(("i".into(), Compression::Gzip, None))
        );
        assert_eq!(
            parse_data_file("i.split-3.json.gz"),
            Some(("i".into(), Compression::Gzip, Some(3)))
        );
        for meta in [
            "i.settings.json.gz",
            "i.template.json",
            "i.index.json.gz",
            "multielasticdump.json",
            ".json",
        ] {
            assert_eq!(parse_data_file(meta), None, "{meta}");
        }
        // a Node-named dump directory, as Node 6.x writes it
        let d = tempfile::tempdir().unwrap();
        let r = d.path();
        for f in [
            "a.json.gz",
            "a.settings.json.gz",
            "a.mapping.json.gz",
            "a.template.json.gz",
            "notes.json",
        ] {
            fs::write(r.join(f), "x").unwrap();
        }
        let found = discover(r, r).unwrap();
        assert_eq!(found.len(), 1, "a lone notes.json is not an index");
        assert_eq!(found[0].index, "a");
        assert!(found[0].data[0].ends_with("a.json.gz"));
        assert_eq!(
            meta_file(r, "a", "settings").map(|m| m.1),
            Some(true),
            "gzipped metadata found"
        );
        // the same part number in both namings is an error, not a silent pick
        fs::write(r.join("b.split-0.json"), "{}\n").unwrap();
        fs::write(r.join("b.data.split-0.json"), "{}\n").unwrap();
        assert!(data_parts(r, "b").is_err());
        // --metaDir defaults
        let mut a = Args::try_parse_from([
            "m",
            "--input=http://h:9200",
            "--output=/backup/x/",
            "--nodeCompat",
        ])
        .unwrap();
        assert_eq!(meta_dir(&a), PathBuf::from("/backup/x.meta"));
        a.node_compat = false;
        assert_eq!(meta_dir(&a), PathBuf::from("/backup/x/"));
        let load = Args::try_parse_from([
            "m",
            "--direction=load",
            &format!("--input={}", r.display()),
            "--output=http://h:9200",
        ])
        .unwrap();
        assert_eq!(
            meta_dir(&load),
            r.to_path_buf(),
            "no DIR.meta: the directory"
        );
        fs::create_dir_all(format!("{}.meta", r.display())).unwrap();
        assert_eq!(
            meta_dir(&load),
            PathBuf::from(format!("{}.meta", r.display()))
        );
        fs::remove_dir_all(format!("{}.meta", r.display())).unwrap();
    }

    /// 1.1-8: a Runner whose "searchdump" is a shell script.
    #[cfg(unix)]
    fn script_runner(dir: &Path, body: &str, grace: Duration) -> Runner {
        use std::os::unix::fs::PermissionsExt;
        // a new file each time: never rewrite a script a child may still be executing
        static N: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
        let n = N.fetch_add(1, Ordering::Relaxed);
        let bin = dir.join(format!("fake-searchdump-{n}.sh"));
        fs::write(&bin, format!("#!/bin/sh\n{body}\n")).unwrap();
        fs::set_permissions(&bin, fs::Permissions::from_mode(0o755)).unwrap();
        Runner {
            bin,
            log_dir: dir.to_path_buf(),
            common: vec![],
            data_flags: vec![],
            cancelled: Arc::new(AtomicBool::new(false)),
            cancel: CancellationToken::new(),
            grace,
            stats: false,
            server_limits: false,
        }
    }

    #[cfg(unix)]
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn cancel_stops_a_running_child_with_sigterm_then_sigkill() {
        let d = tempfile::tempdir().unwrap();
        // exits on SIGTERM
        let r = Arc::new(script_runner(
            d.path(),
            "exec sleep 30",
            Duration::from_secs(5),
        ));
        let started = Instant::now();
        let task = {
            let r = r.clone();
            tokio::spawn(async move { r.run("idx", "data", "in", "out", &[]).await })
        };
        tokio::time::sleep(Duration::from_millis(300)).await;
        r.cancel.cancel();
        let err = tokio::time::timeout(Duration::from_secs(4), task)
            .await
            .expect("stopped within the grace period")
            .unwrap()
            .unwrap_err();
        let msg = format!("{err:#}");
        assert!(
            msg.contains("interrupted") && msg.contains("SIGTERM"),
            "{msg}"
        );
        assert!(
            started.elapsed() < Duration::from_secs(4),
            "{:?}",
            started.elapsed()
        );
        // ignores SIGTERM: killed once the (short) grace period ends
        let r = Arc::new(script_runner(
            d.path(),
            "trap '' TERM\nexec sleep 30",
            Duration::from_millis(300),
        ));
        let task = {
            let r = r.clone();
            tokio::spawn(async move { r.run("idx", "data", "in", "out", &[]).await })
        };
        tokio::time::sleep(Duration::from_millis(300)).await;
        r.cancel.cancel();
        let err = tokio::time::timeout(Duration::from_secs(4), task)
            .await
            .expect("killed after the grace period")
            .unwrap()
            .unwrap_err();
        assert!(format!("{err:#}").contains("SIGKILL"), "{err:#}");
        // a cancelled runner starts nothing new
        let err = r.run("idx", "data", "in", "out", &[]).await.unwrap_err();
        assert_eq!(format!("{err:#}"), "interrupted");
    }

    #[cfg(unix)]
    #[test]
    fn a_failed_load_step_names_its_part_only_when_there_are_several() {
        let p = Path::new("/d/ix.data-2.json.gz");
        let one = format!(
            "{:#}",
            with_part(anyhow::anyhow!("interrupted: stopped"), 1, 1, p)
        );
        assert_eq!(one, "interrupted: stopped");
        let many = format!(
            "{:#}",
            with_part(anyhow::anyhow!("interrupted: stopped"), 3, 2, p)
        );
        assert_eq!(many, "part 2 (/d/ix.data-2.json.gz): interrupted: stopped");
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn a_failed_child_reports_its_log_tail() {
        let d = tempfile::tempdir().unwrap();
        let r = script_runner(
            d.path(),
            "echo \"args: $*\"\necho boom-line >&2\nexit 3",
            CHILD_GRACE,
        );
        let err = r
            .run("idx", "mapping", "http://in/idx", "/tmp/out.json", &[])
            .await
            .unwrap_err();
        let msg = format!("{err:#}");
        assert!(msg.contains("searchdump --type=mapping failed"), "{msg}");
        assert!(msg.contains("boom-line"), "{msg}");
        assert!(
            msg.contains("--type=mapping") && msg.contains("--overwrite"),
            "{msg}"
        );
        assert!(d.path().join("idx.mapping.log").is_file());
        r.run("idx", "data", "in", "out", &[]).await.unwrap_err();
        // success
        let ok = script_runner(d.path(), "exit 0", CHILD_GRACE);
        ok.run("idx", "data", "in", "out", &[]).await.unwrap();
    }

    /// 1.1-8: 16 sleeping children on a 2-thread runtime do not starve other tasks (with
    /// `block_in_place` every child held a runtime thread).
    #[cfg(unix)]
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn many_children_do_not_starve_the_runtime() {
        let d = tempfile::tempdir().unwrap();
        let r = Arc::new(script_runner(d.path(), "exec sleep 1", CHILD_GRACE));
        let ticks = Arc::new(std::sync::atomic::AtomicU64::new(0));
        let ticker = {
            let ticks = ticks.clone();
            tokio::spawn(async move {
                loop {
                    tokio::time::sleep(Duration::from_millis(50)).await;
                    ticks.fetch_add(1, Ordering::Relaxed);
                }
            })
        };
        let mut set = tokio::task::JoinSet::new();
        for i in 0..16 {
            let r = r.clone();
            set.spawn(async move { r.run(&format!("i{i}"), "data", "in", "out", &[]).await });
        }
        let started = Instant::now();
        while let Some(j) = set.join_next().await {
            j.unwrap().unwrap();
        }
        let took = started.elapsed();
        ticker.abort();
        assert!(
            took < Duration::from_secs(5),
            "children ran one after another: {took:?}"
        );
        let n = ticks.load(Ordering::Relaxed);
        assert!(n >= 10, "the ticker ran {n} times in {took:?}");
    }

    /// 1.1-7: counts from `--statsFile` (searchdump's format), the log line (searchdump and Node
    /// formats), and nothing (the caller then reads the data file).
    #[test]
    fn child_counts_from_stats_file_or_log_line() {
        let s = parse_stats_file(
            r#"{"version":1,"complete":true,"written":12345,"failed":2,"overwritten":0,"rejected":1}"#,
        )
        .unwrap();
        assert_eq!(s.written, Some(12345));
        assert_eq!(s.docs_read(), Some(12347));
        assert_eq!(s.source, Some(CountSource::StatsFile));
        assert!(parse_stats_file(r#"{"complete":false,"written":1}"#).is_none());
        assert!(parse_stats_file(r#"{"written":1}"#).is_none());
        assert!(parse_stats_file("not json").is_none());

        let rust = "[2026-11-11T10:00:00Z INFO] Starting dump\nTotal Writes: 4200\nOverwritten: 3 (documents that replaced one with the same _id; included in Total Writes)\nFailed Writes: 5\ndump complete\n";
        let s = parse_total_writes(rust).unwrap();
        assert_eq!((s.written, s.failed), (Some(4200), 5));
        assert_eq!(s.source, Some(CountSource::LogLine));
        let node = "Wed, 11 Nov 2026 15:00:00 GMT | starting dump\nWed, 11 Nov 2026 15:00:01 GMT | got 100 objects from source elasticsearch (offset: 0)\nWed, 11 Nov 2026 15:00:02 GMT | Total Writes: 100\nWed, 11 Nov 2026 15:00:02 GMT | dump complete\n";
        let s = parse_total_writes(node).unwrap();
        assert_eq!((s.written, s.failed), (Some(100), 0));
        // the last one counts (a log that holds an earlier summary)
        assert_eq!(
            parse_total_writes("Total Writes: 1\nTotal Writes: 2\n")
                .unwrap()
                .written,
            Some(2)
        );
        assert!(parse_total_writes("dump complete\n").is_none());
        assert!(parse_total_writes("Total Writes: n/a\n").is_none());

        let d = tempfile::tempdir().unwrap();
        let log = d.path().join("i.data.log");
        let stats = d.path().join("i.data.stats.json");
        fs::write(&log, "Total Writes: 9\n").unwrap();
        // no stats file: the log line
        assert_eq!(child_stats(Some(&stats), &log).written, Some(9));
        fs::write(&stats, r#"{"complete":true,"written":10,"failed":0}"#).unwrap();
        let s = child_stats(Some(&stats), &log);
        assert_eq!(
            (s.written, s.source),
            (Some(10), Some(CountSource::StatsFile))
        );
        // neither: nothing reported, the caller scans
        fs::write(&log, "something else\n").unwrap();
        assert_eq!(child_stats(None, &log), ChildStats::default());
        assert_eq!(
            child_stats(None, &d.path().join("missing.log")),
            ChildStats::default()
        );
    }

    #[test]
    fn stats_file_only_for_searchdump() {
        assert!(takes_stats_file(Path::new("searchdump")));
        assert!(takes_stats_file(Path::new("/opt/x/searchdump")));
        assert!(!takes_stats_file(Path::new(
            "/usr/lib/node_modules/.bin/elasticdump"
        )));
        #[cfg(unix)]
        {
            let d = tempfile::tempdir().unwrap();
            let real = d.path().join("searchdump");
            fs::write(&real, "").unwrap();
            let link = d.path().join("elasticdump");
            std::os::unix::fs::symlink(&real, &link).unwrap();
            assert!(
                takes_stats_file(&link),
                "compatibility symlink to searchdump"
            );
        }
    }

    /// 1.1-7: the runner passes --statsFile to data runs of searchdump and returns its counts;
    /// other runs (and a binary that is not searchdump) fall back to the log line.
    #[cfg(unix)]
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn runner_returns_child_counts() {
        let d = tempfile::tempdir().unwrap();
        let script = r#"for a in "$@"; do
  case "$a" in
    --statsFile=*) printf '{"complete":true,"written":7,"failed":1}' > "${a#--statsFile=}" ;;
  esac
done
echo "Total Writes: 5""#;
        let mut r = script_runner(d.path(), script, CHILD_GRACE);
        r.stats = true;
        let s = r.run("idx", "data", "in", "out", &[]).await.unwrap();
        assert_eq!((s.written, s.failed), (Some(7), 1));
        assert_eq!(s.source, Some(CountSource::StatsFile));
        // not a data run: no --statsFile, the log line
        let s = r.run("idx", "mapping", "in", "out", &[]).await.unwrap();
        assert_eq!((s.written, s.source), (Some(5), Some(CountSource::LogLine)));
        r.stats = false;
        let s = r.run("idx", "data", "in", "out", &[]).await.unwrap();
        assert_eq!((s.written, s.source), (Some(5), Some(CountSource::LogLine)));
    }
}
