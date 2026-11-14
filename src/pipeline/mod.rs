//! Generic source → sink plumbing shared by every transfer direction.

pub mod bulk;
pub mod file_sink;
pub mod file_source;
pub mod json_split;
pub mod sink;
pub mod window;

use anyhow::Result;
use tokio_util::sync::CancellationToken;

pub use bulk::{BulkAction, BulkOptions, BulkSink, WriteReport};
pub use sink::{FileSink, Sink};
pub use window::DocWindow;

/// Copy documents from a file (or stdin) into `sink`, honouring
/// `--offset`/`--size`; documents over `max_doc` bytes (`--maxDocMb`) are
/// a fatal error. Returns the number of documents written.
pub async fn file_to_sink(
    input: impl Into<file_source::InputLoc>,
    limit: usize,
    max_doc: Option<usize>,
    mut window: DocWindow,
    mut sink: Sink,
    cancel: CancellationToken,
) -> Result<WriteReport> {
    let mut rx = file_source::spawn_file_source(input, limit, max_doc, 4, cancel.clone());
    // input documents handed to the sink so far, counting those --offset skipped
    let mut seen = 0u64;
    // Ok(true): stopped by a signal before the input ended
    let result: Result<bool> = async {
        if window.is_exhausted() {
            return Ok(false);
        }
        loop {
            let batch = tokio::select! {
                b = rx.recv() => b,
                _ = cancel.cancelled() => None,
            };
            let Some(batch) = batch else { break };
            let batch = batch?;
            let skipped = window.pending_skip().min(batch.docs);
            let data = bytes::Bytes::from(batch.buf);
            let (slice, docs) = window.apply(&data, batch.docs);
            let slice = data.slice_ref(slice);
            sink.set_next_doc(seen + skipped + 1);
            seen += batch.docs;
            if docs > 0
                && let Err(e) = sink.write_all(slice).await
            {
                if sink.is_stdout() && crate::output::is_broken_pipe(&e) {
                    log::info!("stdout closed by reader; stopping early");
                    return Ok(false);
                }
                return Err(e);
            }
            if window.is_exhausted() {
                return Ok(false);
            }
        }
        Ok(cancel.is_cancelled() && crate::signals::received().is_some())
    }
    .await;
    cancel.cancel(); // stop the reader thread if we ended early
    match result {
        // L-647 (TST-BRK-SIG-002): a restore into a cluster finishes its in-flight bulk
        // requests, then reports where to resume. A file output is removed as before.
        Ok(true) if sink.is_bulk() => match sink.finalize().await {
            Ok(r) => Err(crate::signals::stopped_error(
                r.written,
                r.failed,
                r.overwritten,
                crate::signals::Resume::Offset {
                    offset: seen,
                    size: window.remaining(),
                    exact: true,
                },
            )),
            Err(e) => Err(e.context(crate::signals::interrupted_error().to_string())),
        },
        Ok(true) => {
            sink.abort().await;
            Err(crate::signals::interrupted_error())
        }
        Ok(false) => {
            if let Err(e) = sink.flush().await
                && !(sink.is_stdout() && crate::output::is_broken_pipe(&e))
            {
                sink.abort().await;
                return Err(e);
            }
            sink.finalize().await
        }
        Err(e) => {
            sink.abort().await;
            Err(e)
        }
    }
}
