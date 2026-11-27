//! Moderation (`docs/specs/moderation-api.md`, stories S22 and S26): the
//! moderators the operator names, one queue of what waits for them, their
//! decisions with reasons, appeals decided by another moderator, and the
//! decisions log that gives the yearly transparency report its figures.

mod appeals;
mod figures;
pub(crate) mod http;

pub use figures::Figures;
pub(crate) use figures::of_year as figures;

pub use appeals::{
    APPEAL_PERIOD, Appeal, AppealDecision, AppealError, AppealOutcome, AppealState, AppealStatus,
    Appellant, QueuedAppeal, STATEMENT_LIMIT,
};
pub(crate) use appeals::{decide as decide_appeal, receive as appeal, state as appeal_state};

use jiff::{SignedDuration, Timestamp};
use sqlx::{PgPool, Postgres, Transaction};

use crate::contract::SigningKey;
use crate::reports::{self, DecideError, Decision, Effect, FormalNotice, Outcome, QueuedReport};
use crate::withdrawals::Reason;

/// How soon a moderator should decide what reaches the queue.
pub const DECISION_TARGET: SignedDuration = SignedDuration::from_hours(48);

/// The longest moderator handle, in characters.
pub const HANDLE_LENGTH: usize = 32;

/// Why a moderator could not be named.
#[derive(Debug)]
pub enum ModeratorError {
    /// An empty subject, or a handle not 1 to 32 printable characters.
    BadModerator,
    /// Another moderator has this handle.
    HandleTaken,
    Database(sqlx::Error),
}

impl From<sqlx::Error> for ModeratorError {
    fn from(error: sqlx::Error) -> Self {
        match &error {
            sqlx::Error::Database(database)
                if database.constraint() == Some("moderators_by_handle") =>
            {
                ModeratorError::HandleTaken
            }
            _ => ModeratorError::Database(error),
        }
    }
}

/// Names the Minute Jar account `subject` a moderator, known to other
/// moderators as `handle`; naming it again changes its handle.
pub(crate) async fn grant(
    pool: &PgPool,
    subject: &str,
    handle: &str,
    now: Timestamp,
) -> Result<(), ModeratorError> {
    let handle = handle.trim();
    let length = handle.chars().count();
    if subject.trim().is_empty()
        || !(1..=HANDLE_LENGTH).contains(&length)
        || handle.chars().any(char::is_control)
    {
        return Err(ModeratorError::BadModerator);
    }
    sqlx::query(
        "INSERT INTO moderators (subject, handle, granted_at) VALUES ($1, $2, $3)
         ON CONFLICT (subject) DO UPDATE SET handle = EXCLUDED.handle",
    )
    .bind(subject)
    .bind(handle)
    .bind(now.as_second())
    .execute(pool)
    .await?;
    Ok(())
}

/// Ends the moderator role of `subject`; answers whether it had it.
pub(crate) async fn revoke(pool: &PgPool, subject: &str) -> Result<bool, sqlx::Error> {
    let revoked = sqlx::query("DELETE FROM moderators WHERE subject = $1")
        .bind(subject)
        .execute(pool)
        .await?;
    Ok(revoked.rows_affected() > 0)
}

/// The handle of the moderator `subject`, if it is one.
pub(crate) async fn handle_of(pool: &PgPool, subject: &str) -> Result<Option<String>, sqlx::Error> {
    let row: Option<(String,)> = sqlx::query_as("SELECT handle FROM moderators WHERE subject = $1")
        .bind(subject)
        .fetch_optional(pool)
        .await?;
    Ok(row.map(|(handle,)| handle))
}

/// Something waiting for a moderator in the one queue, oldest first. Held
/// review findings (ticket 13) will be a third kind.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Case {
    Report(QueuedReport),
    Appeal(QueuedAppeal),
}

impl Case {
    /// When it reached the queue.
    pub fn received_at(&self) -> Timestamp {
        match self {
            Case::Report(report) => report.received_at,
            Case::Appeal(appeal) => appeal.received_at,
        }
    }

    /// When a moderator should have decided it.
    pub fn due_at(&self) -> Timestamp {
        self.received_at()
            .checked_add(DECISION_TARGET)
            .expect("a due date within range")
    }
}

/// What waits for a moderator, oldest first.
pub(crate) async fn cases(pool: &PgPool) -> Result<Vec<Case>, sqlx::Error> {
    let mut cases: Vec<Case> = reports::queue(pool)
        .await?
        .into_iter()
        .map(Case::Report)
        .chain(appeals::queue(pool).await?.into_iter().map(Case::Appeal))
        .collect();
    cases.sort_by_key(Case::received_at);
    Ok(cases)
}

/// The title of the latest version of `item`, if it exists.
pub(crate) async fn title(pool: &PgPool, item: &str) -> Result<Option<String>, sqlx::Error> {
    let row: Option<(String,)> = sqlx::query_as(
        "SELECT title FROM item_versions WHERE item = $1 ORDER BY version DESC LIMIT 1",
    )
    .bind(item)
    .fetch_optional(pool)
    .await?;
    Ok(row.map(|(title,)| title))
}

/// A decision as the decisions log keeps it.
pub(crate) struct Logged<'a> {
    pub(crate) case_kind: &'a str,
    pub(crate) case_id: &'a str,
    pub(crate) appeal: Option<&'a str>,
    pub(crate) item: &'a str,
    pub(crate) reason: Reason,
    pub(crate) outcome: &'a str,
    pub(crate) moderator: &'a str,
    pub(crate) reasons: &'a str,
    pub(crate) at: Timestamp,
}

pub(crate) async fn log(
    tx: &mut Transaction<'_, Postgres>,
    logged: Logged<'_>,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO moderation_decisions
             (case_kind, case_id, appeal, item, reason, outcome, moderator, reasons, decided_at)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)",
    )
    .bind(logged.case_kind)
    .bind(logged.case_id)
    .bind(logged.appeal)
    .bind(logged.item)
    .bind(logged.reason.code())
    .bind(logged.outcome)
    .bind(logged.moderator)
    .bind(logged.reasons)
    .bind(logged.at.as_second())
    .execute(&mut **tx)
    .await?;
    Ok(())
}

/// The handle of the moderator `subject`, or the refusal of anyone else.
pub(crate) async fn moderator(pool: &PgPool, subject: &str) -> Result<String, DecideError> {
    handle_of(pool, subject)
        .await?
        .ok_or(DecideError::NotAModerator)
}

/// Records the decision of the moderator `subject` on a report at `now`,
/// acts on it, logs it, and tells the reporter.
pub(crate) async fn decide_report(
    pool: &PgPool,
    signing: &SigningKey,
    desk: &reports::Desk,
    subject: &str,
    report: &str,
    decision: Decision,
    now: Timestamp,
) -> Result<(), DecideError> {
    let handle = moderator(pool, subject).await?;
    if decision.reasons.trim().is_empty() {
        return Err(DecideError::ReasonsRequired);
    }
    let mut tx = pool.begin().await?;
    reports::lock(&mut tx).await?;
    let outcome = decision.outcome.code();
    let reasons = decision.reasons.clone();
    let decided = reports::decide_in(&mut tx, signing, report, decision, now).await?;
    log(
        &mut tx,
        Logged {
            case_kind: "report",
            case_id: report,
            appeal: None,
            item: &decided.item,
            reason: decided.reason,
            outcome,
            moderator: &handle,
            reasons: &reasons,
            at: now,
        },
    )
    .await?;
    tx.commit().await?;
    if let Some((contact, message)) = decided.message {
        // The decision stands even if the message cannot leave.
        let _ = desk.notifier.send(&contact, &message).await;
    }
    Ok(())
}

/// What happened to an item in moderation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Event {
    Reported {
        report: String,
        reason: Reason,
        details: Option<String>,
        effect: Effect,
        findings: Option<String>,
        notice: Option<FormalNotice>,
    },
    /// The author published a new version within 7 days.
    Corrected { report: String },
    Decided {
        report: String,
        reason: Reason,
        outcome: Outcome,
        reasons: String,
        /// The moderator's handle.
        moderator: String,
    },
    Appealed {
        appeal: String,
        report: String,
        appellant: Appellant,
        statement: String,
    },
    AppealDecided {
        appeal: String,
        report: String,
        outcome: AppealOutcome,
        reasons: String,
        moderator: String,
    },
}

/// One moment of an item's history in moderation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HistoryEntry {
    pub at: Timestamp,
    pub event: Event,
}

/// An item's history in moderation, oldest first.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct History {
    pub item: String,
    pub title: String,
    pub entries: Vec<HistoryEntry>,
}

pub(crate) fn instant(second: i64) -> Timestamp {
    Timestamp::from_second(second).expect("a stored instant")
}

pub(crate) fn reason(code: &str) -> Reason {
    Reason::from_code(code).expect("a stored reason")
}

/// The history of `item` in moderation, if it exists.
pub(crate) async fn history(pool: &PgPool, item: &str) -> Result<Option<History>, sqlx::Error> {
    let Some(title) = title(pool, item).await? else {
        return Ok(None);
    };
    let mut entries = Vec::new();

    type Received = (
        String,
        String,
        Option<String>,
        String,
        Option<String>,
        Option<String>,
        Option<String>,
        i64,
        String,
        Option<i64>,
    );
    let received: Vec<Received> = sqlx::query_as(
        "SELECT id, reason, details, effect, findings, notice_name, notice_work, received_at, status, decided_at
         FROM reports WHERE item = $1 ORDER BY received_at, id",
    )
    .bind(item)
    .fetch_all(pool)
    .await?;
    let mut corrected = Vec::new();
    for (report, code, details, effect, findings, name, work, at, status, decided_at) in received {
        if let ("corrected", Some(decided_at)) = (status.as_str(), decided_at) {
            corrected.push(HistoryEntry {
                at: instant(decided_at),
                event: Event::Corrected {
                    report: report.clone(),
                },
            });
        }
        entries.push(HistoryEntry {
            at: instant(at),
            event: Event::Reported {
                report,
                reason: reason(&code),
                details,
                effect: Effect::from_code(&effect),
                findings,
                notice: name
                    .zip(work)
                    .map(|(name, work)| FormalNotice { name, work }),
            },
        });
    }

    let appealed: Vec<(String, String, String, String, i64)> = sqlx::query_as(
        "SELECT a.id, a.report, a.appellant, a.statement, a.received_at
         FROM appeals AS a JOIN reports AS r ON r.id = a.report
         WHERE r.item = $1 ORDER BY a.received_at, a.id",
    )
    .bind(item)
    .fetch_all(pool)
    .await?;
    for (appeal, report, appellant, statement, at) in appealed {
        entries.push(HistoryEntry {
            at: instant(at),
            event: Event::Appealed {
                appeal,
                report,
                appellant: Appellant::from_code(&appellant),
                statement,
            },
        });
    }

    type Decided = (String, Option<String>, String, String, String, String, i64);
    let decided: Vec<Decided> = sqlx::query_as(
        "SELECT case_id, appeal, reason, outcome, reasons, moderator, decided_at
         FROM moderation_decisions WHERE item = $1 AND case_kind = 'report' ORDER BY id",
    )
    .bind(item)
    .fetch_all(pool)
    .await?;
    for (report, appeal, code, outcome, reasons, moderator, at) in decided {
        if let Some(appeal) = appeal {
            entries.push(HistoryEntry {
                at: instant(at),
                event: Event::AppealDecided {
                    appeal,
                    report,
                    outcome: AppealOutcome::from_code(&outcome),
                    reasons,
                    moderator,
                },
            });
            continue;
        }
        entries.push(HistoryEntry {
            at: instant(at),
            event: Event::Decided {
                report,
                reason: reason(&code),
                outcome: if outcome == "upheld" {
                    Outcome::Upheld
                } else {
                    Outcome::Rejected
                },
                reasons,
                moderator,
            },
        });
    }
    entries.extend(corrected);
    entries.sort_by_key(|entry| entry.at);
    Ok(Some(History {
        item: item.to_string(),
        title,
        entries,
    }))
}
