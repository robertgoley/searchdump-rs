//! Shared run context and start pacing.

use crate::child::Runner;
use crate::cli::Args;
use crate::files::meta_dir;
use crate::selection::Selector;
use crate::sizing;
use crate::tls::ClusterTls;
use crate::types::Types;
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::time::Duration;

// ----------------------------------------------------------------------------- dump / load

#[derive(Debug)]
pub(crate) enum Outcome {
    Done(String),
    Skipped(String),
    Failed(String),
}

pub(crate) struct Ctx {
    pub(crate) args: Args,
    /// L4-001: TLS settings of the cluster (parent requests; children get them as flags/env).
    pub(crate) tls: ClusterTls,
    /// Where the manifest and `_cluster/` live (the dump directory unless --metaDir or
    /// --nodeCompat put them elsewhere).
    pub(crate) meta: PathBuf,
    pub(crate) types: Types,
    pub(crate) sel: Selector,
    pub(crate) runner: Runner,
    pub(crate) pacer: Pacer,
    /// 1.2: the sizing plan (set by [`plan_sizes`](crate::planner::plan_sizes) before any index starts).
    pub(crate) plan: std::sync::OnceLock<sizing::Plan>,
    /// 1.2: primary (docs, stored bytes) per selected index, from one stats call.
    pub(crate) index_stats: std::sync::OnceLock<BTreeMap<String, sizing::Stats>>,
}

impl Ctx {
    /// The run context: `--interval` pacing and the meta directory are derived from `args`.
    pub(crate) fn new(
        args: Args,
        tls: ClusterTls,
        types: Types,
        sel: Selector,
        runner: Runner,
    ) -> Ctx {
        let pacer = Pacer::new(Duration::from_millis(args.interval));
        let meta = meta_dir(&args);
        Ctx {
            meta,
            tls,
            args,
            types,
            sel,
            runner,
            pacer,
            plan: Default::default(),
            index_stats: Default::default(),
        }
    }

    pub(crate) fn plan(&self) -> &sizing::Plan {
        self.plan.get_or_init(|| {
            sizing::Plan::new(
                searchdump_rs::limits::Mode::Off,
                self.args.no_default_data_flags,
                None,
                self.args.parallel,
                &[],
            )
        })
    }

    pub(crate) fn stats_of(&self, index: &str) -> Option<sizing::Stats> {
        self.index_stats.get().and_then(|m| m.get(index).copied())
    }
}

/// `--interval`: a minimum gap between job starts, shared by all workers.
pub(crate) struct Pacer {
    gap: Duration,
    next: tokio::sync::Mutex<Option<tokio::time::Instant>>,
}

impl Pacer {
    pub(crate) fn new(gap: Duration) -> Self {
        Self {
            gap,
            next: tokio::sync::Mutex::new(None),
        }
    }

    pub(crate) async fn wait(&self) {
        if self.gap.is_zero() {
            return;
        }
        let mut next = self.next.lock().await;
        if let Some(at) = *next {
            tokio::time::sleep_until(at).await;
        }
        *next = Some(tokio::time::Instant::now() + self.gap);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    #[tokio::test(start_paused = true)]
    async fn interval_spaces_out_starts() {
        let p = Arc::new(Pacer::new(Duration::from_millis(500)));
        let t0 = tokio::time::Instant::now();
        let mut set = tokio::task::JoinSet::new();
        for _ in 0..3 {
            let p = p.clone();
            set.spawn(async move {
                p.wait().await;
                tokio::time::Instant::now()
            });
        }
        let mut at: Vec<Duration> = vec![];
        while let Some(t) = set.join_next().await {
            at.push(t.unwrap() - t0);
        }
        at.sort();
        assert_eq!(at[0], Duration::ZERO);
        assert!(at[1] >= Duration::from_millis(500), "{at:?}");
        assert!(at[2] >= Duration::from_millis(1000), "{at:?}");
        let none = Pacer::new(Duration::ZERO);
        none.wait().await;
        none.wait().await;
    }
}
