//! `--type alias`: `GET /{index}/_alias/{name}` and one atomic
//! `POST /_aliases` with an `add` action per index and alias.

use anyhow::{Result, bail};
use reqwest::Method;
use serde_json::{Map, Value, json};

use super::http::{get_json, send_json};
use super::{MetaOptions, MetaTarget};
use crate::http::EsClient;
use crate::pipeline::WriteReport;

/// `index` defaults to `*` (elasticdump requests `/undefined/_alias/*` for a
/// host-only input).
pub async fn fetch(
    client: &EsClient,
    index: Option<&str>,
    alias: Option<&str>,
    opts: &MetaOptions,
) -> Result<Value> {
    let index = index.unwrap_or("*");
    let alias = alias.unwrap_or("*");
    get_json(
        client,
        opts.retry,
        &[index, "_alias", alias],
        &format!("GET {index}/_alias/{alias}"),
    )
    .await
}

/// Name prefixes of reserved system indices whose aliases the cluster refuses to change, even for
/// a superuser: Elasticsearch restricted indices (`.security`, `.security-7`, `.security-tokens-7`,
/// `.security-profile-8`, `.async-search`, `.fleet-secrets*`) and the OpenSearch / Open Distro
/// security index. A host-only `GET */_alias/*` dump lists them (e.g. `.security-7` with alias
/// `.security`), so a restore skips them instead of failing the whole atomic `POST /_aliases`.
const RESERVED_SYSTEM_INDEX_PREFIXES: &[&str] = &[
    ".security",
    ".async-search",
    ".fleet-secrets",
    ".opendistro_security",
    ".plugins-security",
];

/// True for a reserved system index that alias restore must not touch.
pub fn is_reserved_system_index(index: &str) -> bool {
    RESERVED_SYSTEM_INDEX_PREFIXES
        .iter()
        .any(|p| index.starts_with(p))
}

/// `(target index, alias, options)` for every alias in the document.
/// Aliases of reserved system indices (see [`is_reserved_system_index`]) are skipped with a warning.
pub fn actions(doc: &Value, dst: &MetaTarget, opts: &MetaOptions) -> Result<Vec<(String, Value)>> {
    let Value::Object(indices) = doc else {
        bail!("alias document must be a JSON object keyed by index");
    };
    let with_aliases: Vec<(&String, &Map<String, Value>)> = indices
        .iter()
        .filter_map(
            |(i, def)| match def.get("aliases").and_then(Value::as_object) {
                Some(a) if !a.is_empty() && is_reserved_system_index(i) => {
                    let names: Vec<&str> = a.keys().map(String::as_str).collect();
                    log::warn!(
                        "Skipping alias(es) {names:?} of reserved system index '{i}': the cluster does not allow alias changes on it"
                    );
                    None
                }
                Some(a) if !a.is_empty() => Some((i, a)),
                _ => {
                    log::debug!("Index '{i}' has no aliases; skipping it");
                    None
                }
            },
        )
        .collect();
    if dst.index.is_some() && with_aliases.len() > 1 {
        bail!(
            "the alias document holds aliases of {} indices but a single output index was given; use a host-only --output",
            with_aliases.len()
        );
    }
    let mut out = Vec::new();
    for (index, aliases) in with_aliases {
        let target = dst.target_index(index)?;
        for (alias, options) in aliases {
            let alias = if opts.rename_aliases {
                dst.renamer.alias(alias)?
            } else {
                alias.clone()
            };
            let mut add = Map::new();
            add.insert("index".into(), Value::String(target.clone()));
            add.insert("alias".into(), Value::String(alias));
            if let Value::Object(o) = options {
                for (k, v) in o {
                    add.insert(k.clone(), v.clone());
                }
            }
            out.push((target.clone(), json!({ "add": add })));
        }
    }
    Ok(out)
}

pub async fn write(
    doc: Value,
    client: &EsClient,
    dst: &MetaTarget,
    opts: &MetaOptions,
) -> Result<WriteReport> {
    let acts = actions(&doc, dst, opts)?;
    if acts.is_empty() {
        log::warn!("No aliases to restore");
        return Ok(WriteReport::default());
    }
    let body = json!({"actions": acts.iter().map(|(_, a)| a.clone()).collect::<Vec<_>>()});
    match send_json(
        client,
        opts.retry,
        Method::POST,
        &["_aliases"],
        &body,
        "update aliases",
    )
    .await
    {
        Ok(_) => Ok(WriteReport {
            written: acts.len() as u64,
            failed: 0,
            ..Default::default()
        }),
        Err(e) if opts.ignore_errors => {
            log::warn!("Atomic alias update failed; retrying per index (--ignore-errors): {e:#}");
            let mut report = WriteReport::default();
            let mut targets: Vec<&String> = acts.iter().map(|(t, _)| t).collect();
            targets.dedup();
            for t in targets {
                let mine: Vec<Value> = acts
                    .iter()
                    .filter(|(x, _)| x == t)
                    .map(|(_, a)| a.clone())
                    .collect();
                let n = mine.len() as u64;
                match send_json(
                    client,
                    opts.retry,
                    Method::POST,
                    &["_aliases"],
                    &json!({"actions": mine}),
                    "update aliases",
                )
                .await
                {
                    Ok(_) => report.written += n,
                    Err(e) => {
                        log::error!("aliases of '{t}' failed (--ignore-errors): {e:#}");
                        report.failed += n;
                    }
                }
            }
            Ok(report)
        }
        Err(e) => Err(e.context("restoring aliases")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rename::IndexRenamer;

    fn target() -> MetaTarget {
        MetaTarget {
            client: None,
            info: None,
            index: None,
            doc_type: None,
            renamer: IndexRenamer::new(&[], &[], "", "").unwrap(),
            file: Default::default(),
        }
    }

    fn opts() -> MetaOptions {
        MetaOptions::new(
            super::super::MetaKind::Alias,
            crate::http::RetryPolicy::none(),
        )
    }

    #[test]
    fn reserved_system_indices_are_recognised() {
        for i in [
            ".security",
            ".security-7",
            ".security-6",
            ".security-tokens-7",
            ".security-profile-8",
            ".async-search",
            ".fleet-secrets-7",
            ".opendistro_security",
            ".plugins-security-config",
        ] {
            assert!(is_reserved_system_index(i), "{i}");
        }
        for i in [
            "security",
            "logs-security",
            ".kibana_1",
            ".tasks",
            "my-index",
            "a.security",
        ] {
            assert!(!is_reserved_system_index(i), "{i}");
        }
    }

    #[test]
    fn host_only_dump_with_security_index_skips_it() {
        // What ES 8 returns for `GET */_alias/*` with a superuser: `.security-7` carries `.security`.
        let doc = json!({
            ".security-7": {"aliases": {".security": {"is_hidden": true}}},
            "logs-a": {"aliases": {"logs": {"is_write_index": true}}},
            "logs-b": {"aliases": {}},
        });
        let acts = actions(&doc, &target(), &opts()).unwrap();
        assert_eq!(acts.len(), 1, "{acts:?}");
        assert_eq!(acts[0].0, "logs-a");
        assert_eq!(
            acts[0].1,
            json!({"add": {"index": "logs-a", "alias": "logs", "is_write_index": true}})
        );
    }

    #[test]
    fn reserved_index_does_not_count_against_a_single_output_index() {
        let doc = json!({
            ".security-7": {"aliases": {".security": {"is_hidden": true}}},
            "src": {"aliases": {"a": {}}},
        });
        let mut dst = target();
        dst.index = Some("dst".into());
        let acts = actions(&doc, &dst, &opts()).unwrap();
        assert_eq!(
            acts,
            vec![(
                "dst".to_string(),
                json!({"add": {"index": "dst", "alias": "a"}})
            )]
        );
    }

    #[test]
    fn only_reserved_indices_gives_no_actions() {
        let doc = json!({".security-7": {"aliases": {".security": {"is_hidden": true}}}});
        assert!(actions(&doc, &target(), &opts()).unwrap().is_empty());
    }
}
