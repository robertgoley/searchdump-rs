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
use http::Method;
use regex::Regex;
use searchdump_rs::endpoint::read_auth_file;
use searchdump_rs::http::gate::RequestGate;
use searchdump_rs::http::retry::error_chain;
use searchdump_rs::http::tls::TlsOptions;
use searchdump_rs::http::{Body, ClientOptions, EsClient};
use serde::{Deserialize, Serialize};
use tokio_util::sync::CancellationToken;
use url::Url;

// split (L4-S03..): one module per concern; the glob imports are narrowed in step 15
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
pub mod sizing;
mod storage;
mod tls;
mod types;
use child::*;
use cli::*;
use cluster::*;
use ctx::*;
use discover::*;
use dump::*;
use files::*;
use flags::*;
use load::*;
use manifest::*;
use planner::*;
use run_local::*;
use selection::*;
use storage::*;
use tls::*;
use types::*;

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
