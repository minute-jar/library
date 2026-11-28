//! Comments in moderation: comments the review held wait in the one queue
//! until a moderator publishes or refuses them; reports of published
//! comments wait there too, and a moderator who upholds one removes the
//! comment. The decisions log keeps every decision.

use jiff::Timestamp;
use sqlx::PgPool;

use super::random_id;
use crate::moderation::{Logged, log, moderator};
use crate::reports::{DETAILS_LIMIT, DecideError, Decision, Outcome};
use crate::review::RefusalReason;
use crate::shares::held::{HeldDecision, HeldOutcome};
use crate::withdrawals::Reason;

/// Reasons for which a reported comment is hidden at once, until a
/// moderator decides: a child's personal data, sexual or illegal content.
const URGENT: [Reason; 3] = [
    Reason::PersonalData,
    Reason::SexualContent,
    Reason::IllegalContent,
];

/// A comment the review held, waiting for a moderator.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HeldComment {
    pub comment: String,
    pub item: String,
    /// The pseudonym it would be published under.
    pub pseudonym: String,
    pub text: String,
    /// What the review found.
    pub findings: String,
    pub received_at: Timestamp,
}

/// A report of a published comment, waiting for a moderator.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QueuedCommentReport {
    pub report: String,
    pub comment: String,
    pub item: String,
    pub pseudonym: String,
    pub text: String,
    pub reason: Reason,
    pub details: Option<String>,
    /// Whether the comment is hidden until the decision.
    pub hidden: bool,
    pub received_at: Timestamp,
}

fn instant(second: i64) -> Timestamp {
    Timestamp::from_second(second).expect("a stored instant")
}

/// The held comments, oldest first.
pub(crate) async fn held(pool: &PgPool) -> Result<Vec<HeldComment>, sqlx::Error> {
    let rows: Vec<(String, String, String, String, Option<String>, i64)> = sqlx::query_as(
        "SELECT c.id, c.item, a.pseudonym, c.text, c.findings, c.commented_at
         FROM comments AS c JOIN library_accounts AS a ON a.id = c.account
         WHERE c.outcome = 'held'
         ORDER BY c.commented_at, c.id",
    )
    .fetch_all(pool)
    .await?;
    Ok(rows
        .into_iter()
        .map(
            |(comment, item, pseudonym, text, findings, at)| HeldComment {
                comment,
                item,
                pseudonym,
                text,
                findings: findings.unwrap_or_default(),
                received_at: instant(at),
            },
        )
        .collect())
}

/// The reports of comments waiting for a moderator, oldest first.
pub(crate) async fn reported(pool: &PgPool) -> Result<Vec<QueuedCommentReport>, sqlx::Error> {
    type Row = (
        String,
        String,
        String,
        String,
        String,
        String,
        Option<String>,
        bool,
        i64,
    );
    let rows: Vec<Row> = sqlx::query_as(
        "SELECT r.id, c.id, c.item, c.pseudonym, c.text, r.reason, r.details, c.hidden, r.received_at
         FROM comment_reports AS r JOIN comments AS c ON c.id = r.comment
         WHERE r.status = 'with_moderator'
         ORDER BY r.received_at, r.id",
    )
    .fetch_all(pool)
    .await?;
    Ok(rows
        .into_iter()
        .map(
            |(report, comment, item, pseudonym, text, reason, details, hidden, at)| {
                QueuedCommentReport {
                    report,
                    comment,
                    item,
                    pseudonym,
                    text,
                    reason: crate::moderation::reason(&reason),
                    details,
                    hidden,
                    received_at: instant(at),
                }
            },
        )
        .collect())
}

/// Why a decision on a held comment could not be made.
#[derive(Debug)]
pub enum CommentDecisionError {
    NotAModerator,
    UnknownComment,
    AlreadyDecided,
    ReasonsRequired,
    Unavailable,
}

impl From<sqlx::Error> for CommentDecisionError {
    fn from(_: sqlx::Error) -> Self {
        CommentDecisionError::Unavailable
    }
}

/// Records at `now` the decision of the moderator `subject` on the held
/// comment `comment`: publishes it under its author's pseudonym now, or
/// refuses it with the moderator's reasons; the decisions log keeps it.
pub(crate) async fn decide_held(
    pool: &PgPool,
    subject: &str,
    comment: &str,
    decision: HeldDecision,
    now: Timestamp,
) -> Result<(), CommentDecisionError> {
    let handle = moderator(pool, subject)
        .await
        .map_err(|error| match error {
            DecideError::NotAModerator => CommentDecisionError::NotAModerator,
            _ => CommentDecisionError::Unavailable,
        })?;
    let reasons = decision.reasons.trim();
    if reasons.is_empty() {
        return Err(CommentDecisionError::ReasonsRequired);
    }
    let mut transaction = pool.begin().await?;
    let claimed: Option<(String,)> =
        sqlx::query_as("SELECT item FROM comments WHERE id = $1 AND outcome = 'held' FOR UPDATE")
            .bind(comment)
            .fetch_optional(&mut *transaction)
            .await?;
    let Some((item,)) = claimed else {
        let (known,): (bool,) =
            sqlx::query_as("SELECT EXISTS (SELECT 1 FROM comments WHERE id = $1)")
                .bind(comment)
                .fetch_one(&mut *transaction)
                .await?;
        return Err(if known {
            CommentDecisionError::AlreadyDecided
        } else {
            CommentDecisionError::UnknownComment
        });
    };
    match decision.outcome {
        HeldOutcome::Published => {
            sqlx::query(
                "UPDATE comments AS c SET outcome = 'published', pseudonym = a.pseudonym,
                     published_at = $2, decided_at = $2
                 FROM library_accounts AS a
                 WHERE c.id = $1 AND a.id = c.account",
            )
            .bind(comment)
            .bind(now.as_second())
            .execute(&mut *transaction)
            .await?;
        }
        HeldOutcome::Refused => {
            let refusal = vec![RefusalReason {
                question: None,
                reason: decision.reason,
                explanation: reasons.to_string(),
            }];
            sqlx::query(
                "UPDATE comments SET outcome = 'refused', reasons = $2, decided_at = $3 WHERE id = $1",
            )
            .bind(comment)
            .bind(serde_json::to_value(&refusal).expect("reasons as JSON"))
            .bind(now.as_second())
            .execute(&mut *transaction)
            .await?;
        }
    }
    log(
        &mut transaction,
        Logged {
            case_kind: "held_comment",
            case_id: comment,
            appeal: None,
            item: &item,
            reason: decision.reason,
            // As the transparency figures count decisions, like held shares.
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

/// A report of a published comment, from the website, anonymous.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommentReport {
    pub reason: Reason,
    pub details: Option<String>,
}

/// Why a report of a comment could not be received.
#[derive(Debug)]
pub enum CommentReportError {
    /// No published comment has this identifier.
    UnknownComment,
    /// Details longer than [`DETAILS_LIMIT`].
    DetailsTooLong,
    Database(sqlx::Error),
}

impl From<sqlx::Error> for CommentReportError {
    fn from(error: sqlx::Error) -> Self {
        CommentReportError::Database(error)
    }
}

/// Receives at `now` a report of the published comment `comment`; a child's
/// personal data, sexual or illegal content hides it at once until a
/// moderator decides. Answers the report's identifier.
pub(crate) async fn receive(
    pool: &PgPool,
    comment: &str,
    report: CommentReport,
    now: Timestamp,
) -> Result<String, CommentReportError> {
    let details = report
        .details
        .map(|details| details.trim().to_string())
        .filter(|details| !details.is_empty());
    if details
        .as_ref()
        .is_some_and(|details| details.chars().count() > DETAILS_LIMIT)
    {
        return Err(CommentReportError::DetailsTooLong);
    }
    let mut transaction = pool.begin().await?;
    let published: Option<(String,)> = sqlx::query_as(
        "SELECT id FROM comments WHERE id = $1 AND outcome = 'published' FOR UPDATE",
    )
    .bind(comment)
    .fetch_optional(&mut *transaction)
    .await?;
    if published.is_none() {
        return Err(CommentReportError::UnknownComment);
    }
    let id = random_id();
    sqlx::query(
        "INSERT INTO comment_reports (id, comment, reason, details, status, received_at)
         VALUES ($1, $2, $3, $4, 'with_moderator', $5)",
    )
    .bind(&id)
    .bind(comment)
    .bind(report.reason.code())
    .bind(&details)
    .bind(now.as_second())
    .execute(&mut *transaction)
    .await?;
    if URGENT.contains(&report.reason) {
        sqlx::query("UPDATE comments SET hidden = TRUE WHERE id = $1")
            .bind(comment)
            .execute(&mut *transaction)
            .await?;
    }
    transaction.commit().await?;
    Ok(id)
}

/// Records at `now` the decision of the moderator `subject` on the report
/// `report` of a comment: upheld removes the comment, rejected shows it
/// again once no other urgent report of it waits. The decisions log keeps it.
pub(crate) async fn decide_report(
    pool: &PgPool,
    subject: &str,
    report: &str,
    decision: Decision,
    now: Timestamp,
) -> Result<(), DecideError> {
    let handle = moderator(pool, subject).await?;
    let reasons = decision.reasons.trim();
    if reasons.is_empty() {
        return Err(DecideError::ReasonsRequired);
    }
    let mut transaction = pool.begin().await?;
    let claimed: Option<(String, String, String)> = sqlx::query_as(
        "SELECT r.comment, r.reason, c.item
         FROM comment_reports AS r JOIN comments AS c ON c.id = r.comment
         WHERE r.id = $1 AND r.status = 'with_moderator'
         FOR UPDATE OF r, c",
    )
    .bind(report)
    .fetch_optional(&mut *transaction)
    .await?;
    let Some((comment, reason, item)) = claimed else {
        let (known,): (bool,) =
            sqlx::query_as("SELECT EXISTS (SELECT 1 FROM comment_reports WHERE id = $1)")
                .bind(report)
                .fetch_one(&mut *transaction)
                .await?;
        return Err(if known {
            DecideError::AlreadyDecided
        } else {
            DecideError::UnknownReport
        });
    };
    sqlx::query("UPDATE comment_reports SET status = $2, decided_at = $3 WHERE id = $1")
        .bind(report)
        .bind(decision.outcome.code())
        .bind(now.as_second())
        .execute(&mut *transaction)
        .await?;
    match decision.outcome {
        Outcome::Upheld => {
            sqlx::query("UPDATE comments SET outcome = 'removed', decided_at = $2 WHERE id = $1")
                .bind(&comment)
                .bind(now.as_second())
                .execute(&mut *transaction)
                .await?;
        }
        Outcome::Rejected => {
            let urgent: Vec<&str> = URGENT.iter().map(|reason| reason.code()).collect();
            sqlx::query(
                "UPDATE comments SET hidden = FALSE WHERE id = $1 AND NOT EXISTS (
                     SELECT 1 FROM comment_reports
                     WHERE comment = $1 AND status = 'with_moderator' AND reason = ANY($2))",
            )
            .bind(&comment)
            .bind(&urgent)
            .execute(&mut *transaction)
            .await?;
        }
    }
    log(
        &mut transaction,
        Logged {
            case_kind: "comment_report",
            case_id: report,
            appeal: None,
            item: &item,
            reason: crate::moderation::reason(&reason),
            outcome: decision.outcome.code(),
            moderator: &handle,
            reasons,
            at: now,
        },
    )
    .await?;
    transaction.commit().await?;
    Ok(())
}
