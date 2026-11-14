use anyhow::{Result, anyhow};
use bytes::Bytes;
use bytesize::ByteSize;
use log::warn;
use reqwest::Method;
#[cfg(test)]
use sonic_rs::JsonValueTrait;
use sonic_rs::{JsonContainerTrait, JsonValueMutTrait, Value};
use std::sync::atomic::Ordering;
use tokio::sync::oneshot;

use super::{
    context::RetrievalContext,
    messages::{BatchJob, BatchProcessingFailure, RetrievalMessage},
    pit::SharedPitCoordinator,
    pit_search, scroll, search_after,
    slice_state::SliceState,
};
use crate::cli::SearchType;
use crate::http::{Body, EsClient};

#[cfg(test)]
pub(crate) fn latest_pit_id(response: &Value) -> Option<String> {
    response.get("pit_id").as_str().map(|id| id.to_string())
}

pub(crate) use crate::http::retry::{
    DumpCancelled, RetryMode, RetryPolicy, is_cancelled, send_checked_with_retry,
};

/// Default PIT sort when the user supplied none: `["_shard_doc"]` on
/// Elasticsearch, `[{"_doc":"asc"},{"_id":"asc"}]` on OpenSearch (which
/// rejects `_shard_doc` and loses documents with a `_doc`-only sort).
pub(crate) fn ensure_pit_sort(search_body: &mut Value, default_sort: &Value) {
    let body = search_body
        .as_object_mut()
        .expect("PIT search body must be an object");
    // An empty `sort` array is as unusable for `search_after` pagination as a
    // missing one, so both get the default tiebreaker.
    let missing = match body.get(&"sort") {
        None => true,
        Some(value) => value.as_array().map(|a| a.is_empty()).unwrap_or(false),
    };
    if missing {
        body.insert(&"sort", default_sort.clone());
    }
}

pub(crate) fn abort_shared_pit(shared_pit: &Option<SharedPitCoordinator>, error: &anyhow::Error) {
    if let Some(shared_pit) = shared_pit {
        shared_pit.abort(error.to_string());
    }
}

/// The cluster no longer has the scroll or point-in-time context: it expired, or the node
/// holding it restarted (TST-BRK-NET-009, BRK-SRV-007).
pub(crate) fn search_context_lost(error: &anyhow::Error) -> bool {
    let text = format!("{error:#}");
    [
        "search_context_missing",
        "No search context found",
        "SearchContextMissing",
    ]
    .iter()
    .any(|n| text.contains(n))
}

pub(crate) fn context_lost_error(
    error: anyhow::Error,
    slice_id: usize,
    kind: &str,
) -> anyhow::Error {
    let knob = if kind == "scroll" {
        "--scrollTime"
    } else {
        "--pitKeepAlive"
    };
    error.context(crate::http::ContextLoss(format!(
        "Slice {slice_id}: the {kind} context no longer exists on the cluster: it expired (raise {knob} when pages are slow to write), the index was deleted, or the node holding it restarted; nothing was committed. Re-run the dump"
    )))
}

/// A point-in-time page that failed for good (TST-BRK-NET-009): say whether the context is
/// gone and that a rerun is the way out. Cancellation passes through unchanged.
pub(crate) fn pit_page_failure(error: anyhow::Error, slice_id: usize) -> anyhow::Error {
    if crate::http::is_cancelled(&error) {
        return error;
    }
    if search_context_lost(&error) {
        return context_lost_error(error, slice_id, "point-in-time");
    }
    let status = error
        .downcast_ref::<crate::http::EsHttpError>()
        .map(|h| h.status);
    if status.is_some_and(|s| (400..500).contains(&s) && s != 408) {
        return error;
    }
    error.context(format!(
        "Slice {slice_id}: a point-in-time page failed after its retries; nothing was committed. Re-run the dump (if the cluster restarted, its point-in-time context is gone; a larger --retryAttempts/--retryDelay rides out a short outage only while the cluster keeps the context)"
    ))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct TotalHitsEstimate {
    pub(crate) value: u64,
    pub(crate) is_exact: bool,
}

#[cfg(test)]
pub(crate) fn extract_total_hits_estimate(response: &Value) -> TotalHitsEstimate {
    let value = response["hits"]["total"]["value"]
        .as_u64()
        .unwrap_or_else(|| {
            response["hits"]["hits"]
                .as_array()
                .map_or(0, |hits| hits.len() as u64)
        });
    let relation = response["hits"]["total"]["relation"]
        .as_str()
        .unwrap_or("eq");

    TotalHitsEstimate {
        value,
        is_exact: relation == "eq",
    }
}

pub(crate) fn record_total_hits(
    ctx: &RetrievalContext,
    estimate: TotalHitsEstimate,
) -> TotalHitsEstimate {
    let previous_total = ctx
        .total_hits_count
        .fetch_add(estimate.value, Ordering::Relaxed);
    let new_total = previous_total + estimate.value;

    if let (Some(input_bar), Some(output_bar)) = (&ctx.input_bar, &ctx.output_bar) {
        input_bar.set_length(new_total);
        output_bar.set_length(new_total);
    }

    estimate
}

pub(crate) async fn process_response_batch(
    ctx: &RetrievalContext,
    state: &mut SliceState,
    response_bytes: Bytes,
    search_type: &SearchType,
    batch_bytes: u64,
) -> std::result::Result<super::extract::BatchMetadata, BatchProcessingFailure> {
    let recovery_bytes = response_bytes.clone();
    let worker = state.dispatch_worker(ctx.worker_txs.len());
    let (reply_tx, reply_rx) = oneshot::channel();
    ctx.worker_txs[worker]
        .send(RetrievalMessage::Batch(BatchJob {
            response_bytes,
            search_type: *search_type,
            reply_tx,
        }))
        .await
        .map_err(|error| {
            recover_batch_processing_failure(
                BatchProcessingFailure::from(anyhow!(
                    "Failed to send batch to worker {} for slice {}: {}",
                    worker,
                    state.slice_id,
                    error
                )),
                &recovery_bytes,
                search_type,
            )
        })?;

    let metadata = match reply_rx.await {
        Ok(Ok(metadata)) => metadata,
        Ok(Err(failure)) => {
            return Err(recover_batch_processing_failure(
                failure,
                &recovery_bytes,
                search_type,
            ));
        }
        Err(error) => {
            return Err(recover_batch_processing_failure(
                BatchProcessingFailure::from(anyhow!(
                    "Worker {} dropped metadata reply for slice {}: {}",
                    worker,
                    state.slice_id,
                    error
                )),
                &recovery_bytes,
                search_type,
            ));
        }
    };

    state.retrieved_hits += metadata.doc_count;
    let current_retrieved = ctx
        .retrieved_count
        .fetch_add(metadata.doc_count, Ordering::Relaxed)
        + metadata.doc_count;
    let current_bytes = ctx
        .retrieved_bytes
        .fetch_add(batch_bytes, Ordering::Relaxed)
        + batch_bytes;

    if let Some(input_bar) = &ctx.input_bar {
        input_bar.set_position(current_retrieved);
        let elapsed_secs = ctx.start_time.elapsed().as_secs_f64().max(1e-6);
        input_bar.set_message(format!(
            "{} @ {} /s",
            ByteSize(current_bytes),
            ByteSize((current_bytes as f64 / elapsed_secs) as u64)
        ));
    }

    Ok(metadata)
}

fn recover_batch_processing_failure(
    failure: BatchProcessingFailure,
    response_bytes: &Bytes,
    search_type: &SearchType,
) -> BatchProcessingFailure {
    failure.with_fallback_metadata(
        super::extract::extract_batch_metadata(response_bytes, search_type).ok(),
    )
}

pub(crate) async fn cleanup_search_context(
    client: &EsClient,
    search_type: &SearchType,
    id: &str,
    slice_id: usize,
) {
    match search_type {
        SearchType::Scroll => {
            let body = Body::json(&serde_json::json!({ "scroll_id": [id] }));
            match client
                .send(Method::DELETE, &["_search", "scroll"], &[], false, body)
                .await
            {
                Ok(resp) if resp.status().is_success() || resp.status().as_u16() == 404 => {}
                Ok(resp) => warn!(
                    "Slice {}: Failed to clear scroll context: HTTP {}",
                    slice_id,
                    resp.status()
                ),
                Err(e) => warn!("Slice {}: Failed to clear scroll context: {}", slice_id, e),
            }
        }
        SearchType::PointInTime | SearchType::SearchAfter => {}
    }
}

/// Spawn a retrieval task for a specific slice. A slice failing with a real
/// error (anything but the [`DumpCancelled`] cascade sentinel) cancels the
/// shared token so sibling slices fail fast instead of dumping to completion
/// behind a doomed pipeline.
pub fn spawn_retrieval_task(
    ctx: RetrievalContext,
    mut state: SliceState,
    search_type: SearchType,
    scroll_ttl: String,
    pit_keep_alive: String,
    shared_pit: Option<SharedPitCoordinator>,
) -> tokio::task::JoinHandle<Result<TotalHitsEstimate>> {
    tokio::spawn(async move {
        let slice_id = state.slice_id;
        let result = match search_type {
            SearchType::Scroll => scroll::run_scroll_slice(&ctx, &mut state, &scroll_ttl).await,
            SearchType::PointInTime => {
                pit_search::run_pit_slice(&ctx, &mut state, &pit_keep_alive, shared_pit).await
            }
            SearchType::SearchAfter => search_after::run_search_after_slice(&ctx, &mut state).await,
        };

        if let Err(error) = &result
            && !is_cancelled(error)
        {
            log::debug!(
                "Slice {}: failing fast, cancelling remaining pipeline work: {:#}",
                slice_id,
                error
            );
            ctx.cancel.cancel();
        }

        result
    })
}

#[cfg(test)]
mod tests {
    use bytes::Bytes;
    use indicatif::{ProgressBar, ProgressDrawTarget};
    use sonic_rs::json;
    use std::{
        sync::{Arc, atomic::AtomicU64},
        time::Instant,
    };
    use tokio::sync::mpsc;
    use url::Url;

    use crate::cli::SearchType;

    use super::super::{
        context::RetrievalContext, messages::RetrievalMessage, slice_state::SliceState,
        worker::spawn_worker_task,
    };

    fn test_context(worker_txs: Vec<mpsc::Sender<RetrievalMessage>>) -> RetrievalContext {
        RetrievalContext {
            client: crate::http::EsClient::new(
                &crate::http::ClientOptions::new(Url::parse("http://localhost:9200/").unwrap()),
                crate::http::gate::RequestGate::default(),
            )
            .unwrap(),
            index: Some(Arc::<str>::from("test-index")),
            pit_sort: json!(["_shard_doc"]),
            worker_txs,
            total_hits_count: Arc::new(AtomicU64::new(0)),
            input_bar: None,
            output_bar: None,
            retrieved_count: Arc::new(AtomicU64::new(0)),
            retrieved_bytes: Arc::new(AtomicU64::new(0)),
            start_time: Instant::now(),
            cancel: tokio_util::sync::CancellationToken::new(),
            retry: super::RetryPolicy::none(),
            resume_scroll_id: None,
        }
    }

    #[test]
    fn latest_pit_id_prefers_search_response_id() {
        let response = json!({
            "pit_id": "pit-from-search",
            "hits": {
                "hits": []
            }
        });

        assert_eq!(
            super::latest_pit_id(&response).as_deref(),
            Some("pit-from-search")
        );
    }

    #[test]
    fn ensure_pit_sort_sets_shard_doc_when_missing() {
        let mut body = json!({
            "query": { "match_all": {} },
            "pit": { "id": "pit-id", "keep_alive": "1m" }
        });

        super::ensure_pit_sort(&mut body, &json!(["_shard_doc"]));

        assert_eq!(body["sort"], json!(["_shard_doc"]));
    }

    #[test]
    fn ensure_pit_sort_replaces_empty_sort_array() {
        let mut body = json!({
            "query": { "match_all": {} },
            "pit": { "id": "pit-id", "keep_alive": "1m" },
            "sort": []
        });

        super::ensure_pit_sort(&mut body, &json!(["_shard_doc"]));

        assert_eq!(body["sort"], json!(["_shard_doc"]));
    }

    #[test]
    fn ensure_pit_sort_preserves_user_supplied_sort() {
        let mut body = json!({
            "query": { "match_all": {} },
            "pit": { "id": "pit-id", "keep_alive": "1m" },
            "sort": [{ "created_at": "asc" }]
        });

        super::ensure_pit_sort(&mut body, &json!(["_shard_doc"]));

        assert_eq!(body["sort"], json!([{ "created_at": "asc" }]));
    }

    #[test]
    fn extract_total_hits_estimate_marks_gte_as_inexact() {
        let response = json!({
            "hits": {
                "total": { "value": 10000, "relation": "gte" },
                "hits": []
            }
        });

        let estimate = super::extract_total_hits_estimate(&response);
        assert_eq!(
            estimate,
            super::TotalHitsEstimate {
                value: 10000,
                is_exact: false,
            }
        );
    }

    #[test]
    fn extract_total_hits_estimate_marks_eq_as_exact() {
        let response = json!({
            "hits": {
                "total": { "value": 42, "relation": "eq" },
                "hits": []
            }
        });

        let estimate = super::extract_total_hits_estimate(&response);
        assert_eq!(
            estimate,
            super::TotalHitsEstimate {
                value: 42,
                is_exact: true,
            }
        );
    }

    #[tokio::test]
    async fn process_response_batch_returns_worker_metadata_and_updates_counters() {
        let (worker_tx, worker_rx) = mpsc::channel(2);
        let (processed_tx, mut processed_rx) = mpsc::channel(2);
        let worker = spawn_worker_task(
            0,
            worker_rx,
            processed_tx,
            Arc::new(AtomicU64::new(0)),
            Arc::new(AtomicU64::new(0)),
            None,
            Instant::now(),
        )
        .unwrap();

        let mut ctx = test_context(vec![worker_tx]);
        let input_bar = ProgressBar::new(0);
        input_bar.set_draw_target(ProgressDrawTarget::hidden());
        ctx.input_bar = Some(input_bar.clone());

        let mut state = SliceState::new(3, 0, json!({"size": 10}));
        let response_bytes = Bytes::from_static(
            br#"{"pit_id":"pit-next","hits":{"total":{"value":1,"relation":"eq"},"hits":[{"_id":"1","sort":[1,"a"],"_source":{"message":"a"}}]}}"#,
        );

        let metadata = super::process_response_batch(
            &ctx,
            &mut state,
            response_bytes,
            &SearchType::PointInTime,
            130,
        )
        .await
        .unwrap();

        assert_eq!(metadata.next_pit_id.as_deref(), Some("pit-next"));
        assert_eq!(
            metadata.last_sort_raw.as_deref(),
            Some(br#"[1,"a"]"#.as_slice())
        );
        assert!(metadata.next_scroll_id.is_none());
        assert_eq!(metadata.doc_count, 1);
        assert!(!metadata.hits_are_empty);
        assert_eq!(metadata.total_hits.value, 1);
        assert!(metadata.total_hits.is_exact);

        assert_eq!(state.retrieved_hits, 1);
        assert_eq!(
            ctx.retrieved_count
                .load(std::sync::atomic::Ordering::Relaxed),
            1
        );
        assert_eq!(
            ctx.retrieved_bytes
                .load(std::sync::atomic::Ordering::Relaxed),
            130
        );
        assert_eq!(input_bar.position(), 1);
        assert!(state.current_id.is_none());
        assert!(state.search_after.is_none());

        let processed = processed_rx.recv().await.unwrap();
        assert_eq!(processed.doc_count, 1);
        assert_eq!(
            String::from_utf8(processed.buffer).unwrap(),
            "{\"_id\":\"1\",\"sort\":[1,\"a\"],\"_source\":{\"message\":\"a\"}}\n"
        );

        ctx.worker_txs[0]
            .send(RetrievalMessage::Done)
            .await
            .unwrap();
        worker.wait().await.unwrap();
    }

    #[tokio::test]
    async fn process_response_batch_returns_metadata_before_forward_failure_then_fails_on_dead_worker()
     {
        let (worker_tx, worker_rx) = mpsc::channel(1);
        let (processed_tx, processed_rx) = mpsc::channel(1);
        drop(processed_rx);

        let worker = spawn_worker_task(
            0,
            worker_rx,
            processed_tx,
            Arc::new(AtomicU64::new(0)),
            Arc::new(AtomicU64::new(0)),
            None,
            Instant::now(),
        )
        .unwrap();

        let mut ctx = test_context(vec![worker_tx]);
        let input_bar = ProgressBar::new(0);
        input_bar.set_draw_target(ProgressDrawTarget::hidden());
        ctx.input_bar = Some(input_bar.clone());

        let mut state = SliceState::new(7, 0, json!({"size": 10}));
        let response_bytes = Bytes::from_static(
            br#"{"pit_id":"pit-after-failure","hits":{"total":{"value":1,"relation":"eq"},"hits":[{"_id":"1","sort":[9,"z"],"_source":{"message":"z"}}]}}"#,
        );

        // The reply arrives before the worker forwards the output buffer, so
        // the first batch succeeds even though the forward is doomed to fail.
        let metadata = super::process_response_batch(
            &ctx,
            &mut state,
            response_bytes.clone(),
            &SearchType::PointInTime,
            144,
        )
        .await
        .expect("metadata reply must precede the forward failure");

        assert_eq!(metadata.next_pit_id.as_deref(), Some("pit-after-failure"));
        assert_eq!(
            metadata.last_sort_raw.as_deref(),
            Some(br#"[9,"z"]"#.as_slice())
        );
        assert_eq!(metadata.doc_count, 1);
        assert!(!metadata.hits_are_empty);

        assert_eq!(state.retrieved_hits, 1);
        assert_eq!(
            ctx.retrieved_count
                .load(std::sync::atomic::Ordering::Relaxed),
            1
        );
        assert_eq!(
            ctx.retrieved_bytes
                .load(std::sync::atomic::Ordering::Relaxed),
            144
        );
        assert_eq!(input_bar.position(), 1);

        // The forward failure kills the worker thread and closes its channel.
        let worker_error = worker.wait().await.unwrap_err().to_string();
        assert!(worker_error.contains("Failed to send processed batch"));

        // The next dispatch to the dead worker fails fast with locally
        // recovered fallback metadata instead of hanging.
        let failure = super::process_response_batch(
            &ctx,
            &mut state,
            response_bytes,
            &SearchType::PointInTime,
            144,
        )
        .await
        .unwrap_err();

        let metadata = failure
            .metadata()
            .expect("metadata should be recovered locally");
        assert_eq!(metadata.next_pit_id.as_deref(), Some("pit-after-failure"));
        assert!(
            failure
                .into_error()
                .to_string()
                .contains("Failed to send batch to worker")
        );

        // The failed dispatch must not advance any counters.
        assert_eq!(state.retrieved_hits, 1);
        assert_eq!(
            ctx.retrieved_count
                .load(std::sync::atomic::Ordering::Relaxed),
            1
        );
        assert_eq!(
            ctx.retrieved_bytes
                .load(std::sync::atomic::Ordering::Relaxed),
            144
        );
    }

    #[tokio::test]
    async fn process_response_batch_preserves_metadata_when_worker_channel_is_closed() {
        let (worker_tx, worker_rx) = mpsc::channel(1);
        drop(worker_rx);

        let ctx = test_context(vec![worker_tx]);
        let mut state = SliceState::new(9, 0, json!({"size": 10}));
        let response_bytes = Bytes::from_static(
            br#"{"pit_id":"pit-send-failed","hits":{"total":{"value":1,"relation":"eq"},"hits":[{"_id":"1","sort":[11,"send"],"_source":{"message":"send"}}]}}"#,
        );

        let failure = super::process_response_batch(
            &ctx,
            &mut state,
            response_bytes,
            &SearchType::PointInTime,
            151,
        )
        .await
        .unwrap_err();

        let metadata = failure
            .metadata()
            .expect("metadata should be recovered locally");
        assert_eq!(metadata.next_pit_id.as_deref(), Some("pit-send-failed"));
        assert_eq!(
            metadata.last_sort_raw.as_deref(),
            Some(br#"[11,"send"]"#.as_slice())
        );
        assert_eq!(metadata.doc_count, 1);
        assert!(!metadata.hits_are_empty);
        assert!(
            failure
                .into_error()
                .to_string()
                .contains("Failed to send batch to worker")
        );

        assert_eq!(state.retrieved_hits, 0);
        assert_eq!(
            ctx.retrieved_count
                .load(std::sync::atomic::Ordering::Relaxed),
            0
        );
        assert_eq!(
            ctx.retrieved_bytes
                .load(std::sync::atomic::Ordering::Relaxed),
            0
        );
    }
}
