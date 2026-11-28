//! The routes of comments (`docs/specs/catalogue-api.md`, Comments): a
//! library account comments on an item and reads its own comments; anyone
//! reports a published comment from the website; moderators decide held
//! comments and reports of comments. No route addresses an account.

use axum::Router;
use axum::body::Bytes;
use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use serde::Deserialize;
use serde_json::{Value, json};

use super::moderation::{CommentDecisionError, CommentReport, CommentReportError};
use super::{COMMENTS_PER_DAY, CommentOutcome, CommentRefusal, Desk};
use crate::Library;
use crate::accounts::http::signed_in;
use crate::contract::time;
use crate::moderation::http::moderator;
use crate::reports::{DecideError, Decision, Outcome};
use crate::shares::held::{HeldDecision, HeldOutcome};

pub fn router(library: Library) -> Router {
    Router::new()
        .route("/api/items/{item}/comments", post(comment))
        .route("/api/account/comments", get(own))
        .route("/api/comments/{comment}/reports", post(report))
        .route(
            "/api/moderation/comments/{comment}/decision",
            post(decide_comment),
        )
        .route(
            "/api/moderation/comment-reports/{report}/decision",
            post(decide_report),
        )
        .with_state(library)
}

/// Why a comments route refuses: a code and its body, or another module's
/// refusal (a session's, a moderator's) as it answers.
enum Refusal {
    Code(StatusCode, Value),
    Answered(Box<Response>),
}

impl IntoResponse for Refusal {
    fn into_response(self) -> Response {
        match self {
            Refusal::Code(status, body) => answer(status, body),
            Refusal::Answered(response) => *response,
        }
    }
}

/// Another module's refusal, as it answers.
fn answered(refusal: impl IntoResponse) -> Refusal {
    Refusal::Answered(Box::new(refusal.into_response()))
}

type Answer = Result<Response, Refusal>;

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

fn refused(status: StatusCode, code: &'static str) -> Refusal {
    Refusal::Code(status, json!({"error": code}))
}

fn unavailable() -> Refusal {
    refused(StatusCode::SERVICE_UNAVAILABLE, "unavailable")
}

fn bad_request() -> Refusal {
    refused(StatusCode::BAD_REQUEST, "bad_request")
}

impl CommentRefusal {
    fn refusal(self) -> Refusal {
        match self {
            CommentRefusal::NoAccount => refused(StatusCode::NOT_FOUND, "no_account"),
            CommentRefusal::CardRequired => refused(StatusCode::FORBIDDEN, "card_required"),
            CommentRefusal::UnknownItem => refused(StatusCode::NOT_FOUND, "unknown_item"),
            CommentRefusal::BadComment => refused(StatusCode::BAD_REQUEST, "bad_comment"),
            CommentRefusal::QuotaReached { next_comment_at } => Refusal::Code(
                StatusCode::TOO_MANY_REQUESTS,
                json!({
                    "error": "comment_quota_reached",
                    "comments_per_day": COMMENTS_PER_DAY,
                    "next_comment_at": time(next_comment_at),
                }),
            ),
            CommentRefusal::Unavailable => unavailable(),
        }
    }
}

/// The comments desk, once turned on.
fn desk(library: &Library) -> Result<&Desk, Refusal> {
    library.comments.as_deref().ok_or_else(unavailable)
}

/// A comment: its text alone. Nothing names another account: no
/// recipient, no reply to an account, no mention.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Asked {
    text: String,
}

async fn comment(
    State(library): State<Library>,
    Path(item): Path<String>,
    headers: HeaderMap,
    request: Bytes,
) -> Answer {
    let Ok(Value::Object(body)) = serde_json::from_slice::<Value>(&request) else {
        return Err(bad_request());
    };
    // A comment is never joined to a family server's library identifier.
    if body.contains_key("library_identifier") {
        return Err(refused(
            StatusCode::BAD_REQUEST,
            "library_identifier_not_allowed",
        ));
    }
    let desk = desk(&library)?;
    let (session, now) = signed_in(&library, &headers).await.map_err(answered)?;
    let asked: Asked = serde_json::from_value(Value::Object(body)).map_err(|_| bad_request())?;
    let commented = super::comment(&library.pool, desk, &session, &item, &asked.text, now)
        .await
        .map_err(CommentRefusal::refusal)?;
    let status = match commented.outcome {
        CommentOutcome::Published => StatusCode::CREATED,
        CommentOutcome::Held => StatusCode::ACCEPTED,
        CommentOutcome::Refused | CommentOutcome::Removed => StatusCode::OK,
    };
    Ok(answer(status, commented.json()))
}

async fn own(State(library): State<Library>, headers: HeaderMap) -> Answer {
    desk(&library)?;
    let (session, now) = signed_in(&library, &headers).await.map_err(answered)?;
    let (comments, today) = super::own(&library.pool, &session, now)
        .await
        .map_err(CommentRefusal::refusal)?;
    Ok(answer(
        StatusCode::OK,
        json!({
            "quota": {
                "comments_per_day": COMMENTS_PER_DAY,
                "commented_today": today,
                "next_comment_at": super::next_comment_at(today, now).map(time),
            },
            "comments": comments.iter().map(|comment| comment.json()).collect::<Vec<_>>(),
        }),
    ))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct AskedReport {
    reason: String,
    #[serde(default)]
    details: Option<String>,
}

/// A report of a published comment, from the website, anonymous: the
/// answer is the report's identifier, as for an item's report.
async fn report(
    State(library): State<Library>,
    Path(comment): Path<String>,
    request: Bytes,
) -> Answer {
    let desk = library.comments.as_deref().ok_or_else(unavailable)?;
    let asked: AskedReport = serde_json::from_slice(&request).map_err(|_| bad_request())?;
    let reason = crate::Reason::from_code(&asked.reason)
        .ok_or_else(|| refused(StatusCode::BAD_REQUEST, "bad_reason"))?;
    let now = (desk.clock)();
    let report = CommentReport {
        reason,
        details: asked.details,
    };
    match super::moderation::receive(&library.pool, &comment, report, now).await {
        Ok(report) => Ok((
            StatusCode::ACCEPTED,
            [
                (header::CONTENT_TYPE, "application/json"),
                (header::CACHE_CONTROL, "no-store"),
            ],
            json!({"report": report}).to_string(),
        )
            .into_response()),
        Err(CommentReportError::UnknownComment) => {
            Err(refused(StatusCode::NOT_FOUND, "unknown_comment"))
        }
        Err(CommentReportError::DetailsTooLong) => Err(bad_request()),
        Err(CommentReportError::Database(_)) => Err(unavailable()),
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
enum AskedHeldOutcome {
    Published,
    Refused,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct AskedHeldDecision {
    outcome: AskedHeldOutcome,
    reason: String,
    reasons: String,
}

fn decided() -> Response {
    (
        StatusCode::NO_CONTENT,
        [(header::CACHE_CONTROL, "private, no-store")],
    )
        .into_response()
}

/// A moderator publishes or refuses a comment the review held.
async fn decide_comment(
    State(library): State<Library>,
    Path(comment): Path<String>,
    headers: HeaderMap,
    request: Bytes,
) -> Answer {
    let moderator = moderator(&library, &headers).await.map_err(answered)?;
    let asked: AskedHeldDecision = serde_json::from_slice(&request).map_err(|_| bad_request())?;
    let reason = crate::Reason::from_code(&asked.reason).ok_or_else(bad_request)?;
    let decision = HeldDecision {
        outcome: match asked.outcome {
            AskedHeldOutcome::Published => HeldOutcome::Published,
            AskedHeldOutcome::Refused => HeldOutcome::Refused,
        },
        reason,
        reasons: asked.reasons,
    };
    library
        .decide_comment(&moderator.subject, &comment, decision, moderator.now)
        .await
        .map_err(|error| match error {
            CommentDecisionError::NotAModerator => {
                refused(StatusCode::FORBIDDEN, "not_a_moderator")
            }
            CommentDecisionError::UnknownComment => {
                refused(StatusCode::NOT_FOUND, "unknown_comment")
            }
            CommentDecisionError::AlreadyDecided => {
                refused(StatusCode::CONFLICT, "already_decided")
            }
            CommentDecisionError::ReasonsRequired => {
                refused(StatusCode::BAD_REQUEST, "reasons_required")
            }
            CommentDecisionError::Unavailable => unavailable(),
        })?;
    Ok(decided())
}

#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
enum AskedOutcome {
    Upheld,
    Rejected,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct AskedDecision {
    outcome: AskedOutcome,
    reasons: String,
}

/// A moderator upholds a report of a comment, which removes it, or rejects it.
async fn decide_report(
    State(library): State<Library>,
    Path(report): Path<String>,
    headers: HeaderMap,
    request: Bytes,
) -> Answer {
    let moderator = moderator(&library, &headers).await.map_err(answered)?;
    let asked: AskedDecision = serde_json::from_slice(&request).map_err(|_| bad_request())?;
    let decision = Decision {
        outcome: match asked.outcome {
            AskedOutcome::Upheld => Outcome::Upheld,
            AskedOutcome::Rejected => Outcome::Rejected,
        },
        reasons: asked.reasons,
    };
    library
        .decide_comment_report(&moderator.subject, &report, decision, moderator.now)
        .await
        .map_err(|error| match error {
            DecideError::UnknownReport => refused(StatusCode::NOT_FOUND, "unknown_report"),
            DecideError::AlreadyDecided => refused(StatusCode::CONFLICT, "already_decided"),
            DecideError::ReasonsRequired => refused(StatusCode::BAD_REQUEST, "reasons_required"),
            DecideError::NotAModerator => refused(StatusCode::FORBIDDEN, "not_a_moderator"),
            _ => unavailable(),
        })?;
    Ok(decided())
}
