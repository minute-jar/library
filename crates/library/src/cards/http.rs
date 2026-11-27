//! The website's routes for a library account's card
//! (`docs/specs/accounts-api.md`, Card). No answer carries the provider's
//! reference or the card's fingerprint, and none may be cached.

use axum::Router;
use axum::extract::State;
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use serde_json::{Value, json};

use super::{CardRefusal, Desk, Verification};
use crate::Library;
use crate::accounts::Session;
use crate::accounts::http::{Refusal, signed_in};

pub fn router(library: Library) -> Router {
    Router::new()
        .route("/api/account/card", get(card).post(verify))
        .route("/api/account/card/check", post(finish))
        .with_state(library)
}

type Answer = Result<Response, Refused>;

/// Why a card route refuses.
enum Refused {
    /// Signed out, or accounts are unavailable.
    Session(Refusal),
    Card(CardRefusal),
    NoCard,
}

impl From<CardRefusal> for Refused {
    fn from(refusal: CardRefusal) -> Self {
        Refused::Card(refusal)
    }
}

impl IntoResponse for Refused {
    fn into_response(self) -> Response {
        match self {
            Refused::Session(refusal) => refusal.into_response(),
            Refused::Card(refusal) => refusal.into_response(),
            Refused::NoCard => refused(StatusCode::NOT_FOUND, "no_card"),
        }
    }
}

fn refused(status: StatusCode, error: &str) -> Response {
    answer(status, json!({"error": error}))
}

impl IntoResponse for CardRefusal {
    fn into_response(self) -> Response {
        match self {
            CardRefusal::StrongSignInRequired => {
                refused(StatusCode::FORBIDDEN, "strong_sign_in_required")
            }
            CardRefusal::NoAccount => refused(StatusCode::NOT_FOUND, "no_account"),
            CardRefusal::AlreadyVerified => refused(StatusCode::CONFLICT, "card_verified"),
            CardRefusal::NoCheck => refused(StatusCode::NOT_FOUND, "no_card_check"),
            CardRefusal::Pending => refused(StatusCode::CONFLICT, "card_check_pending"),
            CardRefusal::Refused => refused(StatusCode::BAD_REQUEST, "card_refused"),
            CardRefusal::Unavailable => refused(StatusCode::SERVICE_UNAVAILABLE, "unavailable"),
        }
    }
}

/// The session the request carries, the instant, and the cards' desk.
async fn signed_in_with_cards<'a>(
    library: &'a Library,
    request: &HeaderMap,
) -> Result<(Session, jiff::Timestamp, &'a Desk), Refused> {
    let (session, now) = signed_in(library, request)
        .await
        .map_err(Refused::Session)?;
    let desk = library.cards.as_deref().ok_or(CardRefusal::Unavailable)?;
    Ok((session, now, desk))
}

async fn card(State(library): State<Library>, headers: HeaderMap) -> Answer {
    let (session, _, _) = signed_in_with_cards(&library, &headers).await?;
    let card = super::card(&library.pool, &session)
        .await?
        .ok_or(Refused::NoCard)?;
    Ok(answer(StatusCode::OK, json!(card)))
}

async fn verify(State(library): State<Library>, headers: HeaderMap) -> Answer {
    let (session, now, desk) = signed_in_with_cards(&library, &headers).await?;
    Ok(
        match super::verify(&library.pool, desk, &session, now).await? {
            Verification::Verified(card) => answer(StatusCode::OK, json!({"card": card})),
            Verification::Started(url) => answer(StatusCode::ACCEPTED, json!({"check_url": url})),
        },
    )
}

async fn finish(State(library): State<Library>, headers: HeaderMap) -> Answer {
    let (session, now, desk) = signed_in_with_cards(&library, &headers).await?;
    let card = super::finish(&library.pool, desk, &session, now).await?;
    Ok(answer(StatusCode::OK, json!({"card": card})))
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
