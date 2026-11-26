//! Command line: `Args` and its value enums.

use crate::*;

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
    pub(crate) fn flags(self) -> &'static [&'static str] {
        match self {
            Compression::Gzip => &["--fsCompress"],
            Compression::Zstd1 => &["--compressionType=zstd", "--compressionLevel=1"],
            Compression::Zstd3 => &["--compressionType=zstd", "--compressionLevel=3"],
            Compression::None => &[],
        }
    }
    /// The codec of an existing data file, from its name.
    // only the tests call it (a crate-root `pub` item was exempt from dead_code)
    #[allow(dead_code)]
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
    /// 1.4: CA bundle (PEM) for the cluster; passed to searchdump as --input-ca (dump) or
    /// --output-ca (load)
    #[arg(long)]
    pub ca: Option<String>,
    /// 1.4: client certificate chain (PEM) for the cluster (with --key); passed to searchdump as
    /// --input-cert / --output-cert, with --tlsAuth
    #[arg(long)]
    pub cert: Option<String>,
    /// 1.4: client private key (PEM) for --cert; passed as --input-key / --output-key
    #[arg(long)]
    pub key: Option<String>,
    /// 1.4: passphrase of an encrypted PKCS#8 --key. Shows in `ps` for this process; prefer
    /// $SEARCHDUMP_TLS_PASS. Children never get it on their command line, only as
    /// $SEARCHDUMP_INPUT_TLS_PASS (dump) / $SEARCHDUMP_OUTPUT_TLS_PASS (load)
    #[arg(long, value_parser = parse_secret)]
    pub pass: Option<Secret>,
    /// --ca for a dump's cluster (searchdump's name; --ca wins)
    #[arg(long = "input-ca", hide_short_help = true)]
    pub input_ca: Option<String>,
    /// --cert for a dump's cluster
    #[arg(long = "input-cert", hide_short_help = true)]
    pub input_cert: Option<String>,
    /// --key for a dump's cluster
    #[arg(long = "input-key", hide_short_help = true)]
    pub input_key: Option<String>,
    /// --pass for a dump's cluster (default $SEARCHDUMP_INPUT_TLS_PASS)
    #[arg(long = "input-pass", hide_short_help = true, value_parser = parse_secret)]
    pub input_pass: Option<Secret>,
    /// --ca for a load's cluster (searchdump's name; --ca wins)
    #[arg(long = "output-ca", hide_short_help = true)]
    pub output_ca: Option<String>,
    /// --cert for a load's cluster
    #[arg(long = "output-cert", hide_short_help = true)]
    pub output_cert: Option<String>,
    /// --key for a load's cluster
    #[arg(long = "output-key", hide_short_help = true)]
    pub output_key: Option<String>,
    /// --pass for a load's cluster (default $SEARCHDUMP_OUTPUT_TLS_PASS)
    #[arg(long = "output-pass", hide_short_help = true, value_parser = parse_secret)]
    pub output_pass: Option<Secret>,
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
    /// 1.4: cap each dump page at about this many bytes (e.g. 8mb): pages are sized from each
    /// index's average document within the --serverLimits budget (or alone with off), and
    /// every searchdump child gets --pageBytes so a page that still comes back bigger shrinks
    /// the next ones. Dump only
    #[arg(long = "pageBytes", value_parser = searchdump_rs::cli::parse_page_bytes)]
    pub page_bytes: Option<u64>,
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
