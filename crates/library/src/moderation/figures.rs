//! The yearly transparency report's figures (DSA), read from the reports and
//! the decisions log: counts by reason and outcome, and nothing that names
//! an item, a reporter, an author or a moderator.

use jiff::Timestamp;
use sqlx::PgPool;

use super::appeals::{AppealStatus, Appellant};
use super::reason;
use crate::reports::Outcome;
use crate::withdrawals::Reason;

/// What a year of moderation counts, each list ordered by its codes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Figures {
    pub year: i16,
    /// Reports received in the year, by reason.
    pub reports: Vec<(Reason, i64)>,
    /// Moderators' decisions on reports made in the year, by reason and outcome.
    pub decisions: Vec<(Reason, Outcome, i64)>,
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
        "SELECT reason, count(*) FROM reports
         WHERE received_at >= $1 AND received_at < $2 GROUP BY reason ORDER BY reason",
    )
    .bind(from)
    .bind(until)
    .fetch_all(pool)
    .await?;
    let decisions: Vec<(String, String, i64)> = sqlx::query_as(
        "SELECT reason, outcome, count(*) FROM moderation_decisions
         WHERE appeal IS NULL AND decided_at >= $1 AND decided_at < $2
         GROUP BY reason, outcome ORDER BY reason, outcome",
    )
    .bind(from)
    .bind(until)
    .fetch_all(pool)
    .await?;
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
        decisions: decisions
            .into_iter()
            .map(|(code, outcome, count)| {
                let outcome = if outcome == "upheld" {
                    Outcome::Upheld
                } else {
                    Outcome::Rejected
                };
                (reason(&code), outcome, count)
            })
            .collect(),
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
