//! Withdrawals: shared items or versions taken out of the catalogue, by
//! their author or by moderation, and the signed withdrawal list that tells
//! every family server to stop using them.

use jiff::Timestamp;
use serde_json::{Value, json};
use sqlx::{PgPool, Postgres, Transaction};

use crate::catalogue::{sign_index, store};
use crate::contract::{Kind, SigningKey};

pub const PATH: &str = "/v1/withdrawals";

/// How long a withdrawal list stays current (interface v1: one day).
const LIFETIME: jiff::SignedDuration = jiff::SignedDuration::from_hours(24);

/// Why a shared item or version was withdrawn.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Withdrawal {
    /// Its author's choice.
    Author,
    /// A provisional switch-off, until a moderator's decision.
    SwitchOff(Reason),
    /// A moderation decision.
    Moderation(Reason),
}

/// The reason a switch-off or a moderation decision gives family servers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Reason {
    Rights,
    PersonalData,
    SexualContent,
    IllegalContent,
    UnsuitableContent,
    WrongAnswer,
    Other,
}

impl Reason {
    pub(crate) fn code(self) -> &'static str {
        match self {
            Reason::Rights => "rights",
            Reason::PersonalData => "personal_data",
            Reason::SexualContent => "sexual_content",
            Reason::IllegalContent => "illegal_content",
            Reason::UnsuitableContent => "unsuitable_content",
            Reason::WrongAnswer => "wrong_answer",
            Reason::Other => "other",
        }
    }
}

impl Reason {
    /// The reason a code names, as reports and the withdrawal list write it.
    pub(crate) fn from_code(code: &str) -> Option<Reason> {
        [
            Reason::Rights,
            Reason::PersonalData,
            Reason::SexualContent,
            Reason::IllegalContent,
            Reason::UnsuitableContent,
            Reason::WrongAnswer,
            Reason::Other,
        ]
        .into_iter()
        .find(|reason| reason.code() == code)
    }
}

impl Withdrawal {
    /// Its kind and reason code, as the withdrawal list writes them.
    fn codes(self) -> (&'static str, &'static str) {
        match self {
            Withdrawal::Author => ("author", "author_choice"),
            Withdrawal::SwitchOff(reason) => ("switch_off", reason.code()),
            Withdrawal::Moderation(reason) => ("moderation", reason.code()),
        }
    }
}

/// Why a withdrawal or a reactivation could not be made.
#[derive(Debug)]
pub enum WithdrawError {
    UnknownItem,
    UnknownVersion,
    /// A reactivation of an item or version that is not switched off.
    NotSwitchedOff,
    Database(sqlx::Error),
}

impl From<sqlx::Error> for WithdrawError {
    fn from(error: sqlx::Error) -> Self {
        WithdrawError::Database(error)
    }
}

/// The stored version number: 0 for every version of the item.
fn stored(version: Option<u32>) -> i32 {
    version.map_or(0, |version| version as i32)
}

/// Opens a change of the catalogue, once the item or version is known.
async fn begin<'a>(
    pool: &'a PgPool,
    item: &str,
    version: Option<u32>,
) -> Result<Transaction<'a, Postgres>, WithdrawError> {
    let mut tx = pool.begin().await?;
    // One change of the catalogue at a time, so that documents follow it in order.
    sqlx::query("SELECT pg_advisory_xact_lock(1)")
        .execute(&mut *tx)
        .await?;
    let (latest,): (Option<i32>,) =
        sqlx::query_as("SELECT max(version) FROM item_versions WHERE item = $1")
            .bind(item)
            .fetch_one(&mut *tx)
            .await?;
    let latest = latest.ok_or(WithdrawError::UnknownItem)?;
    if version.is_some_and(|version| version == 0 || i64::from(version) > i64::from(latest)) {
        return Err(WithdrawError::UnknownVersion);
    }
    Ok(tx)
}

/// Withdraws a shared item, or one of its versions, at `now`; a later
/// withdrawal of the same item or version replaces the earlier one.
pub async fn withdraw(
    pool: &PgPool,
    signing: &SigningKey,
    item: &str,
    version: Option<u32>,
    withdrawal: Withdrawal,
    now: Timestamp,
) -> Result<(), WithdrawError> {
    let mut tx = begin(pool, item, version).await?;
    insert(&mut tx, item, version, withdrawal, now).await?;
    sign_index(&mut tx, signing, now).await?;
    sign(&mut tx, signing, now).await?;
    tx.commit().await?;
    Ok(())
}

/// Writes a withdrawal within a change of the catalogue already open, once
/// the item or version is known; the caller signs the documents again.
///
/// An author's withdrawal is final: it replaces any other withdrawal of the
/// same item or version, and none replaces it, so that no reactivation,
/// moderation decision or appeal brings back what its author withdrew.
pub(crate) async fn insert(
    tx: &mut Transaction<'_, Postgres>,
    item: &str,
    version: Option<u32>,
    withdrawal: Withdrawal,
    now: Timestamp,
) -> Result<(), sqlx::Error> {
    let (kind, reason) = withdrawal.codes();
    sqlx::query(
        "INSERT INTO withdrawals (item, version, kind, reason, withdrawn_at) VALUES ($1, $2, $3, $4, $5)
         ON CONFLICT (item, version) DO UPDATE
         SET kind = EXCLUDED.kind, reason = EXCLUDED.reason, withdrawn_at = EXCLUDED.withdrawn_at
         WHERE withdrawals.kind <> 'author'",
    )
    .bind(item)
    .bind(stored(version))
    .bind(kind)
    .bind(reason)
    .bind(now.as_second())
    .execute(&mut **tx)
    .await?;
    Ok(())
}

/// Ends the switch-off of a shared item or version at `now`: it leaves the
/// withdrawal list and comes back to the index.
pub async fn reactivate(
    pool: &PgPool,
    signing: &SigningKey,
    item: &str,
    version: Option<u32>,
    now: Timestamp,
) -> Result<(), WithdrawError> {
    let mut tx = begin(pool, item, version).await?;
    if !end_switch_off(&mut tx, item, version).await? {
        return Err(WithdrawError::NotSwitchedOff);
    }
    sign_index(&mut tx, signing, now).await?;
    sign(&mut tx, signing, now).await?;
    tx.commit().await?;
    Ok(())
}

/// Ends a switch-off within a change of the catalogue already open; answers
/// whether the item or version was switched off. The caller signs again.
pub(crate) async fn end_switch_off(
    tx: &mut Transaction<'_, Postgres>,
    item: &str,
    version: Option<u32>,
) -> Result<bool, sqlx::Error> {
    let ended = sqlx::query(
        "DELETE FROM withdrawals WHERE item = $1 AND version = $2 AND kind = 'switch_off'",
    )
    .bind(item)
    .bind(stored(version))
    .execute(&mut **tx)
    .await?;
    Ok(ended.rows_affected() > 0)
}

/// Signs the withdrawal list at `now`, as the catalogue stands.
pub async fn sign(
    tx: &mut Transaction<'_, Postgres>,
    signing: &SigningKey,
    now: Timestamp,
) -> Result<(), sqlx::Error> {
    let rows: Vec<(String, i32, String, String, i64)> = sqlx::query_as(
        "SELECT item, version, kind, reason, withdrawn_at FROM withdrawals ORDER BY item, version",
    )
    .fetch_all(&mut **tx)
    .await?;
    let listed: Vec<Value> = rows
        .into_iter()
        .map(|(item, version, kind, reason, withdrawn_at)| {
            json!({
                "item": item,
                "version": (version != 0).then_some(version),
                "kind": kind,
                "reason": reason,
                "withdrawn_at": crate::contract::time(
                    Timestamp::from_second(withdrawn_at).expect("a stored instant")
                ),
            })
        })
        .collect();
    let expires_at = now
        .checked_add(LIFETIME)
        .expect("a withdrawal list expiry within range");
    let list = signing.sign(
        Kind::Withdrawals,
        now,
        Some(expires_at),
        json!({"withdrawals": listed}),
    );
    store(tx, PATH, &list).await
}
