//! The routes of moderation (`docs/specs/moderation-api.md`): the
//! administration routes under `/api/moderation/`, and the appeals of
//! reporters, who alone know their report's identifier.
//!
//! Every administration route needs the session of a moderator the operator
//! named, signed in with a passkey within the last 15 minutes; everyone else
//! is refused. No answer carries an e-mail address or a Minute Jar account's
//! subject, and none may be cached.

use axum::Router;
use axum::body::Bytes;
use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use jiff::Timestamp;
use serde::Deserialize;
use serde_json::{Value, json};

use super::{
    Appeal, AppealDecision, AppealError, AppealOutcome, Appellant, Case, Event, HistoryEntry,
};
use crate::Library;
use crate::accounts::http::{Refusal, signed_in};
use crate::contract::time;
use crate::reports::{DecideError, Decision, Outcome};

pub fn router(library: Library) -> Router {
    Router::new()
        .route("/api/moderation/queue", get(queue))
        .route("/api/moderation/items/{item}", get(history))
        .route(
            "/api/moderation/reports/{report}/decision",
            post(decide_report),
        )
        .route(
            "/api/moderation/appeals/{appeal}/decision",
            post(decide_appeal),
        )
        .route("/api/moderation/figures/{year}", get(figures))
        .route("/v1/reports/{report}/appeal", post(family_appeal))
        .route("/api/reports/{report}/appeal", post(website_appeal))
        .route("/v1/appeals/{appeal}", get(appeal_state))
        .with_state(library)
}

type Answer = Result<Response, Refused>;

/// A refusal: a fixed code, or the session's own (`signed_out`, `unavailable`).
enum Refused {
    Code(StatusCode, &'static str),
    Session(Refusal),
}

impl IntoResponse for Refused {
    fn into_response(self) -> Response {
        match self {
            Refused::Code(status, code) => answer(status, json!({"error": code})),
            Refused::Session(refusal) => refusal.into_response(),
        }
    }
}

const NO_STORE: &str = "private, no-store";

fn answer(status: StatusCode, body: Value) -> Response {
    (
        status,
        [
            (header::CONTENT_TYPE, "application/json"),
            (header::CACHE_CONTROL, NO_STORE),
        ],
        body.to_string(),
    )
        .into_response()
}

fn refused(status: StatusCode, code: &'static str) -> Refused {
    Refused::Code(status, code)
}

fn unavailable<E>(_: E) -> Refused {
    refused(StatusCode::SERVICE_UNAVAILABLE, "unavailable")
}

/// The moderator a request comes from.
struct Moderator {
    subject: String,
    now: Timestamp,
}

/// The moderator a request comes from, and the instant it is read at.
async fn moderator(library: &Library, headers: &HeaderMap) -> Result<Moderator, Refused> {
    let (session, now) = signed_in(library, headers)
        .await
        .map_err(Refused::Session)?;
    super::handle_of(&library.pool, &session.subject)
        .await
        .map_err(unavailable)?
        .ok_or_else(|| refused(StatusCode::FORBIDDEN, "not_a_moderator"))?;
    if !session.recently_passkey(now) {
        return Err(refused(StatusCode::FORBIDDEN, "passkey_sign_in_required"));
    }
    Ok(Moderator {
        subject: session.subject,
        now,
    })
}

/// Reads a JSON body that must be exactly `T`.
fn body<T: for<'de> Deserialize<'de>>(body: &Bytes) -> Result<T, Refused> {
    serde_json::from_slice(body).map_err(|_| refused(StatusCode::BAD_REQUEST, "bad_request"))
}

fn decide_refusal(error: DecideError) -> Refused {
    match error {
        DecideError::UnknownReport => refused(StatusCode::NOT_FOUND, "unknown_report"),
        DecideError::UnknownAppeal => refused(StatusCode::NOT_FOUND, "unknown_appeal"),
        DecideError::AlreadyDecided => refused(StatusCode::CONFLICT, "already_decided"),
        DecideError::ReasonsRequired => refused(StatusCode::BAD_REQUEST, "reasons_required"),
        DecideError::NotAModerator => refused(StatusCode::FORBIDDEN, "not_a_moderator"),
        DecideError::SameModerator => refused(StatusCode::FORBIDDEN, "same_moderator"),
        DecideError::Unavailable | DecideError::Database(_) => {
            refused(StatusCode::SERVICE_UNAVAILABLE, "unavailable")
        }
    }
}

fn decided() -> Response {
    (StatusCode::NO_CONTENT, [(header::CACHE_CONTROL, NO_STORE)]).into_response()
}

fn notice_json(notice: &Option<crate::FormalNotice>) -> Value {
    json!(
        notice
            .as_ref()
            .map(|notice| json!({"name": notice.name, "work": notice.work}))
    )
}

fn entry_json(entry: &HistoryEntry) -> Value {
    let at = time(entry.at);
    match &entry.event {
        Event::Reported {
            report,
            reason,
            details,
            effect,
            findings,
            notice,
        } => json!({
            "at": at, "event": "reported", "report": report, "reason": reason.code(),
            "details": details, "effect": effect.code(), "findings": findings,
            "notice": notice_json(notice),
        }),
        Event::Corrected { report } => json!({"at": at, "event": "corrected", "report": report}),
        Event::Decided {
            report,
            reason,
            outcome,
            reasons,
            moderator,
        } => json!({
            "at": at, "event": "decided", "report": report, "reason": reason.code(),
            "outcome": outcome.code(), "reasons": reasons, "moderator": moderator,
        }),
        Event::Appealed {
            appeal,
            report,
            appellant,
            statement,
        } => json!({
            "at": at, "event": "appealed", "appeal": appeal, "report": report,
            "appellant": appellant.code(), "statement": statement,
        }),
        Event::AppealDecided {
            appeal,
            report,
            outcome,
            reasons,
            moderator,
        } => json!({
            "at": at, "event": "appeal_decided", "appeal": appeal, "report": report,
            "outcome": outcome.code(), "reasons": reasons, "moderator": moderator,
        }),
    }
}

async fn history(
    State(library): State<Library>,
    Path(item): Path<String>,
    headers: HeaderMap,
) -> Answer {
    moderator(&library, &headers).await?;
    let history = library
        .moderation_history(&item)
        .await
        .map_err(unavailable)?
        .ok_or_else(|| refused(StatusCode::NOT_FOUND, "unknown_item"))?;
    let entries: Vec<Value> = history.entries.iter().map(entry_json).collect();
    Ok(answer(
        StatusCode::OK,
        json!({"item": history.item, "title": history.title, "history": entries}),
    ))
}

async fn case_json(library: &Library, case: &Case) -> Result<Value, sqlx::Error> {
    let due_at = time(case.due_at());
    Ok(match case {
        Case::Report(report) => json!({
            "kind": "report",
            "report": report.report,
            "item": report.item,
            "title": super::title(&library.pool, &report.item).await?,
            "version": report.version,
            "reason": report.reason.code(),
            "details": report.details,
            "effect": report.effect.code(),
            "findings": report.findings,
            "notice": notice_json(&report.notice),
            "received_at": time(report.received_at),
            "due_at": due_at,
        }),
        Case::Appeal(appeal) => json!({
            "kind": "appeal",
            "appeal": appeal.appeal,
            "report": appeal.report,
            "item": appeal.item,
            "title": super::title(&library.pool, &appeal.item).await?,
            "reason": appeal.reason.code(),
            "appellant": appeal.appellant.code(),
            "statement": appeal.statement,
            "decision": {
                "outcome": appeal.decision.outcome.code(),
                "reasons": appeal.decision.reasons,
                "moderator": appeal.moderator,
                "decided_at": time(appeal.decided_at),
            },
            "received_at": time(appeal.received_at),
            "due_at": due_at,
        }),
    })
}

async fn queue(State(library): State<Library>, headers: HeaderMap) -> Answer {
    moderator(&library, &headers).await?;
    let cases = super::cases(&library.pool).await.map_err(unavailable)?;
    let mut listed = Vec::with_capacity(cases.len());
    for case in &cases {
        listed.push(case_json(&library, case).await.map_err(unavailable)?);
    }
    Ok(answer(StatusCode::OK, json!({"cases": listed})))
}

#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
enum ReportOutcome {
    Upheld,
    Rejected,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ReportDecision {
    outcome: ReportOutcome,
    reasons: String,
}

async fn decide_report(
    State(library): State<Library>,
    Path(report): Path<String>,
    headers: HeaderMap,
    request: Bytes,
) -> Answer {
    let moderator = moderator(&library, &headers).await?;
    let asked: ReportDecision = body(&request)?;
    let decision = Decision {
        outcome: match asked.outcome {
            ReportOutcome::Upheld => Outcome::Upheld,
            ReportOutcome::Rejected => Outcome::Rejected,
        },
        reasons: asked.reasons.trim().to_string(),
    };
    library
        .decide_report(&moderator.subject, &report, decision, moderator.now)
        .await
        .map_err(decide_refusal)?;
    Ok(decided())
}

#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
enum AskedAppealOutcome {
    Maintained,
    Reversed,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct AskedAppealDecision {
    outcome: AskedAppealOutcome,
    reasons: String,
}

async fn decide_appeal(
    State(library): State<Library>,
    Path(appeal): Path<String>,
    headers: HeaderMap,
    request: Bytes,
) -> Answer {
    let moderator = moderator(&library, &headers).await?;
    let asked: AskedAppealDecision = body(&request)?;
    let decision = AppealDecision {
        outcome: match asked.outcome {
            AskedAppealOutcome::Maintained => AppealOutcome::Maintained,
            AskedAppealOutcome::Reversed => AppealOutcome::Reversed,
        },
        reasons: asked.reasons,
    };
    library
        .decide_appeal(&moderator.subject, &appeal, decision, moderator.now)
        .await
        .map_err(decide_refusal)?;
    Ok(decided())
}

/// A family server's appeal: no address, since it polls the appeal.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct FamilyAppeal {
    statement: String,
}

/// A website reporter's appeal, with an address for its decision if they leave one.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct WebsiteAppeal {
    statement: String,
    #[serde(default)]
    contact: Option<String>,
}

/// The reporter's appeal, the same answer as a report's: the family
/// routes' plain `Cache-Control: no-store`.
async fn receive_appeal(library: &Library, report: &str, appeal: Appeal) -> Answer {
    let Some(desk) = &library.reports else {
        return Err(unavailable(()));
    };
    let now = (desk.clock)();
    let refusal = |status: StatusCode, code: &'static str| Err(refused(status, code));
    match library.appeal(report, appeal, now).await {
        Ok(id) => Ok((
            StatusCode::ACCEPTED,
            [
                (header::CONTENT_TYPE, "application/json"),
                (header::CACHE_CONTROL, "no-store"),
            ],
            json!({"appeal": id}).to_string(),
        )
            .into_response()),
        Err(AppealError::UnknownReport) => refusal(StatusCode::NOT_FOUND, "unknown_report"),
        Err(AppealError::NotDecided) => refusal(StatusCode::CONFLICT, "not_decided"),
        Err(AppealError::TooLate) => refusal(StatusCode::CONFLICT, "too_late"),
        Err(AppealError::AlreadyAppealed) => refusal(StatusCode::CONFLICT, "already_appealed"),
        Err(AppealError::StatementRequired) => {
            refusal(StatusCode::BAD_REQUEST, "statement_required")
        }
        Err(AppealError::StatementTooLong) => refusal(StatusCode::BAD_REQUEST, "bad_request"),
        Err(AppealError::BadContact) => refusal(StatusCode::BAD_REQUEST, "bad_contact"),
        Err(AppealError::Unavailable | AppealError::Database(_)) => {
            refusal(StatusCode::SERVICE_UNAVAILABLE, "unavailable")
        }
    }
}

async fn family_appeal(
    State(library): State<Library>,
    Path(report): Path<String>,
    request: Bytes,
) -> Answer {
    let asked: FamilyAppeal = body(&request)?;
    let appeal = Appeal {
        appellant: Appellant::Reporter,
        statement: asked.statement,
        contact: None,
    };
    receive_appeal(&library, &report, appeal).await
}

async fn website_appeal(
    State(library): State<Library>,
    Path(report): Path<String>,
    request: Bytes,
) -> Answer {
    let asked: WebsiteAppeal = body(&request)?;
    let appeal = Appeal {
        appellant: Appellant::Reporter,
        statement: asked.statement,
        contact: asked.contact,
    };
    receive_appeal(&library, &report, appeal).await
}

/// Where an appeal stands, for its appellant, who alone knows its identifier.
async fn appeal_state(State(library): State<Library>, Path(appeal): Path<String>) -> Answer {
    let state = library
        .appeal_state(&appeal)
        .await
        .map_err(unavailable)?
        .ok_or(refused(StatusCode::NOT_FOUND, "unknown_appeal"))?;
    let decision = state.decision.map(|(decision, decided_at)| {
        json!({
            "outcome": decision.outcome.code(),
            "reasons": decision.reasons,
            "decided_at": time(decided_at),
        })
    });
    Ok((
        [
            (header::CONTENT_TYPE, "application/json"),
            (header::CACHE_CONTROL, "no-store"),
        ],
        json!({
            "appeal": state.appeal,
            "report": state.report,
            "status": state.status.code(),
            "received_at": time(state.received_at),
            "decision": decision,
        })
        .to_string(),
    )
        .into_response())
}

async fn figures(
    State(library): State<Library>,
    Path(year): Path<String>,
    headers: HeaderMap,
) -> Answer {
    moderator(&library, &headers).await?;
    let bad = || refused(StatusCode::BAD_REQUEST, "bad_request");
    let year: i16 = year.parse().map_err(|_| bad())?;
    let figures = library
        .transparency_figures(year)
        .await
        .map_err(unavailable)?
        .ok_or_else(bad)?;
    let reports: Vec<Value> = figures
        .reports
        .iter()
        .map(|(reason, count)| json!({"reason": reason.code(), "received": count}))
        .collect();
    let decisions: Vec<Value> = figures
        .decisions
        .iter()
        .map(|(reason, outcome, count)| {
            json!({"reason": reason.code(), "outcome": outcome.code(), "count": count})
        })
        .collect();
    let appeals: Vec<Value> = figures
        .appeals
        .iter()
        .map(|(appellant, status, count)| {
            json!({"appellant": appellant.code(), "outcome": status.code(), "count": count})
        })
        .collect();
    Ok(answer(
        StatusCode::OK,
        json!({"year": figures.year, "reports": reports, "decisions": decisions, "appeals": appeals}),
    ))
}
