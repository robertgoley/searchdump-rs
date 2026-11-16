//! `--searchAfter` without a point-in-time: `POST /{index}/_search` pages
//! chained through the last hit's `sort` values. Not snapshot-consistent;
//! requires a user sort ending in a unique tiebreaker.

use anyhow::Result;
use log::{debug, info};
use sonic_rs::{JsonValueMutTrait, JsonValueTrait, Value};

use crate::cli::SearchType;

use super::{
    context::RetrievalContext,
    retrieval_task::{
        DumpCancelled, RetryMode, TotalHitsEstimate, process_response_batch, record_total_hits,
        send_page_halving,
    },
    slice_state::SliceState,
};

fn with_search_after(base: &Value, search_after: Option<&[u8]>) -> Result<Value> {
    let mut body = base.clone();
    if let Some(raw) = search_after {
        let v: Value = sonic_rs::from_slice(raw)?;
        body.as_object_mut()
            .expect("search body must be an object")
            .insert(&"search_after", v);
    }
    Ok(body)
}

pub(crate) async fn run_search_after_slice(
    ctx: &RetrievalContext,
    state: &mut SliceState,
) -> Result<TotalHitsEstimate> {
    let mut total = None;
    loop {
        if ctx.cancel.is_cancelled() {
            return Err(anyhow::Error::new(DumpCancelled));
        }
        let mut request = with_search_after(&state.search_body, state.search_after.as_deref())?;
        // 1.2-1c: a circuit-breaker 429 halves the page for the rest of the slice
        let bytes = send_page_halving(
            ctx,
            state.slice_id,
            "search_after search",
            RetryMode::Idempotent,
            &mut request,
            |body| ctx.plain_search(body),
        )
        .await?;
        let batch_size = request["size"].as_u64().unwrap_or(0);
        if let (Some(obj), Some(n)) = (state.search_body.as_object_mut(), request.get("size")) {
            obj.insert(&"size", n.clone());
        }
        let n = bytes.len() as u64;
        let metadata = process_response_batch(ctx, state, bytes, &SearchType::SearchAfter, n)
            .await
            .map_err(|e| e.into_error())?;
        if total.is_none() {
            total = Some(record_total_hits(ctx, metadata.total_hits));
        }
        state.update_search_after_from_raw(metadata.last_sort_raw.as_deref());
        debug!(
            "Slice {}: search_after page with {} docs",
            state.slice_id, metadata.doc_count
        );
        if metadata.hits_are_empty || metadata.doc_count < batch_size {
            break;
        }
    }
    info!(
        "Slice {} completed, retrieved {} documents",
        state.slice_id, state.retrieved_hits
    );
    Ok(total.unwrap_or(TotalHitsEstimate {
        value: 0,
        is_exact: true,
    }))
}
