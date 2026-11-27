//! Data-flag sizing for children (`--serverLimits`, ROADMAP-1.2 L2-001..L2-005).
//!
//! 1.2 server-aware sizing for searchdump-multi (ROADMAP-1.2 sections 3-6). The parent probes
//! the cluster once (node stats, thread pools) and reads every index's primary docs and stored
//! bytes in one `_stats?level=indices` call, then gives each child explicit
//! `--limit=N --maxBulkBytes=M --serverLimits=off` (children never probe again).
use searchdump_rs::limits::{self, Budget, Limits, Mode};

/// The `off` profile: the 1.1.0 (1.1-10) fixed data defaults.
pub(crate) const OFF_LIMIT: u64 = 1000;
/// The `off` profile bulk size (1.1-10).
pub(crate) const OFF_BULK: u64 = 16 << 20;
/// 1.1-10: stored (primary, compressed) bytes one dump page holds in the `off` profile.
/// `_source` in the response is usually 1-4x the stored size, so a page stays well under the
/// ~80 MB that a 256 MB heap survived in the 1.1-10 runs.
pub(crate) const PAGE_STORE_BUDGET: u64 = 16 << 20;

/// 1.1-10, the `off` profile: documents per dump page for an index whose primary documents
/// average `avg_doc_bytes` stored bytes: 1000 for small documents, fewer for large ones so a
/// page holds about [`PAGE_STORE_BUDGET`], never less than 1 (identical to 1.1.0).
pub(crate) fn page_limit_for(avg_doc_bytes: u64) -> u64 {
    if avg_doc_bytes == 0 {
        return OFF_LIMIT;
    }
    (PAGE_STORE_BUDGET / avg_doc_bytes).clamp(1, OFF_LIMIT)
}

/// One run's sizing decision.
#[derive(Debug, Clone)]
pub(crate) struct Plan {
    pub(crate) mode: Mode,
    /// `--noDefaultDataFlags`: no sizing flags at all (only `--serverLimits=off`).
    pub(crate) no_defaults: bool,
    pub(crate) limits: Option<Limits>,
    pub(crate) budget: Option<Budget>,
    /// `--parallel` as asked (the upper bound).
    pub(crate) parallel: usize,
    /// Workers the budget fits at the largest index's page (reported; the byte semaphore
    /// enforces it at run time, so small indices still run `parallel` at once).
    pub(crate) fit_parallel: usize,
    /// Largest page response in bytes after the worker-first policy.
    pub(crate) page_cap: u64,
    /// Bulk bytes per load worker.
    pub(crate) bulk_bytes: u64,
    /// 1.4 `--pageBytes` (L4-003): the user's page byte cap, if any.
    pub(crate) page_bytes: Option<u64>,
}

/// One index's sizes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct IndexSize {
    pub(crate) limit: u64,
    pub(crate) max_bulk_bytes: u64,
    /// `default` | `large-docs` | `budget` | `off` | `fallback`
    pub(crate) reason: &'static str,
    /// Estimated bytes of one page in flight (for the byte semaphore; 0 = unknown).
    pub(crate) page_bytes: u64,
}

/// (docs, stored bytes) of an index's primaries.
pub(crate) type Stats = (u64, u64);

fn avg(s: Option<Stats>) -> Option<u64> {
    s.and_then(|(d, b)| (d > 0 && b > 0).then(|| b / d))
}

/// Estimated response bytes of a page of `limit` documents of an index.
pub(crate) fn page_estimate(limit: u64, s: Option<Stats>) -> u64 {
    match s {
        Some((d, _)) if avg(s).is_some() => {
            limit.min(d.max(1)) * avg(s).unwrap_or(0) * limits::EXPANSION
        }
        _ => 0,
    }
}

impl Plan {
    /// Build the plan from the probe (or none for `off` / `--noDefaultDataFlags`) and the
    /// selected indices' stats. Worker-first (ROADMAP-1.2 4.1): if `parallel` pages of the
    /// largest index do not fit the in-flight budget, fewer workers are assumed first
    /// (down to 2, or 1 when 1 was asked), and only then the page shrinks.
    pub(crate) fn new(
        mode: Mode,
        no_defaults: bool,
        limits: Option<Limits>,
        parallel: usize,
        stats: &[Option<Stats>],
    ) -> Self {
        let budget = limits
            .as_ref()
            .and_then(|l| limits::budget(l, mode, 256 << 20));
        let mut plan = Plan {
            mode,
            no_defaults,
            limits,
            budget,
            parallel,
            fit_parallel: parallel,
            page_cap: 0,
            bulk_bytes: OFF_BULK,
            page_bytes: None,
        };
        let Some(b) = budget else {
            // a fallback on AWS OpenSearch Service still keeps bulks at the 10 MiB floor
            if plan.limits.as_ref().is_some_and(|l| l.aws) {
                plan.bulk_bytes = limits::AWS_PAYLOAD_FLOOR;
            }
            return plan;
        };
        let biggest = stats
            .iter()
            .map(|s| page_estimate(limits::MULTI_MAX_PAGE, *s).min(b.page_bytes))
            .max()
            .unwrap_or(0)
            .max(1);
        let floor = parallel.min(2);
        let mut p = parallel.max(1);
        while p > floor && (p as u64) * biggest > b.inflight_bytes {
            p -= 1;
        }
        plan.fit_parallel = p;
        plan.page_cap = b.page_bytes.min(b.inflight_bytes / p.max(1) as u64);
        plan.bulk_bytes = limits::bulk_bytes(&b, parallel.max(1) as u64).min(100 << 20);
        plan
    }

    /// 1.4 `--pageBytes` (L4-003): cap every dump page at `pb` bytes too. With a budget the
    /// page cap becomes the smaller of the two (below the 1.3 floor as well: the user asked
    /// for it); without one (`off`, a fallback) each index's page is cut to fit `pb`.
    pub(crate) fn with_page_bytes(mut self, pb: Option<u64>) -> Self {
        self.page_bytes = pb;
        if let (Some(p), Some(_)) = (pb, self.budget) {
            self.page_cap = self.page_cap.min(p);
        }
        self
    }

    /// Sizes for a dump of one index.
    pub(crate) fn dump(&self, s: Option<Stats>) -> IndexSize {
        match self.budget {
            None => {
                let fixed = avg(s).map_or(OFF_LIMIT, page_limit_for);
                let limit = match self.page_bytes {
                    Some(p) if !self.no_defaults => limits::page_docs(p, avg(s), fixed),
                    _ => fixed,
                };
                IndexSize {
                    limit,
                    max_bulk_bytes: OFF_BULK,
                    reason: if limit < fixed {
                        "page-bytes"
                    } else if self.limits.is_some() {
                        "fallback"
                    } else {
                        "off"
                    },
                    page_bytes: if limit < fixed {
                        page_estimate(limit, s)
                    } else {
                        0
                    },
                }
            }
            Some(b) => {
                let limit = limits::page_docs(self.page_cap, avg(s), limits::MULTI_MAX_PAGE);
                let reason = if limit >= limits::MULTI_MAX_PAGE {
                    "default"
                } else if self.page_bytes == Some(self.page_cap) && self.page_cap < b.page_bytes {
                    "page-bytes"
                } else if self.page_cap < b.page_bytes {
                    "budget"
                } else {
                    "large-docs"
                };
                IndexSize {
                    limit,
                    max_bulk_bytes: self.bulk_bytes,
                    reason,
                    page_bytes: page_estimate(limit, s),
                }
            }
        }
    }

    /// Sizes for a load (the file batch and the bulk bytes; the bulk bytes are the memory
    /// control on the target).
    pub(crate) fn load(&self) -> IndexSize {
        match self.budget {
            None => IndexSize {
                limit: OFF_LIMIT,
                max_bulk_bytes: self.bulk_bytes,
                reason: if self.limits.is_some() {
                    "fallback"
                } else {
                    "off"
                },
                page_bytes: 0,
            },
            Some(_) => IndexSize {
                limit: if self.bulk_bytes >= 64 << 20 {
                    limits::MULTI_MAX_PAGE
                } else {
                    OFF_LIMIT
                },
                max_bulk_bytes: self.bulk_bytes,
                reason: if self.bulk_bytes < OFF_BULK {
                    "budget"
                } else {
                    "default"
                },
                page_bytes: 0,
            },
        }
    }

    /// The child flags for `size` (before the user's flags after `--`, which win).
    pub(crate) fn flags(&self, size: &IndexSize, child_knows_server_limits: bool) -> Vec<String> {
        let mut v = vec![];
        if !self.no_defaults {
            v.push(format!("--limit={}", size.limit));
            v.push(if size.max_bulk_bytes == OFF_BULK {
                "--maxBulkBytes=16mb".to_string()
            } else {
                format!("--maxBulkBytes={}kb", (size.max_bulk_bytes >> 10).max(1))
            });
        }
        if child_knows_server_limits {
            v.push("--serverLimits=off".to_string());
        }
        v
    }

    /// The one "server limits:" line.
    pub(crate) fn line(&self) -> String {
        let head = match &self.limits {
            None if self.no_defaults => {
                return "server limits: not used (--noDefaultDataFlags) [off]".into();
            }
            None => {
                return format!(
                    "server limits: not probed -> fixed 1.1 defaults [{}]",
                    self.mode.as_str()
                );
            }
            Some(l) => limits::describe(l),
        };
        match self.budget {
            None => format!("server limits: {head} [{}]", self.mode.as_str()),
            Some(b) => format!(
                "server limits: {head} -> page budget {} MB, in flight {} MB, bulk {} KB per worker, parallel {}{} [{}]",
                self.page_cap >> 20,
                b.inflight_bytes >> 20,
                self.bulk_bytes >> 10,
                self.parallel,
                if self.fit_parallel < self.parallel {
                    format!(
                        " (the largest index fits {} at its full page; smaller ones run {} at once)",
                        self.fit_parallel, self.parallel
                    )
                } else {
                    String::new()
                },
                self.mode.as_str()
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::flags::DATA_FLAGS;
    use crate::flags::merge_data_flags;

    #[test]
    fn server_limits_off_reproduces_1_1_0_flags() {
        use crate::sizing::Plan;
        use searchdump_rs::limits::{Limits, Mode, Source};
        let v110: Vec<String> = [
            "--limit=1000",
            "--scrollTime=10m",
            "--retryAttempts=5",
            "--retryDelay=2000",
            "--timeout=300000",
            "--maxBulkBytes=16mb",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect();
        let off = Plan::new(Mode::Off, false, None, 2, &[Some((6250, 6250 * 500))]);
        let mut flags = off.flags(&off.dump(Some((6250, 6250 * 500))), true);
        flags.extend(merge_data_flags(DATA_FLAGS, &[]));
        let mut want = v110.clone();
        want.push("--serverLimits=off".into());
        let (mut a, mut b) = (flags.clone(), want.clone());
        a.sort();
        b.sort();
        assert_eq!(a, b);
        // an old child (no --serverLimits) gets exactly the 1.1.0 set
        let mut old = off.flags(&off.dump(None), false);
        old.extend(merge_data_flags(DATA_FLAGS, &[]));
        old.sort();
        let mut v = v110.clone();
        v.sort();
        assert_eq!(old, v);
        // large documents: the 1.1-10 page
        assert_eq!(off.dump(Some((10, 10 * 80 * 1024))).limit, 204);
        assert_eq!(off.load().limit, 1000);
        assert_eq!(off.load().max_bulk_bytes, 16 << 20);
        // a fallback probe (403, serverless, ...) uses the same values
        let fb = Plan::new(
            Mode::Auto,
            false,
            Some(Limits::fallback(Source::Fallback403, "403")),
            2,
            &[],
        );
        assert_eq!(fb.dump(Some((10, 10 * 80 * 1024))).limit, 204);
        assert_eq!(fb.dump(None).reason, "fallback");
        assert_eq!(
            fb.flags(&fb.load(), true),
            ["--limit=1000", "--maxBulkBytes=16mb", "--serverLimits=off"]
        );
        // a fallback on AWS keeps bulks at the 10 MiB floor
        let mut aws = Limits::fallback(Source::Fallback403, "403");
        aws.aws = true;
        let fa = Plan::new(Mode::Auto, false, Some(aws), 2, &[]);
        assert_eq!(
            fa.load().max_bulk_bytes,
            searchdump_rs::limits::AWS_PAYLOAD_FLOOR
        );
        assert_eq!(
            fa.flags(&fa.load(), false),
            ["--limit=1000", "--maxBulkBytes=10240kb"]
        );
        // --noDefaultDataFlags: no sizes
        let nd = Plan::new(Mode::Off, true, None, 8, &[]);
        assert_eq!(nd.flags(&nd.dump(None), true), ["--serverLimits=off"]);
        assert!(nd.flags(&nd.dump(None), false).is_empty());
    }

    /// 1.1-10 (the `off` profile): page size from the average stored document size.
    #[test]
    fn page_limit_shrinks_for_large_documents() {
        use crate::sizing::page_limit_for as f;
        assert_eq!(f(0), 1000); // unknown
        assert_eq!(f(500), 1000); // log lines
        assert_eq!(f(12 * 1024), 1000); // PMC articles (~12 KB stored)
        assert_eq!(f(16 * 1024), 1000);
        assert_eq!(f(80 * 1024), 204); // ~80 KB documents
        assert_eq!(f(1 << 20), 16);
        assert_eq!(f(10 << 20), 1);
        assert_eq!(f(u64::MAX), 1);
    }

    /// 1.2 auto: pages grow to 5000 small documents on a roomy heap, shrink for large documents
    /// and small heaps; worker-first keeps at least 2 workers; min is tighter than auto.
    #[test]
    fn auto_plan_sizes_from_fixture_limits() {
        use crate::sizing::Plan;
        use searchdump_rs::limits::{Limits, Mode, Node, Source};
        let node = |heap: u64| Node {
            name: "n".into(),
            data: true,
            heap_max: heap,
            parent_limit: Some(heap * 95 / 100),
            parent_estimated: Some(heap * 40 / 100),
            request_limit: Some(heap * 60 / 100),
            indexing_pressure: Some(heap / 10),
            max_content_length: Some(100 << 20),
            search_pool: Some(13),
            ..Default::default()
        };
        let lim = |heap| Limits {
            source: Source::Probed,
            nodes: vec![node(heap)],
            aws: false,
            reason: String::new(),
        };
        let small = Some((100_000u64, 100_000u64 * 400)); // log lines
        let big = Some((10_000u64, 10_000u64 * 80 * 1024)); // ~80 KB documents
        let roomy = Plan::new(Mode::Auto, false, Some(lim(1 << 30)), 8, &[small, big]);
        assert_eq!(roomy.dump(small).limit, 5000);
        assert_eq!(roomy.dump(small).reason, "default");
        let b = roomy.dump(big);
        assert!(b.limit < 1000 && b.limit >= 1, "{b:?}");
        let tight = Plan::new(Mode::Auto, false, Some(lim(256 << 20)), 8, &[small, big]);
        assert!(tight.dump(big).limit <= roomy.dump(big).limit);
        assert!(tight.fit_parallel >= 2 && tight.fit_parallel <= 8);
        assert!(tight.bulk_bytes <= roomy.bulk_bytes);
        let min = Plan::new(Mode::Min, false, Some(lim(256 << 20)), 8, &[small, big]);
        assert!(min.dump(big).limit <= tight.dump(big).limit);
        assert!(min.bulk_bytes <= tight.bulk_bytes);
        // the flags carry the sizes and stop the child probing again
        let f = roomy.flags(&roomy.dump(small), true);
        assert_eq!(f[0], "--limit=5000");
        assert!(
            f[1].starts_with("--maxBulkBytes=") && f[1].ends_with("kb"),
            "{f:?}"
        );
        assert_eq!(f[2], "--serverLimits=off");
        // parallel 1 stays 1
        assert_eq!(
            Plan::new(Mode::Auto, false, Some(lim(256 << 20)), 1, &[big]).fit_parallel,
            1
        );
        // L4-003 --pageBytes: the smaller of it and the budget; 4 MB of 400-byte documents
        // (x3 expansion) is 3495 per page, ~80 KB ones 17; below the 1.3 floor too
        let pb = Plan::new(Mode::Auto, false, Some(lim(1 << 30)), 8, &[small, big])
            .with_page_bytes(Some(4 << 20));
        assert_eq!(pb.dump(small).limit, (4u64 << 20) / (400 * 3));
        assert_eq!(pb.dump(small).reason, "page-bytes");
        assert_eq!(pb.dump(big).limit, (4u64 << 20) / (80 * 1024 * 3));
        let tiny = Plan::new(Mode::Auto, false, Some(lim(1 << 30)), 8, &[small])
            .with_page_bytes(Some(100_000));
        assert_eq!(tiny.dump(small).limit, 83);
        // a cap above the budget changes nothing
        let wide = Plan::new(Mode::Auto, false, Some(lim(1 << 30)), 8, &[small, big])
            .with_page_bytes(Some(1 << 40));
        assert_eq!(wide.dump(small), roomy.dump(small));
        assert_eq!(wide.dump(big), roomy.dump(big));
        // off and a fallback: each index cut to fit, never above the 1.1 page
        let off = Plan::new(Mode::Off, false, None, 8, &[]).with_page_bytes(Some(1 << 20));
        assert_eq!(off.dump(small).limit, 873);
        assert_eq!(off.dump(small).reason, "page-bytes");
        assert_eq!(off.dump(big).limit, 4);
        assert_eq!(
            off.dump(None).limit,
            crate::sizing::OFF_LIMIT,
            "size unknown"
        );
        assert_eq!(off.dump(None).reason, "off");
        let off_wide = Plan::new(Mode::Off, false, None, 8, &[]).with_page_bytes(Some(1 << 30));
        assert_eq!(off_wide.dump(small).limit, 1000);
        assert_eq!(off_wide.dump(small).reason, "off");
        // --noDefaultDataFlags: no sizing at all (the child still gets --pageBytes)
        let nd = Plan::new(Mode::Off, true, None, 8, &[]).with_page_bytes(Some(1 << 20));
        assert!(
            nd.flags(&nd.dump(small), true)
                .iter()
                .all(|f| !f.starts_with("--limit"))
        );
    }
}
