//! The `multielasticdump.json` manifest.

use crate::*;

pub const MANIFEST: &str = "multielasticdump.json";

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
    /// L4-002: the child run of each step (details in the log dir's `children.json`).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub children: Vec<ChildRef>,
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
