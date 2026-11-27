//! The HTTP routes of the interface with family servers.
//!
//! Every signed document is answered from its stored bytes, whatever the
//! request carries besides `If-None-Match`, so that every caller receives
//! the same thing (`docs/specs/interface-v1.md`, HTTP).

use axum::Router;
use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::get;

use crate::Library;
use crate::catalogue::{self, Stored, item_path};

const KEYS_CACHE: &str = "public, max-age=3600";
const INDEX_CACHE: &str = "public, max-age=300";
const ITEM_CACHE: &str = "public, max-age=86400";
const WITHDRAWALS_CACHE: &str = "public, max-age=300";

pub fn router(library: Library) -> Router {
    Router::new()
        .route("/.well-known/minute-jar-library.json", get(keys))
        .route("/v1/index", get(index))
        .route("/v1/items/{item}/versions/{version}", get(item_version))
        .route("/v1/withdrawals", get(withdrawals))
        .with_state(library)
}

async fn keys(State(library): State<Library>, request: HeaderMap) -> Response {
    served(&library.keys_document, KEYS_CACHE, &request)
}

async fn index(State(library): State<Library>, request: HeaderMap) -> Response {
    match catalogue::document(&library.pool, catalogue::INDEX_PATH).await {
        Ok(Some(stored)) => served(&stored, INDEX_CACHE, &request),
        _ => refused(StatusCode::SERVICE_UNAVAILABLE, "unavailable"),
    }
}

async fn withdrawals(State(library): State<Library>, request: HeaderMap) -> Response {
    match catalogue::document(&library.pool, crate::withdrawals::PATH).await {
        Ok(Some(stored)) => served(&stored, WITHDRAWALS_CACHE, &request),
        _ => refused(StatusCode::SERVICE_UNAVAILABLE, "unavailable"),
    }
}

async fn item_version(
    State(library): State<Library>,
    Path((item, version)): Path<(String, String)>,
    request: HeaderMap,
) -> Response {
    let known = catalogue::has_item(&library.pool, &item).await;
    let Ok(known) = known else {
        return refused(StatusCode::SERVICE_UNAVAILABLE, "unavailable");
    };
    if !known {
        return refused(StatusCode::NOT_FOUND, "unknown_item");
    }
    let Ok(version) = version.parse::<u32>() else {
        return refused(StatusCode::NOT_FOUND, "unknown_version");
    };
    match catalogue::document(&library.pool, &item_path(&item, version)).await {
        Ok(Some(stored)) => served(&stored, ITEM_CACHE, &request),
        Ok(None) => refused(StatusCode::NOT_FOUND, "unknown_version"),
        Err(_) => refused(StatusCode::SERVICE_UNAVAILABLE, "unavailable"),
    }
}

fn served(stored: &Stored, cache: &'static str, request: &HeaderMap) -> Response {
    let headers = [
        (header::CONTENT_TYPE, "application/json".to_string()),
        (header::CACHE_CONTROL, cache.to_string()),
        (header::ETAG, stored.etag.clone()),
    ];
    let unchanged = request
        .get_all(header::IF_NONE_MATCH)
        .iter()
        .filter_map(|value| value.to_str().ok())
        .flat_map(|value| value.split(','))
        .any(|tag| tag.trim() == stored.etag || tag.trim() == "*");
    if unchanged {
        (StatusCode::NOT_MODIFIED, headers).into_response()
    } else {
        (headers, stored.body.clone()).into_response()
    }
}

/// Every refusal of the interface, as a fixed code (spec: one place).
fn refused(status: StatusCode, code: &'static str) -> Response {
    (
        status,
        [(header::CONTENT_TYPE, "application/json")],
        format!("{{\"error\":\"{code}\"}}"),
    )
        .into_response()
}
