//! Sharing an item from a library account (stories S10, S13, S16,
//! `docs/specs/interface-v1.md`, Shares).
//!
//! The parent app sends a prepared item with the library account's session,
//! never a library identifier. The account needs a recent strong sign-in, a
//! verified card and the sharing terms, accepted at the first share. Every
//! share goes through the review (ADR-0005), whose outcome is published (the
//! item enters the catalogue under the account's pseudonym), held for a
//! moderator, or refused with reasons per question. The review's cost is
//! bounded by a quota of shares per account and day.
//!
//! Each shared item is joined to the library account that answers for it
//! (table `authored_items`), so that the author hears of reports and
//! decisions and may appeal them.

pub(crate) mod held;
pub(crate) mod http;

use jiff::Timestamp;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::PgPool;

use crate::accounts::Session;
use crate::review::{RefusalReason, ReviewOutcome, Reviewer, Submission};
use crate::{Author, Curriculum, ItemType, Library, Metadata, NewVersion, PublishError};

/// The version of the sharing terms an author accepts: changed whenever
/// their text changes, so that every author accepts the new text at their
/// next share.
pub const SHARING_TERMS: &str = "2026-11";

/// How many items one library account may share in a day (UTC), whatever
/// the review's outcome: the review costs the operator at every share.
pub const SHARES_PER_DAY: u32 = 10;

/// The sharing part of the service, once turned on.
pub(crate) struct Desk {
    pub reviewer: Box<dyn Reviewer>,
}

/// Why a share is refused before the review.
#[derive(Debug)]
pub(crate) enum ShareRefusal {
    StrongSignInRequired,
    NoAccount,
    /// The account's card is not verified (ADR-0001).
    CardRequired,
    /// The account has not accepted the current sharing terms, and the
    /// share does not accept them.
    SharingTermsRequired,
    /// The prepared item's title or metadata do not hold.
    BadItem,
    /// The content is not in the bank format.
    BadContent,
    MediumTooLarge,
    MediaTooLarge,
    UnsupportedMedium,
    /// The account has shared [`SHARES_PER_DAY`] items today; it may share
    /// again from `next_share_at`.
    QuotaReached {
        next_share_at: Timestamp,
    },
    Unavailable,
}

impl From<sqlx::Error> for ShareRefusal {
    fn from(_: sqlx::Error) -> Self {
        ShareRefusal::Unavailable
    }
}

impl From<PublishError> for ShareRefusal {
    fn from(error: PublishError) -> Self {
        match error {
            PublishError::MediumTooLarge { .. } => ShareRefusal::MediumTooLarge,
            PublishError::MediaTooLarge => ShareRefusal::MediaTooLarge,
            PublishError::UnsupportedMedium { .. } => ShareRefusal::UnsupportedMedium,
            PublishError::BadItem | PublishError::Storage(_) | PublishError::Database(_) => {
                ShareRefusal::Unavailable
            }
        }
    }
}

/// A prepared item, as the parent app sends it and as a held share keeps it.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Prepared {
    #[serde(rename = "type")]
    pub kind: Kind,
    pub title: String,
    pub metadata: PreparedMetadata,
    /// The bank or learning path, in the bank format.
    pub content: Value,
    #[serde(default)]
    pub media: Vec<PreparedMedium>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Kind {
    Bank,
    Path,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct PreparedMetadata {
    pub language: String,
    pub subject: String,
    pub ages: Ages,
    pub description: String,
    pub curriculum: Option<PreparedCurriculum>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Ages {
    pub min: u8,
    pub max: u8,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct PreparedCurriculum {
    pub country: String,
    pub year: String,
    pub topic: String,
}

/// A picture or sound, its bytes in base64 (RFC 4648, with padding).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct PreparedMedium {
    pub base64: String,
}

/// The longest title, in characters.
pub const TITLE_LENGTH: usize = 120;
/// The longest description, in characters.
pub const DESCRIPTION_LENGTH: usize = 2000;

impl Prepared {
    /// The version to publish as `item` under `pseudonym`, once checked.
    fn version(&self, item: &str, pseudonym: &str) -> Result<NewVersion, ShareRefusal> {
        let filled = |text: &str, longest: usize| {
            let length = text.trim().chars().count();
            (1..=longest).contains(&length) && !text.chars().any(char::is_control)
        };
        let metadata = &self.metadata;
        let curriculum_holds = metadata.curriculum.as_ref().is_none_or(|curriculum| {
            [&curriculum.country, &curriculum.year, &curriculum.topic]
                .iter()
                .all(|text| filled(text, 64))
        });
        if !filled(&self.title, TITLE_LENGTH)
            || !filled(&metadata.language, 35)
            || !filled(&metadata.subject, 64)
            || metadata.description.chars().count() > DESCRIPTION_LENGTH
            || metadata.ages.min > metadata.ages.max
            || !curriculum_holds
        {
            return Err(ShareRefusal::BadItem);
        }
        let kind = match self.kind {
            Kind::Bank => ItemType::Bank,
            Kind::Path => ItemType::Path,
        };
        crate::bank_format::check(kind, &self.content).map_err(|_| ShareRefusal::BadContent)?;
        let media = self
            .media
            .iter()
            .map(|medium| {
                use base64::Engine;
                base64::engine::general_purpose::STANDARD
                    .decode(&medium.base64)
                    .map_err(|_| ShareRefusal::UnsupportedMedium)
            })
            .collect::<Result<Vec<_>, _>>()?;
        // Checked before the review, so that a share the catalogue would
        // refuse costs no review.
        crate::media::prepare(media.clone())?;
        Ok(NewVersion {
            item: item.to_string(),
            kind,
            title: self.title.trim().to_string(),
            author: Author {
                pseudonym: pseudonym.to_string(),
                editorial: false,
            },
            derived_from: None,
            metadata: Metadata {
                language: metadata.language.trim().to_string(),
                subject: metadata.subject.trim().to_string(),
                ages: (metadata.ages.min, metadata.ages.max),
                description: metadata.description.trim().to_string(),
                curriculum: metadata.curriculum.as_ref().map(|curriculum| Curriculum {
                    country: curriculum.country.trim().to_string(),
                    year: curriculum.year.trim().to_string(),
                    topic: curriculum.topic.trim().to_string(),
                }),
            },
            content: self.content.clone(),
            media,
        })
    }
}

/// The outcome of a share, as its author sees it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ShareOutcome {
    Published,
    /// Held for a moderator, who publishes or refuses it.
    Held,
    Refused,
}

impl ShareOutcome {
    fn from_code(code: &str) -> Self {
        match code {
            "published" => ShareOutcome::Published,
            "held" => ShareOutcome::Held,
            _ => ShareOutcome::Refused,
        }
    }
}

/// A share, as its author sees it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Share {
    pub share: String,
    /// The shared item, drawn unless the share was refused.
    pub item: Option<String>,
    pub title: String,
    pub outcome: ShareOutcome,
    /// The version published, once published.
    pub version: Option<u32>,
    /// A refusal's reasons, per question.
    pub reasons: Vec<RefusalReason>,
    pub shared_at: Timestamp,
}

/// The library account a session shares from.
struct Sharer {
    id: i64,
    pseudonym: String,
}

async fn sharer(pool: &PgPool, session: &Session) -> Result<Sharer, ShareRefusal> {
    let row: Option<(i64, String)> =
        sqlx::query_as("SELECT id, pseudonym FROM library_accounts WHERE subject = $1")
            .bind(&session.subject)
            .fetch_optional(pool)
            .await?;
    row.map(|(id, pseudonym)| Sharer { id, pseudonym })
        .ok_or(ShareRefusal::NoAccount)
}

/// 16 random bytes in lowercase hexadecimal: an item's or a share's identifier.
fn random_id() -> String {
    let mut bytes = [0u8; 16];
    getrandom::fill(&mut bytes).expect("the system's randomness");
    hex::encode(bytes)
}

/// A day, in seconds: days are counted in UTC, which has no leap seconds
/// in Unix time.
const DAY: i64 = 86_400;

/// The start of the day (UTC) of `now`.
fn this_day(now: Timestamp) -> Timestamp {
    Timestamp::from_second(now.as_second().div_euclid(DAY) * DAY).expect("a day within range")
}

/// The start of the next day (UTC) after `now`.
fn next_day(now: Timestamp) -> Timestamp {
    Timestamp::from_second(this_day(now).as_second() + DAY).expect("a day within range")
}

/// How many shares the account made on the day of `now`.
async fn shares_today(pool: &PgPool, account: i64, now: Timestamp) -> Result<u32, sqlx::Error> {
    let (count,): (i64,) =
        sqlx::query_as("SELECT count(*) FROM shares WHERE account = $1 AND shared_at >= $2")
            .bind(account)
            .bind(this_day(now).as_second())
            .fetch_one(pool)
            .await?;
    Ok(count as u32)
}

/// The version of the sharing terms the account accepted, if any.
async fn accepted_terms(pool: &PgPool, account: i64) -> Result<Option<String>, sqlx::Error> {
    let row: Option<(String,)> =
        sqlx::query_as("SELECT version FROM sharing_terms WHERE account = $1")
            .bind(account)
            .fetch_optional(pool)
            .await?;
    Ok(row.map(|(version,)| version))
}

async fn accept_terms(pool: &PgPool, account: i64, now: Timestamp) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO sharing_terms (account, version, accepted_at) VALUES ($1, $2, $3)
         ON CONFLICT (account) DO UPDATE SET version = $2, accepted_at = $3",
    )
    .bind(account)
    .bind(SHARING_TERMS)
    .bind(now.as_second())
    .execute(pool)
    .await?;
    Ok(())
}

/// Shares `prepared` from the session's library account at `now`, the
/// sharing terms accepted with it when `accepting` names the current ones;
/// answers the share once reviewed.
pub(crate) async fn share(
    library: &Library,
    desk: &Desk,
    session: &Session,
    prepared: Prepared,
    accepting: Option<&str>,
    now: Timestamp,
) -> Result<Share, ShareRefusal> {
    let pool = &library.pool;
    if !session.recently_strong(now) {
        return Err(ShareRefusal::StrongSignInRequired);
    }
    let sharer = sharer(pool, session).await?;
    let (carded,): (bool,) =
        sqlx::query_as("SELECT EXISTS (SELECT 1 FROM identification_cards WHERE account = $1)")
            .bind(sharer.id)
            .fetch_one(pool)
            .await?;
    if !carded {
        return Err(ShareRefusal::CardRequired);
    }
    let accepts = accepting == Some(SHARING_TERMS);
    if !accepts && accepted_terms(pool, sharer.id).await?.as_deref() != Some(SHARING_TERMS) {
        return Err(ShareRefusal::SharingTermsRequired);
    }
    let item = random_id();
    let version = prepared.version(&item, &sharer.pseudonym)?;
    if accepts {
        accept_terms(pool, sharer.id, now).await?;
    }
    let share = reserve(pool, &sharer, &version.title, now).await?;
    let outcome = desk
        .reviewer
        .review(Submission {
            item: &item,
            version: 1,
            title: &version.title,
            language: &version.metadata.language,
            content: &version.content,
            focus: None,
        })
        .await;
    let outcome = match outcome {
        Ok(outcome) => outcome,
        Err(_) => {
            // Not reviewed: it did not happen, and does not count.
            sqlx::query("DELETE FROM shares WHERE id = $1")
                .bind(&share)
                .execute(pool)
                .await?;
            return Err(ShareRefusal::Unavailable);
        }
    };
    let title = version.title.clone();
    let shared = match outcome {
        ReviewOutcome::Published => {
            author(pool, &item, sharer.id).await?;
            let published = library.publish_version(version, now).await?;
            sqlx::query(
                "UPDATE shares SET item = $2, outcome = 'published', version = $3, decided_at = $4
                 WHERE id = $1",
            )
            .bind(&share)
            .bind(&item)
            .bind(published as i32)
            .bind(now.as_second())
            .execute(pool)
            .await?;
            Share {
                share,
                item: Some(item),
                title,
                outcome: ShareOutcome::Published,
                version: Some(published),
                reasons: vec![],
                shared_at: now,
            }
        }
        ReviewOutcome::Held { findings } => {
            author(pool, &item, sharer.id).await?;
            sqlx::query(
                "UPDATE shares SET item = $2, outcome = 'held', findings = $3, submission = $4
                 WHERE id = $1",
            )
            .bind(&share)
            .bind(&item)
            .bind(&findings)
            .bind(serde_json::to_value(&prepared).expect("a prepared item as JSON"))
            .execute(pool)
            .await?;
            Share {
                share,
                item: Some(item),
                title,
                outcome: ShareOutcome::Held,
                version: None,
                reasons: vec![],
                shared_at: now,
            }
        }
        ReviewOutcome::Refused { reasons } => {
            sqlx::query(
                "UPDATE shares SET outcome = 'refused', reasons = $2, decided_at = $3 WHERE id = $1",
            )
            .bind(&share)
            .bind(serde_json::to_value(&reasons).expect("reasons as JSON"))
            .bind(now.as_second())
            .execute(pool)
            .await?;
            Share {
                share,
                item: None,
                title,
                outcome: ShareOutcome::Refused,
                version: None,
                reasons,
                shared_at: now,
            }
        }
    };
    // The sharing log: the identification data is kept for the legal
    // period after the last share (ticket 26).
    sqlx::query("UPDATE identifications SET last_shared_at = $2 WHERE account = $1")
        .bind(sharer.id)
        .bind(now.as_second())
        .execute(pool)
        .await?;
    Ok(shared)
}

/// Records a share being reviewed, counted in the day's quota, unless the
/// quota is reached; answers its identifier.
async fn reserve(
    pool: &PgPool,
    sharer: &Sharer,
    title: &str,
    now: Timestamp,
) -> Result<String, ShareRefusal> {
    let mut transaction = pool.begin().await?;
    // One share of an account at a time, so that the quota holds.
    sqlx::query("SELECT id FROM library_accounts WHERE id = $1 FOR UPDATE")
        .bind(sharer.id)
        .execute(&mut *transaction)
        .await?;
    let (count,): (i64,) =
        sqlx::query_as("SELECT count(*) FROM shares WHERE account = $1 AND shared_at >= $2")
            .bind(sharer.id)
            .bind(this_day(now).as_second())
            .fetch_one(&mut *transaction)
            .await?;
    if count >= i64::from(SHARES_PER_DAY) {
        return Err(ShareRefusal::QuotaReached {
            next_share_at: next_day(now),
        });
    }
    let share = random_id();
    sqlx::query(
        "INSERT INTO shares (id, account, title, outcome, shared_at)
         VALUES ($1, $2, $3, 'reviewing', $4)",
    )
    .bind(&share)
    .bind(sharer.id)
    .bind(title)
    .bind(now.as_second())
    .execute(&mut *transaction)
    .await?;
    transaction.commit().await?;
    Ok(share)
}

/// Joins `item` to the account that answers for it.
async fn author(pool: &PgPool, item: &str, account: i64) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO authored_items (item, account) VALUES ($1, $2) ON CONFLICT DO NOTHING",
    )
    .bind(item)
    .bind(account)
    .execute(pool)
    .await?;
    Ok(())
}

/// Where the session's account stands for sharing at `now`.
pub(crate) struct Standing {
    /// The version of the sharing terms accepted, if any.
    pub sharing_terms: Option<String>,
    pub shared_today: u32,
    pub shares: Vec<Share>,
}

/// The session's account's shares, newest first, and its quota.
pub(crate) async fn standing(
    pool: &PgPool,
    session: &Session,
    now: Timestamp,
) -> Result<Standing, ShareRefusal> {
    let sharer = sharer(pool, session).await?;
    type Row = (
        String,
        Option<String>,
        String,
        String,
        Option<i32>,
        Value,
        i64,
    );
    let rows: Vec<Row> = sqlx::query_as(
        "SELECT id, item, title, outcome, version, reasons, shared_at FROM shares
         WHERE account = $1 AND outcome <> 'reviewing'
         ORDER BY shared_at DESC, id",
    )
    .bind(sharer.id)
    .fetch_all(pool)
    .await?;
    let shares = rows
        .into_iter()
        .map(
            |(share, item, title, outcome, version, reasons, shared_at)| Share {
                share,
                item,
                title,
                outcome: ShareOutcome::from_code(&outcome),
                version: version.map(|version| version as u32),
                reasons: serde_json::from_value(reasons).unwrap_or_default(),
                shared_at: Timestamp::from_second(shared_at).expect("a stored instant"),
            },
        )
        .collect();
    Ok(Standing {
        sharing_terms: accepted_terms(pool, sharer.id).await?,
        shared_today: shares_today(pool, sharer.id, now).await?,
        shares,
    })
}

/// The instant the next share is possible, when the quota is reached at `now`.
pub(crate) fn next_share_at(shared_today: u32, now: Timestamp) -> Option<Timestamp> {
    (shared_today >= SHARES_PER_DAY).then(|| next_day(now))
}

/// Whether the report `report` is about an item of the session's account.
pub(crate) async fn is_authors_report(
    pool: &PgPool,
    session: &Session,
    report: &str,
) -> Result<bool, sqlx::Error> {
    let (theirs,): (bool,) = sqlx::query_as(
        "SELECT EXISTS (
             SELECT 1 FROM reports AS r
             JOIN authored_items AS i ON i.item = r.item
             JOIN library_accounts AS a ON a.id = i.account
             WHERE r.id = $1 AND a.subject = $2)",
    )
    .bind(report)
    .bind(&session.subject)
    .fetch_one(pool)
    .await?;
    Ok(theirs)
}

/// The published items of the session's account, with their latest title.
pub(crate) async fn authored(
    pool: &PgPool,
    session: &Session,
) -> Result<Vec<(String, String)>, sqlx::Error> {
    sqlx::query_as(
        "SELECT DISTINCT ON (i.item) i.item, v.title
         FROM authored_items AS i
         JOIN library_accounts AS a ON a.id = i.account
         JOIN item_versions AS v ON v.item = i.item
         WHERE a.subject = $1
         ORDER BY i.item, v.version DESC",
    )
    .bind(&session.subject)
    .fetch_all(pool)
    .await
}
