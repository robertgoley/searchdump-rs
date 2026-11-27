//! `run_local` set-up (split step 11): validation, types, selection, log directory, TLS, Ctrl-C and the
//! child runner, in the order `run_local` has always run them.

use crate::child::CHILD_GRACE;
use crate::child::Runner;
use crate::child::accepts_server_limits;
use crate::child::accepts_stats_file;
use crate::child::find_bin;
use crate::child::takes_stats_file;
use crate::cli::Args;
use crate::cli::Compression;
use crate::cli::Direction;
use crate::cli::Layout;
use crate::cli::MatchType;
use crate::flags::DATA_FLAGS;
use crate::flags::merge_data_flags;
use crate::selection::Selector;
use crate::storage::chrono_stamp;
use crate::tls::ClusterTls;
use crate::tls::Side;
use crate::types::CLUSTER_TYPES;
use crate::types::Types;
use crate::types::node_type_flags;
use crate::types::resolve_types;
use anyhow::{Result, bail};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use tokio_util::sync::CancellationToken;
use url::Url;

/// Usage errors of a local run (exit 2 through `main`), checked before anything else.
pub(crate) fn validate_local(args: &Args) -> Result<()> {
    if args.direction == Direction::Load && (args.file_size.is_some() || args.max_rows.is_some()) {
        bail!("--fileSize/--maxRows apply to dumps; load finds split parts on its own");
    }
    if args.direction != Direction::Dump && args.page_bytes.is_some() {
        bail!("--pageBytes applies to dumps (it caps the search pages read from the cluster)");
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
    Ok(())
}

/// The per-index and cluster-wide types of this run (defaults per direction, Node type flags, the
/// `--nodeCompat` template shuffle and its warnings).
pub(crate) fn run_types(args: &Args) -> Result<Types> {
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
    let (want, ignore) = node_type_flags(args, &default_types);
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
    Ok(types)
}

/// `--match` / `--exclude` / `--includeHidden`, and the `--layout group` capture check.
pub(crate) fn selector(args: &Args) -> Result<Selector> {
    let sel = Selector::new(&args.match_re, args.exclude.as_deref(), args.include_hidden)?;
    if args.layout == Layout::Group && !sel.has_group() {
        bail!("--layout group needs a capture group in --match, e.g. '^logs-(?P<group>[a-z]+)-'");
    }
    Ok(sel)
}

/// The warning for a password in a cluster URL (it reaches every child's command line).
pub(crate) fn warn_url_credentials(args: &Args) {
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
}

/// `--logDir`, or a new name under the temp directory.
pub(crate) fn log_dir(args: &Args) -> PathBuf {
    match &args.log_dir {
        Some(d) => d.clone(),
        None => std::env::temp_dir().join(format!(
            "searchdump-multi-{}-{}",
            std::process::id(),
            chrono_stamp()
        )),
    }
}

/// L4-001: the cluster side and its TLS settings (with the NODE_TLS warning and the passphrase
/// registered for scrubbing).
pub(crate) fn cluster_tls(args: &Args) -> Result<(Side, ClusterTls)> {
    // L4-001: the cluster's TLS settings, for the parent and every child
    let side = if args.direction == Direction::Dump {
        Side::Input
    } else {
        Side::Output
    };
    let tls = ClusterTls::resolve(args, side, &|k| std::env::var(k).ok())?;
    if tls.node_insecure {
        // the same words searchdump prints
        eprintln!(
            "warning: NODE_TLS_REJECT_UNAUTHORIZED=0: TLS certificate verification is disabled"
        );
    }
    if let Some(p) = &tls.pass {
        searchdump_rs::secrets::register(&p.0);
    }
    Ok((side, tls))
}

/// 1.1-8: Ctrl-C sets `cancelled` and cancels the token (running children get SIGTERM, then
/// SIGKILL after the grace time; no new ones start).
pub(crate) struct Interrupt {
    pub(crate) cancelled: Arc<AtomicBool>,
    pub(crate) cancel: CancellationToken,
}

impl Interrupt {
    /// Installs the Ctrl-C task. It runs before the binary probes, as it always has.
    pub(crate) fn install() -> Self {
        let interrupt = Interrupt {
            cancelled: Arc::new(AtomicBool::new(false)),
            cancel: CancellationToken::new(),
        };
        {
            let c = interrupt.cancelled.clone();
            let token = interrupt.cancel.clone();
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
        interrupt
    }

    pub(crate) fn cancelled(&self) -> bool {
        self.cancelled.load(Ordering::SeqCst)
    }
}

/// The child runner: TLS/auth flags, default data flags, the searchdump binary and what it accepts.
pub(crate) fn build_runner(
    args: &Args,
    tls: &ClusterTls,
    side: Side,
    log_dir: &Path,
    interrupt: &Interrupt,
) -> Result<Runner> {
    let mut common = tls.child_flags(side);
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
    let bin = find_bin(args.elasticdump_bin.as_deref());
    let stats = takes_stats_file(&bin) && accepts_stats_file(&bin);
    let server_limits = takes_stats_file(&bin) && accepts_server_limits(&bin);
    if tls.pass.is_some() && !takes_stats_file(&bin) {
        bail!(
            "a key passphrase is passed to searchdump through ${}, which {} does not read; use searchdump, or an unencrypted key",
            searchdump_rs::config::tls_pass_env(side),
            bin.display()
        );
    }
    if args.page_bytes.is_some() && !server_limits {
        eprintln!(
            "warning: {} does not take --pageBytes; it only sizes the pages from the index statistics",
            bin.display()
        );
    }
    let (child_env, child_env_remove) = tls.child_env(side);
    Ok(Runner {
        runs: Mutex::new(vec![]),
        child_env,
        child_env_remove,
        bin,
        log_dir: log_dir.to_path_buf(),
        common,
        data_flags,
        cancelled: interrupt.cancelled.clone(),
        cancel: interrupt.cancel.clone(),
        grace: CHILD_GRACE,
        stats,
        server_limits,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;

    fn args(extra: &[&str]) -> Args {
        let mut v = vec!["m", "--input=http://h:9200", "--output=/tmp/x"];
        v.extend_from_slice(extra);
        Args::try_parse_from(v).unwrap()
    }

    #[test]
    fn validate_local_messages() {
        let err = |a: &[&str]| format!("{:#}", validate_local(&args(a)).unwrap_err());
        assert!(validate_local(&args(&[])).is_ok());
        assert_eq!(
            err(&["--direction=load", "--maxRows=5"]),
            "--fileSize/--maxRows apply to dumps; load finds split parts on its own"
        );
        assert_eq!(
            err(&["--direction=load", "--pageBytes=1mb"]),
            "--pageBytes applies to dumps (it caps the search pages read from the cluster)"
        );
        assert_eq!(
            err(&["--direction=load", "--nodeCompat"]),
            "--nodeCompat applies to dumps; load reads Node-named directories on its own"
        );
        assert!(
            err(&["--nodeCompat", "--layout=index"])
                .starts_with("--nodeCompat needs --layout flat")
        );
        assert!(err(&["--nodeCompat", "--compression=zstd3"]).contains("not zstd"));
        assert!(
            err(&["--nodeCompat", "--fileSize=1mb"]).starts_with("--nodeCompat cannot split data")
        );
        assert!(err(&["--nodeCompat", "--matchType=datastream"]).contains("data streams"));
    }

    #[test]
    fn node_compat_moves_the_template_type_to_the_index() {
        let t = run_types(&args(&["--nodeCompat"])).unwrap();
        assert_eq!(t.index, vec!["template", "settings", "mapping", "data"]);
        assert!(t.cluster.is_empty());
        let t = run_types(&args(&[])).unwrap();
        assert_eq!(t.index, vec!["settings", "mapping", "data"]);
    }
}
