//! Start-of-run memory estimate (TST-BRK-RES-001). README "Memory":
//! peak RSS ≈ (bufferSize + slices + concurrency) × limit × avg_doc_size × 1.5 + 10 MB.
//! The average document size is only known once the first batch arrives, so the check runs
//! then, once, against the smaller of the cgroup limit and `MemAvailable`.
//!
//! 1.1: pipeline stages that hold extra copies of a batch (the compression
//! thread's queue, 1.1-9; the bulk batch encoded while one is in flight,
//! 1.1-6) add them with [`add_batches`]; they count like `--bufferSize`.

use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

struct Plan {
    batches: u64,
    limit: u64,
}

static PLAN: OnceLock<Plan> = OnceLock::new();
static CHECKED: AtomicBool = AtomicBool::new(false);
static EXTRA: AtomicU64 = AtomicU64::new(0);

/// Count `n` more batches held by a pipeline stage (call before the first batch is written).
pub fn add_batches(n: u64) {
    EXTRA.fetch_add(n, Ordering::Relaxed);
}

/// Batches the estimate counts: the planned ones plus pipeline extras.
fn batches(plan: &Plan) -> u64 {
    plan.batches.saturating_add(EXTRA.load(Ordering::Relaxed))
}

pub fn set(buffer_size: usize, slices: usize, concurrency: usize, limit: usize) {
    let _ = PLAN.set(Plan {
        batches: (buffer_size as u64)
            .saturating_add(slices as u64)
            .saturating_add(concurrency as u64),
        limit: limit as u64,
    });
}

/// Estimated peak bytes for documents of `avg` bytes.
pub fn estimate(batches: u64, limit: u64, avg: u64) -> u64 {
    (batches.saturating_mul(limit).saturating_mul(avg)).saturating_mul(3) / 2 + (10 << 20)
}

/// Called with each batch written; checks the first one only.
pub fn observe(buf: &[u8]) {
    if CHECKED.swap(true, Ordering::Relaxed) || buf.is_empty() {
        return;
    }
    let Some(plan) = PLAN.get() else { return };
    let docs = memchr::memchr_iter(b'\n', buf).count().max(1) as u64;
    let avg = buf.len() as u64 / docs;
    let batches = batches(plan);
    let need = estimate(batches, plan.limit, avg);
    if let Some((avail, what)) = available()
        && need > avail
    {
        log::warn!(
            "memory: this run may need ~{} MB ((bufferSize + slices + concurrency + pipeline) {} x --limit {} x ~{} B per document x 1.5 + 10 MB) but only {} MB are available ({what}); lower --limit, --bufferSize, --concurrency or --slices to avoid an out-of-memory kill",
            need >> 20,
            batches,
            plan.limit,
            avg,
            avail >> 20
        );
    }
}

/// The tighter of this process's cgroup v2 `memory.max` (walking up) and `MemAvailable`.
fn available() -> Option<(u64, &'static str)> {
    let mut best: Option<(u64, &'static str)> = None;
    if let Ok(cg) = std::fs::read_to_string("/proc/self/cgroup")
        && let Some(rel) = cg.lines().find_map(|l| l.strip_prefix("0::"))
    {
        let mut p = std::path::PathBuf::from("/sys/fs/cgroup").join(rel.trim_start_matches('/'));
        loop {
            if let Ok(v) = std::fs::read_to_string(p.join("memory.max"))
                && let Ok(n) = v.trim().parse::<u64>()
                && best.is_none_or(|(b, _)| n < b)
            {
                best = Some((n, "cgroup memory.max"));
            }
            if !p.pop() || p == std::path::Path::new("/sys/fs") {
                break;
            }
        }
    }
    if let Ok(mi) = std::fs::read_to_string("/proc/meminfo") {
        let kb = mi
            .lines()
            .find_map(|l| l.strip_prefix("MemAvailable:"))
            .and_then(|v| v.trim().trim_end_matches("kB").trim().parse::<u64>().ok());
        if let Some(kb) = kb {
            let n = kb << 10;
            if best.is_none_or(|(b, _)| n < b) {
                best = Some((n, "MemAvailable"));
            }
        }
    }
    best
}

#[cfg(test)]
mod tests {
    #[test]
    fn estimate_follows_the_readme_formula() {
        // defaults: 16 + 0 + 1 batches of 100 docs of 1 KB
        assert_eq!(
            super::estimate(17, 100, 1024),
            17 * 100 * 1024 * 3 / 2 + (10 << 20)
        );
        assert_eq!(super::estimate(u64::MAX, 2, 2), u64::MAX / 2 + (10 << 20));
    }

    #[test]
    fn pipeline_extras_are_counted() {
        let plan = super::Plan {
            batches: 17,
            limit: 100,
        };
        let before = super::batches(&plan);
        super::add_batches(3);
        // other tests in this binary may add too; ours is included
        assert!(super::batches(&plan) >= before + 3);
        assert!(super::batches(&plan) >= 20);
    }
}
