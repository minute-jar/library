//! Comments from library accounts (stories S28, S29,
//! `docs/specs/catalogue-api.md`, Comments).
//!
//! A library account comments on a shared item's page under its pseudonym.
//! Every comment goes through the review (ADR-0005) before it shows: it is
//! published, held for a moderator, or refused with reasons. A published
//! comment can be reported, and a moderator decides the report.
//!
//! A comment is addressed to an item, never to an account: there are no
//! private messages, no replies to an account, no mentions, and no route
//! through which one account reaches another (S29).

pub(crate) mod http;
pub(crate) mod moderation;

use jiff::Timestamp;
use serde_json::{Value, json};
use sqlx::PgPool;

use crate::accounts::Session;
use crate::review::{RefusalReason, ReviewOutcome, ReviewProvider, Submission};

/// The longest comment, in characters.
pub const COMMENT_LENGTH: usize = 1000;

/// How many comments one library account may write in a day (UTC),
/// whatever the review's outcome: the review costs the operator at every
/// comment.
pub const COMMENTS_PER_DAY: u32 = 20;

/// The comments part of the service, once turned on.
pub(crate) struct Desk {
    pub provider: Box<dyn ReviewProvider>,
    /// The clock the anonymous routes read the instant from.
    pub clock: Box<dyn Fn() -> Timestamp + Send + Sync>,
}

/// Why a comment is refused before the review.
#[derive(Debug)]
pub(crate) enum CommentRefusal {
    NoAccount,
    /// The account's card is not verified (ADR-0001).
    CardRequired,
    /// The catalogue does not show the item.
    UnknownItem,
    /// Empty, longer than [`COMMENT_LENGTH`], or with control characters.
    BadComment,
    /// The account has written [`COMMENTS_PER_DAY`] comments today.
    QuotaReached {
        next_comment_at: Timestamp,
    },
    Unavailable,
}

impl From<sqlx::Error> for CommentRefusal {
    fn from(_: sqlx::Error) -> Self {
        CommentRefusal::Unavailable
    }
}

/// The outcome of a comment, as its author sees it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CommentOutcome {
    Published,
    /// Held for a moderator, who publishes or refuses it.
    Held,
    Refused,
    /// Removed by a moderator who upheld a report of it.
    Removed,
}

impl CommentOutcome {
    pub(crate) fn code(self) -> &'static str {
        match self {
            CommentOutcome::Published => "published",
            CommentOutcome::Held => "held",
            CommentOutcome::Refused => "refused",
            CommentOutcome::Removed => "removed",
        }
    }

    fn from_code(code: &str) -> Self {
        match code {
            "published" => CommentOutcome::Published,
            "held" | "deciding" => CommentOutcome::Held,
            "removed" => CommentOutcome::Removed,
            _ => CommentOutcome::Refused,
        }
    }
}

/// A comment, as its author sees it.
#[derive(Debug, Clone)]
pub(crate) struct OwnComment {
    pub comment: String,
    pub item: String,
    pub text: String,
    pub outcome: CommentOutcome,
    pub reasons: Vec<RefusalReason>,
    pub commented_at: Timestamp,
}

impl OwnComment {
    pub(crate) fn json(&self) -> Value {
        json!({
            "comment": self.comment,
            "item": self.item,
            "text": self.text,
            "outcome": self.outcome.code(),
            "reasons": self.reasons,
            "commented_at": crate::contract::time(self.commented_at),
        })
    }
}

/// The library account a session comments from.
struct Commenter {
    id: i64,
}

/// The item's latest version that the catalogue shows: its number, title
/// and language.
async fn shown_item(
    pool: &PgPool,
    item: &str,
) -> Result<Option<(i32, String, String)>, sqlx::Error> {
    sqlx::query_as(
        "SELECT v.version, v.title, v.metadata ->> 'language' FROM item_versions AS v
         WHERE v.item = $1
           AND NOT EXISTS (SELECT 1 FROM withdrawals AS w WHERE w.item = v.item AND w.version IN (0, v.version))
           AND NOT EXISTS (SELECT 1 FROM hidden_items AS h WHERE h.item = v.item)
         ORDER BY v.version DESC LIMIT 1",
    )
    .bind(item)
    .fetch_optional(pool)
    .await
}

/// 16 random bytes in lowercase hexadecimal: a comment's or a report's identifier.
pub(crate) fn random_id() -> String {
    let mut bytes = [0u8; 16];
    getrandom::fill(&mut bytes).expect("the system's randomness");
    hex::encode(bytes)
}

const DAY: i64 = 86_400;

fn this_day(now: Timestamp) -> i64 {
    now.as_second().div_euclid(DAY) * DAY
}

fn next_day(now: Timestamp) -> Timestamp {
    Timestamp::from_second(this_day(now) + DAY).expect("a day within range")
}

/// The comment's text once checked: trimmed, 1 to [`COMMENT_LENGTH`]
/// characters, no control characters but line breaks.
fn checked(text: &str) -> Result<String, CommentRefusal> {
    let text = text.trim();
    let length = text.chars().count();
    if !(1..=COMMENT_LENGTH).contains(&length) || text.chars().any(|c| c.is_control() && c != '\n')
    {
        return Err(CommentRefusal::BadComment);
    }
    Ok(text.to_string())
}

/// Comments `text` on `item` from the session's library account at `now`;
/// answers the comment once reviewed.
pub(crate) async fn comment(
    pool: &PgPool,
    desk: &Desk,
    session: &Session,
    item: &str,
    text: &str,
    now: Timestamp,
) -> Result<OwnComment, CommentRefusal> {
    let row: Option<(i64, String)> =
        sqlx::query_as("SELECT id, pseudonym FROM library_accounts WHERE subject = $1")
            .bind(&session.subject)
            .fetch_optional(pool)
            .await?;
    let (id, pseudonym) = row.ok_or(CommentRefusal::NoAccount)?;
    let commenter = Commenter { id };
    let (carded,): (bool,) =
        sqlx::query_as("SELECT EXISTS (SELECT 1 FROM identification_cards WHERE account = $1)")
            .bind(commenter.id)
            .fetch_one(pool)
            .await?;
    if !carded {
        return Err(CommentRefusal::CardRequired);
    }
    let (version, title, language) = shown_item(pool, item)
        .await?
        .ok_or(CommentRefusal::UnknownItem)?;
    let text = checked(text)?;
    let comment = reserve(pool, &commenter, item, &text, now).await?;
    let content = json!({"comment": text});
    let outcome = desk
        .provider
        .review(Submission {
            item,
            version: version as u32,
            title: &title,
            language: &language,
            content: &content,
            focus: None,
        })
        .await;
    let Ok(outcome) = outcome else {
        // Not reviewed: it did not happen, and does not count.
        sqlx::query("DELETE FROM comments WHERE id = $1")
            .bind(&comment)
            .execute(pool)
            .await?;
        return Err(CommentRefusal::Unavailable);
    };
    let (outcome, reasons) = match outcome {
        ReviewOutcome::Published => {
            sqlx::query(
                "UPDATE comments SET outcome = 'published', pseudonym = $2, published_at = $3, decided_at = $3
                 WHERE id = $1",
            )
            .bind(&comment)
            .bind(&pseudonym)
            .bind(now.as_second())
            .execute(pool)
            .await?;
            (CommentOutcome::Published, vec![])
        }
        ReviewOutcome::Held { findings } => {
            sqlx::query("UPDATE comments SET outcome = 'held', findings = $2 WHERE id = $1")
                .bind(&comment)
                .bind(&findings)
                .execute(pool)
                .await?;
            (CommentOutcome::Held, vec![])
        }
        ReviewOutcome::Refused { reasons } => {
            sqlx::query(
                "UPDATE comments SET outcome = 'refused', reasons = $2, decided_at = $3 WHERE id = $1",
            )
            .bind(&comment)
            .bind(serde_json::to_value(&reasons).expect("reasons as JSON"))
            .bind(now.as_second())
            .execute(pool)
            .await?;
            (CommentOutcome::Refused, reasons)
        }
    };
    Ok(OwnComment {
        comment,
        item: item.to_string(),
        text,
        outcome,
        reasons,
        commented_at: now,
    })
}

/// Records a comment being reviewed, counted in the day's quota, unless the
/// quota is reached; answers its identifier.
async fn reserve(
    pool: &PgPool,
    commenter: &Commenter,
    item: &str,
    text: &str,
    now: Timestamp,
) -> Result<String, CommentRefusal> {
    let mut transaction = pool.begin().await?;
    // One comment of an account at a time, so that the quota holds.
    sqlx::query("SELECT id FROM library_accounts WHERE id = $1 FOR UPDATE")
        .bind(commenter.id)
        .execute(&mut *transaction)
        .await?;
    let (count,): (i64,) =
        sqlx::query_as("SELECT count(*) FROM comments WHERE account = $1 AND commented_at >= $2")
            .bind(commenter.id)
            .bind(this_day(now))
            .fetch_one(&mut *transaction)
            .await?;
    if count >= i64::from(COMMENTS_PER_DAY) {
        return Err(CommentRefusal::QuotaReached {
            next_comment_at: next_day(now),
        });
    }
    let comment = random_id();
    sqlx::query(
        "INSERT INTO comments (id, item, account, text, outcome, commented_at)
         VALUES ($1, $2, $3, $4, 'reviewing', $5)",
    )
    .bind(&comment)
    .bind(item)
    .bind(commenter.id)
    .bind(text)
    .bind(now.as_second())
    .execute(&mut *transaction)
    .await?;
    transaction.commit().await?;
    Ok(comment)
}

/// The comments `item`'s page shows, oldest first: published, not removed,
/// not hidden by a report waiting for a moderator. Each carries only the
/// pseudonym, never anything that would let one account reach another.
pub(crate) async fn shown(pool: &PgPool, item: &str) -> Result<Value, sqlx::Error> {
    let rows: Vec<(String, String, String, i64)> = sqlx::query_as(
        "SELECT id, pseudonym, text, published_at FROM comments
         WHERE item = $1 AND outcome = 'published' AND NOT hidden
         ORDER BY published_at, id",
    )
    .bind(item)
    .fetch_all(pool)
    .await?;
    Ok(rows
        .into_iter()
        .map(|(comment, pseudonym, text, published_at)| {
            json!({
                "comment": comment,
                "pseudonym": pseudonym,
                "text": text,
                "published_at": crate::contract::time(
                    Timestamp::from_second(published_at).expect("a stored instant")
                ),
            })
        })
        .collect())
}

/// The session's account's comments, newest first, and how many it wrote today.
pub(crate) async fn own(
    pool: &PgPool,
    session: &Session,
    now: Timestamp,
) -> Result<(Vec<OwnComment>, u32), CommentRefusal> {
    let row: Option<(i64,)> = sqlx::query_as("SELECT id FROM library_accounts WHERE subject = $1")
        .bind(&session.subject)
        .fetch_optional(pool)
        .await?;
    let (account,) = row.ok_or(CommentRefusal::NoAccount)?;
    type Row = (String, String, String, String, Value, i64);
    let rows: Vec<Row> = sqlx::query_as(
        "SELECT id, item, text, outcome, reasons, commented_at FROM comments
         WHERE account = $1 AND outcome <> 'reviewing'
         ORDER BY commented_at DESC, id",
    )
    .bind(account)
    .fetch_all(pool)
    .await?;
    let comments = rows
        .into_iter()
        .map(
            |(comment, item, text, outcome, reasons, commented_at)| OwnComment {
                comment,
                item,
                text,
                outcome: CommentOutcome::from_code(&outcome),
                reasons: serde_json::from_value(reasons).unwrap_or_default(),
                commented_at: Timestamp::from_second(commented_at).expect("a stored instant"),
            },
        )
        .collect();
    let (today,): (i64,) =
        sqlx::query_as("SELECT count(*) FROM comments WHERE account = $1 AND commented_at >= $2")
            .bind(account)
            .bind(this_day(now))
            .fetch_one(pool)
            .await?;
    Ok((comments, today as u32))
}

/// The instant the next comment is possible, when the quota is reached at `now`.
pub(crate) fn next_comment_at(today: u32, now: Timestamp) -> Option<Timestamp> {
    (today >= COMMENTS_PER_DAY).then(|| next_day(now))
}
