//! The review: the one interface through which the service has content
//! checked by AI (ADR-0005). Adapters to providers live behind it; tests use
//! a fake.

use std::future::Future;
use std::io;
use std::pin::Pin;

use serde_json::Value;

use crate::Reason;

/// A future a [`ReviewProvider`] answers.
pub type ReviewFuture<'a> = Pin<Box<dyn Future<Output = io::Result<ReviewOutcome>> + Send + 'a>>;

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
pub enum ReviewOutcome {
    /// Nothing found.
    Published,
    /// Something a moderator must look at, with what was found.
    Held { findings: String },
    /// Plainly against the rules, with the reasons, per question.
    Refused { reasons: Vec<RefusalReason> },
}

/// One reason the review refuses content for.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct RefusalReason {
    /// The question or lesson card it concerns, by its place in the content
    /// from 1; `None` for the whole item.
    pub question: Option<u32>,
    /// What the rule broken is, in the codes of the withdrawal list.
    #[serde(with = "reason_code")]
    pub reason: Reason,
    /// What was found, in the item's language, for its author to correct.
    pub explanation: String,
}

impl RefusalReason {
    /// The reason in one line, as a moderator reads it.
    pub fn line(&self) -> String {
        match self.question {
            Some(question) => format!(
                "Question {question} ({}): {}",
                self.reason.code(),
                self.explanation
            ),
            None => format!("{}: {}", self.reason.code(), self.explanation),
        }
    }
}

mod reason_code {
    use serde::{Deserialize, Deserializer, Serializer};

    use crate::Reason;

    pub fn serialize<S: Serializer>(reason: &Reason, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(reason.code())
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Reason, D::Error> {
        let code = String::deserialize(deserializer)?;
        Reason::from_code(&code).ok_or_else(|| serde::de::Error::custom("an unknown reason"))
    }
}

/// Checks content by AI, through providers that process in the European
/// Union and keep nothing (ADR-0005).
pub trait ReviewProvider: Send + Sync + 'static {
    fn review<'a>(&'a self, submission: Submission<'a>) -> ReviewFuture<'a>;
}
