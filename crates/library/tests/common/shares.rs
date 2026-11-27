//! A stand-in for the review (ADR-0005), and a library account ready to
//! share: signed in strongly, its card verified.

#![allow(dead_code)]

use std::io;
use std::sync::{Arc, Mutex};

use axum::http::StatusCode;
use library::{
    Library, Message, Notifier, NotifyFuture, ReviewFuture, ReviewOutcome, Reviewer, Submission,
};
use serde_json::{Value, json};

use super::Answer;
use super::cards::*;
use super::identity::*;

/// What the reviewer was asked: the item, its version, title, language and content.
#[derive(Debug, Clone, PartialEq)]
pub struct Asked {
    pub item: String,
    pub version: u32,
    pub title: String,
    pub language: String,
    pub content: Value,
}

/// A reviewer that answers the outcome a test sets, or fails when none is
/// set, and remembers what it was asked.
#[derive(Clone)]
pub struct FakeReviewer {
    outcome: Arc<Mutex<Option<ReviewOutcome>>>,
    asked: Arc<Mutex<Vec<Asked>>>,
}

impl Default for FakeReviewer {
    fn default() -> Self {
        FakeReviewer {
            outcome: Arc::new(Mutex::new(Some(ReviewOutcome::Published))),
            asked: Arc::default(),
        }
    }
}

impl FakeReviewer {
    pub fn answer(&self, outcome: Option<ReviewOutcome>) {
        *self.outcome.lock().unwrap() = outcome;
    }

    pub fn asked(&self) -> Vec<Asked> {
        self.asked.lock().unwrap().clone()
    }
}

impl Reviewer for FakeReviewer {
    fn review<'a>(&'a self, submission: Submission<'a>) -> ReviewFuture<'a> {
        assert_eq!(submission.focus, None, "a share is reviewed whole");
        self.asked.lock().unwrap().push(Asked {
            item: submission.item.into(),
            version: submission.version,
            title: submission.title.into(),
            language: submission.language.into(),
            content: submission.content.clone(),
        });
        let outcome = self.outcome.lock().unwrap().clone();
        Box::pin(async move { outcome.ok_or_else(|| io::Error::other("the provider is down")) })
    }
}

pub struct Sharing {
    pub library: Library,
    pub provider: FakeProvider,
    pub clock: Clock,
    pub payments: FakePayments,
    pub reviewer: FakeReviewer,
    pub database: String,
}

/// The review of reports, which finds nothing.
struct FindingNothing;

impl Reviewer for FindingNothing {
    fn review<'a>(&'a self, _: Submission<'a>) -> ReviewFuture<'a> {
        Box::pin(async { Ok(ReviewOutcome::Published) })
    }
}

/// A notifier that sends nothing.
struct Silent;

impl Notifier for Silent {
    fn send<'a>(&'a self, _: &'a str, _: &'a Message) -> NotifyFuture<'a> {
        Box::pin(async { Ok(()) })
    }
}

/// A library with accounts, cards, sharing and reports, over fakes.
pub async fn with_sharing() -> Sharing {
    let (library, provider, clock, payments, database) = with_cards().await;
    let reviewer = FakeReviewer::default();
    let reading = clock.clone();
    let library =
        library
            .with_sharing(reviewer.clone())
            .with_reports(FindingNothing, Silent, move || reading.now());
    Sharing {
        library,
        provider,
        clock,
        payments,
        reviewer,
        database,
    }
}

impl Sharing {
    /// A strong session on `subject`, whose library account `pseudonym` has
    /// its card verified by its subscription.
    pub async fn author(&self, subject: &str, pseudonym: &str) -> String {
        self.payments.subscribe(subject, &format!("fp_{subject}"));
        let session = account(
            &self.library,
            &self.provider,
            &self.clock,
            subject,
            pseudonym,
            "same_as_subscription",
        )
        .await;
        assert_eq!(
            verify_card(&self.library, &session).await.status,
            StatusCode::OK
        );
        session
    }

    pub async fn share(&self, session: &str, body: Value) -> Answer {
        with_session(&self.library, "POST", "/api/shares", session, Some(body)).await
    }

    pub async fn shares(&self, session: &str) -> Answer {
        with_session(&self.library, "GET", "/api/account/shares", session, None).await
    }
}

/// The current version of the sharing terms.
pub const TERMS: &str = "2026-11";

/// A prepared bank, as the parent app sends it at the first share, the
/// sharing terms accepted.
pub fn prepared(title: &str) -> Value {
    json!({
        "type": "bank",
        "title": title,
        "metadata": {
            "language": "en-GB",
            "subject": "english",
            "ages": {"min": 9, "max": 11},
            "description": "The fifty most common irregular verbs.",
            "curriculum": {"country": "GB", "year": "Year 5", "topic": "Verb forms"}
        },
        "content": {"questions": [
            {"type": "typed", "prompt": "Past of 'go'", "answers": ["went"]},
            {"type": "typed", "prompt": "Past of 'see'", "answers": ["saw"]}
        ]},
        "media": [],
        "sharing_terms": TERMS
    })
}

/// The same without accepting the sharing terms again.
pub fn prepared_again(title: &str) -> Value {
    let mut body = prepared(title);
    body.as_object_mut().unwrap().remove("sharing_terms");
    body
}
