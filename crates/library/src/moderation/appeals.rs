//! Appeals (DSA internal complaint handling): the author or the reporter of
//! a decided report asks, within six months, that another moderator look
//! again. A reversal undoes what the decision did: an item withdrawn comes
//! back, an item kept is withdrawn.

use jiff::{SignedDuration, Timestamp};
use sqlx::{PgPool, Postgres, Transaction};

use super::{Logged, instant, log, moderator, reason};
use crate::contract::SigningKey;
use crate::reports::{self, DecideError, Decision, Message, Outcome, is_contact};
use crate::withdrawals::{self, Reason, Withdrawal};

/// How long after a decision its author or reporter may appeal.
pub const APPEAL_PERIOD: SignedDuration = SignedDuration::from_hours(183 * 24);

/// The longest statement an appellant may write, in characters.
pub const STATEMENT_LIMIT: usize = 2000;

/// Who appeals a decision.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Appellant {
    /// The author of the reported item.
    Author,
    /// The reporter, who alone knows the report's identifier.
    Reporter,
}

impl Appellant {
    pub(crate) fn code(self) -> &'static str {
        match self {
            Appellant::Author => "author",
            Appellant::Reporter => "reporter",
        }
    }

    pub(crate) fn from_code(code: &str) -> Appellant {
        if code == "author" {
            Appellant::Author
        } else {
            Appellant::Reporter
        }
    }
}

/// An appeal against a moderator's decision on a report.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Appeal {
    pub appellant: Appellant,
    /// Why the decision should change.
    pub statement: String,
    /// A website reporter's address for the appeal's decision.
    pub contact: Option<String>,
}

/// What another moderator decides on an appeal.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AppealOutcome {
    /// The decision stands.
    Maintained,
    /// The decision is reversed: an upheld report becomes rejected, and the
    /// other way round.
    Reversed,
}

impl AppealOutcome {
    pub(crate) fn code(self) -> &'static str {
        match self {
            AppealOutcome::Maintained => "maintained",
            AppealOutcome::Reversed => "reversed",
        }
    }

    pub(crate) fn from_code(code: &str) -> AppealOutcome {
        if code == "reversed" {
            AppealOutcome::Reversed
        } else {
            AppealOutcome::Maintained
        }
    }
}

/// An appeal's decision and its reasons, sent to the appellant and the author.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppealDecision {
    pub outcome: AppealOutcome,
    pub reasons: String,
}

/// Why an appeal could not be received.
#[derive(Debug)]
pub enum AppealError {
    UnknownReport,
    /// The report waits for a decision, or was closed by the author's correction.
    NotDecided,
    /// More than six months after the decision.
    TooLate,
    /// This party appealed this report already.
    AlreadyAppealed,
    /// An empty statement.
    StatementRequired,
    /// A statement longer than [`STATEMENT_LIMIT`].
    StatementTooLong,
    /// The contact is not an e-mail address, or is given by the author.
    BadContact,
    Unavailable,
    Database(sqlx::Error),
}

impl From<sqlx::Error> for AppealError {
    fn from(error: sqlx::Error) -> Self {
        match &error {
            sqlx::Error::Database(database)
                if database.constraint() == Some("appeals_report_appellant_key") =>
            {
                AppealError::AlreadyAppealed
            }
            _ => AppealError::Database(error),
        }
    }
}

/// Where an appeal stands.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AppealStatus {
    Waiting,
    Maintained,
    Reversed,
}

impl AppealStatus {
    pub(crate) fn code(self) -> &'static str {
        match self {
            AppealStatus::Waiting => "waiting",
            AppealStatus::Maintained => "maintained",
            AppealStatus::Reversed => "reversed",
        }
    }

    pub(crate) fn from_code(code: &str) -> AppealStatus {
        match code {
            "maintained" => AppealStatus::Maintained,
            "reversed" => AppealStatus::Reversed,
            _ => AppealStatus::Waiting,
        }
    }
}

/// An appeal as its appellant may follow it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppealState {
    pub appeal: String,
    pub report: String,
    pub status: AppealStatus,
    pub received_at: Timestamp,
    pub decision: Option<(AppealDecision, Timestamp)>,
}

/// An appeal waiting for a moderator, with the decision it is against.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QueuedAppeal {
    pub appeal: String,
    pub report: String,
    pub item: String,
    pub reason: Reason,
    pub appellant: Appellant,
    pub statement: String,
    pub decision: Decision,
    /// The handle of the moderator who decided.
    pub moderator: String,
    pub decided_at: Timestamp,
    pub received_at: Timestamp,
}

/// An appeal's identifier: 128 random bits, like a report's.
fn new_id() -> String {
    let mut bits = [0u8; 16];
    getrandom::fill(&mut bits).expect("the system's random source");
    hex::encode(bits)
}

fn outcome(code: &str) -> Outcome {
    if code == "upheld" {
        Outcome::Upheld
    } else {
        Outcome::Rejected
    }
}

/// Receives an appeal of `report` at `now`; answers its identifier.
pub(crate) async fn receive(
    pool: &PgPool,
    report: &str,
    appeal: Appeal,
    now: Timestamp,
) -> Result<String, AppealError> {
    let statement = appeal.statement.trim();
    if statement.is_empty() {
        return Err(AppealError::StatementRequired);
    }
    if statement.chars().count() > STATEMENT_LIMIT {
        return Err(AppealError::StatementTooLong);
    }
    if let Some(contact) = &appeal.contact
        && (appeal.appellant == Appellant::Author || !is_contact(contact))
    {
        return Err(AppealError::BadContact);
    }
    let row: Option<(String, Option<i64>)> = sqlx::query_as(
        "SELECT status,
                (SELECT min(decided_at) FROM moderation_decisions
                 WHERE case_kind = 'report' AND case_id = reports.id AND appeal IS NULL)
         FROM reports WHERE id = $1",
    )
    .bind(report)
    .fetch_optional(pool)
    .await?;
    let (status, decided_at) = row.ok_or(AppealError::UnknownReport)?;
    let decided_at = match (status.as_str(), decided_at) {
        ("upheld" | "rejected", Some(decided_at)) => instant(decided_at),
        _ => return Err(AppealError::NotDecided),
    };
    if now
        > decided_at
            .checked_add(APPEAL_PERIOD)
            .expect("a period's end")
    {
        return Err(AppealError::TooLate);
    }
    let id = new_id();
    sqlx::query(
        "INSERT INTO appeals (id, report, appellant, statement, contact, status, received_at)
         VALUES ($1, $2, $3, $4, $5, 'waiting', $6)",
    )
    .bind(&id)
    .bind(report)
    .bind(appeal.appellant.code())
    .bind(statement)
    .bind(&appeal.contact)
    .bind(now.as_second())
    .execute(pool)
    .await?;
    Ok(id)
}

/// The last moderator's decision on `report` that is not an appeal's: the
/// moderator's handle.
async fn decided_by(
    tx: &mut Transaction<'_, Postgres>,
    report: &str,
) -> Result<Option<String>, sqlx::Error> {
    let row: Option<(String,)> = sqlx::query_as(
        "SELECT moderator FROM moderation_decisions
         WHERE case_kind = 'report' AND case_id = $1 AND appeal IS NULL
         ORDER BY id DESC LIMIT 1",
    )
    .bind(report)
    .fetch_optional(&mut **tx)
    .await?;
    Ok(row.map(|(moderator,)| moderator))
}

/// A decision on a report to reverse.
struct Reversal<'a> {
    report: &'a str,
    item: &'a str,
    reason: Reason,
    /// What the decision was.
    was: Outcome,
    /// The appeal decision's reasons, which become the report's.
    reasons: &'a str,
}

/// Reverses a decision on a report at `now`.
async fn reverse(
    tx: &mut Transaction<'_, Postgres>,
    signing: &SigningKey,
    reversal: Reversal<'_>,
    now: Timestamp,
) -> Result<(), sqlx::Error> {
    let Reversal {
        report,
        item,
        reason,
        was,
        reasons,
    } = reversal;
    let now_status = match was {
        Outcome::Upheld => Outcome::Rejected,
        Outcome::Rejected => Outcome::Upheld,
    };
    sqlx::query(
        "UPDATE reports SET status = $2, decided_at = $3, decision_reasons = $4 WHERE id = $1",
    )
    .bind(report)
    .bind(now_status.code())
    .bind(now.as_second())
    .bind(reasons)
    .execute(&mut **tx)
    .await?;
    match was {
        Outcome::Upheld => {
            // The item comes back, unless another upheld report keeps it withdrawn.
            sqlx::query(
                "DELETE FROM withdrawals
                 WHERE item = $1 AND version = 0 AND kind = 'moderation'
                   AND NOT EXISTS (SELECT 1 FROM reports WHERE item = $1 AND status = 'upheld')",
            )
            .bind(item)
            .execute(&mut **tx)
            .await?;
            reports::put_back(tx, item, reason).await?;
        }
        Outcome::Rejected => {
            withdrawals::insert(tx, item, None, Withdrawal::Moderation(reason), now).await?;
        }
    }
    reports::sign_both(tx, signing, now).await
}

/// Records the decision of the moderator `subject` on an appeal at `now`,
/// acts on it, logs it, and tells the appellant and the author.
pub(crate) async fn decide(
    pool: &PgPool,
    signing: &SigningKey,
    desk: &reports::Desk,
    subject: &str,
    appeal: &str,
    decision: AppealDecision,
    now: Timestamp,
) -> Result<(), DecideError> {
    let handle = moderator(pool, subject).await?;
    let reasons = decision.reasons.trim().to_string();
    if reasons.is_empty() {
        return Err(DecideError::ReasonsRequired);
    }
    let mut tx = pool.begin().await?;
    reports::lock(&mut tx).await?;
    type Row = (String, String, Option<String>, String, String, String);
    let row: Option<Row> = sqlx::query_as(
        "SELECT a.report, a.status, a.contact, r.item, r.reason, r.status
         FROM appeals AS a JOIN reports AS r ON r.id = a.report
         WHERE a.id = $1 FOR UPDATE OF a",
    )
    .bind(appeal)
    .fetch_optional(&mut *tx)
    .await?;
    let (report, status, contact, item, code, report_status) =
        row.ok_or(DecideError::UnknownAppeal)?;
    if AppealStatus::from_code(&status) != AppealStatus::Waiting {
        return Err(DecideError::AlreadyDecided);
    }
    if decided_by(&mut tx, &report).await?.as_deref() == Some(handle.as_str()) {
        return Err(DecideError::SameModerator);
    }
    let reason = reason(&code);
    if decision.outcome == AppealOutcome::Reversed {
        let reversal = Reversal {
            report: &report,
            item: &item,
            reason,
            was: outcome(&report_status),
            reasons: &reasons,
        };
        reverse(&mut tx, signing, reversal, now).await?;
    }
    sqlx::query("UPDATE appeals SET status = $2, contact = NULL WHERE id = $1")
        .bind(appeal)
        .bind(decision.outcome.code())
        .execute(&mut *tx)
        .await?;
    log(
        &mut tx,
        Logged {
            case_kind: "report",
            case_id: &report,
            appeal: Some(appeal),
            item: &item,
            reason,
            outcome: decision.outcome.code(),
            moderator: &handle,
            reasons: &reasons,
            at: now,
        },
    )
    .await?;
    sqlx::query(
        "INSERT INTO author_notifications (item, report, kind, created_at, appeal)
         VALUES ($1, $2, 'appeal', $3, $4)",
    )
    .bind(&item)
    .bind(&report)
    .bind(now.as_second())
    .bind(appeal)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;

    if let Some(contact) = contact {
        let message = Message::AppealDecision {
            appeal: appeal.to_string(),
            report,
            item,
            reason,
            decision: AppealDecision {
                outcome: decision.outcome,
                reasons,
            },
        };
        // The decision stands even if the message cannot leave.
        let _ = desk.notifier.send(&contact, &message).await;
    }
    Ok(())
}

/// An appeal as its appellant may follow it, if it exists.
pub(crate) async fn state(pool: &PgPool, appeal: &str) -> Result<Option<AppealState>, sqlx::Error> {
    type Row = (
        String,
        String,
        i64,
        Option<String>,
        Option<String>,
        Option<i64>,
    );
    let row: Option<Row> = sqlx::query_as(
        "SELECT a.report, a.status, a.received_at, d.outcome, d.reasons, d.decided_at
         FROM appeals AS a LEFT JOIN moderation_decisions AS d ON d.appeal = a.id
         WHERE a.id = $1",
    )
    .bind(appeal)
    .fetch_optional(pool)
    .await?;
    Ok(row.map(
        |(report, status, received_at, outcome, reasons, decided_at)| AppealState {
            appeal: appeal.to_string(),
            report,
            status: AppealStatus::from_code(&status),
            received_at: instant(received_at),
            decision: outcome.zip(reasons).zip(decided_at).map(
                |((outcome, reasons), decided_at)| {
                    (
                        AppealDecision {
                            outcome: AppealOutcome::from_code(&outcome),
                            reasons,
                        },
                        instant(decided_at),
                    )
                },
            ),
        },
    ))
}

/// The appeals waiting for a moderator, oldest first.
pub(crate) async fn queue(pool: &PgPool) -> Result<Vec<QueuedAppeal>, sqlx::Error> {
    type Row = (
        String,
        String,
        String,
        String,
        String,
        String,
        i64,
        String,
        String,
        String,
        i64,
    );
    let rows: Vec<Row> = sqlx::query_as(
        "SELECT a.id, a.report, r.item, r.reason, a.appellant, a.statement, a.received_at,
                d.outcome, d.reasons, d.moderator, d.decided_at
         FROM appeals AS a
         JOIN reports AS r ON r.id = a.report
         JOIN LATERAL (
             SELECT outcome, reasons, moderator, decided_at FROM moderation_decisions
             WHERE case_kind = 'report' AND case_id = a.report AND appeal IS NULL
             ORDER BY id DESC LIMIT 1
         ) AS d ON TRUE
         WHERE a.status = 'waiting'
         ORDER BY a.received_at, a.id",
    )
    .fetch_all(pool)
    .await?;
    Ok(rows
        .into_iter()
        .map(
            |(
                appeal,
                report,
                item,
                code,
                appellant,
                statement,
                received_at,
                decided,
                reasons,
                moderator,
                decided_at,
            )| QueuedAppeal {
                appeal,
                report,
                item,
                reason: reason(&code),
                appellant: Appellant::from_code(&appellant),
                statement,
                decision: Decision {
                    outcome: outcome(&decided),
                    reasons,
                },
                moderator,
                decided_at: instant(decided_at),
                received_at: instant(received_at),
            },
        )
        .collect())
}
