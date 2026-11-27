//! Closing a library account (story S12): its owner leaves, its items stay
//! published under their licence with its pseudonym, which no other account
//! may take; its identification data is kept for the legal period, then
//! erased.

use axum::Router;
use axum::extract::State;
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::delete;
use jiff::{SignedDuration, Timestamp};
use sqlx::PgPool;

use super::http::{Refusal, signed_in};
use super::{AccountError, Session, sqlx_time};
use crate::Library;

/// How long the identification data of a closed account is kept after its
/// last share, by default: one year (ticket 26, to be confirmed by counsel).
pub const IDENTIFICATION_RETENTION: SignedDuration = SignedDuration::from_hours(365 * 24);

pub fn router(library: Library) -> Router {
    Router::new()
        .route("/api/account", delete(close_route))
        .with_state(library)
}

async fn close_route(
    State(library): State<Library>,
    headers: HeaderMap,
) -> Result<Response, Refusal> {
    let (session, now) = signed_in(&library, &headers).await?;
    close(&library.pool, &session, now).await?;
    Ok((
        StatusCode::NO_CONTENT,
        [(header::CACHE_CONTROL, "private, no-store")],
    )
        .into_response())
}

/// Closes the session's library account at `now`, which needs a recent
/// strong sign-in, ends every session of its Minute Jar account and any
/// card check it left unfinished.
pub(crate) async fn close(
    pool: &PgPool,
    session: &Session,
    now: Timestamp,
) -> Result<(), AccountError> {
    if !session.recently_strong(now) {
        return Err(AccountError::StrongSignInRequired);
    }
    let mut transaction = pool.begin().await?;
    let closed: Option<(i64,)> = sqlx::query_as(
        "UPDATE library_accounts SET subject = NULL, closed_at = $2
         WHERE subject = $1
         RETURNING id",
    )
    .bind(&session.subject)
    .bind(sqlx_time(now))
    .fetch_optional(&mut *transaction)
    .await?;
    let Some((account,)) = closed else {
        return Err(AccountError::NoAccount);
    };
    // A card check left unfinished is no identification: it ends at once.
    sqlx::query("DELETE FROM card_checks WHERE account = $1")
        .bind(account)
        .execute(&mut *transaction)
        .await?;
    sqlx::query("DELETE FROM sessions WHERE subject = $1")
        .bind(&session.subject)
        .execute(&mut *transaction)
        .await?;
    transaction.commit().await?;
    Ok(())
}

/// Erases at `now` the identification data of closed accounts whose last
/// share is older than `kept_for`, and of those that never shared: their
/// identification and their verified card.
pub(crate) async fn erase_identifications(
    pool: &PgPool,
    now: Timestamp,
    kept_for: SignedDuration,
) -> Result<(), sqlx::Error> {
    let limit = now.checked_sub(kept_for).expect("a past instant");
    let mut transaction = pool.begin().await?;
    let erased: Vec<(i64,)> = sqlx::query_as(
        "DELETE FROM identifications AS identification
         USING library_accounts AS account
         WHERE account.id = identification.account
           AND account.closed_at IS NOT NULL
           AND (identification.last_shared_at IS NULL
                OR identification.last_shared_at <= $1)
         RETURNING identification.account",
    )
    .bind(sqlx_time(limit))
    .fetch_all(&mut *transaction)
    .await?;
    // The verified card is identification data too, erased with it.
    let accounts: Vec<i64> = erased.into_iter().map(|(account,)| account).collect();
    sqlx::query("DELETE FROM identification_cards WHERE account = ANY($1)")
        .bind(&accounts)
        .execute(&mut *transaction)
        .await?;
    transaction.commit().await?;
    Ok(())
}
