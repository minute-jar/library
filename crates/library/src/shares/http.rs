//! The routes of sharing (`docs/specs/interface-v1.md`, Shares): the parent
//! app's share, with the library account's session, and the author's view
//! of their shares and the sharing terms, for the website. No answer may be
//! cached.

use axum::Router;
use axum::body::Bytes;
use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post, put};
use serde::Deserialize;
use serde_json::{Value, json};

use super::{Desk, Prepared, SHARES_PER_DAY, SHARING_TERMS, Share, ShareOutcome, ShareRefusal};
use crate::accounts::Session;
use crate::accounts::http::{Refusal, signed_in};
use crate::answers::private as answer;
use crate::contract::time;
use crate::moderation::{Appeal, AppealError, Appellant};
use crate::{Library, Outcome};

pub fn router(library: Library) -> Router {
    Router::new()
        .route("/api/shares", post(share))
        .route("/api/account/shares", get(shares))
        .route("/api/sharing-terms", get(terms))
        .route("/api/account/sharing-terms", put(accept))
        .route("/api/account/notifications", get(notifications))
        .route("/api/account/reports/{report}/appeal", post(appeal))
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
                ShareRefusal::UnknownItem => (StatusCode::NOT_FOUND, "unknown_item"),
                ShareRefusal::NotTheAuthor => (StatusCode::FORBIDDEN, "not_the_author"),
                ShareRefusal::WithdrawnItem => (StatusCode::CONFLICT, "withdrawn_item"),
                ShareRefusal::UnknownOrigin => (StatusCode::NOT_FOUND, "unknown_original"),
                ShareRefusal::BadOrigin => (StatusCode::BAD_REQUEST, "bad_original"),
                ShareRefusal::OriginChanged => (StatusCode::CONFLICT, "original_changed"),
                ShareRefusal::NotEditorial => (StatusCode::FORBIDDEN, "not_editorial"),
                ShareRefusal::ReadingRequired => (StatusCode::BAD_REQUEST, "reading_required"),
                ShareRefusal::BadReading => (StatusCode::BAD_REQUEST, "bad_reading"),
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

/// What the author of the session's items must be told about reports on
/// them and their decisions, oldest first.
async fn notifications(State(library): State<Library>, headers: HeaderMap) -> Answer {
    let (session, _, _) = signed_in_to_share(&library, &headers).await?;
    let mut told = Vec::new();
    for (item, title) in super::authored(&library.pool, &session).await? {
        for notification in library.author_notifications(&item).await? {
            told.push((
                notification.at,
                json!({
                    "item": item,
                    "title": title,
                    "report": notification.report,
                    "reason": notification.reason.code(),
                    "details": notification.details,
                    "decision": notification.decision.map(|decision| json!({
                        "outcome": match decision.outcome {
                            Outcome::Upheld => "upheld",
                            Outcome::Rejected => "rejected",
                        },
                        "reasons": decision.reasons,
                    })),
                    "appeal": notification.appeal.map(|appeal| json!({
                        "outcome": appeal.outcome.code(),
                        "reasons": appeal.reasons,
                    })),
                    "at": time(notification.at),
                }),
            ));
        }
    }
    told.sort_by_key(|(at, _)| *at);
    let told: Vec<Value> = told
        .into_iter()
        .map(|(_, notification)| notification)
        .collect();
    Ok(answer(StatusCode::OK, json!({"notifications": told})))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct AuthorsAppeal {
    statement: String,
}

/// The author's appeal of a decision on a report of one of their items.
async fn appeal(
    State(library): State<Library>,
    Path(report): Path<String>,
    headers: HeaderMap,
    request: Bytes,
) -> Answer {
    let (session, now, _) = signed_in_to_share(&library, &headers).await?;
    let asked: AuthorsAppeal = serde_json::from_slice(&request).map_err(|_| BAD_REQUEST)?;
    const UNKNOWN_REPORT: Refused = Refused::Fixed(StatusCode::NOT_FOUND, "unknown_report");
    if !super::is_authors_report(&library.pool, &session, &report).await? {
        return Err(UNKNOWN_REPORT);
    }
    let appeal = Appeal {
        appellant: Appellant::Author,
        statement: asked.statement,
        contact: None,
    };
    let fixed = |status, code| Err(Refused::Fixed(status, code));
    match library.appeal(&report, appeal, now).await {
        Ok(appeal) => Ok(answer(StatusCode::ACCEPTED, json!({"appeal": appeal}))),
        Err(AppealError::UnknownReport) => Err(UNKNOWN_REPORT),
        Err(AppealError::NotDecided) => fixed(StatusCode::CONFLICT, "not_decided"),
        Err(AppealError::TooLate) => fixed(StatusCode::CONFLICT, "too_late"),
        Err(AppealError::AlreadyAppealed) => fixed(StatusCode::CONFLICT, "already_appealed"),
        Err(AppealError::StatementRequired) => fixed(StatusCode::BAD_REQUEST, "statement_required"),
        Err(AppealError::StatementTooLong | AppealError::BadContact) => Err(BAD_REQUEST),
        Err(AppealError::Unavailable | AppealError::Database(_)) => {
            Err(Refused::Share(ShareRefusal::Unavailable))
        }
    }
}
