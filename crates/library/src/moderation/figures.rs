//! The yearly transparency report's figures (DSA), read from the reports and
//! the decisions log: counts by kind of case, reason and outcome, and nothing
//! that names an item, a reporter, an author or a moderator. Decisions on
//! reports and decisions on what the review held are counted apart, each
//! with its own outcomes.

use jiff::Timestamp;
use sqlx::PgPool;

use super::appeals::{AppealStatus, Appellant};
use super::reason;
use crate::reports::Outcome;
use crate::shares::held::HeldOutcome;
use crate::withdrawals::Reason;

/// What a moderator decided on, as the decisions log names it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CaseKind {
    /// A report of a shared item.
    Report,
    /// A report of a published comment.
    CommentReport,
    /// A share the review held.
    HeldReview,
    /// A comment the review held.
    HeldComment,
}

impl CaseKind {
    pub(crate) fn code(self) -> &'static str {
        match self {
            CaseKind::Report => "report",
            CaseKind::CommentReport => "comment_report",
            CaseKind::HeldReview => "held_review",
            CaseKind::HeldComment => "held_comment",
        }
    }

    fn from_code(code: &str) -> Self {
        match code {
            "report" => CaseKind::Report,
            "comment_report" => CaseKind::CommentReport,
            "held_review" => CaseKind::HeldReview,
            "held_comment" => CaseKind::HeldComment,
            _ => panic!("a stored case kind"),
        }
    }
}

/// What a year of moderation counts, each list ordered by its codes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Figures {
    pub year: i16,
    /// Reports received in the year, by reason.
    pub reports: Vec<(Reason, i64)>,
    /// Moderators' decisions on reports of items and comments made in the
    /// year (not their appeals), by kind of case, reason and outcome.
    pub decisions: Vec<(CaseKind, Reason, Outcome, i64)>,
    /// Moderators' decisions on shares and comments the review held, made
    /// in the year, by kind of case, reason and outcome.
    pub reviews: Vec<(CaseKind, Reason, HeldOutcome, i64)>,
    /// Appeals received in the year, by appellant and where they stand.
    pub appeals: Vec<(Appellant, AppealStatus, i64)>,
}

/// The first instant of `year` and of the next, in seconds.
fn bounds(year: i16) -> Option<(i64, i64)> {
    let start = |year: i16| {
        format!("{year:04}-01-01T00:00:00Z")
            .parse::<Timestamp>()
            .ok()
            .map(|at| at.as_second())
    };
    Some((start(year)?, start(year.checked_add(1)?)?))
}

/// The figures of `year`, or `None` for a year out of range.
pub(crate) async fn of_year(pool: &PgPool, year: i16) -> Result<Option<Figures>, sqlx::Error> {
    let Some((from, until)) = bounds(year) else {
        return Ok(None);
    };
    let reports: Vec<(String, i64)> = sqlx::query_as(
        "SELECT reason, count(*) FROM (
             SELECT reason, received_at FROM reports
             UNION ALL SELECT reason, received_at FROM comment_reports
         ) AS received
         WHERE received_at >= $1 AND received_at < $2 GROUP BY reason ORDER BY reason",
    )
    .bind(from)
    .bind(until)
    .fetch_all(pool)
    .await?;
    let decided: Vec<(String, String, String, i64)> = sqlx::query_as(
        "SELECT case_kind, reason, outcome, count(*) FROM moderation_decisions
         WHERE appeal IS NULL AND decided_at >= $1 AND decided_at < $2
         GROUP BY case_kind, reason, outcome ORDER BY case_kind, reason, outcome",
    )
    .bind(from)
    .bind(until)
    .fetch_all(pool)
    .await?;
    let mut decisions = Vec::new();
    let mut reviews = Vec::new();
    for (kind, code, outcome, count) in decided {
        let kind = CaseKind::from_code(&kind);
        // The log keeps `upheld` for what restricts content: an upheld
        // report, or held content refused because its findings hold.
        let upheld = outcome == "upheld";
        match kind {
            CaseKind::Report | CaseKind::CommentReport => {
                let outcome = if upheld {
                    Outcome::Upheld
                } else {
                    Outcome::Rejected
                };
                decisions.push((kind, reason(&code), outcome, count));
            }
            CaseKind::HeldReview | CaseKind::HeldComment => {
                let outcome = if upheld {
                    HeldOutcome::Refused
                } else {
                    HeldOutcome::Published
                };
                reviews.push((kind, reason(&code), outcome, count));
            }
        }
    }
    let appeals: Vec<(String, String, i64)> = sqlx::query_as(
        "SELECT appellant, status, count(*) FROM appeals
         WHERE received_at >= $1 AND received_at < $2
         GROUP BY appellant, status ORDER BY appellant, status",
    )
    .bind(from)
    .bind(until)
    .fetch_all(pool)
    .await?;
    Ok(Some(Figures {
        year,
        reports: reports
            .into_iter()
            .map(|(code, count)| (reason(&code), count))
            .collect(),
        decisions,
        reviews,
        appeals: appeals
            .into_iter()
            .map(|(appellant, status, count)| {
                (
                    Appellant::from_code(&appellant),
                    AppealStatus::from_code(&status),
                    count,
                )
            })
            .collect(),
    }))
}
