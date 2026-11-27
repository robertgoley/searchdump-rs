//! The `multielasticdump.json` manifest.

use crate::child::ChildRef;
use crate::cli::Compression;
use crate::cli::Layout;
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

pub(crate) const MANIFEST: &str = "multielasticdump.json";

// ----------------------------------------------------------------------------- manifest

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub(crate) struct IndexEntry {
    /// Directory relative to the dump root ("" for flat).
    pub(crate) dir: String,
    /// "datastream" for a data stream; absent for an index.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) kind: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) group: Option<String>,
    pub(crate) types: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) data_file: Option<String>,
    /// Split dumps: the part files, in load order.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub(crate) data_parts: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) records: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) source_count: Option<u64>,
    pub(crate) status: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) error: Option<String>,
    /// 1.2: the data run's page / bulk sizes and why (`default|large-docs|budget|off|fallback|user`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) sizing: Option<serde_json::Value>,
    /// L4-002: the child run of each step (details in the log dir's `children.json`).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub(crate) children: Vec<ChildRef>,
}

/// One cluster-scoped file (`_cluster/TYPE.json`).
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub(crate) struct ClusterEntry {
    pub(crate) file: String,
    pub(crate) status: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) error: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub(crate) struct Manifest {
    pub(crate) tool: String,
    pub(crate) version: String,
    #[serde(default)]
    pub(crate) source: String,
    #[serde(default)]
    pub(crate) layout: Option<Layout>,
    #[serde(default)]
    pub(crate) compression: Option<Compression>,
    /// Written with --nodeCompat (Node multielasticdump file names).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub(crate) node_compat: bool,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub(crate) cluster: BTreeMap<String, ClusterEntry>,
    pub(crate) indices: BTreeMap<String, IndexEntry>,
    /// 1.2: the run's `--serverLimits` mode, source, node limits and budget.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) server_limits: Option<serde_json::Value>,
}

impl Manifest {
    pub(crate) fn load(root: &Path) -> Result<Option<Manifest>> {
        let p = root.join(MANIFEST);
        if !p.exists() {
            return Ok(None);
        }
        let text = fs::read_to_string(&p).with_context(|| format!("read {}", p.display()))?;
        Ok(Some(
            serde_json::from_str(&text).with_context(|| format!("parse {}", p.display()))?,
        ))
    }

    pub(crate) fn save(&self, root: &Path) -> Result<()> {
        let tmp = root.join(format!(".{MANIFEST}.tmp"));
        fs::write(&tmp, serde_json::to_vec_pretty(self)?)?;
        fs::rename(&tmp, root.join(MANIFEST))?;
        Ok(())
    }
}
