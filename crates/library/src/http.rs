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

use crate::answers::refused;
use crate::catalogue::{self, Stored, item_path};
use crate::media;
use crate::{
    FormalNotice, ImportError, Library, NewReport, RatingError, Reason, ReportError, Reporter,
};

const KEYS_CACHE: &str = "public, max-age=3600";
const INDEX_CACHE: &str = "public, max-age=300";
const ITEM_CACHE: &str = "public, max-age=86400";
/// A medium never changes: its address is its digest.
const MEDIA_CACHE: &str = "public, max-age=31536000, immutable";
const WITHDRAWALS_CACHE: &str = "public, max-age=300";

pub fn router(library: Library) -> Router {
    Router::new()
        .route("/.well-known/minute-jar-library.json", get(keys))
        .route("/v1/index", get(index))
        .route("/v1/items/{item}/versions/{version}", get(item_version))
        .route("/v1/withdrawals", get(withdrawals))
        .route("/v1/imports", post(import))
        .route("/v1/ratings", post(rate))
        .route("/api/items/{item}/ratings", get(item_ratings))
        .route("/v1/media/{digest}", get(medium))
        .route("/v1/reports", post(family_report))
        .route("/api/reports", post(website_report))
        .route("/v1/reports/{report}", get(report_state))
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

/// A report from a family server, under its library identifier, which
/// travels in the body only, like an import's.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct FamilyReport {
    library_identifier: String,
    item: String,
    reason: String,
    #[serde(default)]
    details: Option<String>,
}

/// A report from the website: anonymous, with an address for the
/// acknowledgement and the decision if the reporter leaves one.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct WebsiteReport {
    item: String,
    reason: String,
    #[serde(default)]
    details: Option<String>,
    #[serde(default)]
    contact: Option<String>,
    #[serde(default)]
    notice: Option<Notice>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Notice {
    name: String,
    work: String,
}

async fn family_report(State(library): State<Library>, body: Bytes) -> Response {
    let Ok(report) = serde_json::from_slice::<FamilyReport>(&body) else {
        return refused(StatusCode::BAD_REQUEST, "bad_request");
    };
    let reporter = Reporter::Family {
        library_identifier: report.library_identifier,
    };
    receive(
        &library,
        report.item,
        &report.reason,
        report.details,
        reporter,
    )
    .await
}

async fn website_report(State(library): State<Library>, body: Bytes) -> Response {
    let Ok(report) = serde_json::from_slice::<WebsiteReport>(&body) else {
        return refused(StatusCode::BAD_REQUEST, "bad_request");
    };
    let reporter = Reporter::Website {
        contact: report.contact,
        notice: report.notice.map(|notice| FormalNotice {
            name: notice.name,
            work: notice.work,
        }),
    };
    receive(
        &library,
        report.item,
        &report.reason,
        report.details,
        reporter,
    )
    .await
}

async fn receive(
    library: &Library,
    item: String,
    reason: &str,
    details: Option<String>,
    reporter: Reporter,
) -> Response {
    let Some(reason) = Reason::from_code(reason) else {
        return refused(StatusCode::BAD_REQUEST, "bad_reason");
    };
    let Some(desk) = &library.reports else {
        return refused(StatusCode::SERVICE_UNAVAILABLE, "unavailable");
    };
    let now = (desk.clock)();
    let report = NewReport {
        item,
        reason,
        details,
        reporter,
    };
    match library.report(report, now).await {
        Ok(id) => (
            StatusCode::ACCEPTED,
            [
                (header::CONTENT_TYPE, "application/json"),
                (header::CACHE_CONTROL, "no-store"),
            ],
            serde_json::json!({"report": id}).to_string(),
        )
            .into_response(),
        Err(ReportError::BadIdentifier) => {
            refused(StatusCode::BAD_REQUEST, "bad_library_identifier")
        }
        Err(ReportError::BadContact) => refused(StatusCode::BAD_REQUEST, "bad_contact"),
        Err(ReportError::BadNotice) => refused(StatusCode::BAD_REQUEST, "bad_notice"),
        Err(ReportError::DetailsTooLong) => refused(StatusCode::BAD_REQUEST, "bad_request"),
        Err(ReportError::UnknownItem) => refused(StatusCode::NOT_FOUND, "unknown_item"),
        Err(ReportError::Unavailable | ReportError::Database(_)) => {
            refused(StatusCode::SERVICE_UNAVAILABLE, "unavailable")
        }
    }
}

/// Where a report stands, for its reporter, who alone knows its identifier.
async fn report_state(State(library): State<Library>, Path(report): Path<String>) -> Response {
    let state = match library.report_state(&report).await {
        Ok(Some(state)) => state,
        Ok(None) => return refused(StatusCode::NOT_FOUND, "unknown_report"),
        Err(_) => return refused(StatusCode::SERVICE_UNAVAILABLE, "unavailable"),
    };
    let decision = state.decision.map(|(decision, decided_at)| {
        serde_json::json!({
            "outcome": decision.outcome.code(),
            "reasons": decision.reasons,
            "decided_at": crate::contract::time(decided_at),
        })
    });
    (
        [
            (header::CONTENT_TYPE, "application/json"),
            (header::CACHE_CONTROL, "no-store"),
        ],
        serde_json::json!({
            "report": state.report,
            "item": state.item,
            "reason": state.reason.code(),
            "status": state.status.code(),
            "received_at": crate::contract::time(state.received_at),
            "decision": decision,
        })
        .to_string(),
    )
        .into_response()
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

async fn medium(
    State(library): State<Library>,
    Path(digest): Path<String>,
    request: HeaderMap,
) -> Response {
    if !media::is_digest(&digest) {
        return refused(StatusCode::NOT_FOUND, "unknown_media");
    }
    let media_type = match media::published_type(&library.pool, &digest).await {
        Ok(Some(media_type)) => media_type,
        Ok(None) => return refused(StatusCode::NOT_FOUND, "unknown_media"),
        Err(_) => return refused(StatusCode::SERVICE_UNAVAILABLE, "unavailable"),
    };
    let headers = [
        (header::CONTENT_TYPE, media_type),
        (header::CACHE_CONTROL, MEDIA_CACHE.to_string()),
        (header::ETAG, format!("\"{digest}\"")),
        (header::X_CONTENT_TYPE_OPTIONS, "nosniff".to_string()),
    ];
    if names(&request, &headers[2].1) {
        return (StatusCode::NOT_MODIFIED, headers).into_response();
    }
    match library.media.get(&digest).await {
        Ok(Some(bytes)) => (headers, bytes).into_response(),
        _ => refused(StatusCode::SERVICE_UNAVAILABLE, "unavailable"),
    }
}

/// Whether the request's `If-None-Match` names `etag`.
fn names(request: &HeaderMap, etag: &str) -> bool {
    request
        .get_all(header::IF_NONE_MATCH)
        .iter()
        .filter_map(|value| value.to_str().ok())
        .flat_map(|value| value.split(','))
        .any(|tag| tag.trim() == etag || tag.trim() == "*")
}

fn served(stored: &Stored, cache: &'static str, request: &HeaderMap) -> Response {
    let headers = [
        (header::CONTENT_TYPE, "application/json".to_string()),
        (header::CACHE_CONTROL, cache.to_string()),
        (header::ETAG, stored.etag.clone()),
    ];
    if names(request, &stored.etag) {
        (StatusCode::NOT_MODIFIED, headers).into_response()
    } else {
        (headers, stored.body.clone()).into_response()
    }
}
