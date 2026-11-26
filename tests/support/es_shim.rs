//! Minimal stand-in for the part of the `elasticsearch` crate (8.19.0-alpha.1)
//! API that `tests/integration_test.rs` uses, on top of `reqwest`.
//!
//! The alpha client pulled in an unmaintained/vulnerable dependency tree
//! (`serde_with` → `time` < 0.3.47, RUSTSEC-2026-0009) for a dozen setup calls;
//! see SECURITY-CVE.md. Import it as `use support::es_shim as elasticsearch;`.
#![allow(dead_code)]

use anyhow::Result;
use serde::de::DeserializeOwned;
use serde_json::Value;

pub mod http {
    pub mod request {
        /// One NDJSON line of a bulk body.
        pub struct JsonBody<T>(pub T);
        impl From<serde_json::Value> for JsonBody<serde_json::Value> {
            fn from(v: serde_json::Value) -> Self {
                JsonBody(v)
            }
        }
    }
    pub mod transport {
        pub struct SingleNodeConnectionPool(pub(crate) url::Url);
        impl SingleNodeConnectionPool {
            pub fn new(url: url::Url) -> Self {
                Self(url)
            }
        }
        pub struct TransportBuilder(url::Url);
        impl TransportBuilder {
            pub fn new(pool: SingleNodeConnectionPool) -> Self {
                Self(pool.0)
            }
            pub fn build(self) -> anyhow::Result<Transport> {
                let client = {
                    searchdump_rs::http::tls::install_default_provider();
                    reqwest::Client::builder()
                }
                .danger_accept_invalid_certs(true)
                .build()?;
                Ok(Transport {
                    base: self.0.as_str().trim_end_matches('/').to_string(),
                    client,
                })
            }
        }
        pub struct Transport {
            pub(crate) base: String,
            pub(crate) client: reqwest::Client,
        }
    }
}

pub mod indices {
    use super::{Elasticsearch, Request};
    pub enum IndicesCreateParts<'a> {
        Index(&'a str),
    }
    pub enum IndicesDeleteParts<'a> {
        Index(&'a [&'a str]),
    }
    pub enum IndicesRefreshParts<'a> {
        Index(&'a [&'a str]),
    }
    pub struct Indices<'a>(pub(crate) &'a Elasticsearch);
    impl<'a> Indices<'a> {
        pub fn create(&self, p: IndicesCreateParts<'_>) -> Request<'a> {
            let IndicesCreateParts::Index(i) = p;
            self.0.req(reqwest::Method::PUT, format!("/{i}"))
        }
        pub fn delete(&self, p: IndicesDeleteParts<'_>) -> Request<'a> {
            let IndicesDeleteParts::Index(i) = p;
            self.0
                .req(reqwest::Method::DELETE, format!("/{}", i.join(",")))
        }
        pub fn refresh(&self, p: IndicesRefreshParts<'_>) -> Request<'a> {
            let IndicesRefreshParts::Index(i) = p;
            self.0
                .req(reqwest::Method::POST, format!("/{}/_refresh", i.join(",")))
        }
    }
}

pub enum BulkParts<'a> {
    Index(&'a str),
}
pub enum IndexParts<'a> {
    Index(&'a str),
    IndexId(&'a str, &'a str),
}
pub enum GetParts<'a> {
    IndexId(&'a str, &'a str),
}
pub enum SearchParts<'a> {
    Index(&'a [&'a str]),
}

pub struct Elasticsearch {
    t: http::transport::Transport,
}

enum Body {
    Json(Value),
    Ndjson(String),
}

pub trait IntoBody {
    fn into_body(self) -> BodyKind;
}
pub struct BodyKind(Body);
impl IntoBody for Value {
    fn into_body(self) -> BodyKind {
        BodyKind(Body::Json(self))
    }
}
impl IntoBody for Vec<http::request::JsonBody<Value>> {
    fn into_body(self) -> BodyKind {
        let mut s = String::new();
        for l in self {
            s.push_str(&l.0.to_string());
            s.push('\n');
        }
        BodyKind(Body::Ndjson(s))
    }
}

pub struct Request<'a> {
    es: &'a Elasticsearch,
    method: reqwest::Method,
    path: String,
    body: Option<Body>,
}

impl Request<'_> {
    pub fn body<B: IntoBody>(mut self, b: B) -> Self {
        self.body = Some(b.into_body().0);
        self
    }
    pub async fn send(self) -> Result<Response> {
        let url = format!("{}{}", self.es.t.base, self.path);
        let mut rb = self.es.t.client.request(self.method, url);
        rb = match self.body {
            Some(Body::Json(v)) => rb.json(&v),
            Some(Body::Ndjson(s)) => rb.header("content-type", "application/x-ndjson").body(s),
            None => rb,
        };
        Ok(Response(rb.send().await?))
    }
}

pub struct Response(reqwest::Response);
impl Response {
    pub fn status_code(&self) -> reqwest::StatusCode {
        self.0.status()
    }
    pub async fn json<T: DeserializeOwned>(self) -> Result<T> {
        Ok(self.0.json().await?)
    }
    pub async fn text(self) -> Result<String> {
        Ok(self.0.text().await?)
    }
}

impl Elasticsearch {
    pub fn new(t: http::transport::Transport) -> Self {
        Self { t }
    }
    fn req(&self, method: reqwest::Method, path: String) -> Request<'_> {
        Request {
            es: self,
            method,
            path,
            body: None,
        }
    }
    pub fn ping(&self) -> Request<'_> {
        self.req(reqwest::Method::HEAD, "/".into())
    }
    pub fn indices(&self) -> indices::Indices<'_> {
        indices::Indices(self)
    }
    pub fn bulk(&self, p: BulkParts<'_>) -> Request<'_> {
        let BulkParts::Index(i) = p;
        self.req(reqwest::Method::POST, format!("/{i}/_bulk"))
    }
    pub fn index(&self, p: IndexParts<'_>) -> Request<'_> {
        match p {
            IndexParts::Index(i) => self.req(reqwest::Method::POST, format!("/{i}/_doc")),
            IndexParts::IndexId(i, id) => self.req(reqwest::Method::PUT, format!("/{i}/_doc/{id}")),
        }
    }
    pub fn get(&self, p: GetParts<'_>) -> Request<'_> {
        let GetParts::IndexId(i, id) = p;
        self.req(reqwest::Method::GET, format!("/{i}/_doc/{id}"))
    }
    pub fn search(&self, p: SearchParts<'_>) -> Request<'_> {
        let SearchParts::Index(i) = p;
        self.req(reqwest::Method::POST, format!("/{}/_search", i.join(",")))
    }
}
