use anyhow::{Result, anyhow};
use log::{debug, info};
use sonic_rs::{Value, json};
use std::sync::atomic::{AtomicBool, Ordering};

use crate::cli::SearchType;
use crate::http::Body;

use super::{
    context::RetrievalContext,
    extract::BatchMetadata,
    retrieval_task::{
        DumpCancelled, RetryMode, TotalHitsEstimate, cleanup_search_context,
        process_response_batch, record_total_hits, send_checked_with_retry,
    },
    slice_state::SliceState,
};

/// A failed scroll continuation (other than cancellation or an exhausted 429)
/// was not retried by design: the server may already have moved the cursor.
/// Say so, and how to recover (TST-API-051).
fn continuation_failure(error: anyhow::Error, slice_id: usize) -> anyhow::Error {
    let status = error
        .downcast_ref::<crate::http::EsHttpError>()
        .map(|h| h.status);
    // 4xx answers (other than 408) are not something a retry would fix, so
    // the "not retried" explanation would only mislead (e.g. a 403 because
    // OpenSearch needs a cluster permission for scroll).
    if crate::http::is_cancelled(&error) {
        return error;
    }
    if super::retrieval_task::search_context_lost(&error) {
        return super::retrieval_task::context_lost_error(error, slice_id, "scroll");
    }
    if status.is_some_and(|s| (400..500).contains(&s) && s != 408) {
        return error;
    }
    error.context(crate::http::ContextLoss(format!(
        "Slice {slice_id}: the scroll continuation was not retried, because the server may already have advanced the scroll cursor and a retry could skip a page (a larger --retryAttempts cannot help); nothing was committed. Re-run the dump (the scroll context was cleared; --scrollId resumes only from a still-open scroll and the page in flight may be missing). A point-in-time dump (--searchAfter --pit, ES 7.10+ / OpenSearch 2.4+) retries a failed page safely; --restartOnContextLoss N restarts a scroll dump into a file automatically"
    )))
}

/// `--scrollId` named a scroll the cluster no longer has (L-650): rerunning
/// with the same id cannot help, so say what can.
fn resume_failure(error: anyhow::Error) -> anyhow::Error {
    if crate::http::is_cancelled(&error) || !super::retrieval_task::search_context_lost(&error) {
        return error;
    }
    error.context(
        "--scrollId: the cluster no longer has this scroll (it expired after the --scrollTime of the request that opened or last continued it, it was cleared, e.g. by Ctrl+C, or the node holding it restarted), so it cannot be resumed; nothing was committed. Rerun the dump without --scrollId (add --overwrite to replace an earlier output file)",
    )
}

pub(crate) fn build_scroll_request_body(scroll_ttl: &str, scroll_id: &str) -> Value {
    json!({
        "scroll": scroll_ttl,
        "scroll_id": scroll_id,
    })
}

pub(crate) fn apply_scroll_batch_metadata(state: &mut SliceState, metadata: &BatchMetadata) {
    // Keep the previous scroll id when a response lacks one: overwriting it
    // with None would make both continuation and cleanup impossible.
    if let Some(next_scroll_id) = &metadata.next_scroll_id {
        state.current_id = Some(next_scroll_id.clone());
    }
}

pub(crate) async fn run_scroll_slice(
    ctx: &RetrievalContext,
    state: &mut SliceState,
    scroll_ttl: &str,
) -> Result<TotalHitsEstimate> {
    debug!(
        "Slice {}: Initiating scroll search with size {}",
        state.slice_id, state.search_body["size"]
    );

    if ctx.cancel.is_cancelled() {
        return Err(anyhow::Error::new(DumpCancelled));
    }

    // The initial search is idempotent: repeating it opens a fresh scroll
    // context instead of advancing an existing cursor, so full retries are
    // safe. A retry after a lost response can strand the previous attempt's
    // scroll context until its TTL expires; the closure logs that case.
    let body = Body::sonic(&state.search_body);
    let body = &body;
    let initial_attempted = &AtomicBool::new(false);
    let slice_id = state.slice_id;
    let response_bytes = if let Some(resume_id) = ctx.resume_scroll_id.as_deref() {
        // `--scrollId`: continue an existing scroll. This advances a cursor,
        // so it gets the scroll-continuation retry rules. The cursor is the
        // server's: pages an earlier (killed) run fetched are not returned
        // again, whether or not that run wrote them (L-650).
        info!(
            "--scrollId: continuing the scroll from where its cursor is; pages already fetched by an earlier request or run are not returned again"
        );
        state.current_id = Some(resume_id.to_string());
        let resume_body = Body::sonic(&build_scroll_request_body(scroll_ttl, resume_id));
        let resume_body = &resume_body;
        send_checked_with_retry(
            ctx.retry,
            &ctx.cancel,
            RetryMode::RejectionOnly,
            state.slice_id,
            "scroll resume",
            move || ctx.scroll_continue(resume_body.clone()),
        )
        .await
        .map_err(resume_failure)?
    } else {
        send_checked_with_retry(
            ctx.retry,
            &ctx.cancel,
            RetryMode::Idempotent,
            state.slice_id,
            "initial search",
            move || {
                if initial_attempted.swap(true, Ordering::Relaxed) {
                    debug!(
                        "Slice {}: retrying initial search; a previous attempt whose response was lost may strand a scroll context until its TTL expires",
                        slice_id
                    );
                }
                ctx.scroll_search(scroll_ttl, body.clone())
            },
        )
        .await?
    };
    let initial_bytes = response_bytes.len() as u64;
    debug!(
        "Slice {}: Read {} bytes for initial response",
        state.slice_id, initial_bytes
    );

    let metadata = match process_response_batch(
        ctx,
        state,
        response_bytes,
        &SearchType::Scroll,
        initial_bytes,
    )
    .await
    {
        Ok(metadata) => metadata,
        Err(error) => {
            if let Some(metadata) = error.metadata() {
                apply_scroll_batch_metadata(state, metadata);
            }
            if let Some(scroll_id) = state.current_id.as_deref() {
                cleanup_search_context(&ctx.client, &SearchType::Scroll, scroll_id, state.slice_id)
                    .await;
            }
            return Err(error.into_error());
        }
    };
    apply_scroll_batch_metadata(state, &metadata);
    if let Some(id) = &state.current_id {
        debug!(
            "Slice {}: Got scroll_id: {}",
            state.slice_id,
            super::redact_context_id(id)
        );
    } else {
        debug!("Slice {}: No scroll_id found in response", state.slice_id);
    }

    let slice_total_hits = record_total_hits(ctx, metadata.total_hits);
    debug!(
        "Slice {}: Total hits estimate: {} (exact: {})",
        state.slice_id, slice_total_hits.value, slice_total_hits.is_exact
    );

    let mut done = metadata.hits_are_empty;

    while !done {
        if ctx.cancel.is_cancelled() {
            if let Some(scroll_id) = state.current_id.as_deref() {
                cleanup_search_context(&ctx.client, &SearchType::Scroll, scroll_id, state.slice_id)
                    .await;
            }
            return Err(anyhow::Error::new(DumpCancelled));
        }

        let request_body = build_scroll_request_body(
            scroll_ttl,
            state.current_id.as_deref().ok_or_else(|| {
                anyhow!(
                    "No ID found for slice {} to continue search",
                    state.slice_id
                )
            })?,
        );

        let body = Body::sonic(&request_body);
        let body = &body;
        // NOT idempotent: every continuation call advances the server-side
        // cursor, so RejectionOnly retries this request only on HTTP 429,
        // where the server rejected it without moving the cursor.
        let next_response_bytes = match send_checked_with_retry(
            ctx.retry,
            &ctx.cancel,
            RetryMode::RejectionOnly,
            state.slice_id,
            "continuation search",
            move || ctx.scroll_continue(body.clone()),
        )
        .await
        {
            Ok(bytes) => bytes,
            Err(error) => {
                if let Some(scroll_id) = state.current_id.as_deref() {
                    cleanup_search_context(
                        &ctx.client,
                        &SearchType::Scroll,
                        scroll_id,
                        state.slice_id,
                    )
                    .await;
                }
                return Err(continuation_failure(error, state.slice_id));
            }
        };

        let batch_bytes = next_response_bytes.len() as u64;
        debug!(
            "Slice {}: Read {} bytes for continuation response",
            state.slice_id, batch_bytes
        );

        let metadata = match process_response_batch(
            ctx,
            state,
            next_response_bytes,
            &SearchType::Scroll,
            batch_bytes,
        )
        .await
        {
            Ok(metadata) => metadata,
            Err(error) => {
                if let Some(metadata) = error.metadata() {
                    apply_scroll_batch_metadata(state, metadata);
                }
                if let Some(scroll_id) = state.current_id.as_deref() {
                    cleanup_search_context(
                        &ctx.client,
                        &SearchType::Scroll,
                        scroll_id,
                        state.slice_id,
                    )
                    .await;
                }
                return Err(error.into_error());
            }
        };
        apply_scroll_batch_metadata(state, &metadata);

        done = metadata.hits_are_empty;

        if done {
            info!(
                "Search finished for slice {}, no more documents.",
                state.slice_id
            );
        }
    }

    if let Some(scroll_id) = state.current_id.as_deref() {
        cleanup_search_context(&ctx.client, &SearchType::Scroll, scroll_id, state.slice_id).await;
    }

    info!(
        "Slice {} completed, retrieved {} documents",
        state.slice_id, state.retrieved_hits
    );

    Ok(slice_total_hits)
}

#[cfg(test)]
mod tests {
    use super::super::{
        extract::BatchMetadata, retrieval_task::TotalHitsEstimate, slice_state::SliceState,
    };
    use super::build_scroll_request_body;
    use sonic_rs::json;

    #[test]
    fn build_scroll_request_body_includes_scroll_id_and_ttl() {
        assert_eq!(
            build_scroll_request_body("5m", "scroll-123"),
            json!({ "scroll": "5m", "scroll_id": "scroll-123" })
        );
    }

    #[test]
    fn apply_scroll_batch_metadata_updates_current_id() {
        let mut state = SliceState::new(0, 0, json!({"size": 10}));
        state.current_id = Some("scroll-old".to_string());

        let metadata = BatchMetadata {
            total_hits: TotalHitsEstimate {
                value: 2,
                is_exact: true,
            },
            next_scroll_id: Some("scroll-new".to_string()),
            next_pit_id: None,
            last_sort_raw: None,
            doc_count: 2,
            hits_are_empty: false,
        };

        super::apply_scroll_batch_metadata(&mut state, &metadata);

        assert_eq!(state.current_id.as_deref(), Some("scroll-new"));
    }

    #[test]
    fn apply_scroll_batch_metadata_keeps_previous_id_when_response_lacks_one() {
        let mut state = SliceState::new(0, 0, json!({"size": 10}));
        state.current_id = Some("scroll-live".into());

        let metadata = BatchMetadata {
            total_hits: TotalHitsEstimate {
                value: 2,
                is_exact: true,
            },
            next_scroll_id: None,
            next_pit_id: None,
            last_sort_raw: None,
            doc_count: 2,
            hits_are_empty: false,
        };

        super::apply_scroll_batch_metadata(&mut state, &metadata);

        assert_eq!(state.current_id.as_deref(), Some("scroll-live"));
    }
}
