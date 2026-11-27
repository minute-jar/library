//! The Minute Jar public library service.
//!
//! [`Library`] is the whole service as callers see it: what is published
//! enters the catalogue, and [`Library::router`] serves the signed documents
//! of the interface with family servers (`docs/specs/interface-v1.md`) and
//! the catalogue the website browses (`docs/specs/catalogue-api.md`).

pub mod contract;

mod accounts;
mod browse;
mod catalogue;
mod http;
mod imports;
mod item_page;
mod media;
mod ratings;
mod reports;
mod review;
mod withdrawals;

use std::sync::Arc;

use jiff::Timestamp;
use serde::Serialize;
use serde_json::Value;
use sqlx::PgPool;

use crate::contract::SigningKey;

pub use crate::accounts::closing::IDENTIFICATION_RETENTION;
pub use crate::accounts::{
    Account, AccountError, Authorization, IdToken, IdentityProvider, MinuteJarAccount,
    ProviderError, ProviderFuture, code_challenge,
};
pub use crate::catalogue::PublishError;
pub use crate::imports::ImportError;
pub use crate::media::{ITEM_MEDIA_LIMIT, LocalDirectory, MEDIUM_LIMIT, MediaStore, StoreFuture};
pub use crate::ratings::{ItemRatings, REVIEW_LENGTH, RatingError, Review};
pub use crate::reports::{
    AuthorNotification, DETAILS_LIMIT, DecideError, Decision, Effect, FormalNotice, Message,
    NewReport, Notifier, NotifyFuture, Outcome, QueuedReport, ReportError, ReportState,
    ReportStatus, Reporter,
};
pub use crate::review::{ReviewFuture, ReviewOutcome, Reviewer, Submission};
pub use crate::withdrawals::{Reason, WithdrawError, Withdrawal};

/// A question bank or a learning path.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ItemType {
    Bank,
    Path,
}

impl ItemType {
    fn name(self) -> &'static str {
        match self {
            ItemType::Bank => "bank",
            ItemType::Path => "path",
        }
    }
}

/// Who answers for a shared item, as others see them.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Author {
    pub pseudonym: String,
    /// The operator's editorial account (ADR-0002).
    pub editorial: bool,
}

/// The shared item a derived work comes from.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Origin {
    pub item: String,
    pub version: u32,
    pub pseudonym: String,
}

/// A curriculum a shared item follows.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Curriculum {
    pub country: String,
    pub year: String,
    pub topic: String,
}

/// What lets families find a shared item.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Metadata {
    /// A BCP 47 language tag.
    pub language: String,
    pub subject: String,
    /// The youngest and oldest ages it is meant for.
    pub ages: (u8, u8),
    pub description: String,
    pub curriculum: Option<Curriculum>,
}

/// One version of a shared item, once reviewed and published.
#[derive(Debug, Clone)]
pub struct NewVersion {
    /// The shared item's identifier: 32 lowercase hexadecimal digits.
    pub item: String,
    pub kind: ItemType,
    pub title: String,
    pub author: Author,
    pub derived_from: Option<Origin>,
    pub metadata: Metadata,
    /// The bank or learning path, in the bank format.
    pub content: Value,
    /// The files of its pictures and sounds, which `content` names by digest.
    pub media: Vec<Vec<u8>>,
}

/// The library service.
#[derive(Clone)]
pub struct Library {
    pool: PgPool,
    signing: Arc<SigningKey>,
    keys_document: Arc<catalogue::Stored>,
    media: Arc<dyn MediaStore>,
    accounts: Option<Arc<accounts::Desk>>,
    reports: Option<Arc<reports::Desk>>,
}

impl Library {
    /// The service over its database, which it brings up to date, signing
    /// with `signing`, which `keys_document` certifies, keeping media in
    /// `media`; the index is signed again at `now`.
    pub async fn open(
        pool: PgPool,
        signing: SigningKey,
        keys_document: String,
        media: impl MediaStore,
        now: Timestamp,
    ) -> Result<Self, sqlx::Error> {
        sqlx::migrate!().run(&pool).await?;
        catalogue::refresh(&pool, &signing, now).await?;
        Ok(Library {
            pool,
            signing: Arc::new(signing),
            keys_document: Arc::new(catalogue::Stored::new(keys_document)),
            media: Arc::new(media),
            accounts: None,
            reports: None,
        })
    }

    /// Turns on sign-in with a Minute Jar account through `provider` and the
    /// library accounts opened on them; their routes read the instant from
    /// `clock`. Without it, they are refused as unavailable.
    pub fn with_accounts(
        mut self,
        provider: impl IdentityProvider,
        clock: impl Fn() -> Timestamp + Send + Sync + 'static,
    ) -> Self {
        self.accounts = Some(Arc::new(accounts::Desk {
            provider: Box::new(provider),
            clock: Box::new(clock),
        }));
        self
    }

    /// Forgets at `now` what sign-in no longer needs: expired sessions,
    /// sign-ins that never came back, and the e-mail of sign-ins no longer
    /// recent. To be called every few minutes.
    pub async fn forget_sign_ins(&self, now: Timestamp) -> Result<(), sqlx::Error> {
        accounts::forget(&self.pool, now).await
    }

    /// Erases at `now` the identification data of closed library accounts
    /// once `kept_for` has passed since their last share
    /// ([`IDENTIFICATION_RETENTION`] by default, the legal period), and at
    /// once for those that never shared. To be called every day or so.
    pub async fn erase_identifications(
        &self,
        now: Timestamp,
        kept_for: jiff::SignedDuration,
    ) -> Result<(), sqlx::Error> {
        accounts::closing::erase_identifications(&self.pool, now, kept_for).await
    }

    /// Publishes the next version of a shared item; answers its version number.
    pub async fn publish_version(
        &self,
        version: NewVersion,
        now: Timestamp,
    ) -> Result<u32, PublishError> {
        catalogue::publish(&self.pool, &self.signing, &*self.media, version, now).await
    }

    /// Records that the family server under `library_identifier` imported
    /// the shared item `item`. The index counts each identifier once per item,
    /// from the next time it is signed.
    pub async fn record_import(
        &self,
        library_identifier: &str,
        item: &str,
    ) -> Result<(), ImportError> {
        imports::record(&self.pool, library_identifier, item).await
    }

    /// Records the rating of `stars`, from 1 to 5, and the short review if
    /// any, that the family server under `library_identifier` gives the
    /// shared item `item`, which it must have imported. It replaces the
    /// identifier's earlier rating of the item; the index counts it from the
    /// next time it is signed, and the review shows once published.
    pub async fn record_rating(
        &self,
        library_identifier: &str,
        item: &str,
        stars: u8,
        review: Option<&str>,
    ) -> Result<(), RatingError> {
        ratings::record(&self.pool, library_identifier, item, stars, review).await
    }

    /// How the shared item `item` is rated now, with its published reviews.
    pub async fn item_ratings(&self, item: &str) -> Result<ItemRatings, RatingError> {
        ratings::of_item(&self.pool, item).await
    }

    /// Withdraws a shared item, or one of its versions when `version` is
    /// given: it enters the withdrawal list and leaves the index.
    pub async fn withdraw(
        &self,
        item: &str,
        version: Option<u32>,
        withdrawal: Withdrawal,
        now: Timestamp,
    ) -> Result<(), WithdrawError> {
        withdrawals::withdraw(&self.pool, &self.signing, item, version, withdrawal, now).await
    }

    /// Ends a switch-off: the item or version leaves the withdrawal list and
    /// comes back to the index.
    pub async fn reactivate(
        &self,
        item: &str,
        version: Option<u32>,
        now: Timestamp,
    ) -> Result<(), WithdrawError> {
        withdrawals::reactivate(&self.pool, &self.signing, item, version, now).await
    }

    /// The service receiving reports: urgent ones are reviewed by `reviewer`,
    /// reporters who left an address hear from `notifier`, and the HTTP
    /// routes read the instant from `clock`. Without it, reports are refused
    /// as unavailable.
    pub fn with_reports(
        mut self,
        reviewer: impl Reviewer,
        notifier: impl Notifier,
        clock: impl Fn() -> Timestamp + Send + Sync + 'static,
    ) -> Self {
        self.reports = Some(Arc::new(reports::Desk {
            reviewer: Box::new(reviewer),
            notifier: Box::new(notifier),
            clock: Box::new(clock),
        }));
        self
    }

    /// Receives a report of a shared item at `now` and follows its path;
    /// answers the report's identifier, by which its reporter follows it.
    pub async fn report(&self, report: NewReport, now: Timestamp) -> Result<String, ReportError> {
        let desk = self.reports.as_ref().ok_or(ReportError::Unavailable)?;
        reports::receive(&self.pool, &self.signing, desk, report, now).await
    }

    /// A report as its reporter may follow it.
    pub async fn report_state(&self, report: &str) -> Result<Option<ReportState>, sqlx::Error> {
        reports::state(&self.pool, report).await
    }

    /// Records a moderator's decision at `now`: an upheld report withdraws
    /// the item by moderation, a rejected one puts it back; the reporter
    /// and the author are told.
    pub async fn decide_report(
        &self,
        report: &str,
        decision: Decision,
        now: Timestamp,
    ) -> Result<(), DecideError> {
        let desk = self.reports.as_ref().ok_or(DecideError::Unavailable)?;
        reports::decide(&self.pool, &self.signing, desk, report, decision, now).await
    }

    /// Sends to a moderator the reports their author left 7 days without a
    /// new version as of `now`, and closes those the author corrected.
    pub async fn escalate_reports(&self, now: Timestamp) -> Result<(), ReportError> {
        let desk = self.reports.as_ref().ok_or(ReportError::Unavailable)?;
        Ok(reports::escalate(&self.pool, desk, now).await?)
    }

    /// The reports waiting for a moderator, oldest first.
    pub async fn moderation_queue(&self) -> Result<Vec<QueuedReport>, sqlx::Error> {
        reports::queue(&self.pool).await
    }

    /// What the author of `item` must be told about reports, oldest first,
    /// kept until their library account receives it.
    pub async fn author_notifications(
        &self,
        item: &str,
    ) -> Result<Vec<AuthorNotification>, sqlx::Error> {
        reports::author_notifications(&self.pool, item).await
    }

    /// Signs the index and the withdrawal list again at `now`, before they expire.
    pub async fn refresh(&self, now: Timestamp) -> Result<(), sqlx::Error> {
        catalogue::refresh(&self.pool, &self.signing, now).await
    }

    /// The HTTP interface.
    pub fn router(&self) -> axum::Router {
        http::router(self.clone())
            .merge(browse::router(self.clone()))
            .merge(item_page::router(self.clone()))
            .merge(accounts::http::router(self.clone()))
            .merge(accounts::closing::router(self.clone()))
            .layer(axum::middleware::from_fn(
                accounts::http::refuse_sessions_on_family_routes,
            ))
    }
}
