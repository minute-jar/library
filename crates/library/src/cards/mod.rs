//! The verified card of a library account (ADR-0001, ADR-0007, stories S7
//! and S8, `docs/specs/accounts-api.md`).
//!
//! The service sees cards only through [`CardVerifier`] (a zero-amount check
//! with 3-D Secure, Stripe's SetupIntent in deployment) and
//! [`SubscriptionCards`] (the card of the subscription of a Minute Jar
//! account). It keeps the provider's reference and the card's fingerprint,
//! never the number, as identification data of the account, answered by no
//! route. The same card may serve several library accounts, and the
//! operator knows it ([`accounts_sharing_card`]).

pub(crate) mod http;

use std::future::Future;
use std::pin::Pin;

use jiff::Timestamp;
use sqlx::PgPool;

use crate::accounts::Session;

/// A card the payment provider verified, as the library keeps it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerifiedCard {
    /// The provider's reference of the check (a SetupIntent or a payment
    /// method), by which the operator can find it at the provider.
    pub reference: String,
    /// The provider's fingerprint of the card number: the same for every
    /// check of the same card.
    pub fingerprint: String,
}

/// A zero-amount check started at the provider.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StartedCheck {
    /// The provider's reference of the check, by which its result is read.
    pub check: String,
    /// Where to send the person's browser to enter the card; the provider
    /// sends it back to the website's card page.
    pub url: String,
}

/// Where a check stands.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CheckResult {
    /// The person has not finished it yet.
    Pending,
    Verified(VerifiedCard),
    /// The bank or the person refused it, or it was abandoned.
    Refused,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CardError {
    Unreachable,
}

pub type CardFuture<'a, T> = Pin<Box<dyn Future<Output = Result<T, CardError>> + Send + 'a>>;

/// The payment provider's zero-amount card check: Stripe in deployment
/// (ADR-0007), a fake in tests.
pub trait CardVerifier: Send + Sync + 'static {
    /// Starts a check.
    fn start(&self) -> CardFuture<'_, StartedCheck>;

    /// Reads where the check `check` stands.
    fn result<'a>(&'a self, check: &'a str) -> CardFuture<'a, CheckResult>;
}

/// The cards of subscriptions to hosted mode, at the same payment provider.
pub trait SubscriptionCards: Send + Sync + 'static {
    /// The card of the subscription paid by the Minute Jar account
    /// `subject`, if it has one.
    fn card_of<'a>(&'a self, subject: &'a str) -> CardFuture<'a, Option<VerifiedCard>>;
}

/// The cards' part of the service, once turned on.
pub(crate) struct Desk {
    pub verifier: Box<dyn CardVerifier>,
    pub subscriptions: Box<dyn SubscriptionCards>,
}

/// How a library account's card was verified.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum VerifiedBy {
    /// The card of the subscription of the same Minute Jar account (S7).
    Subscription,
    /// A zero-amount check (S8).
    Check,
}

/// A library account's card, as its owner sees it: neither the reference
/// nor the fingerprint.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct Card {
    pub verified_by: VerifiedBy,
    pub verified_at: Timestamp,
}

#[derive(Debug)]
pub(crate) enum CardRefusal {
    /// The sign-in is not strong, or no longer recent.
    StrongSignInRequired,
    NoAccount,
    /// The account's card is already verified.
    AlreadyVerified,
    /// No check was started, or the last one was refused.
    NoCheck,
    /// The person has not finished the check at the provider.
    Pending,
    /// The bank or the person refused the check.
    Refused,
    Unavailable,
}

impl From<sqlx::Error> for CardRefusal {
    fn from(_: sqlx::Error) -> Self {
        CardRefusal::Unavailable
    }
}

impl From<CardError> for CardRefusal {
    fn from(CardError::Unreachable: CardError) -> Self {
        CardRefusal::Unavailable
    }
}

/// What asking for the card to be verified gives.
pub(crate) enum Verification {
    /// Verified at once, by the subscription's card.
    Verified(Card),
    /// A check to finish at the provider, at this address.
    Started(String),
}

/// The library account opened on `subject`: its row and its choice.
async fn account_id(pool: &PgPool, subject: &str) -> Result<(i64, bool), CardRefusal> {
    let row: Option<(i64, String)> =
        sqlx::query_as("SELECT id, minute_jar_account FROM library_accounts WHERE subject = $1")
            .bind(subject)
            .fetch_optional(pool)
            .await?;
    row.map(|(id, choice)| (id, choice == "same_as_subscription"))
        .ok_or(CardRefusal::NoAccount)
}

/// The card of the account `account`, if verified.
async fn card_of_account(pool: &PgPool, account: i64) -> Result<Option<Card>, sqlx::Error> {
    let row: Option<(String, i64)> = sqlx::query_as(
        "SELECT verified_by, verified_at FROM identification_cards WHERE account = $1",
    )
    .bind(account)
    .fetch_optional(pool)
    .await?;
    Ok(row.map(|(by, at)| Card {
        verified_by: if by == "subscription" {
            VerifiedBy::Subscription
        } else {
            VerifiedBy::Check
        },
        verified_at: Timestamp::from_second(at).expect("a stored instant"),
    }))
}

/// Keeps `card` as the identification of `account`; the pending check, if
/// any, has served.
async fn record(
    pool: &PgPool,
    account: i64,
    card: &VerifiedCard,
    by: VerifiedBy,
    now: Timestamp,
) -> Result<Card, sqlx::Error> {
    let name = match by {
        VerifiedBy::Subscription => "subscription",
        VerifiedBy::Check => "check",
    };
    let mut transaction = pool.begin().await?;
    sqlx::query(
        "INSERT INTO identification_cards (account, reference, fingerprint, verified_by, verified_at)
         VALUES ($1, $2, $3, $4, $5)
         ON CONFLICT (account) DO NOTHING",
    )
    .bind(account)
    .bind(&card.reference)
    .bind(&card.fingerprint)
    .bind(name)
    .bind(now.as_second())
    .execute(&mut *transaction)
    .await?;
    sqlx::query("DELETE FROM card_checks WHERE account = $1")
        .bind(account)
        .execute(&mut *transaction)
        .await?;
    transaction.commit().await?;
    Ok(card_of_account(pool, account)
        .await?
        .expect("the card just recorded"))
}

/// The card of the session's library account, if verified.
pub(crate) async fn card(pool: &PgPool, session: &Session) -> Result<Option<Card>, CardRefusal> {
    let (account, _) = account_id(pool, &session.subject).await?;
    Ok(card_of_account(pool, account).await?)
}

/// Verifies the card of the session's library account at `now`: by the
/// subscription's card when the account is on the subscription's Minute Jar
/// account and it pays one (S7), otherwise by starting a zero-amount check
/// (S8).
pub(crate) async fn verify(
    pool: &PgPool,
    desk: &Desk,
    session: &Session,
    now: Timestamp,
) -> Result<Verification, CardRefusal> {
    if !session.recently_strong(now) {
        return Err(CardRefusal::StrongSignInRequired);
    }
    let (account, same_as_subscription) = account_id(pool, &session.subject).await?;
    if card_of_account(pool, account).await?.is_some() {
        return Err(CardRefusal::AlreadyVerified);
    }
    if same_as_subscription && let Some(card) = desk.subscriptions.card_of(&session.subject).await?
    {
        let card = record(pool, account, &card, VerifiedBy::Subscription, now).await?;
        return Ok(Verification::Verified(card));
    }
    let started = desk.verifier.start().await?;
    sqlx::query(
        "INSERT INTO card_checks (account, reference, started_at) VALUES ($1, $2, $3)
         ON CONFLICT (account) DO UPDATE SET reference = $2, started_at = $3",
    )
    .bind(account)
    .bind(&started.check)
    .bind(now.as_second())
    .execute(pool)
    .await?;
    Ok(Verification::Started(started.url))
}

/// Reads at `now` the result of the check the session's library account
/// started, once the browser is back from the provider.
pub(crate) async fn finish(
    pool: &PgPool,
    desk: &Desk,
    session: &Session,
    now: Timestamp,
) -> Result<Card, CardRefusal> {
    let (account, _) = account_id(pool, &session.subject).await?;
    if let Some(card) = card_of_account(pool, account).await? {
        return Ok(card);
    }
    let check: Option<(String,)> =
        sqlx::query_as("SELECT reference FROM card_checks WHERE account = $1")
            .bind(account)
            .fetch_optional(pool)
            .await?;
    let Some((check,)) = check else {
        return Err(CardRefusal::NoCheck);
    };
    match desk.verifier.result(&check).await? {
        CheckResult::Pending => Err(CardRefusal::Pending),
        CheckResult::Verified(card) => {
            Ok(record(pool, account, &card, VerifiedBy::Check, now).await?)
        }
        CheckResult::Refused => {
            sqlx::query("DELETE FROM card_checks WHERE account = $1 AND reference = $2")
                .bind(account)
                .bind(&check)
                .execute(pool)
                .await?;
            Err(CardRefusal::Refused)
        }
    }
}

/// The other library accounts, by pseudonym, whose card has the same
/// fingerprint as the account under `pseudonym`'s.
pub(crate) async fn accounts_sharing_card(
    pool: &PgPool,
    pseudonym: &str,
) -> Result<Vec<String>, sqlx::Error> {
    let others: Vec<(String,)> = sqlx::query_as(
        "SELECT other.pseudonym
         FROM library_accounts AS asked
         JOIN identification_cards AS card ON card.account = asked.id
         JOIN identification_cards AS same ON same.fingerprint = card.fingerprint
             AND same.account <> asked.id
         JOIN library_accounts AS other ON other.id = same.account
         WHERE lower(asked.pseudonym) = lower($1)
         ORDER BY other.pseudonym",
    )
    .bind(pseudonym.trim())
    .fetch_all(pool)
    .await?;
    Ok(others.into_iter().map(|(pseudonym,)| pseudonym).collect())
}
