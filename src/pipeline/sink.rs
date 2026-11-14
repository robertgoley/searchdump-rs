//! Output sinks. Every source produces batches of `\n`-terminated JSON
//! documents (Elasticsearch hits or file values); sinks consume them.

use anyhow::Result;
use sonic_rs::JsonValueTrait;

use super::bulk::{BulkSink, WriteReport};
use super::json_split::single_line_slice;

pub enum Sink {
    File(Box<FileSink>),
    Bulk(Box<BulkSink>),
}

pub use super::file_sink::{FileOptions, FileSink};

/// Lazily located `_source` of one hit line, if present.
pub fn source_of(line: &[u8]) -> Option<sonic_rs::LazyValue<'_>> {
    sonic_rs::get(line, ["_source"]).ok()
}

fn source_only_lines(buf: &[u8], out: &mut Vec<u8>) {
    out.clear();
    for line in crate::pipeline::bulk::nonempty_lines(buf) {
        match source_of(line) {
            Some(src) => out.extend_from_slice(&single_line_slice(src.as_raw_str().as_bytes())),
            None => {
                // Not a hit (e.g. file→file of plain documents): keep as-is.
                let v: Option<sonic_rs::Value> = sonic_rs::from_slice(line).ok();
                if v.as_ref().is_some_and(|v| v.get("_source").is_some()) {
                    log::debug!("Could not extract _source; writing document unchanged");
                }
                out.extend_from_slice(line);
            }
        }
        out.push(b'\n');
    }
}

impl Sink {
    /// Write one batch of `\n`-terminated documents. The batch is shared (`Bytes`) so a bulk
    /// sink with `--concurrency` > 1 hands it to its worker task without copying it (L-808r).
    pub async fn write_all(&mut self, data: bytes::Bytes) -> Result<()> {
        let buf: &[u8] = &data;
        crate::memplan::observe(buf);
        match self {
            Sink::File(f) => {
                let docs = memchr::memchr_iter(b'\n', buf).count() as u64;
                if f.source_only() {
                    let mut scratch = std::mem::take(&mut f.scratch);
                    source_only_lines(buf, &mut scratch);
                    let r = f.write_batch(&scratch).await;
                    // L-802a: an outlier batch must not keep its size resident.
                    super::json_split::release_excess(&mut scratch);
                    f.scratch = scratch;
                    r?;
                } else {
                    f.write_batch(buf).await?;
                }
                f.written += docs;
                Ok(())
            }
            Sink::Bulk(b) => b.write_batch(data).await,
        }
    }

    /// The input number of the next document (bulk sinks name it when they reject one).
    pub fn set_next_doc(&mut self, n: u64) {
        if let Sink::Bulk(b) = self {
            b.set_next_doc(n);
        }
    }

    pub async fn flush(&mut self) -> Result<()> {
        match self {
            Sink::File(f) => f.flush().await,
            Sink::Bulk(_) => Ok(()),
        }
    }

    /// Writes into a cluster (bulk requests).
    pub fn is_bulk(&self) -> bool {
        matches!(self, Sink::Bulk(_))
    }

    pub fn is_stdout(&self) -> bool {
        match self {
            Sink::File(f) => f.is_stdout(),
            Sink::Bulk(_) => false,
        }
    }

    pub async fn finalize(self) -> Result<WriteReport> {
        match self {
            Sink::File(f) => {
                let report = WriteReport {
                    written: f.written,
                    failed: 0,
                    ..Default::default()
                };
                f.finalize().await?;
                Ok(report)
            }
            Sink::Bulk(mut b) => {
                b.drain().await?;
                Ok(b.report)
            }
        }
    }

    pub async fn abort(self) {
        match self {
            Sink::File(f) => f.abort().await,
            Sink::Bulk(mut b) => b.abort(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn source_only_extracts_raw_source() {
        let mut out = vec![];
        source_only_lines(
            b"{\"_index\":\"i\",\"_id\":\"1\",\"_source\":{\"n\":12345678901234567890123,\"s\":\"\\u00e9\"}}\n{\"plain\":1}\n",
            &mut out,
        );
        assert_eq!(
            String::from_utf8(out).unwrap(),
            "{\"n\":12345678901234567890123,\"s\":\"\\u00e9\"}\n{\"plain\":1}\n"
        );
    }
}
