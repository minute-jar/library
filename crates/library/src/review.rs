//! The review: the one interface through which the service has content
//! checked by AI (ADR-0005). Adapters to providers live behind it; tests use
//! a fake.

use std::future::Future;
use std::io;
use std::pin::Pin;

use serde_json::Value;

use crate::Reason;

/// A future a [`Reviewer`] answers.
pub type ReviewFuture<'a> = Pin<Box<dyn Future<Output = io::Result<Review>> + Send + 'a>>;

/// What is given to the review: one version of a shared item.
#[derive(Debug, Clone, Copy)]
pub struct Submission<'a> {
    pub item: &'a str,
    pub version: u32,
    pub title: &'a str,
    /// A BCP 47 language tag: the review reads the item in its language.
    pub language: &'a str,
    /// The bank or learning path, in the bank format.
    pub content: &'a Value,
    /// The one reason to check, when a report asks it again on that reason;
    /// `None` for the whole review of a new version.
    pub focus: Option<Reason>,
}

/// The outcome of the review.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Review {
    /// Nothing found.
    Published,
    /// Something a moderator must look at, with what was found.
    Held { findings: String },
    /// Plainly against the rules, with the reasons.
    Refused { reasons: String },
}

/// Checks content by AI, through providers that process in the European
/// Union and keep nothing (ADR-0005).
pub trait Reviewer: Send + Sync + 'static {
    fn review<'a>(&'a self, submission: Submission<'a>) -> ReviewFuture<'a>;
}
