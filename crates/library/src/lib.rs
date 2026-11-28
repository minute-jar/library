//! The Minute Jar public library service.
//!
//! [`Library`] is the whole service as callers see it: what is published
//! enters the catalogue, and [`Library::router`] serves the signed documents
//! of the interface with family servers (`docs/specs/interface-v1.md`) and
//! the catalogue the website browses (`docs/specs/catalogue-api.md`).

pub mod contract;

mod accounts;
mod authors_withdrawal;
mod bank_format;
mod browse;
mod cards;
mod catalogue;
mod comments;
mod editorial;
mod http;
mod imports;
mod item_page;
mod media;
mod moderation;
mod ratings;
mod reports;
mod review;
mod shares;
mod suggestions;
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
pub use crate::cards::{
    Card, CardError, CardFuture, CardVerifier, CheckResult, StartedCheck, SubscriptionCards,
    VerifiedBy, VerifiedCard,
};
pub use crate::catalogue::PublishError;
pub use crate::comments::moderation::{
    CommentDecisionError, CommentReport, CommentReportError, HeldComment, QueuedCommentReport,
};
pub use crate::comments::{COMMENT_LENGTH, COMMENTS_PER_DAY};
pub use crate::editorial::EditorialError;
pub use crate::editorial::selection::SelectionError;
pub use crate::imports::ImportError;
pub use crate::media::{ITEM_MEDIA_LIMIT, LocalDirectory, MEDIUM_LIMIT, MediaStore, StoreFuture};
pub use crate::moderation::{
    APPEAL_PERIOD, Appeal, AppealDecision, AppealError, AppealOutcome, AppealState, AppealStatus,
    Appellant, Case, DECISION_TARGET, Event, Figures, History, HistoryEntry, ModeratorError,
    QueuedAppeal, STATEMENT_LIMIT,
};
pub use crate::ratings::{ItemRatings, REVIEW_LENGTH, RatingError, Review};
pub use crate::reports::{
    AuthorNotification, DETAILS_LIMIT, DecideError, Decision, Effect, FormalNotice, Message,
    NewReport, Notifier, NotifyFuture, Outcome, QueuedReport, ReportError, ReportState,
    ReportStatus, Reporter,
};
pub use crate::review::{RefusalReason, ReviewFuture, ReviewOutcome, Reviewer, Submission};
pub use crate::shares::held::{HeldDecision, HeldDecisionError, HeldOutcome, HeldShare};
pub use crate::shares::{SHARES_PER_DAY, SHARING_TERMS};
pub use crate::suggestions::{
    ANSWER_LENGTH, ANSWERS_PER_SUGGESTION, Accepted, ItemSuggestions, QuestionSuggestions,
    SuggestedAnswer, SuggestionError,
};
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
    cards: Option<Arc<cards::Desk>>,
    reports: Option<Arc<reports::Desk>>,
    sharing: Option<Arc<shares::Desk>>,
    comments: Option<Arc<comments::Desk>>,
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
            cards: None,
            reports: None,
            sharing: None,
            comments: None,
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

    /// Turns on the verification of library accounts' cards: by a
    /// zero-amount check through `verifier`, or by the subscription's card
    /// from `subscriptions` when the account is on the subscription's Minute
    /// Jar account. Needs [`Library::with_accounts`]; without it, the card's
    /// routes are refused as unavailable.
    pub fn with_cards(
        mut self,
        verifier: impl CardVerifier,
        subscriptions: impl SubscriptionCards,
    ) -> Self {
        self.cards = Some(Arc::new(cards::Desk {
            verifier: Box::new(verifier),
            subscriptions: Box::new(subscriptions),
        }));
        self
    }

    /// The other library accounts, by pseudonym, verified with the same card
    /// as the account under `pseudonym`: a card may serve several accounts,
    /// and the operator knows it (ADR-0007).
    pub async fn accounts_sharing_card(&self, pseudonym: &str) -> Result<Vec<String>, sqlx::Error> {
        cards::accounts_sharing_card(&self.pool, pseudonym).await
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

    /// Turns on sharing from library accounts: every share is reviewed by
    /// `reviewer` (ADR-0005). Needs [`Library::with_accounts`] and
    /// [`Library::with_cards`] (a share needs a verified card); without it,
    /// the sharing routes are refused as unavailable.
    pub fn with_sharing(mut self, reviewer: impl Reviewer) -> Self {
        self.sharing = Some(Arc::new(shares::Desk {
            reviewer: Box::new(reviewer),
        }));
        self
    }

    /// Turns on comments from library accounts: every comment is reviewed
    /// by `reviewer` (ADR-0005) before it shows; the anonymous routes
    /// (reports of comments) read the instant from `clock`. Needs
    /// [`Library::with_accounts`] and [`Library::with_cards`] (a comment
    /// needs a verified card); without it, the comments' routes are refused
    /// as unavailable.
    pub fn with_comments(
        mut self,
        reviewer: impl Reviewer,
        clock: impl Fn() -> Timestamp + Send + Sync + 'static,
    ) -> Self {
        self.comments = Some(Arc::new(comments::Desk {
            reviewer: Box::new(reviewer),
            clock: Box::new(clock),
        }));
        self
    }

    /// Receives at `now` a report of the published comment `comment`;
    /// answers the report's identifier.
    pub async fn report_comment(
        &self,
        comment: &str,
        report: CommentReport,
        now: Timestamp,
    ) -> Result<String, CommentReportError> {
        comments::moderation::receive(&self.pool, comment, report, now).await
    }

    /// Records at `now` the decision of the moderator `moderator` on a
    /// comment the review held: publishes it under its author's pseudonym,
    /// or refuses it with the reasons its author reads. The decisions log
    /// keeps it.
    pub async fn decide_comment(
        &self,
        moderator: &str,
        comment: &str,
        decision: HeldDecision,
        now: Timestamp,
    ) -> Result<(), CommentDecisionError> {
        comments::moderation::decide_held(&self.pool, moderator, comment, decision, now).await
    }

    /// Records at `now` the decision of the moderator `moderator` on a
    /// report of a comment: upheld removes the comment, rejected shows it
    /// again. The decisions log keeps it.
    pub async fn decide_comment_report(
        &self,
        moderator: &str,
        report: &str,
        decision: Decision,
        now: Timestamp,
    ) -> Result<(), DecideError> {
        comments::moderation::decide_report(&self.pool, moderator, report, decision, now).await
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

    /// Records the answers to accept that the family server under
    /// `library_identifier` suggests for `question` (from 1) of `version` of
    /// the shared item `item`, which it must have imported. Answers the
    /// question already accepts are left out; suggesting again changes
    /// nothing.
    pub async fn record_suggestion(
        &self,
        library_identifier: &str,
        item: &str,
        version: u32,
        question: u32,
        answers: &[&str],
    ) -> Result<(), SuggestionError> {
        suggestions::record(
            &self.pool,
            library_identifier,
            item,
            version,
            question,
            answers,
        )
        .await
    }

    /// The suggestions of the shared item `item` against its latest
    /// version, each answer counted by the families that suggested it;
    /// `None` for an unknown item.
    pub async fn item_suggestions(
        &self,
        item: &str,
    ) -> Result<Option<ItemSuggestions>, SuggestionError> {
        suggestions::of_item(&self.pool, item).await
    }

    /// The new version of `item` that accepts the suggested answers
    /// `accepted`: the latest version with those answers added, as the body
    /// of the share route, which reviews it like any share. Only for the
    /// item's author: the route checks it.
    pub async fn prepare_from_suggestions(
        &self,
        item: &str,
        accepted: &[Accepted],
    ) -> Result<Value, SuggestionError> {
        suggestions::prepare(&self.pool, &*self.media, item, accepted).await
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

    /// Records the decision of the moderator `moderator` (their Minute Jar
    /// account) at `now`: an upheld report withdraws the item by
    /// moderation, a rejected one puts it back; the reporter and the author
    /// are told, and the decisions log keeps it.
    pub async fn decide_report(
        &self,
        moderator: &str,
        report: &str,
        decision: Decision,
        now: Timestamp,
    ) -> Result<(), DecideError> {
        let desk = self.reports.as_ref().ok_or(DecideError::Unavailable)?;
        moderation::decide_report(
            &self.pool,
            &self.signing,
            desk,
            moderator,
            report,
            decision,
            now,
        )
        .await
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

    /// Names the Minute Jar account `subject` a moderator at `now`, known to
    /// other moderators as `handle`. Only the operator does this: no route does.
    pub async fn grant_moderator(
        &self,
        subject: &str,
        handle: &str,
        now: Timestamp,
    ) -> Result<(), ModeratorError> {
        moderation::grant(&self.pool, subject, handle, now).await
    }

    /// Marks at `now` the library account on the Minute Jar account
    /// `subject` as the operator's editorial account (ADR-0002): its shares
    /// are then signed as the software's publisher's, and must say how they
    /// were made and who read them. Only the operator does this: no route
    /// does.
    pub async fn mark_editorial(
        &self,
        subject: &str,
        now: Timestamp,
    ) -> Result<(), EditorialError> {
        editorial::mark(&self.pool, subject, now).await
    }

    /// Ends the editorial mark of the library account on `subject`; answers
    /// whether it had it.
    pub async fn unmark_editorial(&self, subject: &str) -> Result<bool, sqlx::Error> {
        editorial::unmark(&self.pool, subject).await
    }

    /// The moderator `moderator` marks `item` Selection at `now`; the
    /// index is signed again.
    pub async fn select(
        &self,
        moderator: &str,
        item: &str,
        now: Timestamp,
    ) -> Result<(), SelectionError> {
        editorial::selection::select(&self.pool, &self.signing, moderator, item, now).await
    }

    /// The moderator `moderator` removes the Selection mark of `item` at
    /// `now`; the index is signed again.
    pub async fn unselect(
        &self,
        moderator: &str,
        item: &str,
        now: Timestamp,
    ) -> Result<(), SelectionError> {
        editorial::selection::unselect(&self.pool, &self.signing, moderator, item, now).await
    }

    /// Ends the moderator role of `subject`; answers whether it had it.
    pub async fn revoke_moderator(&self, subject: &str) -> Result<bool, sqlx::Error> {
        moderation::revoke(&self.pool, subject).await
    }

    /// Receives at `now` an appeal of the decision on `report` by its author
    /// or its reporter; answers the appeal's identifier.
    pub async fn appeal(
        &self,
        report: &str,
        appeal: Appeal,
        now: Timestamp,
    ) -> Result<String, AppealError> {
        moderation::appeal(&self.pool, report, appeal, now).await
    }

    /// An appeal as its appellant may follow it.
    pub async fn appeal_state(&self, appeal: &str) -> Result<Option<AppealState>, sqlx::Error> {
        moderation::appeal_state(&self.pool, appeal).await
    }

    /// Records at `now` the decision of the moderator `moderator` on an
    /// appeal, who must not be the one who decided the report: a reversal
    /// undoes the decision. The appellant and the author are told, and the
    /// decisions log keeps it.
    pub async fn decide_appeal(
        &self,
        moderator: &str,
        appeal: &str,
        decision: AppealDecision,
        now: Timestamp,
    ) -> Result<(), DecideError> {
        let desk = self.reports.as_ref().ok_or(DecideError::Unavailable)?;
        moderation::decide_appeal(
            &self.pool,
            &self.signing,
            desk,
            moderator,
            appeal,
            decision,
            now,
        )
        .await
    }

    /// The figures of `year` for the yearly transparency report, from the
    /// decisions log: counts by reason and outcome, without personal data.
    /// `None` for a year out of range.
    pub async fn transparency_figures(&self, year: i16) -> Result<Option<Figures>, sqlx::Error> {
        moderation::figures(&self.pool, year).await
    }

    /// The history of `item` in moderation, oldest first, if it exists.
    pub async fn moderation_history(&self, item: &str) -> Result<Option<History>, sqlx::Error> {
        moderation::history(&self.pool, item).await
    }

    /// Records at `now` the decision of the moderator `moderator` on a share
    /// the review held: publishes it under its author's pseudonym, or
    /// refuses it with the reasons its author reads. The decisions log keeps it.
    pub async fn decide_share(
        &self,
        moderator: &str,
        share: &str,
        decision: HeldDecision,
        now: Timestamp,
    ) -> Result<(), HeldDecisionError> {
        shares::held::decide(self, moderator, share, decision, now).await
    }

    /// What waits for a moderator, in one queue, oldest first.
    pub async fn moderation_cases(&self) -> Result<Vec<Case>, sqlx::Error> {
        moderation::cases(&self.pool).await
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
            .merge(moderation::http::router(self.clone()))
            .merge(accounts::closing::router(self.clone()))
            .merge(cards::http::router(self.clone()))
            .merge(shares::http::router(self.clone()))
            .merge(suggestions::http::router(self.clone()))
            .merge(comments::http::router(self.clone()))
            .merge(editorial::selection::router(self.clone()))
            .merge(authors_withdrawal::router(self.clone()))
            .layer(axum::middleware::from_fn(
                accounts::http::refuse_sessions_on_family_routes,
            ))
    }
}
