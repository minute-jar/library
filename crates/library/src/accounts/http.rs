//! The website's routes for sign-in, sessions and library accounts
//! (`docs/specs/accounts-api.md`). The session travels as
//! `Authorization: Bearer <session>`; no answer carries an e-mail, a name or
//! the Minute Jar account's subject, and none may be cached.

use axum::Router;
use axum::body::Bytes;
use axum::extract::{Request, State};
use axum::http::{HeaderMap, Method, StatusCode, header};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post, put};
use serde::Deserialize;
use serde_json::json;

use super::{AccountError, Desk, MinuteJarAccount, Opening, Session, SignInError};
use crate::Library;
use crate::answers::{NO_STORE, private as answer};

pub fn router(library: Library) -> Router {
    Router::new()
        .route("/api/sign-in", post(start))
        .route("/api/sign-in/callback", post(callback))
        .route("/api/session", get(current).delete(sign_out))
        .route("/api/account", get(account).post(open))
        .route("/api/account/pseudonym", put(rename))
        .route(
            "/api/account/earlier-pseudonyms",
            put(show_earlier_pseudonyms),
        )
        .with_state(library)
}

/// Refuses a family server's call (`POST /v1/...`, which carries a library
/// identifier) that also carries a session: the two are never joined.
pub async fn refuse_sessions_on_family_routes(request: Request, next: Next) -> Response {
    if request.method() == Method::POST
        && request.uri().path().starts_with("/v1/")
        && request.headers().contains_key(header::AUTHORIZATION)
    {
        return Refusal(StatusCode::BAD_REQUEST, "session_not_allowed").into_response();
    }
    next.run(request).await
}

/// A refusal, as a fixed code (`docs/specs/accounts-api.md`).
pub(crate) struct Refusal(StatusCode, &'static str);

const UNAVAILABLE: Refusal = Refusal(StatusCode::SERVICE_UNAVAILABLE, "unavailable");

impl IntoResponse for Refusal {
    fn into_response(self) -> Response {
        answer(self.0, json!({"error": self.1}))
    }
}

impl From<sqlx::Error> for Refusal {
    fn from(_: sqlx::Error) -> Self {
        UNAVAILABLE
    }
}

impl From<SignInError> for Refusal {
    fn from(error: SignInError) -> Self {
        match error {
            SignInError::BadSignIn => Refusal(StatusCode::BAD_REQUEST, "bad_sign_in"),
            SignInError::Unavailable => UNAVAILABLE,
        }
    }
}

impl From<AccountError> for Refusal {
    fn from(error: AccountError) -> Self {
        match error {
            AccountError::StrongSignInRequired => {
                Refusal(StatusCode::FORBIDDEN, "strong_sign_in_required")
            }
            AccountError::VerifiedEmailRequired => {
                Refusal(StatusCode::FORBIDDEN, "verified_email_required")
            }
            AccountError::StatementRequired => {
                Refusal(StatusCode::BAD_REQUEST, "statement_required")
            }
            AccountError::BadPseudonym => Refusal(StatusCode::BAD_REQUEST, "bad_pseudonym"),
            AccountError::PseudonymTaken => Refusal(StatusCode::CONFLICT, "pseudonym_taken"),
            AccountError::AccountExists => Refusal(StatusCode::CONFLICT, "account_exists"),
            AccountError::NoAccount => Refusal(StatusCode::NOT_FOUND, "no_account"),
            AccountError::Database(_) => UNAVAILABLE,
        }
    }
}

type Answer = Result<Response, Refusal>;

fn desk(library: &Library) -> Result<&Desk, Refusal> {
    library.accounts.as_deref().ok_or(UNAVAILABLE)
}

/// Reads a JSON body that must be exactly `T`, refusing anything else as
/// `bad_request` (unknown fields included, a library identifier among them).
fn body<T: for<'de> Deserialize<'de>>(body: &Bytes) -> Result<T, Refusal> {
    serde_json::from_slice(body).map_err(|_| Refusal(StatusCode::BAD_REQUEST, "bad_request"))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Start {
    #[serde(default)]
    strong: bool,
}

async fn start(State(library): State<Library>, request: Bytes) -> Answer {
    let desk = desk(&library)?;
    let start: Start = if request.is_empty() {
        Start { strong: false }
    } else {
        body(&request)?
    };
    let (state, url) = super::start(&library.pool, desk, start.strong).await?;
    Ok(answer(
        StatusCode::OK,
        json!({"state": state, "authorization_url": url}),
    ))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Callback {
    state: String,
    code: String,
}

async fn callback(State(library): State<Library>, request: Bytes) -> Answer {
    let desk = desk(&library)?;
    let callback: Callback = body(&request)?;
    let session = super::finish(&library.pool, desk, &callback.state, &callback.code).await?;
    Ok(answer(
        StatusCode::OK,
        json!({
            "session": session.token,
            "expires_at": crate::contract::time(session.expires_at),
        }),
    ))
}

/// The session the request carries, and the instant it is read at.
pub(crate) async fn signed_in(
    library: &Library,
    request: &HeaderMap,
) -> Result<(Session, jiff::Timestamp), Refusal> {
    const SIGNED_OUT: Refusal = Refusal(StatusCode::UNAUTHORIZED, "signed_out");
    let desk = desk(library)?;
    let now = (desk.clock)();
    let token = request
        .get(header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.strip_prefix("Bearer "))
        .ok_or(SIGNED_OUT)?;
    let session = super::session(&library.pool, token, now).await?;
    Ok((session.ok_or(SIGNED_OUT)?, now))
}

async fn current(State(library): State<Library>, request: HeaderMap) -> Answer {
    let (session, now) = signed_in(&library, &request).await?;
    let account = super::account_of(&library.pool, &session.subject).await?;
    let strong_until = session
        .strong_until
        .filter(|_| session.recently_strong(now))
        .map(crate::contract::time);
    Ok(answer(
        StatusCode::OK,
        json!({"strong_sign_in_until": strong_until, "account": account}),
    ))
}

async fn sign_out(State(library): State<Library>, request: HeaderMap) -> Answer {
    let (session, _) = signed_in(&library, &request).await?;
    super::end(&library.pool, &session).await?;
    Ok((StatusCode::NO_CONTENT, [(header::CACHE_CONTROL, NO_STORE)]).into_response())
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct OpenAccount {
    pseudonym: String,
    adult_and_parent: bool,
    minute_jar_account: MinuteJarAccount,
}

async fn open(State(library): State<Library>, headers: HeaderMap, request: Bytes) -> Answer {
    let (session, now) = signed_in(&library, &headers).await?;
    let opening: OpenAccount = body(&request)?;
    let opening = Opening {
        pseudonym: &opening.pseudonym,
        adult_and_parent: opening.adult_and_parent,
        minute_jar_account: opening.minute_jar_account,
    };
    let account = super::open(&library.pool, &session, opening, now).await?;
    Ok(answer(StatusCode::CREATED, json!(account)))
}

async fn account(State(library): State<Library>, headers: HeaderMap) -> Answer {
    let (session, _) = signed_in(&library, &headers).await?;
    let account = super::account_of(&library.pool, &session.subject)
        .await?
        .ok_or(AccountError::NoAccount)?;
    Ok(answer(StatusCode::OK, json!(account)))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Rename {
    pseudonym: String,
}

async fn rename(State(library): State<Library>, headers: HeaderMap, request: Bytes) -> Answer {
    let (session, _) = signed_in(&library, &headers).await?;
    let rename: Rename = body(&request)?;
    let account = super::rename(&library.pool, &session, &rename.pseudonym).await?;
    Ok(answer(StatusCode::OK, json!(account)))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct EarlierPseudonyms {
    shown: bool,
}

async fn show_earlier_pseudonyms(
    State(library): State<Library>,
    headers: HeaderMap,
    request: Bytes,
) -> Answer {
    let (session, _) = signed_in(&library, &headers).await?;
    let asked: EarlierPseudonyms = body(&request)?;
    let account = super::show_earlier_pseudonyms(&library.pool, &session, asked.shown).await?;
    Ok(answer(StatusCode::OK, json!(account)))
}
