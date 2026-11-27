//! Item types (`--types` / `--ignoreType`) and the Node-compatible type flags.

use crate::cli::Args;
use anyhow::{Result, bail};

/// Per-index item types, in the order they are dumped and loaded.
pub(crate) const TYPE_ORDER: &[&str] = &["settings", "analyzer", "mapping", "alias", "data"];

/// Cluster-scoped types, dumped once per run into `DIR/_cluster/TYPE.json` and loaded before
/// any index, in this order (policies before the templates that name them, component templates
/// before the index templates composed of them).
pub(crate) const CLUSTER_TYPES: &[&str] = &[
    "policy",
    "component_template",
    "index_template",
    "template",
    "script",
];

/// Per-index and cluster-wide types to process, in processing order.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(crate) struct Types {
    pub(crate) index: Vec<&'static str>,
    pub(crate) cluster: Vec<&'static str>,
}

/// Node multielasticdump's per-type flags folded into `--types`/`--ignoreType` lists:
/// `--includeType` replaces the type list, `--ignoreX=true` removes X, `--ignoreX=false` adds it
/// (Node ignores analyzer, alias and index unless told otherwise). Node's `index` type (the
/// index create body) is our settings + mapping.
pub(crate) fn node_type_flags(args: &Args, default_types: &[String]) -> (Vec<String>, Vec<String>) {
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

pub(crate) fn resolve_types(types: &[String], ignore: &[String]) -> Result<Types> {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cli::Compression;
    use crate::files::item_file;
    use crate::selection::check_index_name;
    use clap::Parser;

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
}
