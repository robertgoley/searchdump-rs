//! Command line interface. Flag spellings follow elasticdump exactly; flags
//! marked "extension" are searchdump additions.

use clap::{ArgAction, Parser, ValueEnum};

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum SearchType {
    #[value(name = "scroll")]
    Scroll,
    #[value(name = "pit")]
    PointInTime,
    /// `--searchAfter` without PIT (selected via that flag, not `--searchType`).
    #[value(skip)]
    SearchAfter,
}

impl SearchType {
    /// Pages are chained through the last hit's `sort` values.
    pub fn uses_sort(&self) -> bool {
        matches!(self, SearchType::PointInTime | SearchType::SearchAfter)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum, Default)]
pub enum RetryBackoffArg {
    #[default]
    Fixed,
    Exponential,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum CompressionType {
    Gzip,
    Zstd,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DumpType {
    Data,
    Meta(crate::meta::MetaKind),
}

pub const TYPES: &str = "data, mapping, settings, index, analyzer, alias, template, component_template, index_template, policy, script";

impl DumpType {
    pub fn parse(raw: &str) -> Result<Self, String> {
        use crate::meta::{MetaKind as K, TemplateKind as T};
        Ok(match raw {
            "data" => Self::Data,
            "mapping" => Self::Meta(K::Mapping),
            "settings" => Self::Meta(K::Settings),
            "index" => Self::Meta(K::Index),
            "analyzer" => Self::Meta(K::Analyzer),
            "alias" => Self::Meta(K::Alias),
            "template" => Self::Meta(K::Template(T::Legacy)),
            "component_template" => Self::Meta(K::Template(T::Component)),
            "index_template" => Self::Meta(K::Template(T::Composable)),
            "policy" => Self::Meta(K::Policy),
            "script" => Self::Meta(K::Script),
            other => return Err(format!("unknown --type '{other}' (supported: {TYPES})")),
        })
    }

    pub fn name(self) -> &'static str {
        match self {
            DumpType::Data => "data",
            DumpType::Meta(k) => k.type_name(),
        }
    }
}

/// Loose boolean parser: `true|false|1|0|yes|no|on|off` (case-insensitive).
pub fn parse_bool(value: &str) -> Result<bool, String> {
    match value.to_ascii_lowercase().as_str() {
        "true" | "1" | "yes" | "y" | "on" => Ok(true),
        "false" | "0" | "no" | "n" | "off" => Ok(false),
        _ => Err(format!("'{value}' is not a boolean")),
    }
}

fn parse_non_zero_usize(value: &str) -> Result<usize, String> {
    let parsed = value
        .parse::<usize>()
        .map_err(|_| format!("'{value}' is not a valid positive integer"))?;
    if parsed == 0 {
        return Err("value must be greater than 0".to_string());
    }
    Ok(parsed)
}

fn parse_type(value: &str) -> Result<DumpType, String> {
    DumpType::parse(value)
}

#[derive(Parser, Clone)]
#[command(
    name = "searchdump",
    author,
    version,
    about = "Back up, restore and copy Elasticsearch and OpenSearch indices, mappings and settings (elasticdump-compatible CLI)",
    after_long_help = "Exit status: 0 success; 1 error; 2 usage error; 3 finished, but documents were not written;\n\
        4 transient failure (network, HTTP 429/5xx, scroll context lost) after the retries, nothing was\n\
        committed that a rerun would duplicate, so rerunning the same command is safe; 130/143 SIGINT/SIGTERM.\n\n\
        Provided as is, without warranty (MIT license). Verify every backup by restoring it and\n\
        comparing document counts, and test outside production first.",
    allow_negative_numbers = true,
    max_term_width = 100
)]
pub struct Cli {
    // ---------------------------------------------------------------- core
    /// Source: http(s)://[user:pass@]host:port[/base]/index, a file path, or $ for stdin
    #[arg(long, required = true)]
    pub input: String,

    /// Destination: http(s)://[user:pass@]host:port[/base]/index, a file path, or $ for stdout
    #[arg(long, required = true)]
    pub output: String,

    /// Input index[/type], overriding the index in --input
    #[arg(long = "input-index")]
    pub input_index: Option<String>,

    /// Output index[/type], overriding the index in --output
    #[arg(long = "output-index")]
    pub output_index: Option<String>,

    /// What to move: data, mapping, settings, index, analyzer, alias, template,
    /// component_template, index_template, policy or script
    #[arg(long = "type", default_value = "data", value_parser = parse_type)]
    pub r#type: DumpType,

    /// Overwrite the output file if it exists
    #[arg(long, num_args = 0..=1, default_missing_value = "true", value_parser = parse_bool)]
    pub overwrite: Option<bool>,

    /// Extension: allow --input and --output to name the same file or index (refused
    /// otherwise; the output index is compared after --prefix/--suffix, and a host-only
    /// --output is resolved to each input index name after the rename rules and affixes)
    #[arg(long, num_args = 0..=1, default_missing_value = "true", value_parser = parse_bool)]
    pub force: Option<bool>,

    /// Suppress all output except errors
    #[arg(long, num_args = 0..=1, default_missing_value = "true", value_parser = parse_bool)]
    pub quiet: Option<bool>,

    /// Verbose logging, including every HTTP request line
    #[arg(long, num_args = 0..=1, default_missing_value = "true", value_parser = parse_bool)]
    pub debug: Option<bool>,

    /// Accepted for compatibility; logs always go to stderr (false suppresses info logs)
    #[arg(long = "toLog", num_args = 0..=1, default_missing_value = "true", value_parser = parse_bool)]
    pub to_log: Option<bool>,

    /// Output format for file sinks: '' (default, one JSON doc per line) or 'human' (pretty-printed)
    #[arg(long)]
    pub format: Option<String>,

    // ----------------------------------------------------------- retrieval
    /// Documents per page / batch (search size, file batch, bulk size)
    #[arg(long, default_value_t = 100, value_parser = parse_non_zero_usize)]
    pub limit: usize,

    /// Skip the first N documents of the input
    #[arg(long, default_value_t = 0)]
    pub offset: u64,

    /// Maximum number of documents to move (-1 = all)
    #[arg(long, default_value_t = -1)]
    pub size: i64,

    /// Search body as JSON, or @/path/to/file.json
    #[arg(long = "searchBody")]
    pub search_body: Option<String>,

    /// Treat --searchBody as a search template ({"id"|"source":..,"params":..}), rendered with _render/template
    #[arg(long = "searchWithTemplate", num_args = 0..=1, default_missing_value = "true", value_parser = parse_bool)]
    pub search_with_template: Option<bool>,

    /// Write only each document's _source
    #[arg(long = "sourceOnly", num_args = 0..=1, default_missing_value = "true", value_parser = parse_bool)]
    pub source_only: Option<bool>,

    /// Scroll keep-alive
    #[arg(long = "scrollTime", default_value = "10m")]
    pub scroll: String,

    /// Resume an existing scroll by id
    #[arg(long = "scrollId")]
    pub scroll_id: Option<String>,

    /// Accepted for compatibility (requests are always POST)
    #[arg(long = "scroll-with-post", num_args = 0..=1, default_missing_value = "true", value_parser = parse_bool)]
    pub scroll_with_post: Option<bool>,

    /// Accepted and ignored (partial pages are a hard error)
    #[arg(long = "scrollRetryDelay", hide = true)]
    pub scroll_retry_delay: Option<u64>,

    /// Page with search_after instead of scroll (needs a sort with a unique tiebreaker unless --pit)
    #[arg(long = "searchAfter", num_args = 0..=1, default_missing_value = "true", value_parser = parse_bool)]
    pub search_after: Option<bool>,

    /// Use a point-in-time with --searchAfter
    #[arg(long, num_args = 0..=1, default_missing_value = "true", value_parser = parse_bool)]
    pub pit: Option<bool>,

    /// Point-in-time keep-alive
    #[arg(long = "pitKeepAlive", default_value = "5m")]
    pub pit_keep_alive: String,

    /// Extension: scroll or pit (same as --searchAfter --pit)
    #[arg(long = "searchType")]
    pub search_type: Option<SearchType>,

    /// Extra fields copied from each document into the bulk action, comma separated
    #[arg(long = "parseExtraFields")]
    pub parse_extra_fields: Option<String>,

    /// Accepted for compatibility: big integers are always preserved byte-for-byte
    #[arg(long = "support-big-int", num_args = 0..=1, default_missing_value = "true", value_parser = parse_bool)]
    pub support_big_int: Option<bool>,

    /// Accepted for compatibility: big integers are always preserved byte-for-byte
    #[arg(long = "big-int-fields")]
    pub big_int_fields: Option<String>,

    // -------------------------------------------------------------- writing
    /// Bulk action: index, create, update or delete
    #[arg(long = "bulkAction", default_value = "index", value_parser = ["index", "create", "update", "delete"])]
    pub bulk_action: String,

    /// Copy each document's _version into the bulk action
    #[arg(long = "handleVersion", num_args = 0..=1, default_missing_value = "true", value_parser = parse_bool)]
    pub handle_version: Option<bool>,

    /// version_type for --handleVersion (internal, external, external_gte)
    #[arg(long = "versionType")]
    pub version_type: Option<String>,

    /// Do not refresh the index after each bulk request
    #[arg(long = "noRefresh", num_args = 0..=1, default_missing_value = "true", value_parser = parse_bool)]
    pub no_refresh: Option<bool>,

    /// Keep going after a failed write batch
    #[arg(long = "ignore-errors", num_args = 0..=1, default_missing_value = "true", value_parser = parse_bool)]
    pub ignore_errors: Option<bool>,

    /// Log and skip bulk items that Elasticsearch rejects (default true)
    #[arg(long = "ignore-es-write-errors", num_args = 0..=1, default_missing_value = "true", value_parser = parse_bool)]
    pub ignore_es_write_errors: Option<bool>,

    /// Prefix for the output index name
    #[arg(long, default_value = "")]
    pub prefix: String,

    /// Suffix for the output index name
    #[arg(long, default_value = "")]
    pub suffix: String,

    /// Extension: treat "already exists" as success when restoring mapping/settings
    #[arg(long = "skip-existing", num_args = 0..=1, default_missing_value = "true", value_parser = parse_bool)]
    pub skip_existing: Option<bool>,

    /// Extension: split bulk requests above this many bytes (e.g. 50mb)
    #[arg(long = "maxBulkBytes")]
    pub max_bulk_bytes: Option<String>,

    /// Extension: comma-separated settings to drop when restoring settings
    #[arg(long = "excludeSettings")]
    pub exclude_settings: Option<String>,

    // ------------------------------------------------------ metadata types
    /// Skip templates whose name matches --templateRegex on dump (template types)
    #[arg(long = "filterSystemTemplates", num_args = 0..=1, default_missing_value = "true", value_parser = parse_bool)]
    pub filter_system_templates: Option<bool>,

    /// Regex of template names skipped by --filterSystemTemplates (unanchored match, like JavaScript's RegExp.test)
    #[arg(long = "templateRegex", default_value = crate::meta::templates::DEFAULT_TEMPLATE_REGEX)]
    pub template_regex: String,

    /// Extension: skip Elasticsearch-managed templates and ILM policies (_meta.managed: true); default true
    #[arg(long = "filterManaged", num_args = 0..=1, default_missing_value = "true", value_parser = parse_bool)]
    pub filter_managed: Option<bool>,

    /// Extension: --type analyzer on an existing index closes it, updates the analysis and reopens it (takes it offline)
    #[arg(long = "closeIndex", num_args = 0..=1, default_missing_value = "true", value_parser = parse_bool)]
    pub close_index: Option<bool>,

    /// Extension: comma-separated dotted fields dropped from template bodies (e.g. template.lifecycle,allow_auto_create)
    #[arg(long = "excludeTemplateFields")]
    pub exclude_template_fields: Option<String>,

    /// Extension: translate Elasticsearch ILM policies to OpenSearch ISM on restore/copy (off by default)
    #[arg(long = "convertIlmToIsm", num_args = 0..=1, default_missing_value = "true", value_parser = parse_bool)]
    pub convert_ilm_to_ism: Option<bool>,

    /// Extension: with --convertIlmToIsm, drop ILM parts without an ISM equivalent (warned) instead of failing
    #[arg(long = "lenientPolicyConversion", num_args = 0..=1, default_missing_value = "true", value_parser = parse_bool)]
    pub lenient_policy_conversion: Option<bool>,

    /// Extension: metadata file line format: string (elasticdump's JSON string literal, default) or object
    #[arg(long = "metaFormat", default_value = "string", value_parser = ["string", "object"])]
    pub meta_format: String,

    // ------------------------------------------------------------ renaming
    /// Extension: rename index name prefixes, OLD:NEW (repeatable; first matching rule wins)
    #[arg(long = "renamePrefix", action = ArgAction::Append)]
    pub rename_prefix: Vec<String>,

    /// Extension: rename indices with REGEX:REPLACEMENT ($1 / ${name} captures; repeatable; tried after --renamePrefix)
    #[arg(long = "renameIndex", action = ArgAction::Append)]
    pub rename_index: Vec<String>,

    /// Extension: apply the rename rules to alias names too
    #[arg(long = "renameAliases", num_args = 0..=1, default_missing_value = "true", value_parser = parse_bool)]
    pub rename_aliases: Option<bool>,

    /// Extension: apply the rename rules to template index_patterns and ISM ism_template patterns (affects future indices)
    #[arg(long = "renamePatterns", num_args = 0..=1, default_missing_value = "true", value_parser = parse_bool)]
    pub rename_patterns: Option<bool>,

    // ------------------------------------------------- headers and params
    /// JSON object of headers for every request
    #[arg(long)]
    pub headers: Option<String>,

    /// JSON object of headers for input requests
    #[arg(long = "input-headers")]
    pub input_headers: Option<String>,

    /// JSON object of headers for output requests
    #[arg(long = "output-headers")]
    pub output_headers: Option<String>,

    /// JSON object of query parameters for both directions (fallback)
    #[arg(long)]
    pub params: Option<String>,

    /// JSON object of query parameters for input searches
    #[arg(long = "input-params")]
    pub input_params: Option<String>,

    /// JSON object of query parameters for output writes
    #[arg(long = "output-params")]
    pub output_params: Option<String>,

    /// Ask for gzip responses and gzip bulk request bodies
    #[arg(long = "esCompress", num_args = 0..=1, default_missing_value = "true", value_parser = parse_bool)]
    pub es_compress: Option<bool>,

    // ------------------------------------------------------------- files
    /// Split file output into parts of this size (e.g. 10mb)
    #[arg(long = "fileSize")]
    pub file_size: Option<String>,

    /// Split file output into parts of this many documents
    #[arg(long = "maxRows")]
    pub max_rows: Option<u64>,

    /// gzip file output (off by default; gzip is the default codec)
    #[arg(long = "fsCompress", num_args = 0..=1, default_missing_value = "true", value_parser = parse_bool)]
    pub fs_compress: Option<bool>,

    /// Output compression codec: gzip (same as --fsCompress) or zstd
    /// (extension, used only when given explicitly). Input compression is
    /// always auto-detected.
    #[arg(long = "compressionType", value_enum)]
    pub compression_type: Option<CompressionType>,

    /// Compression level: gzip 0-9 (-1 = 6), zstd 1-22 (-1 = 3). gzip 9 and zstd
    /// 19+ print a warning: slow for little or no size gain (README table)
    #[arg(long = "compressionLevel", allow_hyphen_values = true)]
    pub compression_level: Option<i32>,

    // ---------------------------------------- concurrency and throttling
    /// Maximum concurrent write batches
    #[arg(long, default_value_t = 1, value_parser = parse_non_zero_usize)]
    pub concurrency: usize,

    /// Window (ms) for --intervalCap
    #[arg(long = "concurrencyInterval")]
    pub concurrency_interval: Option<u64>,

    /// Maximum requests started per --concurrencyInterval window
    #[arg(long = "intervalCap")]
    pub interval_cap: Option<u64>,

    /// Count in-flight requests against the next --intervalCap window
    #[arg(long = "carryoverConcurrencyCount", num_args = 0..=1, default_missing_value = "true", value_parser = parse_bool)]
    pub carryover_concurrency_count: Option<bool>,

    /// Delay (ms) after each batch is queued for writing
    #[arg(long = "throttleInterval")]
    pub throttle_interval: Option<u64>,

    /// Maximum concurrent HTTP requests / pooled connections per host
    #[arg(long = "maxSockets", value_parser = parse_non_zero_usize)]
    pub max_sockets: Option<usize>,

    /// Milliseconds of server silence (while connecting, waiting for or reading a response)
    /// before a request fails or is retried, like Node's; a slow but progressing response is
    /// not cut off. Overrides --idleTimeout
    #[arg(long)]
    pub timeout: Option<u64>,

    /// Extension: deadline in seconds for a whole request, body included (0 = none)
    #[arg(long = "requestTimeout")]
    pub request_timeout_secs: Option<u64>,

    /// Extension: fail (or retry) a request when the server sends no data for this many
    /// seconds, so a stalled server cannot hang the run forever (0 = never)
    #[arg(long = "idleTimeout", default_value_t = 300)]
    pub idle_timeout_secs: u64,

    /// Extension: largest HTTP response body read into memory, in MB (0 = unlimited); a
    /// bigger page fails with a hint to lower --limit instead of exhausting memory
    #[arg(long = "maxResponseMb", default_value_t = crate::http::DEFAULT_MAX_RESPONSE_MB)]
    pub max_response_mb: usize,

    /// Extension: largest single document read from a file or stdin, in MB (0 = unlimited);
    /// a bigger one stops the run with its position instead of being buffered whole.
    /// The default matches the http.max_content_length default of Elasticsearch/OpenSearch
    #[arg(long = "maxDocMb", default_value_t = 100)]
    pub max_doc_mb: usize,

    /// Extension: let --renamePrefix/--renameIndex rename several source indices to the same
    /// target index (merging them; equal _ids overwrite each other). Refused by default
    #[arg(long = "allowMerge")]
    pub allow_merge: bool,

    /// Extension: dump data from indices whose mapping disables _source (documents then
    /// carry only _id and metadata), with a --searchBody that turns _source off
    /// ("_source": false, a _source filter that leaves none, or stored_fields without
    /// _source), or with documents whose _source is empty ({}), including documents that
    /// are really empty in the source index. By default "_source": false and a filter
    /// that excludes every field ("excludes": ["*"]) are refused before the first search,
    /// and the first hit without _source or with an empty _source stops the dump
    #[arg(long = "allowMissingSource")]
    pub allow_missing_source: bool,

    // ------------------------------------------------------------ retries
    /// Retries per request (0 = 4 implicit retries unless --noImplicitRetry)
    #[arg(long = "retryAttempts", default_value_t = 0)]
    pub retry_attempts: usize,

    /// Delay between retries in ms
    #[arg(long = "retryDelay", default_value_t = 5000)]
    pub retry_delay_ms: u64,

    /// Extension: retry conditions, e.g. network,timeout,5xx,429,408,409
    #[arg(long = "retryOn")]
    pub retry_on: Option<String>,

    /// Extension: fixed or exponential retry delay
    #[arg(long = "retryBackoff", value_enum, default_value_t = RetryBackoffArg::Fixed)]
    pub retry_backoff: RetryBackoffArg,

    /// Extension (L-804b): a scroll dump from a cluster into a file restarts from the start, up to
    /// N times, when its scroll context is lost (node restart, expiry) or a scroll continuation
    /// fails and cannot be retried safely; the staged file is discarded each time. 0 = never
    #[arg(long = "restartOnContextLoss", default_value_t = 0)]
    pub restart_on_context_loss: u32,

    /// Extension: with --retryAttempts 0, do not retry at all
    #[arg(long = "noImplicitRetry", num_args = 0..=1, default_missing_value = "true", value_parser = parse_bool)]
    pub no_implicit_retry: Option<bool>,

    /// Extension: ignore HTTP_PROXY/HTTPS_PROXY/ALL_PROXY from the environment
    #[arg(long = "noProxy", num_args = 0..=1, default_missing_value = "true", value_parser = parse_bool)]
    pub no_proxy: Option<bool>,

    // -------------------------------------------------------- auth and TLS
    /// INI file with user= and password= for both directions; when given it wins over the URL,
    /// the --input-/--output- credential flags and env (L-645b)
    #[arg(long = "httpAuthFile")]
    pub http_auth_file: Option<String>,

    /// Extension (L-809a): INI file with settings and credentials for an s3://, b2:// or sftp://
    /// --input (key=, secret=, endpoint=, region=, key_file=, sse_c_key_file=, ...), like
    /// --httpAuthFile; default $SEARCHDUMP_INPUT_STORAGE_CONFIG
    #[arg(long = "inputStorageConfig")]
    pub input_storage_config: Option<String>,

    /// Extension (L-809a): the same for an s3://, b2:// or sftp:// --output; default
    /// $SEARCHDUMP_OUTPUT_STORAGE_CONFIG
    #[arg(long = "outputStorageConfig")]
    pub output_storage_config: Option<String>,

    /// Basic-auth username for the input (overrides the URL and env; --httpAuthFile wins over it); alias --username
    #[arg(long = "input-username", alias = "username")]
    pub input_username: Option<String>,

    /// Basic-auth password for the input (overrides the URL and env; --httpAuthFile wins over it); alias --password
    #[arg(long = "input-password", alias = "password")]
    pub input_password: Option<String>,

    /// Basic-auth username for the output (overrides the URL and env; --httpAuthFile wins over it)
    #[arg(long = "output-username")]
    pub output_username: Option<String>,

    /// Basic-auth password for the output (overrides the URL and env; --httpAuthFile wins over it)
    #[arg(long = "output-password")]
    pub output_password: Option<String>,

    /// Accepted for compatibility (certificate flags always apply)
    #[arg(long = "tlsAuth", num_args = 0..=1, default_missing_value = "true", value_parser = parse_bool)]
    pub tls_auth: Option<bool>,

    /// CA bundle (PEM) for both directions
    #[arg(long)]
    pub ca: Option<String>,
    /// Client certificate chain (PEM) for both directions
    #[arg(long)]
    pub cert: Option<String>,
    /// Client private key (PEM) for both directions
    #[arg(long)]
    pub key: Option<String>,
    /// Passphrase for an encrypted PKCS#8 --key
    #[arg(long)]
    pub pass: Option<String>,

    #[arg(long = "input-ca")]
    pub input_ca: Option<String>,
    #[arg(long = "input-cert")]
    pub input_cert: Option<String>,
    #[arg(long = "input-key")]
    pub input_key: Option<String>,
    #[arg(long = "input-pass")]
    pub input_pass: Option<String>,
    #[arg(long = "output-ca")]
    pub output_ca: Option<String>,
    #[arg(long = "output-cert")]
    pub output_cert: Option<String>,
    #[arg(long = "output-key")]
    pub output_key: Option<String>,
    #[arg(long = "output-pass")]
    pub output_pass: Option<String>,

    /// Extension: alias of --input-ca
    #[arg(long = "caFile")]
    pub ca_file: Option<String>,

    /// Extension: skip TLS verification on both directions (DANGEROUS)
    #[arg(long, num_args = 0..=1, default_missing_value = "true", value_parser = parse_bool)]
    pub insecure: Option<bool>,
    #[arg(long = "input-insecure", num_args = 0..=1, default_missing_value = "true", value_parser = parse_bool)]
    pub input_insecure: Option<bool>,
    #[arg(long = "output-insecure", num_args = 0..=1, default_missing_value = "true", value_parser = parse_bool)]
    pub output_insecure: Option<bool>,

    // ------------------------------------------------ rs extensions (perf)
    /// Extension: parallel slices for sliced scroll / PIT (0 = unsliced)
    #[arg(long, default_value_t = 0)]
    pub slices: usize,

    /// Extension: JSON extraction workers
    #[arg(long, default_value_t = 4, value_parser = parse_non_zero_usize)]
    pub workers: usize,

    /// Extension: pipeline channel capacity
    #[arg(long = "bufferSize", default_value_t = 16, value_parser = parse_non_zero_usize)]
    pub buffer_size: usize,

    #[arg(long = "retryDelayBase", hide = true)]
    pub retry_delay_base: Option<String>,
    #[arg(long = "customBackoff", hide = true, action = ArgAction::Set, num_args = 0..=1, default_missing_value = "true")]
    pub custom_backoff: Option<String>,
}

/// Manual `Debug`: the CLI holds passwords (`--password`, `--input-pass`,
/// ...), URLs with credentials and headers, none of which may ever be
/// printed by a stray `{:?}` (F-08).
impl std::fmt::Debug for Cli {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Cli").finish_non_exhaustive()
    }
}

fn is(v: Option<bool>) -> bool {
    v.unwrap_or(false)
}

impl Cli {
    pub fn overwrite(&self) -> bool {
        is(self.overwrite)
    }
    pub fn quiet(&self) -> bool {
        is(self.quiet)
    }
    pub fn debug(&self) -> bool {
        is(self.debug)
    }
    pub fn no_proxy(&self) -> bool {
        is(self.no_proxy)
    }
    pub fn es_compress(&self) -> bool {
        is(self.es_compress)
    }
    pub fn source_only(&self) -> bool {
        is(self.source_only)
    }
    pub fn fs_compress(&self) -> bool {
        is(self.fs_compress)
    }
    pub fn no_refresh(&self) -> bool {
        is(self.no_refresh)
    }
    pub fn ignore_errors(&self) -> bool {
        is(self.ignore_errors)
    }
    pub fn ignore_es_write_errors(&self) -> bool {
        self.ignore_es_write_errors.unwrap_or(true)
    }
    pub fn handle_version(&self) -> bool {
        is(self.handle_version)
    }
    pub fn force(&self) -> bool {
        is(self.force)
    }

    pub fn skip_existing(&self) -> bool {
        is(self.skip_existing)
    }
    pub fn no_implicit_retry(&self) -> bool {
        is(self.no_implicit_retry)
    }
    pub fn filter_system_templates(&self) -> bool {
        self.filter_system_templates.unwrap_or(true)
    }
    pub fn filter_managed(&self) -> bool {
        self.filter_managed.unwrap_or(true)
    }
    pub fn search_with_template(&self) -> bool {
        is(self.search_with_template)
    }

    /// Effective retrieval mode: `--searchType` (extension) or Node's
    /// `--searchAfter [--pit]`.
    pub fn effective_search(&self) -> EffectiveSearch {
        match self.search_type {
            Some(SearchType::PointInTime) => EffectiveSearch::Pit,
            Some(SearchType::Scroll) => EffectiveSearch::Scroll,
            Some(SearchType::SearchAfter) => EffectiveSearch::SearchAfter,
            None if is(self.search_after) && is(self.pit) => EffectiveSearch::Pit,
            None if is(self.search_after) => EffectiveSearch::SearchAfter,
            None => {
                if is(self.pit) {
                    log::warn!(
                        "--pit only takes effect together with --searchAfter (elasticdump semantics); using scroll"
                    );
                }
                EffectiveSearch::Scroll
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EffectiveSearch {
    Scroll,
    Pit,
    SearchAfter,
}

// ------------------------------------------------- out-of-scope pre-scan

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum OosAction {
    Warn,
    Error,
}

/// `(flag name or prefix*, action, reason)`.
const OUT_OF_SCOPE: &[(&str, OosAction, &str)] = &[
    (
        "transform",
        OosAction::Error,
        "transforms are not supported",
    ),
    (
        "searchBodyTemplate",
        OosAction::Error,
        "JavaScript search body templates are not supported",
    ),
    (
        "inputTransport",
        OosAction::Error,
        "custom transports are not supported",
    ),
    (
        "outputTransport",
        OosAction::Error,
        "custom transports are not supported",
    ),
    ("delete", OosAction::Error, "--delete is not supported"),
    (
        "delete-with-routing",
        OosAction::Error,
        "--delete is not supported",
    ),
    (
        "aws*",
        OosAction::Error,
        "AWS request signing is not supported",
    ),
    (
        "sessionToken",
        OosAction::Error,
        "AWS request signing is not supported",
    ),
    (
        "inputSocksProxy",
        OosAction::Error,
        "SOCKS proxies are not supported",
    ),
    (
        "inputSocksPort",
        OosAction::Error,
        "SOCKS proxies are not supported",
    ),
    (
        "outputSocksProxy",
        OosAction::Error,
        "SOCKS proxies are not supported",
    ),
    (
        "outputSocksPort",
        OosAction::Error,
        "SOCKS proxies are not supported",
    ),
    (
        "openSearchServerless",
        OosAction::Error,
        "OpenSearch Serverless is not supported",
    ),
    (
        "s3*",
        OosAction::Warn,
        "Node's --s3* flags are not supported; s3:// URLs take their settings from AWS_*/SEARCHDUMP_S3_* variables or --inputStorageConfig/--outputStorageConfig (README, OpenDAL storage)",
    ),
    ("csv*", OosAction::Warn, "CSV is not supported"),
    (
        "heapSnapshot",
        OosAction::Warn,
        "heap snapshots are a Node.js feature",
    ),
    (
        "jsonLines",
        OosAction::Warn,
        "has no effect in elasticdump either",
    ),
    (
        "force-os-version",
        OosAction::Warn,
        "cluster flavor is detected automatically",
    ),
    ("ignoreChildError", OosAction::Warn, "not applicable"),
];

fn oos_lookup(name: &str) -> Option<(OosAction, &'static str)> {
    OUT_OF_SCOPE.iter().find_map(|(pat, action, why)| {
        let hit = match pat.strip_suffix('*') {
            Some(prefix) => name.starts_with(prefix),
            None => name == *pat,
        };
        hit.then_some((*action, *why))
    })
}

/// Remove out-of-scope flags that are safe to ignore (with a warning) and
/// fail on those that would change the result. Returns the filtered argv and
/// the warnings to log once logging is configured.
pub fn prescan_out_of_scope(args: Vec<String>) -> Result<(Vec<String>, Vec<String>), String> {
    let mut out = Vec::with_capacity(args.len());
    let mut warnings = Vec::new();
    let mut iter = args.into_iter().peekable();
    if let Some(bin) = iter.next() {
        out.push(bin);
    }
    while let Some(arg) = iter.next() {
        if arg == "--" {
            out.push(arg);
            out.extend(iter);
            break;
        }
        let Some(body) = arg.strip_prefix("--") else {
            out.push(arg);
            continue;
        };
        let (name, has_inline_value) = match body.split_once('=') {
            Some((n, _)) => (n, true),
            None => (body, false),
        };
        match oos_lookup(name) {
            Some((OosAction::Error, why)) => {
                return Err(format!("--{name}: {why} by searchdump"));
            }
            Some((OosAction::Warn, why)) => {
                warnings.push(format!("Ignoring --{name}: {why} by searchdump"));
                if !has_inline_value
                    && let Some(next) = iter.peek()
                    && !next.starts_with("--")
                {
                    iter.next();
                }
            }
            None => out.push(arg),
        }
    }
    Ok((out, warnings))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(extra: &[&str]) -> Result<Cli, clap::Error> {
        let mut v = vec![
            "searchdump",
            "--input",
            "http://localhost:9200/i",
            "--output",
            "$",
        ];
        v.extend_from_slice(extra);
        Cli::try_parse_from(v)
    }

    /// F-08 / SUP-018: no `{:?}` of a config struct may print a secret.
    #[test]
    fn debug_never_prints_secrets() {
        let c = parse(&[
            "--password=S3cr3t-pw",
            "--output-password=S3cr3t-opw",
            "--pass=S3cr3t-pass",
            "--input-pass=S3cr3t-in",
            "--output-pass=S3cr3t-out",
        ])
        .unwrap();
        assert!(!format!("{c:?}").contains("S3cr3t"));
        let tls = crate::http::tls::TlsOptions {
            pass: Some("S3cr3t-tls".into()),
            ..Default::default()
        };
        let d = format!("{tls:?}");
        assert!(!d.contains("S3cr3t") && d.contains("***"), "{d}");
        let ep = crate::endpoint::EsEndpoint {
            base: url::Url::parse("http://h:9200/").unwrap(),
            index: None,
            doc_type: None,
            username: Some("u".into()),
            password: Some("S3cr3t-ep".into()),
        };
        let d = format!("{ep:?}");
        assert!(!d.contains("S3cr3t") && d.contains("***"), "{d}");
    }

    #[test]
    fn defaults_match_elasticdump() {
        let c = parse(&[]).unwrap();
        assert_eq!(c.limit, 100);
        assert_eq!(c.size, -1);
        assert_eq!(c.offset, 0);
        assert_eq!(c.pit_keep_alive, "5m");
        assert_eq!(c.scroll, "10m");
        assert_eq!(c.bulk_action, "index");
        assert_eq!(c.retry_attempts, 0);
        assert_eq!(c.retry_delay_ms, 5000);
        assert_eq!(c.concurrency, 1);
        assert!(c.ignore_es_write_errors());
        assert!(!c.ignore_errors());
        assert_eq!(c.r#type, DumpType::Data);
        assert_eq!(c.effective_search(), EffectiveSearch::Scroll);
    }

    #[test]
    fn compression_flags() {
        let c = parse(&["--compressionType", "zstd", "--compressionLevel", "-1"]).unwrap();
        assert_eq!(c.compression_type, Some(CompressionType::Zstd));
        assert_eq!(c.compression_level, Some(-1));
        let c = parse(&["--compressionType=gzip"]).unwrap();
        assert_eq!(c.compression_type, Some(CompressionType::Gzip));
        assert!(parse(&["--compressionType", "brotli"]).is_err());
        assert_eq!(parse(&[]).unwrap().compression_type, None);
    }

    #[test]
    fn rejects_zero_limit_workers_buffer() {
        assert!(parse(&["--limit", "0"]).is_err());
        assert!(parse(&["--workers", "0"]).is_err());
        assert!(parse(&["--bufferSize", "0"]).is_err());
    }

    #[test]
    fn booleans_accept_optional_values() {
        let c = parse(&["--fsCompress", "--noRefresh", "true", "--overwrite=false"]).unwrap();
        assert!(c.fs_compress());
        assert!(c.no_refresh());
        assert!(!c.overwrite());
        let c = parse(&["--ignore-es-write-errors", "false"]).unwrap();
        assert!(!c.ignore_es_write_errors());
        let c = parse(&["--quiet", "--debug"]).unwrap();
        assert!(c.quiet() && c.debug());
    }

    #[test]
    fn equals_syntax_and_negative_numbers() {
        let c = parse(&["--limit=500", "--size=-1", "--searchBody={\"query\":{}}"]).unwrap();
        assert_eq!(c.limit, 500);
        assert_eq!(c.size, -1);
        assert!(c.search_body.is_some());
    }

    #[test]
    fn type_validation() {
        use crate::meta::{MetaKind, TemplateKind};
        assert_eq!(
            parse(&["--type", "mapping"]).unwrap().r#type,
            DumpType::Meta(MetaKind::Mapping)
        );
        assert_eq!(
            parse(&["--type=settings"]).unwrap().r#type,
            DumpType::Meta(MetaKind::Settings)
        );
        for t in TYPES.split(", ") {
            assert_eq!(parse(&["--type", t]).unwrap().r#type.name(), t);
        }
        assert_eq!(
            parse(&["--type", "index_template"]).unwrap().r#type,
            DumpType::Meta(MetaKind::Template(TemplateKind::Composable))
        );
        let err = parse(&["--type", "bogus"]).unwrap_err().to_string();
        assert!(err.contains("script"), "{err}");
    }

    #[test]
    fn bulk_action_validation() {
        assert!(parse(&["--bulkAction", "create"]).is_ok());
        assert!(parse(&["--bulkAction", "upsert"]).is_err());
    }

    #[test]
    fn search_mode_mapping() {
        assert_eq!(
            parse(&["--searchAfter", "--pit"])
                .unwrap()
                .effective_search(),
            EffectiveSearch::Pit
        );
        assert_eq!(
            parse(&["--searchAfter"]).unwrap().effective_search(),
            EffectiveSearch::SearchAfter
        );
        assert_eq!(
            parse(&["--searchType", "pit"]).unwrap().effective_search(),
            EffectiveSearch::Pit
        );
    }

    #[test]
    fn prescan_warns_strips_and_errors() {
        let argv = |v: &[&str]| v.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        let (out, warns) = prescan_out_of_scope(argv(&[
            "bin",
            "--input",
            "a",
            "--s3Region",
            "us-east-1",
            "--csvDelimiter=;",
            "--heapSnapshot",
            "--output",
            "b",
        ]))
        .unwrap();
        assert_eq!(out, argv(&["bin", "--input", "a", "--output", "b"]));
        assert_eq!(warns.len(), 3);

        let err = prescan_out_of_scope(argv(&["bin", "--transform", "x"])).unwrap_err();
        assert!(err.contains("--transform"));
        assert!(prescan_out_of_scope(argv(&["bin", "--awsChain"])).is_err());
        assert!(prescan_out_of_scope(argv(&["bin", "--delete=true"])).is_err());
        // in-scope flags sharing a prefix are untouched
        let (out, _) = prescan_out_of_scope(argv(&["bin", "--searchBody", "{}"])).unwrap();
        assert_eq!(out.len(), 3);
    }
}
