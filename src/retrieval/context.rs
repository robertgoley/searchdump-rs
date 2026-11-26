use indicatif::ProgressBar;
use reqwest::Method;
use sonic_rs::Value;
use std::{
    sync::{Arc, atomic::AtomicU64},
    time::Instant,
};
use tokio::sync::mpsc::Sender;
use tokio_util::sync::CancellationToken;

use super::messages::RetrievalMessage;
use super::retrieval_task::RetryPolicy;
use crate::http::{Body, EsClient};

#[derive(Clone)]
pub(crate) struct RetrievalContext {
    pub(crate) client: EsClient,
    /// Index (or comma list / pattern). `None` searches the whole cluster.
    pub(crate) index: Option<Arc<str>>,
    /// Default sort injected into PIT searches without a user sort.
    pub(crate) pit_sort: Value,
    pub(crate) worker_txs: Vec<Sender<RetrievalMessage>>,
    pub(crate) total_hits_count: Arc<AtomicU64>,
    pub(crate) input_bar: Option<ProgressBar>,
    pub(crate) output_bar: Option<ProgressBar>,
    pub(crate) retrieved_count: Arc<AtomicU64>,
    pub(crate) retrieved_bytes: Arc<AtomicU64>,
    pub(crate) start_time: Instant,
    /// Cancels the whole pipeline: signal handler, output failures, and
    /// failing sibling slices all fire the same token.
    pub(crate) cancel: CancellationToken,
    pub(crate) retry: RetryPolicy,
    /// `--scrollId`: resume this scroll instead of running an initial search.
    pub(crate) resume_scroll_id: Option<std::sync::Arc<str>>,
    /// 1.4 `--pageBytes` (L4-003): a page response above this shrinks the next pages.
    pub(crate) page_bytes: Option<u64>,
}

impl RetrievalContext {
    /// `POST /{index}/_search?scroll=..&allow_partial_search_results=false`
    pub(crate) async fn scroll_search(
        &self,
        scroll_ttl: &str,
        body: Body,
    ) -> Result<reqwest::Response, crate::http::SendError> {
        let segments: Vec<&str> = match self.index.as_deref() {
            Some(index) => vec![index, "_search"],
            None => vec!["_search"],
        };
        self.client
            .send(
                Method::POST,
                &segments,
                &[
                    ("scroll", scroll_ttl),
                    ("allow_partial_search_results", "false"),
                ],
                true,
                body,
            )
            .await
    }

    /// `POST /{index}/_search?allow_partial_search_results=false` (no scroll).
    pub(crate) async fn plain_search(
        &self,
        body: Body,
    ) -> Result<reqwest::Response, crate::http::SendError> {
        let segments: Vec<&str> = match self.index.as_deref() {
            Some(index) => vec![index, "_search"],
            None => vec!["_search"],
        };
        self.client
            .send(
                Method::POST,
                &segments,
                &[("allow_partial_search_results", "false")],
                true,
                body,
            )
            .await
    }

    /// `POST /_search/scroll`
    pub(crate) async fn scroll_continue(
        &self,
        body: Body,
    ) -> Result<reqwest::Response, crate::http::SendError> {
        self.client
            .send(Method::POST, &["_search", "scroll"], &[], false, body)
            .await
    }

    /// `POST /_search?allow_partial_search_results=false` with a `pit` body.
    pub(crate) async fn pit_search(
        &self,
        body: Body,
    ) -> Result<reqwest::Response, crate::http::SendError> {
        self.client
            .send(
                Method::POST,
                &["_search"],
                &[("allow_partial_search_results", "false")],
                true,
                body,
            )
            .await
    }
}
