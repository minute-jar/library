//! The HTTP routes of the interface with family servers.
//!
//! Every signed document is answered from its stored bytes, whatever the
//! request carries besides `If-None-Match`, so that every caller receives
//! the same thing (`docs/specs/interface-v1.md`, HTTP).

use axum::Router;
use axum::body::Bytes;
use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use serde::Deserialize;

use crate::catalogue::{self, Stored, item_path};
use crate::{ImportError, Library, RatingError};

const KEYS_CACHE: &str = "public, max-age=3600";
const INDEX_CACHE: &str = "public, max-age=300";
const ITEM_CACHE: &str = "public, max-age=86400";

pub fn router(library: Library) -> Router {
    Router::new()
        .route("/.well-known/minute-jar-library.json", get(keys))
        .route("/v1/index", get(index))
        .route("/v1/items/{item}/versions/{version}", get(item_version))
        .route("/v1/imports", post(import))
        .route("/v1/ratings", post(rate))
        .route("/api/items/{item}/ratings", get(item_ratings))
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

/// An import reported by a family server. The library identifier travels in
/// the body only, never in the path, a header or a cookie, so that no access
/// log in front of the service records it.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Import {
    library_identifier: String,
    item: String,
}

async fn import(State(library): State<Library>, body: Bytes) -> Response {
    let Ok(import) = serde_json::from_slice::<Import>(&body) else {
        return refused(StatusCode::BAD_REQUEST, "bad_request");
    };
    match library
        .record_import(&import.library_identifier, &import.item)
        .await
    {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(ImportError::BadIdentifier) => {
            refused(StatusCode::BAD_REQUEST, "bad_library_identifier")
        }
        Err(ImportError::UnknownItem) => refused(StatusCode::NOT_FOUND, "unknown_item"),
        Err(ImportError::Database(_)) => refused(StatusCode::SERVICE_UNAVAILABLE, "unavailable"),
    }
}

/// A rating left by a family server, under its library identifier carried
/// in the body only, as for an import.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Rating {
    library_identifier: String,
    item: String,
    stars: u8,
    #[serde(default)]
    review: Option<String>,
}

async fn rate(State(library): State<Library>, body: Bytes) -> Response {
    let Ok(rating) = serde_json::from_slice::<Rating>(&body) else {
        return refused(StatusCode::BAD_REQUEST, "bad_request");
    };
    match library
        .record_rating(
            &rating.library_identifier,
            &rating.item,
            rating.stars,
            rating.review.as_deref(),
        )
        .await
    {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(error) => rating_refused(error),
    }
}

/// How an item is rated, for the website's item page: counted as things
/// stand, not as of the last signed index.
async fn item_ratings(State(library): State<Library>, Path(item): Path<String>) -> Response {
    match library.item_ratings(&item).await {
        Ok(ratings) => axum::Json(serde_json::json!({
            "item": item,
            "ratings": ratings.counts,
            "reviews": ratings.reviews,
        }))
        .into_response(),
        Err(error) => rating_refused(error),
    }
}

fn rating_refused(error: RatingError) -> Response {
    match error {
        RatingError::BadIdentifier => refused(StatusCode::BAD_REQUEST, "bad_library_identifier"),
        RatingError::BadRating => refused(StatusCode::BAD_REQUEST, "bad_rating"),
        RatingError::BadReview => refused(StatusCode::BAD_REQUEST, "bad_review"),
        RatingError::UnknownItem => refused(StatusCode::NOT_FOUND, "unknown_item"),
        RatingError::NotImported => refused(StatusCode::FORBIDDEN, "not_imported"),
        RatingError::Database(_) => refused(StatusCode::SERVICE_UNAVAILABLE, "unavailable"),
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
