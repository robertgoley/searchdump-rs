//! Slice planning (1.1-4): cap `--slices` to the primary-shard count and validate
//! `--sliceField`.
//!
//! A sliced scroll is split per shard first. When there are more slices than shards, each
//! extra slice is a per-document filter (a hash of `_id` by default) that the engine has to
//! evaluate over the whole shard on the first pages: on a one-shard Elasticsearch 7.10 index,
//! 2 scroll slices took 2.1-2.2 s against 0.8-0.9 s unsliced. The cap uses the smallest
//! primary-shard count of the indices the input names and applies to scroll only (sliced PIT
//! was measured faster with more slices than shards).
//!
//! **1.1-4a (2026-11-14): `--sliceCap=auto`, the default, caps only on Elasticsearch 7.10**
//! ([`auto_applies`]). The 1.1 point-in-time perf run (scripts/perf/results/pit-1.1.md) showed
//! the cap 63% faster for `dump-scroll-s2` on ES 7.10 but 45-59% slower sliced scrolls on
//! ES 8.19 and 62-101% slower on OpenSearch 3, so other engines and unknown versions keep
//! `--slices` as given (the pre-1.1-4 behaviour). `--sliceCap=min`/`total` still force the cap
//! on any engine, with the basis below.
//!
//! Which shard count is the limit depends on how the engine assigns slices to shards
//! (`--sliceCap=auto` picks it from the detected cluster version; on ES 7.10 that is the
//! smallest count):
//!
//! - Elasticsearch 5.0-7.10 and every OpenSearch (1.x-3.x) assign slices per index, so every
//!   index gets all the slices: the smallest primary-shard count is the limit ([`CapBasis::Min`]).
//! - Elasticsearch 7.11-9.4 assign slices across all the shards of the request
//!   (elastic/elasticsearch#65706): the total primary-shard count is the limit
//!   ([`CapBasis::Total`]).
//! - Elasticsearch 9.5+ keeps that only for point-in-time searches; a scroll on indices
//!   created on 9.5+ is back to per-shard filtering (elastic/elasticsearch#148664), so scroll
//!   uses the smallest count again.
//!
//! For the basis alone, an unknown version uses the smallest count; but `auto` does not cap
//! at all when the version is unknown (1.1-4a).

use anyhow::{Result, anyhow, bail};
use serde_json::Value;

use crate::http::{ClusterInfo, Flavor};

/// `--sliceCap`: how scroll `--slices` is limited by the primary-shard count.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SliceCap {
    /// Pick [`CapBasis::Min`] or [`CapBasis::Total`] from the cluster version (default;
    /// `true` is an alias).
    Auto,
    /// Always the smallest primary-shard count of the input indices.
    Min,
    /// Always the total primary-shard count of the input indices.
    Total,
    /// No cap: `--slices` as given (`false`).
    Off,
}

impl SliceCap {
    pub fn name(self) -> &'static str {
        match self {
            SliceCap::Auto => "auto",
            SliceCap::Min => "min",
            SliceCap::Total => "total",
            SliceCap::Off => "false",
        }
    }
}

/// `--sliceCap` value parser: `auto` (or a true boolean), `min`, `total`, or a false boolean.
pub fn parse_slice_cap(value: &str) -> Result<SliceCap, String> {
    match value.trim().to_ascii_lowercase().as_str() {
        "auto" | "true" | "1" | "yes" | "y" | "on" => Ok(SliceCap::Auto),
        "min" => Ok(SliceCap::Min),
        "total" => Ok(SliceCap::Total),
        "false" | "0" | "no" | "n" | "off" => Ok(SliceCap::Off),
        _ => Err(format!(
            "'{value}' is not a --sliceCap value (auto, min, total or false)"
        )),
    }
}

/// Which primary-shard count limits the slices.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CapBasis {
    /// The smallest count of the input indices (slices assigned per index).
    Min,
    /// The sum over the input indices (slices assigned across all shards of the request).
    Total,
}

/// Smallest and total primary-shard count of the input indices.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ShardCounts {
    pub min: usize,
    pub total: usize,
    pub indices: usize,
}

impl ShardCounts {
    pub fn get(self, basis: CapBasis) -> usize {
        match basis {
            CapBasis::Min => self.min,
            CapBasis::Total => self.total,
        }
    }
}

/// The basis `--sliceCap=auto` uses on `info` (`None`: version unknown) for a scroll
/// (`pit == false`) or point-in-time search. See the module docs for the rules.
pub fn auto_basis(info: Option<&ClusterInfo>, pit: bool) -> CapBasis {
    let Some(info) = info else {
        return CapBasis::Min;
    };
    let v = (info.version.major, info.version.minor);
    match info.flavor {
        Flavor::OpenSearch => CapBasis::Min,
        Flavor::Elastic if v < (7, 11) => CapBasis::Min,
        Flavor::Elastic if v < (9, 5) => CapBasis::Total,
        Flavor::Elastic if pit => CapBasis::Total,
        Flavor::Elastic => CapBasis::Min,
    }
}

/// The basis for `mode`. `info` is only consulted for [`SliceCap::Auto`]; callers do not cap
/// at all for [`SliceCap::Off`] (it maps to `Min` here only to keep the function total).
pub fn select_basis(mode: SliceCap, info: Option<&ClusterInfo>, pit: bool) -> CapBasis {
    match mode {
        SliceCap::Min | SliceCap::Off => CapBasis::Min,
        SliceCap::Total => CapBasis::Total,
        SliceCap::Auto => auto_basis(info, pit),
    }
}

/// Whether `--sliceCap=auto` (the default) caps at all on `info` (1.1-4a, 2026-11-14): only on
/// Elasticsearch 7.10.x, where the cap was measured to help (`dump-scroll-s2` 63% faster). On
/// Elasticsearch 8.19 and OpenSearch 3 it made sliced scrolls 45-101% slower
/// (scripts/perf/results/pit-1.1.md), so every other engine and an unknown version keep
/// `--slices` as given, as before 1.1-4. `--sliceCap=min`/`total` still cap on any engine.
pub fn auto_applies(info: Option<&ClusterInfo>) -> bool {
    info.is_some_and(|i| {
        i.flavor == Flavor::Elastic && i.version.major == 7 && i.version.minor == 10
    })
}

/// Numeric field types with doc values that the slice `field` option accepts.
const SLICE_FIELD_TYPES: &[&str] = &[
    "long",
    "integer",
    "short",
    "byte",
    "double",
    "float",
    "half_float",
    "scaled_float",
    "unsigned_long",
    "date",
    "date_nanos",
];

/// Smallest and total `index.number_of_shards` in a
/// `GET /<index>/_settings/index.number_of_shards?flat_settings=true` response
/// (`{"idx":{"settings":{"index.number_of_shards":"3"}},…}`). `None` when the response names
/// no index or a value is unreadable.
pub fn primary_shard_counts(settings: &Value) -> Option<ShardCounts> {
    let indices = settings.as_object()?;
    let mut min: Option<usize> = None;
    let mut total: usize = 0;
    for v in indices.values() {
        let raw = v.get("settings")?.get("index.number_of_shards")?;
        let n = match raw {
            Value::String(s) => s.parse::<usize>().ok()?,
            Value::Number(n) => usize::try_from(n.as_u64()?).ok()?,
            _ => return None,
        };
        if n == 0 {
            return None;
        }
        min = Some(min.map_or(n, |m: usize| m.min(n)));
        total = total.saturating_add(n);
    }
    Some(ShardCounts {
        min: min?,
        total,
        indices: indices.len(),
    })
}

/// Smallest `index.number_of_shards` of a `_settings` response (see
/// [`primary_shard_counts`]).
pub fn min_primary_shards(settings: &Value) -> Option<usize> {
    primary_shard_counts(settings).map(|c| c.min)
}

/// The slice count to use for `requested` slices when the limit is `shards`, and the warning
/// to print when it was lowered. Capping to 1 means an unsliced dump (slice `max` must be at
/// least 2).
pub fn cap_slices(requested: usize, shards: usize) -> (usize, Option<String>) {
    cap_slices_with(
        requested,
        shards,
        &format!("the {shards} primary shard(s) of the input index"),
    )
}

/// [`cap_slices`] on `counts` with `basis`; the warning names the basis, and for a per-index
/// cap over indices with different counts it also gives the total.
pub fn cap_slices_by(
    requested: usize,
    counts: ShardCounts,
    basis: CapBasis,
) -> (usize, Option<String>) {
    if counts.indices <= 1 || counts.min == counts.total {
        return cap_slices(requested, counts.get(basis));
    }
    let what = match basis {
        CapBasis::Min => format!(
            "the smallest primary-shard count ({}) of the {} input indices (this engine gives every index all the slices; {} primary shards in total, --sliceCap=total to use that)",
            counts.min, counts.indices, counts.total
        ),
        CapBasis::Total => format!(
            "the {} primary shards of the {} input indices in total",
            counts.total, counts.indices
        ),
    };
    cap_slices_with(requested, counts.get(basis), &what)
}

fn cap_slices_with(requested: usize, shards: usize, what_limit: &str) -> (usize, Option<String>) {
    if requested <= 1 || shards == 0 || requested <= shards {
        return (requested, None);
    }
    let capped = if shards <= 1 { 0 } else { shards };
    let what = if capped == 0 {
        "running unsliced".to_string()
    } else {
        format!("using --slices {capped}")
    };
    (
        capped,
        Some(format!(
            "--slices {requested} is more than {what_limit}; extra scroll slices make the engine filter every document of a shard per slice, which is slower than fewer slices. {what} (pass --sliceCap=false to keep {requested}, or --sliceField with a numeric doc-values field)"
        )),
    )
}

/// Check a `GET /<index>/_mapping/field/<field>` response: every index must map `field` as a
/// numeric or date type with doc values. Handles typeless (7+, OpenSearch) and typed (6.x)
/// response shapes.
pub fn check_slice_field(mapping: &Value, field: &str) -> Result<()> {
    let indices = mapping
        .as_object()
        .ok_or_else(|| anyhow!("unexpected _mapping/field response"))?;
    if indices.is_empty() {
        bail!("--sliceField {field}: no index matched the input");
    }
    for (index, v) in indices {
        let leaf =
            find_field(v.get("mappings").unwrap_or(&Value::Null), field).ok_or_else(|| {
                anyhow!("--sliceField {field}: index {index} has no field {field} in its mapping")
            })?;
        let ty = leaf.get("type").and_then(Value::as_str).unwrap_or("object");
        if !SLICE_FIELD_TYPES.contains(&ty) {
            bail!(
                "--sliceField {field}: in index {index} it is a {ty} field; slicing needs a numeric or date field with doc values ({})",
                SLICE_FIELD_TYPES.join(", ")
            );
        }
        if leaf.get("doc_values").and_then(Value::as_bool) == Some(false) {
            bail!(
                "--sliceField {field}: in index {index} it has doc_values disabled; slicing needs doc values"
            );
        }
    }
    Ok(())
}

/// The leaf mapping (`{"type":…}`) of `field` under an index's `mappings`: typeless
/// `{field:{full_name,mapping:{leaf:{…}}}}` or typed `{type:{field:{…}}}`.
fn find_field<'a>(mappings: &'a Value, field: &str) -> Option<&'a Value> {
    let obj = mappings.as_object()?;
    if let Some(f) = obj.get(field)
        && f.get("full_name").is_some()
    {
        let m = f.get("mapping")?.as_object()?;
        return m.values().next();
    }
    // typed (ES 6.x): one more level
    obj.values().find_map(|t| {
        let f = t.get(field)?;
        f.get("full_name")?;
        f.get("mapping")?.as_object()?.values().next()
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn primary_shards_minimum_over_indices() {
        let s = json!({"a":{"settings":{"index.number_of_shards":"3"}},"b":{"settings":{"index.number_of_shards":"1"}}});
        assert_eq!(min_primary_shards(&s), Some(1));
        let s = json!({"a":{"settings":{"index.number_of_shards":5}}});
        assert_eq!(min_primary_shards(&s), Some(5));
        assert_eq!(min_primary_shards(&json!({})), None);
        assert_eq!(min_primary_shards(&json!({"a":{"settings":{}}})), None);
        assert_eq!(
            min_primary_shards(&json!({"a":{"settings":{"index.number_of_shards":"x"}}})),
            None
        );
    }

    #[test]
    fn slices_are_capped_to_shards_with_a_warning() {
        assert_eq!(cap_slices(4, 8), (4, None));
        assert_eq!(cap_slices(4, 4), (4, None));
        assert_eq!(cap_slices(0, 1), (0, None));
        let (n, w) = cap_slices(8, 3);
        assert_eq!(n, 3);
        assert!(w.unwrap().contains("using --slices 3"));
        let (n, w) = cap_slices(2, 1);
        assert_eq!(n, 0, "one shard: unsliced");
        let w = w.unwrap();
        assert!(
            w.contains("running unsliced") && w.contains("--sliceCap=false"),
            "{w}"
        );
    }

    fn es(v: &str) -> ClusterInfo {
        ClusterInfo::new(Flavor::Elastic, v)
    }
    fn os(v: &str) -> ClusterInfo {
        ClusterInfo::new(Flavor::OpenSearch, v)
    }

    #[test]
    fn auto_basis_per_version_and_flavour() {
        use CapBasis::{Min, Total};
        // Elasticsearch 5.0-7.10: slices per index
        for v in ["5.0.0", "5.6.16", "6.8.23", "7.0.0", "7.9.3", "7.10.2"] {
            assert_eq!(auto_basis(Some(&es(v)), false), Min, "ES {v} scroll");
            assert_eq!(auto_basis(Some(&es(v)), true), Min, "ES {v} pit");
        }
        // Elasticsearch 7.11-9.4: across all shards of the request (#65706)
        for v in [
            "7.11.0", "7.17.29", "8.0.0", "8.19.3", "9.0.0", "9.1.2", "9.4.9",
        ] {
            assert_eq!(auto_basis(Some(&es(v)), false), Total, "ES {v} scroll");
            assert_eq!(auto_basis(Some(&es(v)), true), Total, "ES {v} pit");
        }
        // Elasticsearch 9.5+: scroll back to per-shard filtering (#148664), PIT keeps total;
        // a 9.5 pre-release counts as 9.5
        for v in ["9.5.0", "9.5.0-SNAPSHOT", "9.6.1", "10.0.0"] {
            assert_eq!(auto_basis(Some(&es(v)), false), Min, "ES {v} scroll");
            assert_eq!(auto_basis(Some(&es(v)), true), Total, "ES {v} pit");
        }
        // every OpenSearch: per index
        for v in ["1.0.0", "1.3.20", "2.4.0", "2.19.0", "3.0.0", "3.2.0"] {
            assert_eq!(auto_basis(Some(&os(v)), false), Min, "OS {v} scroll");
            assert_eq!(auto_basis(Some(&os(v)), true), Min, "OS {v} pit");
        }
        // OSS build of 7.10 is still 7.10
        let mut oss = es("7.10.2");
        oss.oss = true;
        assert_eq!(auto_basis(Some(&oss), false), Min);
        // unknown version: the conservative smallest count
        assert_eq!(auto_basis(None, false), Min);
        assert_eq!(auto_basis(None, true), Min);
    }

    #[test]
    fn select_basis_honours_explicit_modes() {
        let es8 = es("8.19.3");
        let os2 = os("2.19.0");
        for info in [Some(&es8), Some(&os2), None] {
            assert_eq!(select_basis(SliceCap::Min, info, false), CapBasis::Min);
            assert_eq!(select_basis(SliceCap::Total, info, false), CapBasis::Total);
        }
        assert_eq!(
            select_basis(SliceCap::Auto, Some(&es8), false),
            CapBasis::Total
        );
        assert_eq!(
            select_basis(SliceCap::Auto, Some(&os2), false),
            CapBasis::Min
        );
        assert_eq!(select_basis(SliceCap::Auto, None, false), CapBasis::Min);
    }

    #[test]
    fn slice_cap_values_parse() {
        for (v, want) in [
            ("auto", SliceCap::Auto),
            ("AUTO", SliceCap::Auto),
            ("true", SliceCap::Auto),
            ("yes", SliceCap::Auto),
            ("1", SliceCap::Auto),
            ("min", SliceCap::Min),
            ("Total", SliceCap::Total),
            ("false", SliceCap::Off),
            ("off", SliceCap::Off),
            ("0", SliceCap::Off),
        ] {
            assert_eq!(parse_slice_cap(v), Ok(want), "{v}");
        }
        for bad in ["", "max", "2", "minimum", "sum"] {
            let e = parse_slice_cap(bad).unwrap_err();
            assert!(e.contains("auto, min, total or false"), "{bad}: {e}");
        }
        assert_eq!(SliceCap::Off.name(), "false");
    }

    #[test]
    fn auto_caps_only_on_elasticsearch_7_10() {
        for v in ["7.10.0", "7.10.2"] {
            assert!(auto_applies(Some(&es(v))), "ES {v}");
        }
        let mut oss = es("7.10.2");
        oss.oss = true;
        assert!(auto_applies(Some(&oss)), "ES 7.10.2 OSS");
        for v in [
            "5.6.16", "6.8.23", "7.9.3", "7.11.0", "7.17.29", "8.19.23", "9.1.2", "9.5.0",
        ] {
            assert!(!auto_applies(Some(&es(v))), "ES {v}");
        }
        // OpenSearch never, including one reporting 7.10.2 (detected at its real version)
        for v in ["1.3.20", "2.19.7", "3.0.0", "3.9.0", "7.10.2"] {
            assert!(!auto_applies(Some(&os(v))), "OS {v}");
        }
        assert!(!auto_applies(None), "unknown version");
    }

    #[test]
    fn shard_counts_min_and_total() {
        let s = json!({"a":{"settings":{"index.number_of_shards":"1"}},"b":{"settings":{"index.number_of_shards":5}}});
        assert_eq!(
            primary_shard_counts(&s),
            Some(ShardCounts {
                min: 1,
                total: 6,
                indices: 2
            })
        );
        assert_eq!(primary_shard_counts(&json!({})), None);
        assert_eq!(
            primary_shard_counts(&json!({"a":{"settings":{"index.number_of_shards":"0"}}})),
            None
        );
    }

    #[test]
    fn cap_by_basis_over_mixed_indices() {
        let mixed = ShardCounts {
            min: 1,
            total: 6,
            indices: 2,
        };
        // per-index engine: smallest count, unsliced, the warning names the total
        let (n, w) = cap_slices_by(8, mixed, CapBasis::Min);
        assert_eq!(n, 0);
        let w = w.unwrap();
        assert!(
            w.contains("smallest primary-shard count (1)")
                && w.contains("6 primary shards in total")
                && w.contains("--sliceCap=total")
                && w.contains("running unsliced"),
            "{w}"
        );
        // across-shards engine: total
        let (n, w) = cap_slices_by(8, mixed, CapBasis::Total);
        assert_eq!(n, 6);
        let w = w.unwrap();
        assert!(
            w.contains("6 primary shards of the 2 input indices in total"),
            "{w}"
        );
        assert!(w.contains("using --slices 6"), "{w}");
        assert_eq!(cap_slices_by(6, mixed, CapBasis::Total), (6, None));
        assert_eq!(cap_slices_by(4, mixed, CapBasis::Total), (4, None));
        // one index: the single-index wording either way
        let one = ShardCounts {
            min: 3,
            total: 3,
            indices: 1,
        };
        for b in [CapBasis::Min, CapBasis::Total] {
            let (n, w) = cap_slices_by(8, one, b);
            assert_eq!(n, 3);
            assert!(
                w.unwrap()
                    .contains("the 3 primary shard(s) of the input index")
            );
        }
    }

    #[test]
    fn slice_field_must_be_numeric_with_doc_values_everywhere() {
        let typeless =
            |leaf: Value| json!({"i":{"mappings":{"ts":{"full_name":"ts","mapping":{"ts":leaf}}}}});
        assert!(check_slice_field(&typeless(json!({"type":"date"})), "ts").is_ok());
        assert!(check_slice_field(&typeless(json!({"type":"long"})), "ts").is_ok());
        let e = check_slice_field(&typeless(json!({"type":"keyword"})), "ts").unwrap_err();
        assert!(format!("{e}").contains("keyword field"), "{e}");
        let e = check_slice_field(&typeless(json!({"type":"long","doc_values":false})), "ts")
            .unwrap_err();
        assert!(format!("{e}").contains("doc_values disabled"), "{e}");
        let missing = json!({"i":{"mappings":{}},"j":{"mappings":{"ts":{"full_name":"ts","mapping":{"ts":{"type":"long"}}}}}});
        let e = check_slice_field(&missing, "ts").unwrap_err();
        assert!(format!("{e}").contains("index i has no field ts"), "{e}");
        let typed = json!({"i":{"mappings":{"doc":{"ts":{"full_name":"ts","mapping":{"ts":{"type":"integer"}}}}}}});
        assert!(check_slice_field(&typed, "ts").is_ok());
        let nested =
            json!({"i":{"mappings":{"a.b":{"full_name":"a.b","mapping":{"b":{"type":"double"}}}}}});
        assert!(check_slice_field(&nested, "a.b").is_ok());
        assert!(check_slice_field(&json!({}), "ts").is_err());
    }
}
