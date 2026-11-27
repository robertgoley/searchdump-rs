//! Which indices, aliases and data streams are selected, and their directories.

use crate::cli::Layout;
use crate::cli::MatchType;
use anyhow::{Context, Result, anyhow, bail};
use regex::Regex;
use std::path::PathBuf;

// ----------------------------------------------------------------------------- selection

/// Name selection: `--match`, `--exclude`, hidden indices, and the break-out group.
pub(crate) struct Selector {
    include: Regex,
    exclude: Option<Regex>,
    include_hidden: bool,
}

impl Selector {
    pub(crate) fn new(include: &str, exclude: Option<&str>, include_hidden: bool) -> Result<Self> {
        let include =
            Regex::new(include).with_context(|| format!("invalid --match regex '{include}'"))?;
        let exclude = exclude
            .map(|e| Regex::new(e).with_context(|| format!("invalid --exclude regex '{e}'")))
            .transpose()?;
        Ok(Self {
            include,
            exclude,
            include_hidden,
        })
    }

    pub(crate) fn selects(&self, name: &str) -> bool {
        // Hidden/system indices only with --includeHidden or a --match that starts with a literal dot.
        let p = self.include.as_str();
        let explicit_dot = p.starts_with("^\\.") || p.starts_with("\\.");
        if name.starts_with('.') && !self.include_hidden && !explicit_dot {
            return false;
        }
        self.include.is_match(name) && !self.exclude.as_ref().is_some_and(|e| e.is_match(name))
    }

    pub(crate) fn has_group(&self) -> bool {
        self.include.captures_len() > 1
    }

    /// The break-out group: the `group` named capture, else the first capture group.
    pub(crate) fn group(&self, name: &str) -> Option<String> {
        let caps = self.include.captures(name)?;
        let m = caps.name("group").or_else(|| caps.get(1))?;
        Some(sanitize_dir(m.as_str()))
    }
}

/// What one dump job covers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TargetKind {
    Index,
    DataStream,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Target {
    /// Index or data stream name.
    pub(crate) name: String,
    pub(crate) kind: TargetKind,
    /// The name --match was tested against (the alias for --matchType alias); the group comes
    /// from it.
    pub(crate) matched: String,
}

/// The dump jobs for a cluster's indices, aliases and data streams. Backing indices of a
/// selected data stream are left to the stream; an index reached twice is dumped once.
pub(crate) fn select_targets(
    sel: &Selector,
    match_types: &[MatchType],
    indices: &[String],
    aliases: &[(String, String)],
    streams: &[(String, Vec<String>)],
) -> Vec<Target> {
    let mut out: Vec<Target> = Vec::new();
    let mut seen = std::collections::BTreeSet::new();
    let mut backing = std::collections::BTreeSet::new();
    if match_types.contains(&MatchType::Datastream) {
        for (name, members) in streams {
            if sel.selects(name) && seen.insert(name.clone()) {
                backing.extend(members.iter().cloned());
                out.push(Target {
                    name: name.clone(),
                    kind: TargetKind::DataStream,
                    matched: name.clone(),
                });
            }
        }
    }
    let open: std::collections::BTreeSet<&str> = indices.iter().map(String::as_str).collect();
    let mut add = |index: &str, matched: &str, out: &mut Vec<Target>| {
        if !backing.contains(index) && open.contains(index) && seen.insert(index.to_string()) {
            out.push(Target {
                name: index.to_string(),
                kind: TargetKind::Index,
                matched: matched.to_string(),
            });
        }
    };
    if match_types.contains(&MatchType::Index) {
        for i in indices.iter().filter(|i| sel.selects(i)) {
            add(i, i, &mut out);
        }
    }
    if match_types.contains(&MatchType::Alias) {
        for (alias, index) in aliases.iter().filter(|(a, _)| sel.selects(a)) {
            add(index, alias, &mut out);
        }
    }
    out.sort_by(|a, b| a.name.cmp(&b.name));
    out
}

/// A group value as one safe directory name.
pub(crate) fn sanitize_dir(raw: &str) -> String {
    let s: String = raw
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.' | '+' | '@') {
                c
            } else {
                '_'
            }
        })
        .collect();
    match s.as_str() {
        "" => "_".into(),
        "." | ".." => s.replace('.', "_"),
        _ => s,
    }
}

/// Index names are used as file names: refuse anything that could leave the directory.
pub(crate) fn check_index_name(name: &str) -> Result<()> {
    if name.is_empty()
        || name == "."
        || name == ".."
        || name.contains(['/', '\\', '\0', '\n', '\r'])
        || name.starts_with('-')
    {
        bail!("refusing unsafe index name {name:?}");
    }
    Ok(())
}

/// The directory (relative to the dump root) that holds one index's files. `matched` is the
/// name the group capture is taken from (the index itself, or the alias that selected it).
pub(crate) fn rel_dir(
    layout: Layout,
    sel: &Selector,
    index: &str,
    matched: &str,
) -> Result<PathBuf> {
    Ok(match layout {
        Layout::Flat => PathBuf::new(),
        Layout::Index => PathBuf::from(sanitize_dir(index)),
        Layout::Group => PathBuf::from(sel.group(matched).ok_or_else(|| {
            anyhow!("--layout group: --match has no capture group for {matched}")
        })?),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn selection_include_exclude_hidden() {
        let s = Selector::new("^logs-", Some("-debug-"), false).unwrap();
        assert!(s.selects("logs-app-2026.10.01"));
        assert!(!s.selects("logs-debug-2026.10.01"));
        assert!(!s.selects("metrics-1"));
        let all = Selector::new("^.*$", None, false).unwrap();
        assert!(all.selects("a"));
        assert!(
            !all.selects(".kibana"),
            "hidden indices need --includeHidden or an explicit match"
        );
        assert!(
            Selector::new("^.*$", None, true)
                .unwrap()
                .selects(".kibana")
        );
        assert!(
            Selector::new("^\\.opendistro", None, false)
                .unwrap()
                .selects(".opendistro-ism-config")
        );
        assert!(Selector::new("(", None, false).is_err());
    }

    #[test]
    fn group_breakout() {
        let s = Selector::new(r"^logs-.*-(?P<group>\d{4}\.\d{2})\.\d{2}$", None, false).unwrap();
        assert!(s.has_group());
        assert_eq!(s.group("logs-app-2026.10.31").as_deref(), Some("2026.10"));
        assert_eq!(
            rel_dir(
                Layout::Group,
                &s,
                "logs-app-2026.10.31",
                "logs-app-2026.10.31"
            )
            .unwrap(),
            PathBuf::from("2026.10")
        );
        let first = Selector::new(r"^([a-z]+)-", None, false).unwrap();
        assert_eq!(first.group("web-1").as_deref(), Some("web"));
        let none = Selector::new("^x", None, false).unwrap();
        assert!(!none.has_group());
        assert_eq!(
            rel_dir(Layout::Flat, &none, "x1", "x1").unwrap(),
            PathBuf::new()
        );
        assert_eq!(
            rel_dir(Layout::Index, &none, "x1", "x1").unwrap(),
            PathBuf::from("x1")
        );
        assert_eq!(sanitize_dir("a/b"), "a_b");
        assert_eq!(sanitize_dir(".."), "__");
        assert_eq!(sanitize_dir(""), "_");
    }

    #[test]
    fn targets_by_index_alias_and_datastream() {
        let indices: Vec<String> = [
            "logs-a",
            "logs-b",
            "other",
            ".ds-web-2026.11.01-000001",
            ".ds-web-2026.11.02-000002",
        ]
        .map(String::from)
        .to_vec();
        let aliases = vec![
            ("logs".to_string(), "logs-a".to_string()),
            ("logs".to_string(), "logs-b".to_string()),
            ("ali-other".to_string(), "other".to_string()),
            ("gone".to_string(), "closed-idx".to_string()),
        ];
        let streams = vec![(
            "web".to_string(),
            vec![
                ".ds-web-2026.11.01-000001".to_string(),
                ".ds-web-2026.11.02-000002".to_string(),
            ],
        )];
        let names = |t: &[Target]| t.iter().map(|t| t.name.clone()).collect::<Vec<_>>();
        let all = Selector::new("^.*$", None, true).unwrap();
        // index matching with hidden indices still leaves stream backing indices alone
        let t = select_targets(
            &all,
            &[MatchType::Index, MatchType::Datastream],
            &indices,
            &aliases,
            &streams,
        );
        assert_eq!(names(&t), vec!["logs-a", "logs-b", "other", "web"]);
        assert_eq!(t[3].kind, TargetKind::DataStream);
        // alias matching selects the indices behind the alias, grouped by the alias name
        let s = Selector::new("^(?P<group>ali)-|^logs$", None, false).unwrap();
        let t = select_targets(&s, &[MatchType::Alias], &indices, &aliases, &streams);
        assert_eq!(names(&t), vec!["logs-a", "logs-b", "other"]);
        assert_eq!(t[2].matched, "ali-other");
        assert_eq!(
            rel_dir(Layout::Group, &s, &t[2].name, &t[2].matched).unwrap(),
            PathBuf::from("ali")
        );
        // index and alias together dump an index once
        let s = Selector::new("logs", None, false).unwrap();
        let t = select_targets(
            &s,
            &[MatchType::Index, MatchType::Alias],
            &indices,
            &aliases,
            &streams,
        );
        assert_eq!(names(&t), vec!["logs-a", "logs-b"]);
        // without datastream, backing indices are ordinary hidden indices
        let t = select_targets(&all, &[MatchType::Index], &indices, &aliases, &streams);
        assert_eq!(t.len(), 5);
    }
}
