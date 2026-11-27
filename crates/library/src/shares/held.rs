//! Shares the review held for a moderator (ADR-0005): they wait in the one
//! moderation queue until a moderator publishes or refuses them, and the
//! decisions log keeps the decision.

use jiff::Timestamp;
use serde_json::Value;
use sqlx::PgPool;

use super::{Kind, Prepared};
use crate::moderation::{Logged, log, moderator};
use crate::review::RefusalReason;
use crate::{DecideError, Library, Reason};

/// A share the review held, waiting for a moderator.
#[derive(Debug, Clone, PartialEq)]
pub struct HeldShare {
    pub share: String,
    pub item: String,
    pub title: String,
    /// The author's pseudonym now, under which the share would be published.
    pub pseudonym: String,
    /// What the review found.
    pub findings: String,
    /// `bank` or `path`.
    pub kind: &'static str,
    /// The prepared item's metadata, as the parent app sent it.
    pub metadata: Value,
    /// The bank or learning path, for the moderator to read.
    pub content: Value,
    pub received_at: Timestamp,
}

/// The shares waiting for a moderator, oldest first.
pub(crate) async fn queue(pool: &PgPool) -> Result<Vec<HeldShare>, sqlx::Error> {
    type Row = (String, String, String, String, Option<String>, Value, i64);
    let rows: Vec<Row> = sqlx::query_as(
        "SELECT s.id, s.item, s.title, a.pseudonym, s.findings, s.submission, s.shared_at
         FROM shares AS s JOIN library_accounts AS a ON a.id = s.account
         WHERE s.outcome = 'held'
         ORDER BY s.shared_at, s.id",
    )
    .fetch_all(pool)
    .await?;
    Ok(rows
        .into_iter()
        .filter_map(
            |(share, item, title, pseudonym, findings, submission, shared_at)| {
                let prepared: Prepared = serde_json::from_value(submission).ok()?;
                Some(HeldShare {
                    share,
                    item,
                    title,
                    pseudonym,
                    findings: findings.unwrap_or_default(),
                    kind: match prepared.kind {
                        Kind::Bank => "bank",
                        Kind::Path => "path",
                    },
                    metadata: serde_json::to_value(&prepared.metadata).expect("metadata as JSON"),
                    content: prepared.content,
                    received_at: Timestamp::from_second(shared_at).expect("a stored instant"),
                })
            },
        )
        .collect())
}

/// What a moderator decides on a held share.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HeldOutcome {
    /// The findings do not hold: the share is published.
    Published,
    /// The findings hold: the share is refused, with the moderator's reasons.
    Refused,
}

/// A moderator's decision on a held share: the rule the findings were
/// about, and the reasons, which the author reads when it is refused.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HeldDecision {
    pub outcome: HeldOutcome,
    pub reason: Reason,
    pub reasons: String,
}

/// Why a decision on a held share could not be made.
#[derive(Debug)]
pub enum HeldDecisionError {
    NotAModerator,
    UnknownShare,
    AlreadyDecided,
    ReasonsRequired,
    Unavailable,
}

impl From<sqlx::Error> for HeldDecisionError {
    fn from(_: sqlx::Error) -> Self {
        HeldDecisionError::Unavailable
    }
}

/// Records at `now` the decision of the moderator `subject` on the held
/// share `share`: publishes it under its author's pseudonym, or refuses it
/// with the moderator's reasons; the decisions log keeps it.
pub(crate) async fn decide(
    library: &Library,
    subject: &str,
    share: &str,
    decision: HeldDecision,
    now: Timestamp,
) -> Result<(), HeldDecisionError> {
    let pool = &library.pool;
    let handle = moderator(pool, subject)
        .await
        .map_err(|error| match error {
            DecideError::NotAModerator => HeldDecisionError::NotAModerator,
            _ => HeldDecisionError::Unavailable,
        })?;
    let reasons = decision.reasons.trim();
    if reasons.is_empty() {
        return Err(HeldDecisionError::ReasonsRequired);
    }
    // Taken out of the queue at once, so that two moderators never both decide it.
    let claimed: Option<(String, Value, String)> = sqlx::query_as(
        "UPDATE shares AS s SET outcome = 'deciding'
         FROM library_accounts AS a
         WHERE s.id = $1 AND s.outcome = 'held' AND a.id = s.account
         RETURNING s.item, s.submission, a.pseudonym",
    )
    .bind(share)
    .fetch_optional(pool)
    .await?;
    let Some((item, submission, pseudonym)) = claimed else {
        let (known,): (bool,) =
            sqlx::query_as("SELECT EXISTS (SELECT 1 FROM shares WHERE id = $1)")
                .bind(share)
                .fetch_one(pool)
                .await?;
        return Err(if known {
            HeldDecisionError::AlreadyDecided
        } else {
            HeldDecisionError::UnknownShare
        });
    };
    let published = match decision.outcome {
        HeldOutcome::Published => {
            match publish(library, &item, submission, &pseudonym, now).await {
                Some(version) => Some(version),
                None => {
                    // Back in the queue, to be decided again.
                    sqlx::query("UPDATE shares SET outcome = 'held' WHERE id = $1")
                        .bind(share)
                        .execute(pool)
                        .await?;
                    return Err(HeldDecisionError::Unavailable);
                }
            }
        }
        HeldOutcome::Refused => None,
    };
    let mut transaction = pool.begin().await?;
    match published {
        Some(version) => {
            sqlx::query(
                "UPDATE shares SET outcome = 'published', version = $2, submission = NULL, decided_at = $3
                 WHERE id = $1",
            )
            .bind(share)
            .bind(version as i32)
            .bind(now.as_second())
            .execute(&mut *transaction)
            .await?;
        }
        None => {
            let refusal = vec![RefusalReason {
                question: None,
                reason: decision.reason,
                explanation: reasons.to_string(),
            }];
            sqlx::query(
                "UPDATE shares SET outcome = 'refused', item = NULL, reasons = $2, submission = NULL, decided_at = $3
                 WHERE id = $1",
            )
            .bind(share)
            .bind(serde_json::to_value(&refusal).expect("reasons as JSON"))
            .bind(now.as_second())
            .execute(&mut *transaction)
            .await?;
            // Never published: no author answers for it.
            sqlx::query("DELETE FROM authored_items WHERE item = $1")
                .bind(&item)
                .execute(&mut *transaction)
                .await?;
        }
    }
    log(
        &mut transaction,
        Logged {
            case_kind: "held_review",
            case_id: share,
            appeal: None,
            item: &item,
            reason: decision.reason,
            // As the transparency figures count decisions: findings upheld
            // restrict the content, findings rejected let it through.
            outcome: match decision.outcome {
                HeldOutcome::Published => "rejected",
                HeldOutcome::Refused => "upheld",
            },
            moderator: &handle,
            reasons,
            at: now,
        },
    )
    .await?;
    transaction.commit().await?;
    Ok(())
}

/// Publishes the held prepared item as `item` under `pseudonym`; answers its version.
async fn publish(
    library: &Library,
    item: &str,
    submission: Value,
    pseudonym: &str,
    now: Timestamp,
) -> Option<u32> {
    let prepared: Prepared = serde_json::from_value(submission).ok()?;
    let version = prepared.version(item, pseudonym).ok()?;
    library.publish_version(version, now).await.ok()
}
