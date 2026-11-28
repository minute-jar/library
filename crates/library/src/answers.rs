//! The JSON answers the service's routes share: a private answer and a
//! refusal, which no cache keeps.

use axum::http::{StatusCode, header};
use axum::response::{IntoResponse, Response};
use serde_json::{Value, json};

/// The `Cache-Control` of an answer that only its caller may read.
pub(crate) const NO_STORE: &str = "private, no-store";

/// `body` as JSON with `status`, kept by caches as `cache` says.
pub(crate) fn json(status: StatusCode, cache: &'static str, body: Value) -> Response {
    (
        status,
        [
            (header::CONTENT_TYPE, "application/json"),
            (header::CACHE_CONTROL, cache),
        ],
        body.to_string(),
    )
        .into_response()
}

/// `body` as JSON with `status`, kept by no cache.
pub(crate) fn private(status: StatusCode, body: Value) -> Response {
    json(status, NO_STORE, body)
}

/// The refusal `{"error": code}` with `status`, kept by no cache.
pub(crate) fn refused(status: StatusCode, code: &str) -> Response {
    private(status, json!({"error": code}))
}
