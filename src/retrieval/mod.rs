mod context;
mod extract;
mod messages;
mod pit;
mod pit_search;
mod progress;
mod retrieval_task;
mod scroll;
mod search_after;
pub mod search_body;
mod slice_state;
mod worker;

use anyhow::Result;
use bytesize::ByteSize;
use indicatif::{MultiProgress, ProgressDrawTarget};
use log::{debug, info};
use sonic_rs::{JsonValueMutTrait, Value, json};
use std::future::Future;
use std::sync::{
    Arc, OnceLock,
    atomic::{AtomicU64, Ordering},
};
use std::time::Instant;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use crate::cli::SearchType;
use crate::http::{EsClient, PitApi};
use crate::pipeline::{DocWindow, Sink};

use self::context::RetrievalContext;
use self::messages::RetrievalMessage;
use self::pit::SharedPitCoordinator;
use self::progress::setup_progress_bars;
use self::retrieval_task::{RetryPolicy, is_cancelled};
use self::slice_state::SliceState;

/// Why the pipeline token was cancelled. `Interrupt` (Ctrl+C) makes the dump
/// fail after cleanup; `OutputClosed` (stdout reader went away) makes the
/// cancellation cascade count as an early, successful stop.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CancelReason {
    Interrupt,
    /// stdout reader went away, or `--size` documents were written.
    OutputClosed,
}

/// Whether every hit must carry `_source` (see [`extract`]); set once per run.
static REQUIRE_SOURCE: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
/// Whether the search body itself may make the cluster leave `_source` out, so a missing
/// `_source` is not the mapping's doing: [`BodySource`] as u8 (L-455a, L-455b).
static BODY_DROPS_SOURCE: std::sync::atomic::AtomicU8 = std::sync::atomic::AtomicU8::new(0);

/// What in the search body can leave `_source` out of the hits.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BodySource {
    /// nothing: a hit without `_source` comes from the mapping
    Kept = 0,
    /// `stored_fields` without `_source` (L-455a)
    StoredFields = 1,
    /// a `_source` filter other than `true` (L-455b)
    Filter = 2,
}

impl BodySource {
    /// Reads a search body's `_source` / `stored_fields`.
    pub fn of(body: &sonic_rs::Value) -> Self {
        use sonic_rs::JsonValueTrait;
        match body.get("_source") {
            Some(v) if v.as_bool() == Some(true) => Self::Kept,
            Some(_) => Self::Filter,
            None if body.get("stored_fields").is_some() => Self::StoredFields,
            None => Self::Kept,
        }
    }
}

/// Fuzz hook (TST-API-084): search response extraction, both pagination shapes.
#[cfg(any(feature = "fuzz", test))]
pub fn fuzz_extract_batch(data: &[u8]) {
    let bytes = bytes::Bytes::copy_from_slice(data);
    for st in [
        crate::cli::SearchType::Scroll,
        crate::cli::SearchType::PointInTime,
    ] {
        if let Ok(b) = extract::extract_batch(&bytes, &st) {
            assert!(b.output.buffer.len() <= data.len() + b.output.doc_count as usize);
            // INJ-015 fuzz_extract_batch: every output line is exactly one JSON object
            let lines: Vec<&[u8]> = b
                .output
                .buffer
                .split(|c| *c == b'\n')
                .filter(|l| !l.is_empty())
                .collect();
            assert_eq!(lines.len() as u64, b.output.doc_count);
            for l in lines {
                assert!(
                    crate::pipeline::bulk::check_doc_line(l).is_ok(),
                    "{}",
                    String::from_utf8_lossy(l)
                );
            }
        }
        let _ = extract::extract_batch_metadata(&bytes, &st);
    }
}

pub fn set_require_source(on: bool) {
    REQUIRE_SOURCE.store(on, std::sync::atomic::Ordering::Relaxed);
}

pub(crate) fn require_source() -> bool {
    REQUIRE_SOURCE.load(std::sync::atomic::Ordering::Relaxed)
}

/// L-455c: the pattern of a search body's `_source` filter that excludes every field
/// (`{"excludes":["*"]}`, `{"exclude":"**"}`, ...): every hit then comes back with an
/// empty `_source` (`{}`), which is known before the first search.
pub fn source_filter_excludes_all(body: &sonic_rs::Value) -> Option<String> {
    use sonic_rs::{JsonContainerTrait, JsonValueTrait};
    let filter = body.get("_source")?;
    let all = |v: &sonic_rs::Value| {
        v.as_str()
            .filter(|p| !p.is_empty() && p.chars().all(|c| c == '*'))
            .map(str::to_string)
    };
    ["excludes", "exclude"].iter().find_map(|k| {
        let v = filter.get(*k)?;
        match v.as_array() {
            Some(a) => a.iter().find_map(all),
            None => all(v),
        }
    })
}

pub fn set_body_drops_source(cause: BodySource) {
    BODY_DROPS_SOURCE.store(cause as u8, std::sync::atomic::Ordering::Relaxed);
}

pub(crate) fn body_drops_source() -> BodySource {
    match BODY_DROPS_SOURCE.load(std::sync::atomic::Ordering::Relaxed) {
        1 => BodySource::StoredFields,
        2 => BodySource::Filter,
        _ => BodySource::Kept,
    }
}

/// Everything the Elasticsearch/OpenSearch reader needs.
pub struct EsSourceOptions {
    pub client: EsClient,
    pub index: Option<String>,
    /// Prepared search body (query, sort, page size).
    pub search_body: Value,
    pub search_type: SearchType,
    pub pit_api: PitApi,
    /// Default PIT sort for this cluster flavor.
    pub pit_sort: Value,
    pub scroll_ttl: String,
    pub pit_keep_alive: String,
    pub slices: usize,
    pub workers: usize,
    pub buffer_size: usize,
    pub quiet: bool,
    pub retry: RetryPolicy,
    /// `--scrollId`
    pub resume_scroll_id: Option<String>,
    /// Compare the documents retrieved with the server's exact `hits.total` at the end
    /// (false with `--size`/`--offset` or `--scrollId`, where fewer documents are expected).
    pub reconcile_total: bool,
}

/// Aborts the wrapped task when dropped, so the Ctrl+C listener dies with
/// `dump_data` on every return path instead of leaking.
struct AbortOnDrop(tokio::task::JoinHandle<()>);

impl Drop for AbortOnDrop {
    fn drop(&mut self) {
        self.0.abort();
    }
}

async fn create_output_target_and_shared_pit<
    O,
    P,
    OutputFut,
    AbortFut,
    PitFut,
    OutputFactory,
    AbortFactory,
    PitFactory,
>(
    search_type: &SearchType,
    create_output_target: OutputFactory,
    abort_output_target: AbortFactory,
    open_shared_pit: PitFactory,
) -> Result<(O, Option<P>)>
where
    OutputFut: Future<Output = Result<O>>,
    AbortFut: Future<Output = ()>,
    PitFut: Future<Output = Result<P>>,
    OutputFactory: FnOnce() -> OutputFut,
    AbortFactory: FnOnce(O) -> AbortFut,
    PitFactory: FnOnce() -> PitFut,
{
    let output_target = create_output_target().await?;
    let shared_pit = if matches!(search_type, SearchType::PointInTime) {
        match open_shared_pit().await {
            Ok(shared_pit) => Some(shared_pit),
            Err(error) => {
                abort_output_target(output_target).await;
                return Err(error);
            }
        }
    } else {
        None
    };

    Ok((output_target, shared_pit))
}

/// Pick the pipeline's root-cause error. The output task's error (e.g. disk
/// full) outranks retrieval errors, which outrank worker errors: the latter
/// two are usually "channel closed" cascades of the former. A `DumpCancelled`
/// cascade never outranks a real error regardless of position. Every
/// suppressed error is still logged.
/// Shorten a scroll/PIT context id for logs (L-208 U-1): the id is a capability
/// that lets anyone with cluster access read the open search context.
pub(crate) fn redact_context_id(id: &str) -> String {
    let head: String = id.chars().take(8).collect();
    format!("{head}…({} bytes)", id.len())
}

fn select_root_cause(
    output_error: Option<anyhow::Error>,
    retrieval_error: Option<anyhow::Error>,
    worker_error: Option<anyhow::Error>,
) -> Option<anyhow::Error> {
    let mut root_cause: Option<anyhow::Error> = None;
    for error in [output_error, retrieval_error, worker_error]
        .into_iter()
        .flatten()
    {
        let replaces_current = match &root_cause {
            None => true,
            Some(current) => is_cancelled(current) && !is_cancelled(&error),
        };
        if replaces_current {
            if let Some(previous) = root_cause.replace(error) {
                log::error!("Suppressed pipeline error: {:#}", previous);
            }
        } else {
            log::error!("Suppressed pipeline error: {:#}", error);
        }
    }
    root_cause
}

/// Close the shared PIT as best-effort cleanup. Failures are only warned
/// about: the PIT expires on its own via its keep-alive, and a close failure
/// must never override the pipeline outcome.
async fn close_pit_best_effort(
    client: &EsClient,
    api: PitApi,
    shared_pit: &Option<SharedPitCoordinator>,
) {
    let Some(shared_pit) = shared_pit else {
        return;
    };
    if let Err(e) = SharedPitCoordinator::close(client, api, &shared_pit.latest_id()).await {
        log::warn!("Failed to close PIT (it will expire via keep-alive): {e}");
    }
}

/// Read documents from Elasticsearch/OpenSearch into `sink`, honouring
/// `window` (`--offset`/`--size`). Returns the number of documents written.
pub async fn dump_data(
    opts: EsSourceOptions,
    sink: Sink,
    window: DocWindow,
) -> Result<crate::pipeline::WriteReport> {
    let client = &opts.client;
    let pit_api = opts.pit_api;
    debug!(
        "Starting data dump operation for index: {}",
        opts.index.as_deref().unwrap_or("<all>")
    );
    let start_time = Instant::now();

    // Pipeline-wide cancellation: fired by Ctrl+C, output failures, and
    // failing slices. `cancel_reason` records WHY (set at most once) so the
    // final outcome can distinguish an interrupt from a closed stdout reader.
    let cancel = CancellationToken::new();
    let cancel_reason: Arc<OnceLock<CancelReason>> = Arc::new(OnceLock::new());

    // The first SIGINT/SIGTERM (crate::signals) cancels the pipeline for an orderly shutdown (scroll/PIT
    // cleanup, staged-output abort); a second one force-quits with the
    // conventional 130 exit status.
    let graceful = crate::signals::graceful();
    let signal_task = tokio::spawn({
        let cancel = cancel.clone();
        let reason = cancel_reason.clone();
        let interrupt = crate::signals::token();
        async move {
            interrupt.cancelled().await;
            let _ = reason.set(CancelReason::Interrupt);
            cancel.cancel();
        }
    });
    let _signal_guard = (AbortOnDrop(signal_task), graceful);

    let retry = opts.retry;
    let search_body = opts.search_body.clone();
    let pit_index = match (&opts.search_type, opts.index.as_deref()) {
        (SearchType::PointInTime, None) => {
            return Err(anyhow::anyhow!(
                "Point-in-time search needs an index in --input (or --input-index)"
            ));
        }
        (_, index) => index.unwrap_or("").to_string(),
    };

    // Create channels for the pipeline
    let buffer_size = opts.buffer_size;
    let slices = if opts.slices == 1 {
        log::warn!(
            "--slices 1 is equivalent to an unsliced dump (Elasticsearch rejects slice.max=1); running unsliced"
        );
        0
    } else {
        opts.slices
    };

    // Setup for sliced search
    let use_sliced_scroll = slices > 0;
    let num_slices = if use_sliced_scroll { slices } else { 1 };

    // Extra workers beyond the slice count would still receive round-robined
    // jobs, but each slice keeps at most one unreplied job in flight, so
    // concurrent extraction can never exceed num_slices; more workers add
    // threads without adding parallelism.
    let workers = opts.workers.min(num_slices);
    if workers < opts.workers {
        info!(
            "Clamping worker count from {} to {} to match the {} search slice(s)",
            opts.workers, workers, num_slices
        );
    }

    debug!(
        "Setting up pipeline with {} workers, buffer size {}, {} slices",
        workers, buffer_size, num_slices
    );

    // Create a channel for each worker
    let mut worker_txs = Vec::with_capacity(workers);
    let mut worker_rxs = Vec::with_capacity(workers);

    for _ in 0..workers {
        let (tx, rx) = mpsc::channel(buffer_size);
        worker_txs.push(tx);
        worker_rxs.push(rx);
    }

    // Create a channel for processed results
    let (processed_tx, mut processed_rx) = mpsc::channel(buffer_size);

    // Shared counters for stats
    let processed_count = Arc::new(AtomicU64::new(0));
    let processed_bytes = Arc::new(AtomicU64::new(0));
    let retrieved_count = Arc::new(AtomicU64::new(0));
    let retrieved_bytes = Arc::new(AtomicU64::new(0));

    // Setup progress bars if not in quiet mode
    let (_multi_progress, input_bar, output_bar) = if !opts.quiet {
        let mp = MultiProgress::with_draw_target(ProgressDrawTarget::stderr());
        let (ib, ob) = setup_progress_bars(&mp);
        (Some(mp), ib, ob)
    } else {
        (None, None, None)
    };

    let mut estimated_total_hits = 0u64;
    let mut total_hits_are_exact = true;

    // Start worker tasks before any staged output or retrieval work begins.
    let worker_tasks = worker::spawn_worker_tasks(
        worker_rxs,
        processed_tx.clone(),
        Arc::clone(&processed_count),
        Arc::clone(&processed_bytes),
        output_bar.clone(),
        start_time,
    )?;

    let (output_target, shared_pit) = match create_output_target_and_shared_pit(
        &opts.search_type,
        || async { Ok(sink) },
        |output_target: Sink| async move { output_target.abort().await },
        || {
            SharedPitCoordinator::open(
                client,
                pit_api,
                &pit_index,
                &opts.pit_keep_alive,
                num_slices,
                retry,
                cancel.clone(),
            )
        },
    )
    .await
    {
        Ok(targets) => targets,
        // Ctrl+C while the PIT open is retrying with backoff surfaces here as
        // a DumpCancelled; report it as the interrupt it is.
        Err(error) => {
            if matches!(cancel_reason.get(), Some(CancelReason::Interrupt)) && is_cancelled(&error)
            {
                return Err(crate::signals::interrupted_error());
            }
            return Err(error);
        }
    };

    // Drop the sender to signal no more processing will happen after worker clones are done.
    drop(processed_tx);

    // Output task. Any write failure cancels the pipeline so retrieval stops
    // promptly instead of dumping into a dead sink. A broken pipe on stdout is
    // the reader legitimately closing early (e.g. `| head`): record it, then
    // drain-and-discard so blocked workers/slices can unwind and observe the
    // cancellation.
    let output_task = tokio::spawn({
        let cancel = cancel.clone();
        let cancel_reason = Arc::clone(&cancel_reason);
        async move {
            let mut output_target = output_target;
            let mut window = window;
            let mut written = 0u64;
            // L-647: hits handed to the output, counting those --offset skipped
            let mut consumed = 0u64;
            if window.is_exhausted() {
                let _ = cancel_reason.set(CancelReason::OutputClosed);
                cancel.cancel();
                while processed_rx.recv().await.is_some() {}
            }

            // Process output as it comes in
            while let Some(processed) = processed_rx.recv().await {
                if matches!(cancel_reason.get(), Some(CancelReason::Interrupt)) {
                    // L-647: nothing new after a signal; the resume offset stays exact
                    while processed_rx.recv().await.is_some() {}
                    break;
                }
                let docs = memchr::memchr_iter(b'\n', &processed.buffer).count() as u64;
                consumed += docs;
                let data = bytes::Bytes::from(processed.buffer);
                let (slice, docs) = window.apply(&data, docs);
                let slice = data.slice_ref(slice);
                written += docs;
                if let Err(e) = output_target.write_all(slice).await {
                    if output_target.is_stdout() && crate::output::is_broken_pipe(&e) {
                        let _ = cancel_reason.set(CancelReason::OutputClosed);
                        cancel.cancel();
                        while processed_rx.recv().await.is_some() {}
                        return Ok((output_target, written, consumed, window));
                    }
                    cancel.cancel();
                    output_target.abort().await;
                    return Err(anyhow::anyhow!("Failed to write batch buffer: {}", e));
                }
                if window.is_exhausted() {
                    // --size reached: stop retrieval; the cascade counts as success.
                    let _ = cancel_reason.set(CancelReason::OutputClosed);
                    cancel.cancel();
                    while processed_rx.recv().await.is_some() {}
                    break;
                }
            }

            if let Err(e) = output_target.flush().await {
                if output_target.is_stdout() && crate::output::is_broken_pipe(&e) {
                    let _ = cancel_reason.set(CancelReason::OutputClosed);
                    cancel.cancel();
                    return Ok((output_target, written, consumed, window));
                }
                cancel.cancel();
                output_target.abort().await;
                return Err(anyhow::anyhow!("Failed to flush writer: {}", e));
            }
            Ok((output_target, written, consumed, window))
        }
    });

    let total_hits_count = Arc::new(AtomicU64::new(0));
    let ctx = RetrievalContext {
        client: client.clone(),
        index: opts.index.as_deref().map(Arc::<str>::from),
        pit_sort: opts.pit_sort.clone(),
        worker_txs: worker_txs.clone(),
        total_hits_count: Arc::clone(&total_hits_count),
        input_bar: input_bar.clone(),
        output_bar: output_bar.clone(),
        retrieved_count: Arc::clone(&retrieved_count),
        retrieved_bytes: Arc::clone(&retrieved_bytes),
        start_time,
        cancel: cancel.clone(),
        retry,
        resume_scroll_id: opts.resume_scroll_id.as_deref().map(Arc::<str>::from),
    };

    debug!(
        "Starting {} retrieval task{}",
        num_slices,
        if num_slices > 1 { "s" } else { "" }
    );

    // Start retrieval tasks for each slice
    let mut retrieval_tasks = Vec::with_capacity(num_slices);

    for slice_id in 0..num_slices {
        let mut slice_search_body = search_body.clone();
        if use_sliced_scroll {
            slice_search_body.as_object_mut().unwrap().insert(
                &"slice",
                json!({
                    "id": slice_id,
                    "max": num_slices
                }),
            );
            info!("Starting slice {}/{}", slice_id + 1, num_slices);
        }

        let slice_state = SliceState::new(slice_id, slice_id % workers, slice_search_body);
        let task = retrieval_task::spawn_retrieval_task(
            ctx.clone(),
            slice_state,
            opts.search_type,
            opts.scroll_ttl.clone(),
            opts.pit_keep_alive.clone(),
            shared_pit.clone(),
        );

        retrieval_tasks.push(task);
    }

    // First retrieval error becomes a root-cause candidate; the rest are
    // logged. A real failure always displaces a `DumpCancelled` cascade, no
    // matter which slice finished first.
    let mut retrieval_error: Option<anyhow::Error> = None;

    // Wait for all retrieval tasks to complete and get total hits
    for task in retrieval_tasks {
        let error = match task.await {
            Ok(Ok(slice_hits)) => {
                estimated_total_hits += slice_hits.value;
                total_hits_are_exact &= slice_hits.is_exact;
                continue;
            }
            // `context` (not a reformatting `anyhow!`) keeps the DumpCancelled
            // sentinel downcastable for the final outcome selection.
            Ok(Err(e)) => e.context("Retrieval task failed"),
            Err(e) => anyhow::anyhow!("Retrieval task panicked: {}", e),
        };

        let replaces_current = match &retrieval_error {
            None => true,
            Some(current) => is_cancelled(current) && !is_cancelled(&error),
        };
        if replaces_current {
            if let Some(previous) = retrieval_error.replace(error) {
                log::error!(
                    "Suppressed additional retrieval task failure: {:#}",
                    previous
                );
            }
        } else {
            log::error!("Suppressed additional retrieval task failure: {:#}", error);
        }
    }

    if retrieval_error.is_none() && opts.reconcile_total && total_hits_are_exact {
        let retrieved = retrieved_count.load(Ordering::Relaxed);
        if let Some(e) = reconcile_total(&opts.search_type, retrieved, estimated_total_hits) {
            retrieval_error = Some(e);
        }
    }

    // Final update of progress bar total
    if let Some(ib) = &input_bar {
        debug!(
            "Input bar final length: {}, Total hits: {}",
            ib.length().unwrap_or(0),
            estimated_total_hits
        );
        let final_input_length = if total_hits_are_exact {
            estimated_total_hits
        } else {
            estimated_total_hits.max(retrieved_count.load(Ordering::Relaxed))
        };
        if ib.length().unwrap_or(0) != final_input_length {
            ib.set_length(final_input_length);
            ib.set_message("Retrieving...");
        }
    }
    if let Some(ob) = &output_bar {
        let final_output_length = if total_hits_are_exact {
            estimated_total_hits
        } else {
            estimated_total_hits.max(processed_count.load(Ordering::Relaxed))
        };
        if ob.length().unwrap_or(0) != final_output_length {
            ob.set_length(final_output_length);
        }
    }

    // Signal that we're done retrieving by sending Done to all workers
    for (i, tx) in worker_txs.iter().enumerate() {
        if let Err(e) = tx.send(RetrievalMessage::Done).await {
            log::warn!("Failed to send Done signal to worker {}: {}", i, e);
        }
    }

    // Wait for all workers to finish. First failure becomes a root-cause
    // candidate; the rest are logged.
    let mut worker_error: Option<anyhow::Error> = None;
    for (i, task) in worker_tasks.into_iter().enumerate() {
        if let Err(e) = task.wait().await {
            let error = anyhow::anyhow!("Worker {} processing failed: {}", i, e);
            if worker_error.is_none() {
                worker_error = Some(error);
            } else {
                log::error!("Suppressed additional worker failure: {:#}", error);
            }
        }
    }

    // Wait for output to finish
    let mut completed_output_target = None;
    let mut written_docs = 0u64;
    let mut resume_at = (0u64, None);
    let output_error = match output_task.await {
        Ok(Ok((output_target, written, consumed, window))) => {
            completed_output_target = Some(output_target);
            written_docs = written;
            resume_at = (consumed, window.remaining());
            None
        }
        Ok(Err(e)) => Some(anyhow::anyhow!("Output task failed: {}", e)),
        Err(e) => Some(anyhow::anyhow!("Output task panicked: {}", e)),
    };

    let mut root_cause = select_root_cause(output_error, retrieval_error, worker_error);

    // The stdout reader closing early (e.g. `| head`) is a legitimate way to
    // end a dump: the resulting DumpCancelled cascade counts as success. A
    // real error that surfaced alongside it still wins.
    if matches!(cancel_reason.get(), Some(CancelReason::OutputClosed))
        && root_cause.as_ref().is_some_and(is_cancelled)
    {
        root_cause = None;
    }

    // Ctrl+C makes the dump fail deterministically after the normal
    // error-path cleanup; a concurrent real error still takes priority over
    // the generic interrupt message.
    if matches!(cancel_reason.get(), Some(CancelReason::Interrupt)) {
        let error = match root_cause {
            Some(error) if !is_cancelled(&error) => error,
            Some(cascade) => {
                log::debug!(
                    "Suppressed cancellation cascade after interrupt: {:#}",
                    cascade
                );
                crate::signals::interrupted_error()
            }
            None => crate::signals::interrupted_error(),
        };
        let plain_interrupt = error
            .downcast_ref::<crate::signals::Interrupted>()
            .is_some();
        let error = match completed_output_target {
            // L-647 (TST-BRK-SIG-002): a copy into a cluster finishes its in-flight bulk
            // requests and says where to resume; a file output is removed as before.
            Some(target) if plain_interrupt && target.is_bulk() => match target.finalize().await {
                Ok(r) => {
                    let resume = if num_slices > 1 {
                        crate::signals::Resume::NotByOffset(
                            "a sliced dump has no single order, so --offset cannot resume it; rerun it (documents keep their _id, so the ones already written are overwritten, not duplicated, with --bulkAction index)",
                        )
                    } else {
                        crate::signals::Resume::Offset {
                            offset: resume_at.0,
                            size: resume_at.1,
                            exact: false,
                        }
                    };
                    crate::signals::stopped_error(r.written, r.failed, r.overwritten, resume)
                }
                Err(e) => e.context(error.to_string()),
            },
            Some(target) => {
                target.abort().await;
                error
            }
            None => error,
        };
        close_pit_best_effort(client, pit_api, &shared_pit).await;
        return Err(error);
    }

    if let Some(error) = root_cause {
        if let Some(output_target) = completed_output_target {
            output_target.abort().await;
        }
        close_pit_best_effort(client, pit_api, &shared_pit).await;
        return Err(error);
    }

    if matches!(cancel_reason.get(), Some(CancelReason::OutputClosed)) {
        log::info!("Output closed or --size reached; stopped dump early");
    }

    let output_target = completed_output_target.ok_or_else(|| {
        anyhow::anyhow!("Output task completed without returning an output target")
    })?;

    // Make the dump durable FIRST; PIT cleanup afterwards can no longer
    // destroy it and is best-effort only.
    let report = output_target.finalize().await?;
    close_pit_best_effort(client, pit_api, &shared_pit).await;

    let elapsed = start_time.elapsed();
    let count = processed_count.load(Ordering::Relaxed);
    let bytes = processed_bytes.load(Ordering::Relaxed);
    let elapsed_secs = elapsed.as_secs_f64().max(1e-6);
    let bytes_per_sec = bytes as f64 / elapsed_secs;
    let docs_per_sec = count as f64 / elapsed_secs;

    // Finish progress bars
    let retrieved_docs = retrieved_count.load(Ordering::Relaxed);
    let final_retrieved_bytes = retrieved_bytes.load(Ordering::Relaxed);
    let retrieved_bytes_per_sec = final_retrieved_bytes as f64 / elapsed_secs;
    if let Some(ib) = input_bar {
        let summary = if total_hits_are_exact {
            format!(
                "Retrieved {} docs ({} @ {} /s)",
                retrieved_docs,
                ByteSize(final_retrieved_bytes),
                ByteSize(retrieved_bytes_per_sec as u64)
            )
        } else {
            format!(
                "Retrieved {} docs ({} @ {} /s, Elasticsearch reported total >= {})",
                retrieved_docs,
                ByteSize(final_retrieved_bytes),
                ByteSize(retrieved_bytes_per_sec as u64),
                estimated_total_hits
            )
        };
        ib.finish_with_message(summary);
    }
    if let Some(ob) = output_bar {
        ob.finish_with_message(format!(
            "Processed {} docs ({} @ {} /s)",
            count,
            ByteSize(bytes),
            ByteSize(bytes_per_sec as u64)
        ));
    }

    debug!(
        "Final stats: {} documents, {} bytes, {:?} elapsed",
        count, bytes, elapsed
    );

    info!(
        "Dump completed: {} documents ({}) in {:.2?} ({:.0} docs/sec, {}/sec)",
        count,
        ByteSize(bytes),
        elapsed,
        docs_per_sec,
        ByteSize(bytes_per_sec as u64)
    );

    let _ = written_docs;
    Ok(report)
}

/// BRK-SRV-009 / TST-API-012: the documents retrieved against the server's exact total.
/// `search_after` without a PIT is not a snapshot, so only a shortfall is an error there (the
/// usual cause is a sort without a unique tiebreaker: documents tied on the sort values at a
/// page boundary are skipped); more documents than the total is a concurrent-write warning.
fn reconcile_total(search_type: &SearchType, retrieved: u64, total: u64) -> Option<anyhow::Error> {
    match search_type {
        SearchType::SearchAfter if retrieved < total => Some(anyhow::anyhow!(
            "--searchAfter retrieved {retrieved} of {total} documents: the sort in --searchBody is \
             probably not unique (documents with equal sort values at a page boundary are skipped); \
             end the sort with a unique field, or use --searchType pit"
        )),
        SearchType::SearchAfter => {
            if retrieved > total {
                log::warn!(
                    "--searchAfter retrieved {retrieved} documents but the index reported {total} \
                     (documents were written during the dump)"
                );
            }
            None
        }
        // scroll and PIT read a snapshot: anything but exactly the total means pages were lost
        // or the server ended the read early (BRK-SRV-009)
        _ if retrieved != total => {
            let how = if matches!(search_type, SearchType::Scroll) {
                "scroll"
            } else {
                "point-in-time"
            };
            Some(anyhow::anyhow!(
                "the {how} read retrieved {retrieved} of {total} documents, but a {how} is a snapshot whose exact total is {total}: the server ended the read early or returned short or extra pages. Nothing is reported as complete; re-run the dump (with --searchType pit if scroll contexts expire)"
            ))
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn snapshot_reads_must_match_the_exact_total() {
        for st in [
            crate::cli::SearchType::Scroll,
            crate::cli::SearchType::PointInTime,
        ] {
            let e = super::reconcile_total(&st, 7, 10).expect("shortfall");
            assert!(format!("{e}").contains("retrieved 7 of 10"), "{e}");
            assert!(super::reconcile_total(&st, 11, 10).is_some(), "surplus");
            assert!(super::reconcile_total(&st, 10, 10).is_none());
        }
    }

    #[test]
    fn search_after_shortfall_is_an_error_and_surplus_only_a_warning() {
        let sa = crate::cli::SearchType::SearchAfter;
        let e = super::reconcile_total(&sa, 100, 1000).expect("shortfall is an error");
        assert!(format!("{e}").contains("retrieved 100 of 1000"), "{e}");
        assert!(super::reconcile_total(&sa, 1000, 1000).is_none());
        assert!(super::reconcile_total(&sa, 1001, 1000).is_none());
    }

    use super::{create_output_target_and_shared_pit, select_root_cause};
    use anyhow::anyhow;
    use std::sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    };

    #[test]
    fn select_root_cause_prefers_output_error_over_cascades() {
        let root = select_root_cause(
            Some(anyhow!("disk full")),
            Some(anyhow!("send batch failed")),
            Some(anyhow!("processed channel closed")),
        )
        .expect("root cause expected");

        assert_eq!(root.to_string(), "disk full");
    }

    #[test]
    fn select_root_cause_falls_back_to_retrieval_then_worker_error() {
        let root = select_root_cause(None, Some(anyhow!("retrieval")), Some(anyhow!("worker")))
            .expect("root cause expected");
        assert_eq!(root.to_string(), "retrieval");

        let root =
            select_root_cause(None, None, Some(anyhow!("worker"))).expect("root cause expected");
        assert_eq!(root.to_string(), "worker");
    }

    #[test]
    fn select_root_cause_is_none_without_errors() {
        assert!(select_root_cause(None, None, None).is_none());
    }

    #[test]
    fn select_root_cause_prefers_real_errors_over_cancellation_cascades() {
        use super::retrieval_task::DumpCancelled;

        // A cancellation cascade in a higher-priority slot must not mask the
        // real failure that triggered it.
        let root = select_root_cause(
            Some(anyhow::Error::new(DumpCancelled).context("Output task failed")),
            Some(anyhow!("HTTP 500 from slice 2")),
            None,
        )
        .expect("root cause expected");
        assert!(root.to_string().contains("HTTP 500"));

        // With nothing but cascades, the first one is still reported.
        let root = select_root_cause(
            None,
            Some(anyhow::Error::new(DumpCancelled).context("Retrieval task failed")),
            None,
        )
        .expect("root cause expected");
        assert!(super::retrieval_task::is_cancelled(&root));
    }

    #[tokio::test]
    async fn create_output_target_and_shared_pit_skips_pit_when_output_fails() {
        let pit_opened = Arc::new(AtomicBool::new(false));

        let error = create_output_target_and_shared_pit(
            &crate::cli::SearchType::PointInTime,
            || async { Err::<(), anyhow::Error>(anyhow!("output target failed")) },
            |_output_target| async {},
            {
                let pit_opened = Arc::clone(&pit_opened);
                move || async move {
                    pit_opened.store(true, Ordering::Relaxed);
                    Ok::<_, anyhow::Error>("pit-opened")
                }
            },
        )
        .await
        .unwrap_err()
        .to_string();

        assert!(error.contains("output target failed"));
        assert!(!pit_opened.load(Ordering::Relaxed));
    }

    #[tokio::test]
    async fn create_output_target_and_shared_pit_aborts_output_when_pit_open_fails() {
        let output_aborted = Arc::new(AtomicBool::new(false));

        let error = create_output_target_and_shared_pit(
            &crate::cli::SearchType::PointInTime,
            || async { Ok::<_, anyhow::Error>("output-target") },
            {
                let output_aborted = Arc::clone(&output_aborted);
                move |_output_target| async move {
                    output_aborted.store(true, Ordering::Relaxed);
                }
            },
            || async { Err::<&'static str, anyhow::Error>(anyhow!("pit open failed")) },
        )
        .await
        .unwrap_err()
        .to_string();

        assert!(error.contains("pit open failed"));
        assert!(output_aborted.load(Ordering::Relaxed));
    }
}
