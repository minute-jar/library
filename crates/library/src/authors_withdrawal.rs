//! The author's withdrawal (story S17, `docs/specs/interface-v1.md`,
//! Shares): the library account that answers for a shared item (table
//! `authored_items`) withdraws it, or one of its versions, after a recent
//! strong sign-in. It leaves the index and the catalogue and enters the
//! withdrawal list with the author's kind, for good: no reactivation, no
//! moderation decision and no appeal ends it (see `withdrawals::insert`).
//!
//! The author also reads their published items, their versions and what
//! withdrew them, for the website. No answer may be cached.

use axum::Router;
use axum::body::Bytes;
use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use jiff::Timestamp;
use serde::Deserialize;
use serde_json::{Value, json};
use sqlx::PgPool;

use crate::Library;
use crate::accounts::Session;
use crate::accounts::http::{Refusal, signed_in};
use crate::answers::private as answer;
use crate::contract::time;
use crate::withdrawals::{self, WithdrawError, Withdrawal};

pub fn router(library: Library) -> Router {
    Router::new()
        .route("/api/account/items", get(items))
        .route("/api/account/items/{item}/withdrawal", post(withdraw))
        .with_state(library)
}

/// Why an author's route refuses.
enum Refused {
    /// Signed out, or accounts are unavailable.
    Session(Refusal),
    Fixed(StatusCode, &'static str),
}

const BAD_REQUEST: Refused = Refused::Fixed(StatusCode::BAD_REQUEST, "bad_request");
const UNKNOWN_ITEM: Refused = Refused::Fixed(StatusCode::NOT_FOUND, "unknown_item");
const UNAVAILABLE: Refused = Refused::Fixed(StatusCode::SERVICE_UNAVAILABLE, "unavailable");

impl From<sqlx::Error> for Refused {
    fn from(_: sqlx::Error) -> Self {
        UNAVAILABLE
    }
}

impl IntoResponse for Refused {
    fn into_response(self) -> Response {
        match self {
            Refused::Session(refusal) => refusal.into_response(),
            Refused::Fixed(status, error) => answer(status, json!({"error": error})),
        }
    }
}

type Answer = Result<Response, Refused>;

/// The id of the session's library account.
async fn account(pool: &PgPool, session: &Session) -> Result<i64, Refused> {
    let row: Option<(i64,)> = sqlx::query_as("SELECT id FROM library_accounts WHERE subject = $1")
        .bind(&session.subject)
        .fetch_optional(pool)
        .await?;
    row.map(|(id,)| id)
        .ok_or(Refused::Fixed(StatusCode::NOT_FOUND, "no_account"))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Asked {
    /// The version to withdraw; every version when `null`.
    version: Option<u32>,
}

/// The author's withdrawal of their item, or of one of its versions.
async fn withdraw(
    State(library): State<Library>,
    Path(item): Path<String>,
    headers: HeaderMap,
    request: Bytes,
) -> Answer {
    let (session, now) = signed_in(&library, &headers)
        .await
        .map_err(Refused::Session)?;
    let asked: Asked = serde_json::from_slice(&request).map_err(|_| BAD_REQUEST)?;
    if !session.recently_strong(now) {
        return Err(Refused::Fixed(
            StatusCode::FORBIDDEN,
            "strong_sign_in_required",
        ));
    }
    let pool = &library.pool;
    let account = account(pool, &session).await?;
    // Another account's item, and an item never published, answer alike.
    let (theirs,): (bool,) = sqlx::query_as(
        "SELECT EXISTS (SELECT 1 FROM authored_items WHERE item = $1 AND account = $2)",
    )
    .bind(&item)
    .bind(account)
    .fetch_one(pool)
    .await?;
    if !theirs {
        return Err(UNKNOWN_ITEM);
    }
    let (already,): (bool,) = sqlx::query_as(
        "SELECT EXISTS (SELECT 1 FROM withdrawals
                        WHERE item = $1 AND version IN (0, $2) AND kind = 'author')",
    )
    .bind(&item)
    .bind(asked.version.map_or(0, |version| version as i32))
    .fetch_one(pool)
    .await?;
    if already {
        return Err(Refused::Fixed(StatusCode::CONFLICT, "already_withdrawn"));
    }
    match withdrawals::withdraw(
        pool,
        &library.signing,
        &item,
        asked.version,
        Withdrawal::Author,
        now,
    )
    .await
    {
        Ok(()) => Ok(answer(
            StatusCode::OK,
            json!({
                "item": item,
                "version": asked.version,
                "kind": "author",
                "reason": "author_choice",
                "withdrawn_at": time(now),
            }),
        )),
        Err(WithdrawError::UnknownItem) => Err(UNKNOWN_ITEM),
        Err(WithdrawError::UnknownVersion) => {
            Err(Refused::Fixed(StatusCode::NOT_FOUND, "unknown_version"))
        }
        Err(WithdrawError::NotSwitchedOff | WithdrawError::Database(_)) => Err(UNAVAILABLE),
    }
}

/// A withdrawal, as the author reads it.
fn withdrawal(kind: Option<String>, reason: Option<String>, at: Option<i64>) -> Value {
    match (kind, reason, at) {
        (Some(kind), Some(reason), Some(at)) => json!({
            "kind": kind,
            "reason": reason,
            "withdrawn_at": time(Timestamp::from_second(at).expect("a stored instant")),
        }),
        _ => Value::Null,
    }
}

/// The session's account's published items, the latest published first,
/// each with its versions, newest first, and what withdrew them.
async fn items(State(library): State<Library>, headers: HeaderMap) -> Answer {
    let (session, _) = signed_in(&library, &headers)
        .await
        .map_err(Refused::Session)?;
    let pool = &library.pool;
    let account = account(pool, &session).await?;
    type Row = (
        String,
        i32,
        String,
        i64,
        Option<String>,
        Option<String>,
        Option<i64>,
        Option<String>,
        Option<String>,
        Option<i64>,
    );
    let rows: Vec<Row> = sqlx::query_as(
        "SELECT v.item, v.version, v.title, v.published_at,
                w.kind, w.reason, w.withdrawn_at, wi.kind, wi.reason, wi.withdrawn_at
         FROM authored_items AS a
         JOIN item_versions AS v ON v.item = a.item
         LEFT JOIN withdrawals AS w ON w.item = v.item AND w.version = v.version
         LEFT JOIN withdrawals AS wi ON wi.item = v.item AND wi.version = 0
         WHERE a.account = $1
         ORDER BY max(v.published_at) OVER (PARTITION BY v.item) DESC, v.item, v.version DESC",
    )
    .bind(account)
    .fetch_all(pool)
    .await?;
    let mut listed: Vec<Value> = Vec::new();
    for (item, version, title, published_at, kind, reason, at, item_kind, item_reason, item_at) in
        rows
    {
        let entry = json!({
            "version": version,
            "published_at": time(Timestamp::from_second(published_at).expect("a stored instant")),
            "withdrawal": withdrawal(kind, reason, at),
        });
        match listed.last_mut() {
            Some(last) if last["item"] == item.as_str() => {
                last["versions"].as_array_mut().unwrap().push(entry);
            }
            _ => listed.push(json!({
                "item": item,
                "title": title,
                "withdrawal": withdrawal(item_kind, item_reason, item_at),
                "versions": [entry],
            })),
        }
    }
    Ok(answer(StatusCode::OK, json!({"items": listed})))
}
