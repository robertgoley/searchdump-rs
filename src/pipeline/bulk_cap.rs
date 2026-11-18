//! 1.2-1c / 1.3: the bulk byte cap after HTTP 429 splits, shared by the bulk workers of a run.
//!
//! A size 429 (circuit breaker, indexing pressure) splits the refused request; later bulks are
//! kept at half its size, and with the 1.3 floor (decision 3) never below
//! [`crate::limits::BULK_FLOOR_BYTES`]. With the opt-in `growback` evaluation
//! (`SEARCHDUMP_LIMITS_EXPERIMENT=growback`, decision 2 open) the cap doubles again after a run
//! of accepted bulks, never above the run's starting size (`--maxBulkBytes`, as given or as
//! sized by `--serverLimits`). A grow that is answered by another size 429 (a "failed grow") doubles the
//! number of accepted bulks needed before the next grow (8, 16, 32, ... up to
//! [`GROW_AFTER_MAX`]), so a sustained 429 gives at most a logarithmic number of failed grows
//! per run instead of one every 9 bulks.
//!
//! The counters feed the end-of-run summary line (`bulk size summary: ...`) so a log shows how
//! often bulks were refused, split and grown, and what failed grows cost.

use std::sync::Mutex;
use std::time::Duration;

/// Accepted chunks in a row before a split cap may double again (the first time).
pub const GROW_AFTER: usize = 8;

/// The most accepted chunks a grow can wait for after repeated failed grows.
pub const GROW_AFTER_MAX: usize = 256;

/// The next split cap when growing back: double, and 0 (no cap) once it reaches `max`.
pub fn grown_cap(cap: usize, max: usize) -> usize {
    let next = cap.saturating_mul(2);
    if next >= max { 0 } else { next }
}

/// Counters for the end-of-run summary.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct CapStats {
    /// Bulk requests refused with a size 429 (every attempt, including the halves).
    pub rejects: u64,
    /// Refused requests that were split.
    pub splits: u64,
    /// Refused requests resent once at the same size (`retryfirst`).
    pub retryfirst: u64,
    /// Times the cap doubled (`growback`).
    pub grows: u64,
    /// Grows answered by a size 429 for a request above the pre-grow cap.
    pub failed_grows: u64,
    /// Bytes of refused requests above the pre-grow cap (sent again as halves or as-is).
    pub failed_grow_bytes: u64,
    /// Time of those refused requests (send to 429) plus the back-off after each.
    pub failed_grow_ms: u64,
    /// The lowest cap a split set (0 = no split).
    pub min_cap: u64,
}

#[derive(Debug, Default)]
struct State {
    /// 0 = no cap (`--maxBulkBytes` as given).
    cap: usize,
    streak: usize,
    /// Accepted chunks needed before the next grow (0 = [`GROW_AFTER`]).
    need: usize,
    /// The cap before the last grow (0 = none since the last split).
    grown_from: usize,
    /// The last grow was already counted as failed.
    grow_charged: bool,
    stats: CapStats,
}

/// The shared cap state of one run.
#[derive(Debug, Default)]
pub struct CapControl {
    state: Mutex<State>,
}

/// What a size 429 for a request of `len` bytes means for the grow-back accounting.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Reject {
    /// The request was larger than the cap before the last grow: the grow caused it.
    pub after_grow: bool,
    /// This is the first such rejection since that grow (counted as one failed grow).
    pub failed_grow: bool,
}

impl CapControl {
    /// The cap in force for `max` (`--maxBulkBytes`): `max`, or lower after a split.
    pub fn cap(&self, max: usize) -> usize {
        match self.state.lock().unwrap().cap {
            0 => max,
            c => c.min(max),
        }
    }

    pub fn stats(&self) -> CapStats {
        self.state.lock().unwrap().stats
    }

    /// A request of `len` bytes was refused with a size 429 (before a split or a `retryfirst`
    /// resend).
    pub fn on_reject(&self, len: usize) -> Reject {
        let mut s = self.state.lock().unwrap();
        s.stats.rejects += 1;
        let after_grow = s.grown_from != 0 && len > s.grown_from;
        let failed_grow = after_grow && !s.grow_charged;
        if failed_grow {
            // one failed grow per grow
            s.grow_charged = true;
            s.stats.failed_grows += 1;
            s.need = (s.need.max(GROW_AFTER) * 2).min(GROW_AFTER_MAX);
        }
        if after_grow {
            s.stats.failed_grow_bytes += len as u64;
        }
        Reject {
            after_grow,
            failed_grow,
        }
    }

    /// Time lost to a refused request above the pre-grow cap (its round trip and back-off).
    pub fn add_failed_grow_time(&self, d: Duration) {
        self.state.lock().unwrap().stats.failed_grow_ms += d.as_millis() as u64;
    }

    pub fn on_retryfirst(&self) {
        self.state.lock().unwrap().stats.retryfirst += 1;
    }

    /// A refused request of `len` bytes is split: later bulks are kept at
    /// [`crate::limits::split_cap`] (never raised by a split). Returns the new cap.
    pub fn on_split(&self, len: usize, floor: bool) -> usize {
        let mut s = self.state.lock().unwrap();
        s.stats.splits += 1;
        let next = crate::limits::split_cap(len, floor);
        s.cap = if s.cap == 0 { next } else { s.cap.min(next) };
        s.streak = 0;
        s.grown_from = 0;
        let c = s.cap as u64;
        s.stats.min_cap = if s.stats.min_cap == 0 {
            c
        } else {
            s.stats.min_cap.min(c)
        };
        s.cap
    }

    /// A chunk went through without a split (`growback` only). After the required run of
    /// them the cap doubles (0 once it reaches `max`). Returns the new cap when it grew.
    pub fn on_clean(&self, max: usize) -> Option<usize> {
        let mut s = self.state.lock().unwrap();
        if s.cap == 0 {
            return None;
        }
        s.streak += 1;
        if s.streak < s.need.max(GROW_AFTER) {
            return None;
        }
        s.streak = 0;
        s.grown_from = s.cap;
        s.grow_charged = false;
        s.cap = grown_cap(s.cap, max);
        s.stats.grows += 1;
        Some(if s.cap == 0 { max } else { s.cap })
    }

    /// The end-of-run summary, `None` when nothing was refused or grown.
    pub fn summary(&self, max: usize) -> Option<String> {
        let st = self.stats();
        if st.rejects == 0 && st.grows == 0 {
            return None;
        }
        Some(format!(
            "bulk size summary: size 429s {}, splits {}, retryfirst resends {}, grows {}, failed grows {} ({} ms, {} KB refused above the pre-grow cap), lowest cap {} KB, final cap {} KB",
            st.rejects,
            st.splits,
            st.retryfirst,
            st.grows,
            st.failed_grows,
            st.failed_grow_ms,
            st.failed_grow_bytes.div_ceil(1024),
            st.min_cap.div_ceil(1024),
            (self.cap(max) as u64).div_ceil(1024)
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::limits::BULK_FLOOR_BYTES;

    const MB: usize = 1 << 20;

    #[test]
    fn grown_cap_doubles_back_to_no_cap() {
        let max = 16 * MB;
        assert_eq!(grown_cap(MB, max), 2 * MB);
        assert_eq!(grown_cap(8 * MB, max), 0, "16 MB = max: no cap");
        assert_eq!(grown_cap(12 * MB, max), 0);
    }

    /// Grows after exactly GROW_AFTER clean chunks, doubles, and ends at `max` (no cap).
    #[test]
    fn grows_after_n_clean_bulks_and_never_above_max() {
        let c = CapControl::default();
        let max = 16 * MB;
        assert_eq!(c.on_clean(max), None, "no split yet: nothing to grow");
        assert_eq!(c.on_split(4 * MB, true), 2 * MB);
        for _ in 0..GROW_AFTER - 1 {
            assert_eq!(c.on_clean(max), None);
        }
        assert_eq!(c.on_clean(max), Some(4 * MB));
        for _ in 0..GROW_AFTER - 1 {
            c.on_clean(max);
        }
        assert_eq!(c.on_clean(max), Some(8 * MB));
        for _ in 0..GROW_AFTER {
            c.on_clean(max);
        }
        assert_eq!(c.cap(max), max, "back to --maxBulkBytes, not above");
        for _ in 0..100 {
            assert_eq!(c.on_clean(max), None, "no cap: nothing more to grow");
        }
        assert!(c.cap(max) <= max);
        assert_eq!(c.stats().grows, 3);
    }

    /// A user's (or auto-sized) lower `--maxBulkBytes` caps the cap at every step.
    #[test]
    fn never_above_a_user_or_auto_limit() {
        let c = CapControl::default();
        let user = 3 * MB / 2;
        c.on_split(2 * MB, true); // floor 1 MiB
        for _ in 0..1000 {
            c.on_clean(user);
            assert!(c.cap(user) <= user);
        }
        // a user limit below the floor still applies
        let tiny = 256 << 10;
        let c = CapControl::default();
        c.on_split(64 << 10, true);
        assert_eq!(c.cap(tiny), tiny);
    }

    /// A new 429 after a grow shrinks the cap again and counts one failed grow.
    #[test]
    fn shrinks_again_on_a_new_429() {
        let c = CapControl::default();
        let max = 16 * MB;
        c.on_split(4 * MB, true); // cap 2 MB
        for _ in 0..GROW_AFTER {
            c.on_clean(max);
        } // cap 4 MB, grown from 2 MB
        assert_eq!(c.cap(max), 4 * MB);
        let r = c.on_reject(4 * MB);
        assert!(r.after_grow && r.failed_grow);
        assert_eq!(c.on_split(4 * MB, true), 2 * MB);
        let r = c.on_reject(2 * MB);
        assert!(
            !r.after_grow,
            "a 429 at the pre-grow size is not the grow's fault"
        );
        let st = c.stats();
        assert_eq!(
            (st.grows, st.failed_grows, st.splits, st.rejects),
            (1, 1, 2, 2)
        );
        assert_eq!(st.failed_grow_bytes, 4 * MB as u64);
    }

    /// The floor holds through splits and grows: the cap never goes below 1 MiB.
    #[test]
    fn respects_the_floor() {
        let c = CapControl::default();
        let max = 16 * MB;
        for len in [16 * MB, 3 * MB, 700 << 10, 1, 100] {
            c.on_reject(len);
            c.on_split(len, true);
            assert!(c.cap(max) >= BULK_FLOOR_BYTES as usize);
            c.on_clean(max);
        }
        assert_eq!(c.stats().min_cap, BULK_FLOOR_BYTES);
        // without the floor (`nofloor`) a split goes to half the request
        let c = CapControl::default();
        c.on_split(700 << 10, false);
        assert_eq!(c.cap(max), 350 << 10);
    }

    /// Sustained 429 above 1 MiB: the server takes 1 MiB and refuses anything bigger, for
    /// 100,000 chunks. Without the back-off this would be a failed grow every 9 chunks
    /// (~11,000); with it the failed grows stay logarithmic and the grow waits saturate.
    #[test]
    fn no_oscillation_storm_under_a_sustained_429() {
        let c = CapControl::default();
        let max = 16 * MB;
        let fits = MB;
        c.on_reject(16 * MB);
        c.on_split(16 * MB, true); // 8 MB
        let mut sent = 0u64;
        let mut chunk_len = c.cap(max);
        for _ in 0..100_000 {
            // one chunk: split until it fits
            let mut len = chunk_len;
            while len > fits {
                c.on_reject(len);
                c.on_split(len, true);
                len /= 2;
                sent += 1;
            }
            sent += 1;
            c.on_clean(max);
            chunk_len = c.cap(max);
        }
        let st = c.stats();
        assert!(st.grows <= 400, "{st:?}");
        assert!(st.failed_grows <= st.grows);
        // 8 + 16 + ... + 256 and then one grow every 256 clean chunks
        assert!(st.failed_grows < 100_000 / 200, "{st:?}");
        assert!(
            st.rejects <= st.failed_grows + 10,
            "each failed grow costs one refused request: {st:?}"
        );
        assert!(sent < 100_000 + 1000);
        assert!(
            c.cap(max) <= 2 * fits,
            "ends near the size that fits: {}",
            c.cap(max)
        );
    }

    /// The same chunk sequence without growback (no `on_clean` calls): the cap stays at the
    /// first split result, floored, and nothing grows.
    #[test]
    fn without_growback_the_cap_only_goes_down() {
        let c = CapControl::default();
        let max = 16 * MB;
        c.on_split(16 * MB, true);
        c.on_split(4 * MB, true);
        c.on_split(8 * MB, true);
        assert_eq!(c.cap(max), 2 * MB, "a split never raises the cap");
        assert_eq!(c.stats().grows, 0);
    }

    /// Page halving and bulk splitting follow the same floor rule: from a large start both
    /// stop at their floor (10 documents / 1 MiB) and go below it only for a request that is
    /// refused at the floor itself (the bulk cap for later bulks never does). Page sizes have
    /// no grow-back: the halved size is kept for the slice (its end is detected from it).
    #[test]
    fn page_halving_and_bulk_split_share_the_floor_rule() {
        use crate::limits::{PAGE_FLOOR_DOCS, halved_page, split_cap};
        let (mut page, mut bulk) = (5000u64, 64 * MB);
        for _ in 0..20 {
            if page > PAGE_FLOOR_DOCS {
                page = halved_page(page, true);
            }
            bulk = split_cap(bulk, true);
        }
        assert_eq!(page, PAGE_FLOOR_DOCS);
        assert_eq!(bulk as u64, BULK_FLOOR_BYTES);
        assert_eq!(halved_page(PAGE_FLOOR_DOCS, true), PAGE_FLOOR_DOCS / 2);
        assert_eq!(
            split_cap(BULK_FLOOR_BYTES as usize, true) as u64,
            BULK_FLOOR_BYTES
        );
    }

    #[test]
    fn summary_line() {
        let c = CapControl::default();
        assert_eq!(c.summary(16 * MB), None);
        c.on_reject(4 * MB);
        c.on_split(4 * MB, true);
        let s = c.summary(16 * MB).unwrap();
        assert!(
            s.starts_with("bulk size summary: size 429s 1, splits 1,"),
            "{s}"
        );
        assert!(s.contains("final cap 2048 KB"), "{s}");
    }
}
