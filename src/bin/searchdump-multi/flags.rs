//! Child flag lists: default data flags, merging user flags, cluster-wide type flags.

use crate::*;

/// Default run-control flags for data runs (from scripts/oslogs.py). 1.2 (ROADMAP-1.2 section 6):
/// page and bulk sizes are no longer fixed here; they come from [`sizing`] per index
/// (`--serverLimits`), and `--serverLimits=off` reproduces the 1.1.0 values exactly.
pub(crate) const DATA_FLAGS: &[&str] = &[
    "--scrollTime=10m",
    "--retryAttempts=5",
    "--retryDelay=2000",
    "--timeout=300000",
];

/// The flag name of an argument (`--limit=5` / `--limit` -> `--limit`), or None for a value.
pub(crate) fn flag_name(arg: &str) -> Option<&str> {
    if !arg.starts_with("--") || arg == "--" {
        return None;
    }
    Some(arg.split_once('=').map_or(arg, |(n, _)| n))
}

/// Default data flags followed by the arguments after `--`. A flag given after `--` replaces
/// the default of the same name (`-- --limit=200` runs with `--limit=200`, not both), so
/// every default can be overridden without `--noDefaultDataFlags`.
pub(crate) fn merge_data_flags(defaults: &[&str], extra: &[String]) -> Vec<String> {
    let given: std::collections::HashSet<&str> =
        extra.iter().filter_map(|a| flag_name(a)).collect();
    defaults
        .iter()
        .filter(|d| flag_name(d).is_none_or(|n| !given.contains(n)))
        .map(|d| d.to_string())
        .chain(extra.iter().cloned())
        .collect()
}

/// [`merge_data_flags`] for computed (owned) defaults: `extra` (the run's data flags, ending in
/// the user's flags after `--`) replaces a default of the same name.
pub(crate) fn merge_data_flags_owned(defaults: &[String], extra: &[String]) -> Vec<String> {
    let d: Vec<&str> = defaults.iter().map(String::as_str).collect();
    merge_data_flags(&d, extra)
}

pub(crate) const LIFECYCLE_DROP: &str =
    "--excludeSettings=index.lifecycle,index.routing.allocation.include._tier_preference";

pub(crate) fn strings(flags: &[&str]) -> Vec<String> {
    flags.iter().map(|s| s.to_string()).collect()
}

/// Flags for a cluster-scoped run.
pub(crate) fn cluster_flags(args: &Args, kind: &str, load: bool) -> Vec<String> {
    let mut v = vec![];
    if load {
        match (kind, args.lifecycle) {
            ("policy", Lifecycle::Convert) => v.push("--convertIlmToIsm".to_string()),
            ("template" | "index_template" | "component_template", Lifecycle::Drop) => {
                v.push(LIFECYCLE_DROP.to_string())
            }
            _ => {}
        }
        if args.skip_existing {
            v.push("--skip-existing".to_string());
        }
    }
    v
}

/// 1.2: a data child's sizing flags from `plan` for `size` (only the flags `runner`'s binary accepts),
/// followed by the default/user data flags, the later ones winning.
pub(crate) fn child_data_flags(
    plan: &sizing::Plan,
    size: &sizing::IndexSize,
    runner: &Runner,
) -> Vec<String> {
    merge_data_flags_owned(&plan.flags(size, runner.server_limits), &runner.data_flags)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extra_flags_override_default_data_flags() {
        let extra: Vec<String> = ["--limit=200", "--scrollTime", "1m", "--noRefresh"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        let got = crate::merge_data_flags(crate::DATA_FLAGS, &extra);
        assert!(!got.iter().any(|a| a == "--scrollTime=10m"), "{got:?}");
        assert_eq!(got.iter().filter(|a| a.starts_with("--limit")).count(), 1);
        assert!(got.contains(&"--retryAttempts=5".to_string()));
        assert!(got.ends_with(&extra));
        // no extras: the defaults unchanged
        assert_eq!(
            crate::merge_data_flags(crate::DATA_FLAGS, &[]),
            crate::strings(crate::DATA_FLAGS)
        );
        assert!(crate::merge_data_flags(&[], &[]).is_empty());
        // computed sizes: the user's --limit replaces the plan's
        let sized = crate::merge_data_flags_owned(
            &[
                "--limit=5000".to_string(),
                "--maxBulkBytes=16mb".to_string(),
            ],
            &got,
        );
        assert!(!sized.contains(&"--limit=5000".to_string()), "{sized:?}");
        assert!(
            sized.contains(&"--maxBulkBytes=16mb".to_string()),
            "{sized:?}"
        );
    }

    /// 1.2: the run-control flags are the 1.1.0 ones minus the sizes (which moved to `sizing`).
    #[test]
    fn run_control_flags_unchanged() {
        assert_eq!(
            crate::strings(crate::DATA_FLAGS),
            [
                "--scrollTime=10m",
                "--retryAttempts=5",
                "--retryDelay=2000",
                "--timeout=300000"
            ]
        );
    }

    #[test]
    fn split_flag_detection_and_cluster_flags() {
        let parse = |a: &[&str]| {
            Args::try_parse_from(
                [
                    "searchdump-multi",
                    "--input",
                    "http://h:9200",
                    "--output",
                    "/tmp/x",
                ]
                .iter()
                .chain(a),
            )
            .unwrap()
        };
        assert!(!parse(&[]).splits());
        assert!(parse(&["--fileSize", "1gb"]).splits());
        assert!(parse(&["--maxRows", "10"]).splits());
        assert!(parse(&["--", "--maxRows=10"]).splits());
        assert!(
            Args::try_parse_from([
                "m",
                "--input",
                "a",
                "--output",
                "b",
                "--fileSize",
                "1gb",
                "--maxRows",
                "2"
            ])
            .is_err()
        );
        let a = parse(&["--matchType", "alias,datastream", "--interval", "250"]);
        assert_eq!(a.match_type, vec![MatchType::Alias, MatchType::Datastream]);
        assert_eq!(a.interval, 250);
        let mut a = parse(&["--skipExisting"]);
        assert_eq!(
            cluster_flags(&a, "index_template", true),
            vec![LIFECYCLE_DROP.to_string(), "--skip-existing".to_string()]
        );
        assert!(cluster_flags(&a, "index_template", false).is_empty());
        a.lifecycle = Lifecycle::Convert;
        assert_eq!(
            cluster_flags(&a, "policy", true),
            vec![
                "--convertIlmToIsm".to_string(),
                "--skip-existing".to_string()
            ]
        );
    }
}
