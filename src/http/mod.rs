//! HTTP layer: client, TLS, retries, admission control, cluster detection.

pub mod client;
pub mod gate;
pub mod info;
pub mod retry;
pub mod tls;

pub use client::{Body, ClientOptions, EsClient, SendError};
pub use info::{ClusterInfo, Flavor, PitApi};
pub use retry::{
    Backoff, ContextLoss, DEFAULT_MAX_RESPONSE_MB, DumpCancelled, EsHttpError, NotRerunSafe,
    RetryMode, RetryOn, RetryPolicy, Transient, is_cancelled, send_checked_with_retry,
    send_with_retry,
};
