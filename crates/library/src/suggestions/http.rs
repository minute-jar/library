//! The routes of suggestions (`docs/specs/interface-v1.md`, Suggestions):
//! the family server's anonymous suggestion, under its library identifier
//! and never with a session, and the author's view and acceptance, with the
//! library account's session, for the website. No author's answer may be
//! cached.

use axum::Router;
use axum::body::Bytes;
use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use serde::Deserialize;
use serde_json::json;

use super::{Accepted, SuggestionError};
use crate::Library;
use crate::accounts::http::signed_in;
use crate::answers::{private as answer, refused};

pub fn router(library: Library) -> Router {
    Router::new()
        .route("/v1/suggestions", post(suggest))
        .route("/api/account/suggestions", get(of_account))
        .route("/api/account/items/{item}/suggestions/accept", post(accept))
        .with_state(library)
}

fn suggestion_refused(error: SuggestionError) -> Response {
    match error {
        SuggestionError::BadIdentifier => {
            refused(StatusCode::BAD_REQUEST, "bad_library_identifier")
        }
        SuggestionError::BadSuggestion => refused(StatusCode::BAD_REQUEST, "bad_suggestion"),
        SuggestionError::UnknownItem => refused(StatusCode::NOT_FOUND, "unknown_item"),
        SuggestionError::WithdrawnItem => refused(StatusCode::CONFLICT, "withdrawn_item"),
        SuggestionError::UnknownVersion => refused(StatusCode::NOT_FOUND, "unknown_version"),
        SuggestionError::UnknownQuestion => refused(StatusCode::BAD_REQUEST, "unknown_question"),
        SuggestionError::NotImported => refused(StatusCode::FORBIDDEN, "not_imported"),
        SuggestionError::NotSuggested => refused(StatusCode::CONFLICT, "not_suggested"),
        SuggestionError::Unavailable => refused(StatusCode::SERVICE_UNAVAILABLE, "unavailable"),
    }
}

/// A suggestion from a family server, under its library identifier carried
/// in the body only, as for an import.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Suggestion {
    library_identifier: String,
    item: String,
    version: u32,
    question: u32,
    answers: Vec<String>,
}

async fn suggest(State(library): State<Library>, body: Bytes) -> Response {
    let Ok(suggestion) = serde_json::from_slice::<Suggestion>(&body) else {
        return refused(StatusCode::BAD_REQUEST, "bad_request");
    };
    let answers: Vec<&str> = suggestion.answers.iter().map(String::as_str).collect();
    match library
        .record_suggestion(
            &suggestion.library_identifier,
            &suggestion.item,
            suggestion.version,
            suggestion.question,
            &answers,
        )
        .await
    {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(error) => suggestion_refused(error),
    }
}

/// The suggestions of the session's items that have any.
async fn of_account(State(library): State<Library>, headers: HeaderMap) -> Response {
    let session = match signed_in(&library, &headers).await {
        Ok((session, _)) => session,
        Err(refusal) => return refusal.into_response(),
    };
    let Ok(authored) = crate::shares::authored(&library.pool, &session).await else {
        return suggestion_refused(SuggestionError::Unavailable);
    };
    let mut items = Vec::new();
    for (item, _) in authored {
        // A withdrawn item's suggestions can no longer be accepted.
        match super::is_withdrawn(&library.pool, &item).await {
            Ok(false) => {}
            Ok(true) => continue,
            Err(_) => return suggestion_refused(SuggestionError::Unavailable),
        }
        match library.item_suggestions(&item).await {
            Ok(Some(suggestions)) if !suggestions.questions.is_empty() => items.push(suggestions),
            Ok(_) => {}
            Err(error) => return suggestion_refused(error),
        }
    }
    answer(StatusCode::OK, json!({"items": items}))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Acceptance {
    accepted: Vec<AcceptedAnswers>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct AcceptedAnswers {
    question: u32,
    answers: Vec<String>,
}

/// The author accepts suggested answers of one of their items: answers the
/// prepared new version, for the share route.
async fn accept(
    State(library): State<Library>,
    Path(item): Path<String>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    let session = match signed_in(&library, &headers).await {
        Ok((session, _)) => session,
        Err(refusal) => return refusal.into_response(),
    };
    let Ok(acceptance) = serde_json::from_slice::<Acceptance>(&body) else {
        return refused(StatusCode::BAD_REQUEST, "bad_request");
    };
    let Ok(authored) = crate::shares::authored(&library.pool, &session).await else {
        return suggestion_refused(SuggestionError::Unavailable);
    };
    if !authored.iter().any(|(theirs, _)| *theirs == item) {
        return suggestion_refused(SuggestionError::UnknownItem);
    }
    let accepted: Vec<Accepted> = acceptance
        .accepted
        .into_iter()
        .map(|accepted| Accepted {
            question: accepted.question,
            answers: accepted.answers,
        })
        .collect();
    match library.prepare_from_suggestions(&item, &accepted).await {
        Ok(prepared) => answer(StatusCode::OK, json!({"prepared": prepared})),
        Err(error) => suggestion_refused(error),
    }
}
