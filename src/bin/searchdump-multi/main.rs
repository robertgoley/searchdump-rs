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
//!
//! TLS (1.4, L4-001): `--ca`, `--cert`/`--key` and `--insecure` apply to the cluster and go to
//! every child as `--input-*` (dump) or `--output-*` (load). The key passphrase (`--pass`, or
//! `$SEARCHDUMP_TLS_PASS`) reaches children only through `$SEARCHDUMP_{INPUT,OUTPUT}_TLS_PASS`,
//! never their command line. Per-child ids (L4-002): every child run gets a UUID in
//! `$SEARCHDUMP_CHILD_ID`, printed by the parent, tagged on the child's log lines and recorded in
//! its stats, the manifest and `LOGDIR/children.json`.
//!
//! Module map (1.4 split, L4-008..L4-028; no behaviour change):
//! * [`cli`]: `Args` and its value enums; [`tls`]: cluster TLS options and the passphrase
//!   (L4-001, L4-006); [`cluster`]: the parent's own requests and retries.
//! * [`selection`], [`types`]: what is selected and which item types run; [`files`],
//!   [`manifest`], [`discover`]: file names, the manifest and finding a dump's parts.
//! * [`sizing`], [`planner`], [`flags`]: `--serverLimits` sizing and the flags every child gets.
//! * [`child`]: probing, running and counting child `searchdump` processes (L4-002 ids).
//! * [`ctx`]: the shared run context; [`dump`], [`load`]: one index or the cluster-wide types.
//! * [`storage`]: remote storage URLs around [`run_local`], which orchestrates a local run
//!   (set-up, dump or load plan, dry run, report).
#![forbid(unsafe_code)]
#![warn(unreachable_pub)]

use crate::cli::Args;
use crate::storage::run;
use clap::Parser;

mod child;
mod cli;
mod cluster;
mod ctx;
mod discover;
mod dump;
mod files;
mod flags;
mod load;
mod manifest;
mod planner;
mod run_local;
mod selection;
mod sizing;
mod storage;
mod tls;
mod types;

fn main() {
    let args = Args::parse();
    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("tokio runtime");
    let code = match rt.block_on(run(args)) {
        Ok(c) => c,
        Err(e) => {
            // L4-001: never show a registered secret (the key passphrase) in an error
            eprintln!(
                "searchdump-multi: {}",
                searchdump_rs::secrets::scrub(&format!("{e:#}"))
            );
            2
        }
    };
    std::process::exit(code);
}
