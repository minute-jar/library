//! Reports of shared items and their three paths (`docs/specs/library-service.md`,
//! Reports and moderation):
//!
//! - a child's personal data, sexual or illegal content: reviewed again at
//!   once on that reason, switched off when plausible, otherwise hidden from
//!   new imports, until a moderator decides; a rejection puts the item back;
//! - rights: hidden from new imports after three distinct families that
//!   imported the item, or at once on a rights holder's formal notice;
//! - a wrong answer or unsuitable content: sent to the author, then to a
//!   moderator if the item has no new version after 7 days.
//!
//! Every other reason goes to a moderator. A report keeps no library
//! identifier: the identifiers that reported an item are kept apart, with
//! nothing but the item and the reason, only to count distinct families.

use std::future::Future;
use std::io;
use std::pin::Pin;

use jiff::{SignedDuration, Timestamp};
use serde_json::Value;
use sqlx::{PgPool, Postgres, Transaction};

use crate::catalogue::{is_item_id, sign_index};
use crate::contract::SigningKey;
use crate::imports::is_library_identifier;
use crate::moderation::{AppealDecision, AppealOutcome};
use crate::review::{ReviewOutcome, Reviewer, Submission};
use crate::withdrawals::{self, Reason, Withdrawal};

/// The longest details a reporter may write, in characters.
pub const DETAILS_LIMIT: usize = 2000;

/// How long the author has to answer a wrong answer before a moderator does.
const AUTHOR_DELAY: SignedDuration = SignedDuration::from_hours(7 * 24);

/// Distinct families that imported an item and reported it for its rights
/// before it is hidden from new imports.
const RIGHTS_FAMILIES: i64 = 3;

/// Distinct families that imported an item and reported a child's personal
/// data, sexual or illegal content before it is switched off without review.
const URGENT_FAMILIES: i64 = 2;

const URGENT: [Reason; 3] = [
    Reason::PersonalData,
    Reason::SexualContent,
    Reason::IllegalContent,
];

/// Who sends a report.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Reporter {
    /// A family server, under its library identifier.
    Family { library_identifier: String },
    /// Someone on the website, anonymous, who may leave an e-mail address
    /// for the acknowledgement and the decision.
    Website {
        contact: Option<String>,
        /// A rights holder's formal notice; it needs `contact`.
        notice: Option<FormalNotice>,
    },
}

/// A rights holder's formal notice, with their identity.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FormalNotice {
    /// The rights holder's name, or their company's.
    pub name: String,
    /// The protected work the item copies.
    pub work: String,
}

/// A report of a shared item.
#[derive(Debug, Clone)]
pub struct NewReport {
    pub item: String,
    pub reason: Reason,
    pub details: Option<String>,
    pub reporter: Reporter,
}

/// Why a report could not be received.
#[derive(Debug)]
pub enum ReportError {
    /// The library identifier is not 32 lowercase hexadecimal digits.
    BadIdentifier,
    /// The contact is not an e-mail address.
    BadContact,
    /// A formal notice without a name, a work or a contact, or for another
    /// reason than rights.
    BadNotice,
    /// Details longer than [`DETAILS_LIMIT`].
    DetailsTooLong,
    UnknownItem,
    /// The service was opened without a reviewer and a notifier.
    Unavailable,
    Database(sqlx::Error),
}

impl From<sqlx::Error> for ReportError {
    fn from(error: sqlx::Error) -> Self {
        ReportError::Database(error)
    }
}

/// Where a report stands.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReportStatus {
    /// Sent to the author, who has 7 days to publish a new version.
    WithAuthor,
    /// Waiting for a moderator's decision.
    WithModerator,
    /// The author published a new version within 7 days.
    Corrected,
    Upheld,
    Rejected,
}

impl ReportStatus {
    pub(crate) fn code(self) -> &'static str {
        match self {
            ReportStatus::WithAuthor => "with_author",
            ReportStatus::WithModerator => "with_moderator",
            ReportStatus::Corrected => "corrected",
            ReportStatus::Upheld => "upheld",
            ReportStatus::Rejected => "rejected",
        }
    }

    fn from_code(code: &str) -> ReportStatus {
        match code {
            "with_author" => ReportStatus::WithAuthor,
            "with_moderator" => ReportStatus::WithModerator,
            "corrected" => ReportStatus::Corrected,
            "upheld" => ReportStatus::Upheld,
            _ => ReportStatus::Rejected,
        }
    }
}

/// What a report did to the item at once.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Effect {
    None,
    SwitchedOff,
    /// Hidden from new imports: out of the index, still served.
    Hidden,
}

impl Effect {
    pub(crate) fn code(self) -> &'static str {
        match self {
            Effect::None => "none",
            Effect::SwitchedOff => "switched_off",
            Effect::Hidden => "hidden",
        }
    }

    pub(crate) fn from_code(code: &str) -> Effect {
        match code {
            "switched_off" => Effect::SwitchedOff,
            "hidden" => Effect::Hidden,
            _ => Effect::None,
        }
    }
}

/// A moderator's decision on a report.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    /// The report holds: the item is withdrawn by moderation for its reason.
    Upheld,
    /// The report does not hold: a switch-off or hiding it caused ends.
    Rejected,
}

impl Outcome {
    pub(crate) fn code(self) -> &'static str {
        match self {
            Outcome::Upheld => "upheld",
            Outcome::Rejected => "rejected",
        }
    }
}

/// A decision and its reasons, sent to the reporter and the author.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Decision {
    pub outcome: Outcome,
    pub reasons: String,
}

/// Why a decision could not be made.
#[derive(Debug)]
pub enum DecideError {
    UnknownReport,
    /// The report was decided already, or closed by the author's correction.
    AlreadyDecided,
    /// The reasons are empty.
    ReasonsRequired,
    /// The Minute Jar account deciding is not a moderator.
    NotAModerator,
    UnknownAppeal,
    /// An appeal must be decided by another moderator than the decision.
    SameModerator,
    Unavailable,
    Database(sqlx::Error),
}

impl From<sqlx::Error> for DecideError {
    fn from(error: sqlx::Error) -> Self {
        DecideError::Database(error)
    }
}

/// A report as its reporter may follow it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReportState {
    pub report: String,
    pub item: String,
    pub reason: Reason,
    pub status: ReportStatus,
    pub received_at: Timestamp,
    /// The decision's reasons and instant, once decided.
    pub decision: Option<(Decision, Timestamp)>,
}

/// A report waiting for a moderator.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QueuedReport {
    pub report: String,
    pub item: String,
    /// The item's latest version when the report arrived.
    pub version: u32,
    pub reason: Reason,
    pub details: Option<String>,
    pub effect: Effect,
    /// What the review found, or why the item was switched off without it.
    pub findings: Option<String>,
    pub notice: Option<FormalNotice>,
    pub received_at: Timestamp,
}

/// Something the author of an item must be told, kept until their library
/// account receives it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthorNotification {
    pub report: String,
    pub reason: Reason,
    pub details: Option<String>,
    /// `None` for a report to answer, the decision otherwise.
    pub decision: Option<Decision>,
    /// An appeal's decision, from the author or the reporter.
    pub appeal: Option<AppealDecision>,
    pub at: Timestamp,
}

/// A message to a reporter who left an address.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Message {
    Acknowledgement {
        report: String,
        item: String,
        reason: Reason,
    },
    Decision {
        report: String,
        item: String,
        reason: Reason,
        decision: Decision,
    },
    /// The author published a new version within 7 days.
    Corrected {
        report: String,
        item: String,
        reason: Reason,
    },
    /// Another moderator decided the reporter's appeal.
    AppealDecision {
        appeal: String,
        report: String,
        item: String,
        reason: Reason,
        decision: AppealDecision,
    },
}

/// A future a [`Notifier`] answers.
pub type NotifyFuture<'a> = Pin<Box<dyn Future<Output = io::Result<()>> + Send + 'a>>;

/// Sends messages to reporters' e-mail addresses.
pub trait Notifier: Send + Sync + 'static {
    fn send<'a>(&'a self, to: &'a str, message: &'a Message) -> NotifyFuture<'a>;
}

/// What the service needs to receive reports: the reviewer, the notifier,
/// and the clock the HTTP routes read the instant from.
pub struct Desk {
    pub reviewer: Box<dyn Reviewer>,
    pub notifier: Box<dyn Notifier>,
    pub clock: Box<dyn Fn() -> Timestamp + Send + Sync>,
}

pub(crate) fn is_contact(contact: &str) -> bool {
    let mut parts = contact.split('@');
    let (Some(local), Some(domain), None) = (parts.next(), parts.next(), parts.next()) else {
        return false;
    };
    contact.len() <= 254
        && !local.is_empty()
        && domain.contains('.')
        && !domain.starts_with('.')
        && !domain.ends_with('.')
        && !contact.chars().any(|c| c.is_whitespace() || c.is_control())
}

fn check(report: &NewReport) -> Result<(), ReportError> {
    if report
        .details
        .as_ref()
        .is_some_and(|details| details.chars().count() > DETAILS_LIMIT)
    {
        return Err(ReportError::DetailsTooLong);
    }
    match &report.reporter {
        Reporter::Family { library_identifier } => {
            if !is_library_identifier(library_identifier) {
                return Err(ReportError::BadIdentifier);
            }
        }
        Reporter::Website { contact, notice } => {
            if contact
                .as_deref()
                .is_some_and(|contact| !is_contact(contact))
            {
                return Err(ReportError::BadContact);
            }
            if let Some(notice) = notice
                && (report.reason != Reason::Rights
                    || contact.is_none()
                    || notice.name.trim().is_empty()
                    || notice.work.trim().is_empty())
            {
                return Err(ReportError::BadNotice);
            }
        }
    }
    Ok(())
}

/// A report's identifier: 128 random bits from the system's cryptographic
/// source in lowercase hexadecimal, which only its reporter knows.
fn new_id() -> String {
    let mut bits = [0u8; 16];
    getrandom::fill(&mut bits).expect("the system's random source");
    hex::encode(bits)
}

fn codes(reasons: &[Reason]) -> Vec<String> {
    reasons
        .iter()
        .map(|reason| reason.code().to_string())
        .collect()
}

fn is_urgent(reason: Reason) -> bool {
    URGENT.contains(&reason)
}

/// The distinct families that imported `item` and reported it for one of
/// `reasons`, counting `identifier` too.
async fn families(
    pool: &PgPool,
    item: &str,
    reasons: &[Reason],
    identifier: Option<&str>,
) -> Result<i64, sqlx::Error> {
    let (count,): (i64,) = sqlx::query_as(
        "SELECT count(*) FROM (
             SELECT library_identifier FROM report_identifiers WHERE item = $1 AND reason = ANY($2)
             UNION SELECT $3::text WHERE $3::text IS NOT NULL
         ) AS reporters
         WHERE EXISTS (SELECT 1 FROM imports WHERE imports.item = $1 AND imports.library_identifier = reporters.library_identifier)",
    )
    .bind(item)
    .bind(codes(reasons))
    .bind(identifier)
    .fetch_one(pool)
    .await?;
    Ok(count)
}

/// Whether every version of the item is withdrawn or switched off already.
async fn withdrawn_whole(
    executor: impl sqlx::PgExecutor<'_>,
    item: &str,
) -> Result<bool, sqlx::Error> {
    let (withdrawn,): (bool,) =
        sqlx::query_as("SELECT EXISTS (SELECT 1 FROM withdrawals WHERE item = $1 AND version = 0)")
            .bind(item)
            .fetch_one(executor)
            .await?;
    Ok(withdrawn)
}

pub(crate) async fn lock(tx: &mut Transaction<'_, Postgres>) -> Result<(), sqlx::Error> {
    // One change of the catalogue at a time, so that documents follow it in order.
    sqlx::query("SELECT pg_advisory_xact_lock(1)")
        .execute(&mut **tx)
        .await?;
    Ok(())
}

pub(crate) async fn sign_both(
    tx: &mut Transaction<'_, Postgres>,
    signing: &SigningKey,
    now: Timestamp,
) -> Result<(), sqlx::Error> {
    sign_index(tx, signing, now).await?;
    withdrawals::sign(tx, signing, now).await
}

/// What the review of an urgent report leads to.
async fn urgent_effect(
    pool: &PgPool,
    desk: &Desk,
    report: &NewReport,
    identifier: Option<&str>,
) -> Result<(Effect, Option<String>), ReportError> {
    if withdrawn_whole(pool, &report.item).await? {
        return Ok((Effect::None, None));
    }
    if families(pool, &report.item, &URGENT, identifier).await? >= URGENT_FAMILIES {
        return Ok((
            Effect::SwitchedOff,
            Some("reported by several families that imported it".into()),
        ));
    }
    type Row = (i32, String, Value, Value);
    let (version, title, metadata, content): Row = sqlx::query_as(
        "SELECT version, title, metadata, content FROM item_versions WHERE item = $1 ORDER BY version DESC LIMIT 1",
    )
    .bind(&report.item)
    .fetch_one(pool)
    .await?;
    let submission = Submission {
        item: &report.item,
        version: version as u32,
        title: &title,
        language: metadata["language"].as_str().unwrap_or_default(),
        content: &content,
        focus: Some(report.reason),
    };
    Ok(match desk.reviewer.review(submission).await {
        Ok(ReviewOutcome::Published) => (Effect::Hidden, None),
        Ok(ReviewOutcome::Held { findings }) => (Effect::SwitchedOff, Some(findings)),
        Ok(ReviewOutcome::Refused { reasons }) => (Effect::SwitchedOff, Some(reasons)),
        // A child may be at stake: the item is switched off until a moderator decides.
        Err(_) => (
            Effect::SwitchedOff,
            Some("the review could not be made".into()),
        ),
    })
}

/// Receives a report at `now`; answers its identifier.
pub async fn receive(
    pool: &PgPool,
    signing: &SigningKey,
    desk: &Desk,
    report: NewReport,
    now: Timestamp,
) -> Result<String, ReportError> {
    check(&report)?;
    let (latest,): (Option<i32>,) = if is_item_id(&report.item) {
        sqlx::query_as("SELECT max(version) FROM item_versions WHERE item = $1")
            .bind(&report.item)
            .fetch_one(pool)
            .await?
    } else {
        (None,)
    };
    let latest = latest.ok_or(ReportError::UnknownItem)?;
    let identifier = match &report.reporter {
        Reporter::Family { library_identifier } => Some(library_identifier.as_str()),
        Reporter::Website { .. } => None,
    };
    let (contact, notice) = match &report.reporter {
        Reporter::Website { contact, notice } => (contact.clone(), notice.clone()),
        Reporter::Family { .. } => (None, None),
    };

    let (mut effect, findings) = if is_urgent(report.reason) {
        urgent_effect(pool, desk, &report, identifier).await?
    } else if report.reason == Reason::Rights
        && (notice.is_some()
            || families(pool, &report.item, &[Reason::Rights], identifier).await?
                >= RIGHTS_FAMILIES)
    {
        (Effect::Hidden, None)
    } else {
        (Effect::None, None)
    };
    let status = match report.reason {
        Reason::WrongAnswer | Reason::UnsuitableContent => ReportStatus::WithAuthor,
        _ => ReportStatus::WithModerator,
    };

    let mut tx = pool.begin().await?;
    lock(&mut tx).await?;
    if effect == Effect::SwitchedOff && withdrawn_whole(&mut *tx, &report.item).await? {
        effect = Effect::None;
    }
    if let Some(identifier) = identifier {
        sqlx::query(
            "INSERT INTO report_identifiers (item, reason, library_identifier) VALUES ($1, $2, $3) ON CONFLICT DO NOTHING",
        )
        .bind(&report.item)
        .bind(report.reason.code())
        .bind(identifier)
        .execute(&mut *tx)
        .await?;
    }
    let id = new_id();
    sqlx::query(
        "INSERT INTO reports (id, item, version, reason, details, contact, notice_name, notice_work, status, effect, findings, received_at)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12)",
    )
    .bind(&id)
    .bind(&report.item)
    .bind(latest)
    .bind(report.reason.code())
    .bind(&report.details)
    .bind(&contact)
    .bind(notice.as_ref().map(|notice| &notice.name))
    .bind(notice.as_ref().map(|notice| &notice.work))
    .bind(status.code())
    .bind(effect.code())
    .bind(&findings)
    .bind(now.as_second())
    .execute(&mut *tx)
    .await?;
    if status == ReportStatus::WithAuthor {
        notify_author(&mut tx, &report.item, &id, now).await?;
    }
    match effect {
        Effect::None => {}
        Effect::SwitchedOff => {
            withdrawals::insert(
                &mut tx,
                &report.item,
                None,
                Withdrawal::SwitchOff(report.reason),
                now,
            )
            .await?;
            sign_both(&mut tx, signing, now).await?;
        }
        Effect::Hidden => {
            let cause = if is_urgent(report.reason) {
                "urgent"
            } else {
                "rights"
            };
            sqlx::query(
                "INSERT INTO hidden_items (item, cause) VALUES ($1, $2) ON CONFLICT DO NOTHING",
            )
            .bind(&report.item)
            .bind(cause)
            .execute(&mut *tx)
            .await?;
            sign_both(&mut tx, signing, now).await?;
        }
    }
    tx.commit().await?;

    if let Some(contact) = contact {
        let message = Message::Acknowledgement {
            report: id.clone(),
            item: report.item.clone(),
            reason: report.reason,
        };
        // The report stands even if the acknowledgement cannot leave.
        let _ = desk.notifier.send(&contact, &message).await;
    }
    Ok(id)
}

async fn notify_author(
    tx: &mut Transaction<'_, Postgres>,
    item: &str,
    report: &str,
    now: Timestamp,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO author_notifications (item, report, kind, created_at) VALUES ($1, $2, 'report', $3)",
    )
    .bind(item)
    .bind(report)
    .bind(now.as_second())
    .execute(&mut **tx)
    .await?;
    Ok(())
}

/// A decision recorded, not yet committed: what it was on, and the
/// message to send the reporter once committed.
pub(crate) struct Decided {
    pub item: String,
    pub reason: Reason,
    pub message: Option<(String, Message)>,
}

/// Records a moderator's decision on a report at `now` within a change of
/// the catalogue already open and locked, and acts on it.
pub(crate) async fn decide_in(
    tx: &mut Transaction<'_, Postgres>,
    signing: &SigningKey,
    report: &str,
    decision: Decision,
    now: Timestamp,
) -> Result<Decided, DecideError> {
    let row: Option<(String, String, String, Option<String>)> =
        sqlx::query_as("SELECT item, reason, status, contact FROM reports WHERE id = $1")
            .bind(report)
            .fetch_optional(&mut **tx)
            .await?;
    let (item, reason, status, contact) = row.ok_or(DecideError::UnknownReport)?;
    let reason = Reason::from_code(&reason).expect("a stored reason");
    if !matches!(
        ReportStatus::from_code(&status),
        ReportStatus::WithAuthor | ReportStatus::WithModerator
    ) {
        return Err(DecideError::AlreadyDecided);
    }
    sqlx::query(
        "UPDATE reports SET status = $2, decided_at = $3, decision_reasons = $4, contact = NULL WHERE id = $1",
    )
    .bind(report)
    .bind(decision.outcome.code())
    .bind(now.as_second())
    .bind(&decision.reasons)
    .execute(&mut **tx)
    .await?;
    sqlx::query(
        "INSERT INTO author_notifications (item, report, kind, created_at) VALUES ($1, $2, 'decision', $3)",
    )
    .bind(&item)
    .bind(report)
    .bind(now.as_second())
    .execute(&mut **tx)
    .await?;

    let changed = match decision.outcome {
        Outcome::Upheld => {
            withdrawals::insert(tx, &item, None, Withdrawal::Moderation(reason), now).await?;
            true
        }
        Outcome::Rejected => put_back(tx, &item, reason).await?,
    };
    if changed {
        sign_both(tx, signing, now).await?;
    }
    let message = contact.map(|contact| {
        (
            contact,
            Message::Decision {
                report: report.to_string(),
                item: item.clone(),
                reason,
                decision,
            },
        )
    });
    Ok(Decided {
        item,
        reason,
        message,
    })
}

/// After a rejected report, ends what reports for its reason did to the
/// item once none of them waits for a moderator: the switch-off and the
/// hiding, and the count of families that reported it. Answers whether the
/// catalogue changed.
pub(crate) async fn put_back(
    tx: &mut Transaction<'_, Postgres>,
    item: &str,
    reason: Reason,
) -> Result<bool, sqlx::Error> {
    let (reasons, cause): (Vec<Reason>, &str) = if is_urgent(reason) {
        (URGENT.to_vec(), "urgent")
    } else if reason == Reason::Rights {
        (vec![Reason::Rights], "rights")
    } else {
        return Ok(false);
    };
    let (waiting,): (bool,) = sqlx::query_as(
        "SELECT EXISTS (SELECT 1 FROM reports WHERE item = $1 AND reason = ANY($2) AND status = 'with_moderator')",
    )
    .bind(item)
    .bind(codes(&reasons))
    .fetch_one(&mut **tx)
    .await?;
    if waiting {
        return Ok(false);
    }
    let switched_on = if is_urgent(reason) {
        withdrawals::end_switch_off(tx, item, None).await?
    } else {
        false
    };
    let shown = sqlx::query("DELETE FROM hidden_items WHERE item = $1 AND cause = $2")
        .bind(item)
        .bind(cause)
        .execute(&mut **tx)
        .await?
        .rows_affected()
        > 0;
    sqlx::query("DELETE FROM report_identifiers WHERE item = $1 AND reason = ANY($2)")
        .bind(item)
        .bind(codes(&reasons))
        .execute(&mut **tx)
        .await?;
    Ok(switched_on || shown)
}

/// Sends to a moderator, at `now`, the reports the author left 7 days
/// without a new version, and closes those the author corrected.
pub async fn escalate(pool: &PgPool, desk: &Desk, now: Timestamp) -> Result<(), sqlx::Error> {
    let due = now
        .checked_sub(AUTHOR_DELAY)
        .expect("an instant within range");
    let mut tx = pool.begin().await?;
    let rows: Vec<(String, String, String, Option<String>, bool)> = sqlx::query_as(
        "SELECT id, item, reason, contact,
                (SELECT max(version) FROM item_versions AS v WHERE v.item = r.item) > r.version
         FROM reports AS r
         WHERE status = 'with_author' AND received_at <= $1
         FOR UPDATE",
    )
    .bind(due.as_second())
    .fetch_all(&mut *tx)
    .await?;
    let mut messages = Vec::new();
    for (id, item, reason, contact, corrected) in rows {
        if corrected {
            sqlx::query(
                "UPDATE reports SET status = 'corrected', decided_at = $2, contact = NULL WHERE id = $1",
            )
            .bind(&id)
            .bind(now.as_second())
            .execute(&mut *tx)
            .await?;
            if let Some(contact) = contact {
                let reason = Reason::from_code(&reason).expect("a stored reason");
                messages.push((
                    contact,
                    Message::Corrected {
                        report: id,
                        item,
                        reason,
                    },
                ));
            }
        } else {
            sqlx::query("UPDATE reports SET status = 'with_moderator' WHERE id = $1")
                .bind(&id)
                .execute(&mut *tx)
                .await?;
        }
    }
    tx.commit().await?;
    for (contact, message) in messages {
        let _ = desk.notifier.send(&contact, &message).await;
    }
    Ok(())
}

fn instant(second: i64) -> Timestamp {
    Timestamp::from_second(second).expect("a stored instant")
}

fn decision(status: ReportStatus, reasons: Option<String>) -> Option<Decision> {
    let outcome = match status {
        ReportStatus::Upheld => Outcome::Upheld,
        ReportStatus::Rejected => Outcome::Rejected,
        _ => return None,
    };
    Some(Decision {
        outcome,
        reasons: reasons.unwrap_or_default(),
    })
}

/// A report as its reporter may follow it, if it exists.
pub async fn state(pool: &PgPool, report: &str) -> Result<Option<ReportState>, sqlx::Error> {
    type Row = (String, String, String, i64, Option<i64>, Option<String>);
    let row: Option<Row> = sqlx::query_as(
        "SELECT item, reason, status, received_at, decided_at, decision_reasons FROM reports WHERE id = $1",
    )
    .bind(report)
    .fetch_optional(pool)
    .await?;
    Ok(
        row.map(|(item, reason, status, received_at, decided_at, reasons)| {
            let status = ReportStatus::from_code(&status);
            ReportState {
                report: report.to_string(),
                item,
                reason: Reason::from_code(&reason).expect("a stored reason"),
                status,
                received_at: instant(received_at),
                decision: decision(status, reasons).zip(decided_at.map(instant)),
            }
        }),
    )
}

/// The reports waiting for a moderator, oldest first.
pub async fn queue(pool: &PgPool) -> Result<Vec<QueuedReport>, sqlx::Error> {
    type Row = (
        String,
        String,
        i32,
        String,
        Option<String>,
        String,
        Option<String>,
        Option<String>,
        Option<String>,
        i64,
    );
    let rows: Vec<Row> = sqlx::query_as(
        "SELECT id, item, version, reason, details, effect, findings, notice_name, notice_work, received_at
         FROM reports WHERE status = 'with_moderator' ORDER BY received_at, id",
    )
    .fetch_all(pool)
    .await?;
    Ok(rows
        .into_iter()
        .map(
            |(
                report,
                item,
                version,
                reason,
                details,
                effect,
                findings,
                name,
                work,
                received_at,
            )| {
                QueuedReport {
                    report,
                    item,
                    version: version as u32,
                    reason: Reason::from_code(&reason).expect("a stored reason"),
                    details,
                    effect: Effect::from_code(&effect),
                    findings,
                    notice: name
                        .zip(work)
                        .map(|(name, work)| FormalNotice { name, work }),
                    received_at: instant(received_at),
                }
            },
        )
        .collect())
}

/// What the author of `item` must be told, oldest first.
pub async fn author_notifications(
    pool: &PgPool,
    item: &str,
) -> Result<Vec<AuthorNotification>, sqlx::Error> {
    type Row = (
        String,
        String,
        String,
        Option<String>,
        String,
        Option<String>,
        i64,
        Option<String>,
        Option<String>,
    );
    let rows: Vec<Row> = sqlx::query_as(
        "SELECT n.report, n.kind, r.reason, r.details, r.status, r.decision_reasons, n.created_at,
                d.outcome, d.reasons
         FROM author_notifications AS n JOIN reports AS r ON r.id = n.report
         LEFT JOIN moderation_decisions AS d ON d.appeal = n.appeal
         WHERE n.item = $1 ORDER BY n.id",
    )
    .bind(item)
    .fetch_all(pool)
    .await?;
    Ok(rows
        .into_iter()
        .map(
            |(report, kind, reason, details, status, reasons, at, appealed, appeal_reasons)| {
                AuthorNotification {
                    report,
                    reason: Reason::from_code(&reason).expect("a stored reason"),
                    details,
                    decision: (kind == "decision")
                        .then(|| decision(ReportStatus::from_code(&status), reasons))
                        .flatten(),
                    appeal: appealed
                        .zip(appeal_reasons)
                        .map(|(outcome, reasons)| AppealDecision {
                            outcome: AppealOutcome::from_code(&outcome),
                            reasons,
                        }),
                    at: instant(at),
                }
            },
        )
        .collect())
}
