//! The Selection mark: the items the operator's team read and chose
//! (`docs/specs/moderation-api.md`, Selection). Moderators set and remove
//! it; the index carries it as `selection`, and is signed again whenever it
//! changes.

use axum::Router;
use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::put;
use jiff::Timestamp;
use serde_json::json;
use sqlx::PgPool;

use crate::Library;
use crate::accounts::http::signed_in;
use crate::contract::SigningKey;

/// Why the Selection mark could not be set or removed.
#[derive(Debug)]
pub enum SelectionError {
    /// The Minute Jar account is not a moderator.
    NotAModerator,
    /// The catalogue does not show the item: unknown, withdrawn or hidden.
    UnknownItem,
    Database(sqlx::Error),
}

impl From<sqlx::Error> for SelectionError {
    fn from(error: sqlx::Error) -> Self {
        SelectionError::Database(error)
    }
}

/// The SQL of whether the index entry of `latest` (a version) is marked.
pub(crate) const SELECTED: &str =
    "EXISTS (SELECT 1 FROM selections WHERE selections.item = latest.item)";

/// Marks `item` Selection by the moderator `subject` at `now`; marking it
/// again changes nothing.
pub(crate) async fn select(
    pool: &PgPool,
    signing: &SigningKey,
    subject: &str,
    item: &str,
    now: Timestamp,
) -> Result<(), SelectionError> {
    change(pool, signing, subject, item, now, true).await
}

/// Removes the Selection mark of `item`, by the moderator `subject` at `now`.
pub(crate) async fn unselect(
    pool: &PgPool,
    signing: &SigningKey,
    subject: &str,
    item: &str,
    now: Timestamp,
) -> Result<(), SelectionError> {
    change(pool, signing, subject, item, now, false).await
}

async fn change(
    pool: &PgPool,
    signing: &SigningKey,
    subject: &str,
    item: &str,
    now: Timestamp,
    selected: bool,
) -> Result<(), SelectionError> {
    let handle = crate::moderation::handle_of(pool, subject)
        .await?
        .ok_or(SelectionError::NotAModerator)?;
    let mut tx = pool.begin().await?;
    // One change of the catalogue at a time, as at publication.
    sqlx::query("SELECT pg_advisory_xact_lock(1)")
        .execute(&mut *tx)
        .await?;
    let (shown,): (bool,) = sqlx::query_as(
        "SELECT EXISTS (SELECT 1 FROM item_versions AS v WHERE v.item = $1
             AND NOT EXISTS (SELECT 1 FROM withdrawals AS w WHERE w.item = v.item AND w.version IN (0, v.version))
             AND NOT EXISTS (SELECT 1 FROM hidden_items AS h WHERE h.item = v.item))",
    )
    .bind(item)
    .fetch_one(&mut *tx)
    .await?;
    if !shown {
        return Err(SelectionError::UnknownItem);
    }
    if selected {
        sqlx::query(
            "INSERT INTO selections (item, moderator, selected_at) VALUES ($1, $2, $3)
             ON CONFLICT DO NOTHING",
        )
        .bind(item)
        .bind(&handle)
        .bind(now.as_second())
        .execute(&mut *tx)
        .await?;
    } else {
        sqlx::query("DELETE FROM selections WHERE item = $1")
            .bind(item)
            .execute(&mut *tx)
            .await?;
    }
    crate::catalogue::sign_index(&mut tx, signing, now).await?;
    tx.commit().await?;
    Ok(())
}

/// Who marked `item` Selection and when, as moderators read it; `null`
/// when it is not marked.
pub(crate) async fn of_item(pool: &PgPool, item: &str) -> Result<serde_json::Value, sqlx::Error> {
    let row: Option<(String, i64)> =
        sqlx::query_as("SELECT moderator, selected_at FROM selections WHERE item = $1")
            .bind(item)
            .fetch_optional(pool)
            .await?;
    Ok(match row {
        None => serde_json::Value::Null,
        Some((moderator, selected_at)) => json!({
            "moderator": moderator,
            "selected_at": crate::contract::time(Timestamp::from_second(selected_at).expect("a stored instant")),
        }),
    })
}

pub fn router(library: Library) -> Router {
    Router::new()
        .route(
            "/api/moderation/items/{item}/selection",
            put(set).delete(remove),
        )
        .with_state(library)
}

async fn set(
    State(library): State<Library>,
    Path(item): Path<String>,
    headers: HeaderMap,
) -> Response {
    answer(&library, &headers, &item, true).await
}

async fn remove(
    State(library): State<Library>,
    Path(item): Path<String>,
    headers: HeaderMap,
) -> Response {
    answer(&library, &headers, &item, false).await
}

const NO_STORE: &str = "private, no-store";

fn refused(status: StatusCode, code: &str) -> Response {
    (
        status,
        [
            (header::CONTENT_TYPE, "application/json"),
            (header::CACHE_CONTROL, NO_STORE),
        ],
        json!({"error": code}).to_string(),
    )
        .into_response()
}

/// Sets or removes the mark for the moderator a request comes from, signed
/// in with a recent passkey, as every moderation route asks.
async fn answer(library: &Library, headers: &HeaderMap, item: &str, selected: bool) -> Response {
    let (session, now) = match signed_in(library, headers).await {
        Ok(signed) => signed,
        Err(refusal) => return refusal.into_response(),
    };
    match crate::moderation::handle_of(&library.pool, &session.subject).await {
        Ok(Some(_)) => {}
        Ok(None) => return refused(StatusCode::FORBIDDEN, "not_a_moderator"),
        Err(_) => return refused(StatusCode::SERVICE_UNAVAILABLE, "unavailable"),
    }
    if !session.recently_passkey(now) {
        return refused(StatusCode::FORBIDDEN, "passkey_sign_in_required");
    }
    let changed = if selected {
        library.select(&session.subject, item, now).await
    } else {
        library.unselect(&session.subject, item, now).await
    };
    match changed {
        Ok(()) => (StatusCode::NO_CONTENT, [(header::CACHE_CONTROL, NO_STORE)]).into_response(),
        Err(SelectionError::NotAModerator) => refused(StatusCode::FORBIDDEN, "not_a_moderator"),
        Err(SelectionError::UnknownItem) => refused(StatusCode::NOT_FOUND, "unknown_item"),
        Err(SelectionError::Database(_)) => refused(StatusCode::SERVICE_UNAVAILABLE, "unavailable"),
    }
}
