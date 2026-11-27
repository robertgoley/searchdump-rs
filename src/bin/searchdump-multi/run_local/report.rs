//! The result report of `run_local` (split step 13): ok/skipped lines on stdout, FAILED on stderr (each
//! flushed as it arrives, cluster-wide results first), the summary line and the exit code.

use crate::ctx::Outcome;
use anyhow::Result;
use std::io::Write;
use std::time::Duration;

#[derive(Debug, Default)]
pub(crate) struct Tally {
    pub(crate) ok: usize,
    pub(crate) skipped: usize,
    pub(crate) failed: Vec<String>,
}

impl Tally {
    /// Prints one result and counts it.
    pub(crate) fn record(&mut self, index: String, outcome: Outcome) {
        match outcome {
            Outcome::Done(s) => {
                self.ok += 1;
                println!("ok      {index}: {s}");
            }
            Outcome::Skipped(s) => {
                self.skipped += 1;
                println!("skipped {index}: {s}");
            }
            Outcome::Failed(s) => {
                eprintln!("FAILED  {index}: {s}");
                self.failed.push(index);
            }
        }
        let _ = std::io::stdout().flush();
    }

    /// The last line of a run (printed after an empty line), failures sorted by name.
    pub(crate) fn summary_line(&self, elapsed: Duration) -> String {
        format!(
            "\n{} done, {} skipped, {} failed in {:.0?}{}",
            self.ok,
            self.skipped,
            self.failed.len(),
            elapsed,
            if self.failed.is_empty() {
                String::new()
            } else {
                format!(": {}", self.failed.join(", "))
            }
        )
    }

    /// 130 after Ctrl-C (even with failures), else 1 with failures, else 0.
    pub(crate) fn exit_code(&self, cancelled: bool) -> i32 {
        if cancelled {
            return 130;
        }
        if self.failed.is_empty() { 0 } else { 1 }
    }
}

/// Reports the cluster-wide results, then each task as it finishes; a panicked task (JoinError)
/// ends the run with an error (exit 2), as before.
pub(crate) async fn collect(
    early: Vec<(String, Outcome)>,
    mut set: tokio::task::JoinSet<(String, Outcome)>,
) -> Result<Tally> {
    let mut tally = Tally::default();
    for (index, outcome) in early {
        tally.record(index, outcome);
    }
    while let Some(r) = set.join_next().await {
        let (index, outcome) = r?;
        tally.record(index, outcome);
    }
    tally.failed.sort();
    Ok(tally)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exit_codes_and_summary_line() {
        let mut t = Tally::default();
        assert_eq!(t.exit_code(false), 0);
        assert_eq!(t.exit_code(true), 130);
        t.ok = 2;
        t.skipped = 1;
        assert_eq!(
            t.summary_line(Duration::from_millis(1400)),
            "\n2 done, 1 skipped, 0 failed in 1s"
        );
        t.failed = vec!["a".into(), "b".into()];
        assert_eq!(t.exit_code(false), 1);
        assert_eq!(t.exit_code(true), 130, "130 wins over failures");
        assert_eq!(
            t.summary_line(Duration::from_secs(3)),
            "\n2 done, 1 skipped, 2 failed in 3s: a, b"
        );
    }

    #[tokio::test]
    async fn collect_reports_early_results_first_and_sorts_failures() {
        let mut set = tokio::task::JoinSet::new();
        set.spawn(async { ("z".to_string(), Outcome::Failed("x".into())) });
        let early = vec![
            ("m".to_string(), Outcome::Failed("y".into())),
            ("_cluster/policy".to_string(), Outcome::Done("1".into())),
        ];
        let t = collect(early, set).await.unwrap();
        assert_eq!((t.ok, t.skipped), (1, 0));
        assert_eq!(t.failed, vec!["m".to_string(), "z".to_string()]);
    }
}
