//! The routes of sharing (`docs/specs/interface-v1.md`, Shares): the parent
//! app's share, with the library account's session, and the author's view
//! of their shares and the sharing terms, for the website. No answer may be
//! cached.

use axum::Router;
use axum::body::Bytes;
use axum::extract::State;
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post, put};
use serde::Deserialize;
use serde_json::{Value, json};

use super::{Desk, Prepared, SHARES_PER_DAY, SHARING_TERMS, Share, ShareOutcome, ShareRefusal};
use crate::Library;
use crate::accounts::Session;
use crate::accounts::http::{Refusal, signed_in};
use crate::contract::time;

pub fn router(library: Library) -> Router {
    Router::new()
        .route("/api/shares", post(share))
        .route("/api/account/shares", get(shares))
        .route("/api/sharing-terms", get(terms))
        .route("/api/account/sharing-terms", put(accept))
        .with_state(library)
}

/// Why a sharing route refuses.
enum Refused {
    /// Signed out, or accounts are unavailable.
    Session(Refusal),
    Share(ShareRefusal),
    Fixed(StatusCode, &'static str),
}

impl From<ShareRefusal> for Refused {
    fn from(refusal: ShareRefusal) -> Self {
        Refused::Share(refusal)
    }
}

impl From<sqlx::Error> for Refused {
    fn from(_: sqlx::Error) -> Self {
        Refused::Share(ShareRefusal::Unavailable)
    }
}

impl IntoResponse for Refused {
    fn into_response(self) -> Response {
        let (status, error) = match self {
            Refused::Session(refusal) => return refusal.into_response(),
            Refused::Fixed(status, error) => (status, error),
            Refused::Share(refusal) => match refusal {
                ShareRefusal::StrongSignInRequired => {
                    (StatusCode::FORBIDDEN, "strong_sign_in_required")
                }
                ShareRefusal::NoAccount => (StatusCode::NOT_FOUND, "no_account"),
                ShareRefusal::CardRequired => (StatusCode::FORBIDDEN, "card_required"),
                ShareRefusal::SharingTermsRequired => {
                    (StatusCode::FORBIDDEN, "sharing_terms_required")
                }
                ShareRefusal::BadItem => (StatusCode::BAD_REQUEST, "bad_item"),
                ShareRefusal::BadContent => (StatusCode::BAD_REQUEST, "bad_content"),
                ShareRefusal::MediumTooLarge => (StatusCode::BAD_REQUEST, "medium_too_large"),
                ShareRefusal::MediaTooLarge => (StatusCode::BAD_REQUEST, "media_too_large"),
                ShareRefusal::UnsupportedMedium => (StatusCode::BAD_REQUEST, "unsupported_medium"),
                ShareRefusal::QuotaReached { next_share_at } => {
                    return answer(
                        StatusCode::TOO_MANY_REQUESTS,
                        json!({
                            "error": "share_quota_reached",
                            "shares_per_day": SHARES_PER_DAY,
                            "next_share_at": time(next_share_at),
                        }),
                    );
                }
                ShareRefusal::Unavailable => (StatusCode::SERVICE_UNAVAILABLE, "unavailable"),
            },
        };
        answer(status, json!({"error": error}))
    }
}

type Answer = Result<Response, Refused>;

const BAD_REQUEST: Refused = Refused::Fixed(StatusCode::BAD_REQUEST, "bad_request");

/// The session the request carries and the instant, with sharing turned on.
async fn signed_in_to_share<'a>(
    library: &'a Library,
    request: &HeaderMap,
) -> Result<(Session, jiff::Timestamp, &'a Desk), Refused> {
    let desk = library
        .sharing
        .as_deref()
        .ok_or(Refused::Share(ShareRefusal::Unavailable))?;
    let (session, now) = signed_in(library, request)
        .await
        .map_err(Refused::Session)?;
    Ok((session, now, desk))
}

async fn share(State(library): State<Library>, headers: HeaderMap, request: Bytes) -> Answer {
    let Ok(Value::Object(mut body)) = serde_json::from_slice::<Value>(&request) else {
        return Err(BAD_REQUEST);
    };
    // A share is never joined to a family server's library identifier.
    if body.contains_key("library_identifier") {
        return Err(Refused::Fixed(
            StatusCode::BAD_REQUEST,
            "library_identifier_not_allowed",
        ));
    }
    let (session, now, desk) = signed_in_to_share(&library, &headers).await?;
    let accepting = match body.remove("sharing_terms") {
        None | Some(Value::Null) => None,
        Some(Value::String(version)) => Some(version),
        Some(_) => return Err(BAD_REQUEST),
    };
    let prepared: Prepared =
        serde_json::from_value(Value::Object(body)).map_err(|_| BAD_REQUEST)?;
    let shared = super::share(
        &library,
        desk,
        &session,
        prepared,
        accepting.as_deref(),
        now,
    )
    .await?;
    let status = match shared.outcome {
        ShareOutcome::Published => StatusCode::CREATED,
        ShareOutcome::Held => StatusCode::ACCEPTED,
        ShareOutcome::Refused => StatusCode::OK,
    };
    Ok(answer(status, share_json(&shared)))
}

fn share_json(share: &Share) -> Value {
    json!({
        "share": share.share,
        "item": share.item,
        "title": share.title,
        "outcome": share.outcome,
        "version": share.version,
        "reasons": share.reasons,
        "shared_at": time(share.shared_at),
    })
}

async fn shares(State(library): State<Library>, headers: HeaderMap) -> Answer {
    let (session, now, _) = signed_in_to_share(&library, &headers).await?;
    let standing = super::standing(&library.pool, &session, now).await?;
    Ok(answer(
        StatusCode::OK,
        json!({
            "sharing_terms": {
                "current": SHARING_TERMS,
                "accepted": standing.sharing_terms,
            },
            "quota": {
                "shares_per_day": SHARES_PER_DAY,
                "shared_today": standing.shared_today,
                "next_share_at": super::next_share_at(standing.shared_today, now).map(time),
            },
            "shares": standing.shares.iter().map(share_json).collect::<Vec<_>>(),
        }),
    ))
}

async fn terms(State(library): State<Library>) -> Answer {
    library
        .sharing
        .as_deref()
        .ok_or(Refused::Share(ShareRefusal::Unavailable))?;
    Ok(answer(
        StatusCode::OK,
        json!({"version": SHARING_TERMS, "shares_per_day": SHARES_PER_DAY}),
    ))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Accept {
    version: String,
}

async fn accept(State(library): State<Library>, headers: HeaderMap, request: Bytes) -> Answer {
    let (session, now, _) = signed_in_to_share(&library, &headers).await?;
    let accept: Accept = serde_json::from_slice(&request).map_err(|_| BAD_REQUEST)?;
    if accept.version != SHARING_TERMS {
        return Err(Refused::Fixed(
            StatusCode::CONFLICT,
            "outdated_sharing_terms",
        ));
    }
    let sharer = super::sharer(&library.pool, &session).await?;
    super::accept_terms(&library.pool, sharer.id, now).await?;
    Ok(answer(
        StatusCode::OK,
        json!({"current": SHARING_TERMS, "accepted": SHARING_TERMS}),
    ))
}

fn answer(status: StatusCode, body: Value) -> Response {
    (
        status,
        [
            (header::CONTENT_TYPE, "application/json"),
            (header::CACHE_CONTROL, "private, no-store"),
        ],
        body.to_string(),
    )
        .into_response()
}
