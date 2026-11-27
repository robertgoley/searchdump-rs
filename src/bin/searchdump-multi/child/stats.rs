//! Document counts reported by children (stats file or log line).

use std::fs;
use std::io::Read;
use std::path::Path;

// ----------------------------------------------------------------------------- child runs

/// 1.1-7: where a child's record count came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CountSource {
    /// The JSON the child wrote through `--statsFile`.
    StatsFile,
    /// Its `Total Writes: N` log line (searchdump and Node elasticdump print it).
    LogLine,
}

/// 1.1-7: what a finished child reported. `written` is `None` when it reported nothing usable
/// (the caller then counts the data file itself).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct ChildStats {
    pub(crate) written: Option<u64>,
    pub(crate) failed: u64,
    pub(crate) source: Option<CountSource>,
}

impl ChildStats {
    /// Documents the child read from its input: written plus failed (a load compares this with
    /// the target's `_count`, so documents the child skipped still show as a mismatch).
    pub(crate) fn docs_read(&self) -> Option<u64> {
        self.written.map(|w| w + self.failed)
    }
}

/// `--statsFile` JSON of a finished run (see searchdump's `write_stats_file`).
pub(crate) fn parse_stats_file(text: &str) -> Option<ChildStats> {
    let v: serde_json::Value = serde_json::from_str(text).ok()?;
    if v.get("complete").and_then(|c| c.as_bool()) != Some(true) {
        return None;
    }
    Some(ChildStats {
        written: Some(v.get("written")?.as_u64()?),
        failed: v.get("failed").and_then(|f| f.as_u64()).unwrap_or(0),
        source: Some(CountSource::StatsFile),
    })
}

/// The last `Total Writes: N` (and `Failed Writes: N`) of a child's log. searchdump prints
/// `Total Writes: N`; Node elasticdump prints `<date> | Total Writes: N`.
pub(crate) fn parse_total_writes(log: &str) -> Option<ChildStats> {
    fn number_after(line: &str, key: &str) -> Option<u64> {
        let rest = &line[line.rfind(key)? + key.len()..];
        let digits: String = rest
            .trim_start()
            .chars()
            .take_while(|c| c.is_ascii_digit())
            .collect();
        digits.parse().ok()
    }
    let mut written = None;
    let mut failed = 0;
    for line in log.lines() {
        if let Some(n) = number_after(line, "Total Writes:") {
            written = Some(n);
            failed = 0;
        } else if let Some(n) = number_after(line, "Failed Writes:") {
            failed = n;
        }
    }
    Some(ChildStats {
        written: Some(written?),
        failed,
        source: Some(CountSource::LogLine),
    })
}

/// The last `max` bytes of a file, as text (a child's log can be long; the counts are at its end).
pub(crate) fn read_tail(path: &Path, max: u64) -> Option<String> {
    use std::io::{Seek, SeekFrom};
    let mut f = fs::File::open(path).ok()?;
    let len = f.metadata().ok()?.len();
    f.seek(SeekFrom::Start(len.saturating_sub(max))).ok()?;
    let mut buf = Vec::new();
    f.read_to_end(&mut buf).ok()?;
    Some(String::from_utf8_lossy(&buf).into_owned())
}

/// 1.1-7: the counts of a child that exited 0: its `--statsFile`, else its log line, else none.
pub(crate) fn child_stats(stats_file: Option<&Path>, log: &Path) -> ChildStats {
    if let Some(p) = stats_file
        && let Ok(text) = fs::read_to_string(p)
        && let Some(s) = parse_stats_file(&text)
    {
        return s;
    }
    read_tail(log, 64 * 1024)
        .and_then(|t| parse_total_writes(&t))
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 1.1-7: counts from `--statsFile` (searchdump's format), the log line (searchdump and Node
    /// formats), and nothing (the caller then reads the data file).
    #[test]
    fn child_counts_from_stats_file_or_log_line() {
        let s = parse_stats_file(
            r#"{"version":1,"complete":true,"written":12345,"failed":2,"overwritten":0,"rejected":1}"#,
        )
        .unwrap();
        assert_eq!(s.written, Some(12345));
        assert_eq!(s.docs_read(), Some(12347));
        assert_eq!(s.source, Some(CountSource::StatsFile));
        assert!(parse_stats_file(r#"{"complete":false,"written":1}"#).is_none());
        assert!(parse_stats_file(r#"{"written":1}"#).is_none());
        assert!(parse_stats_file("not json").is_none());

        let rust = "[2026-11-11T10:00:00Z INFO] Starting dump\nTotal Writes: 4200\nOverwritten: 3 (documents that replaced one with the same _id; included in Total Writes)\nFailed Writes: 5\ndump complete\n";
        let s = parse_total_writes(rust).unwrap();
        assert_eq!((s.written, s.failed), (Some(4200), 5));
        assert_eq!(s.source, Some(CountSource::LogLine));
        let node = "Wed, 11 Nov 2026 15:00:00 GMT | starting dump\nWed, 11 Nov 2026 15:00:01 GMT | got 100 objects from source elasticsearch (offset: 0)\nWed, 11 Nov 2026 15:00:02 GMT | Total Writes: 100\nWed, 11 Nov 2026 15:00:02 GMT | dump complete\n";
        let s = parse_total_writes(node).unwrap();
        assert_eq!((s.written, s.failed), (Some(100), 0));
        // the last one counts (a log that holds an earlier summary)
        assert_eq!(
            parse_total_writes("Total Writes: 1\nTotal Writes: 2\n")
                .unwrap()
                .written,
            Some(2)
        );
        assert!(parse_total_writes("dump complete\n").is_none());
        assert!(parse_total_writes("Total Writes: n/a\n").is_none());

        let d = tempfile::tempdir().unwrap();
        let log = d.path().join("i.data.log");
        let stats = d.path().join("i.data.stats.json");
        fs::write(&log, "Total Writes: 9\n").unwrap();
        // no stats file: the log line
        assert_eq!(child_stats(Some(&stats), &log).written, Some(9));
        fs::write(&stats, r#"{"complete":true,"written":10,"failed":0}"#).unwrap();
        let s = child_stats(Some(&stats), &log);
        assert_eq!(
            (s.written, s.source),
            (Some(10), Some(CountSource::StatsFile))
        );
        // neither: nothing reported, the caller scans
        fs::write(&log, "something else\n").unwrap();
        assert_eq!(child_stats(None, &log), ChildStats::default());
        assert_eq!(
            child_stats(None, &d.path().join("missing.log")),
            ChildStats::default()
        );
    }
}
