//! A library where library accounts comment on a published item, the
//! review (ADR-0005) a fake a test sets.

#![allow(dead_code)]

use library::ReviewOutcome;
use serde_json::{Value, json};

use super::identity::*;
use super::shares::*;
use super::*;

pub struct Commenting {
    pub sharing: Sharing,
}

/// A library with accounts, cards, sharing, reports and comments over
/// fakes, the bank of irregular verbs published.
pub async fn with_commenting() -> Commenting {
    let mut sharing = with_sharing().await;
    sharing.library = sharing
        .library
        .clone()
        .with_comments(sharing.reviewer.clone(), {
            let clock = sharing.clock.clone();
            move || clock.now()
        });
    sharing
        .library
        .publish_version(verbs("Irregular verbs"), at("2026-11-20T18:00:00Z"))
        .await
        .unwrap();
    Commenting { sharing }
}

impl Commenting {
    pub fn library(&self) -> &Library {
        &self.sharing.library
    }

    /// A session on `subject`, whose library account `pseudonym` has its card verified.
    pub async fn commenter(&self, subject: &str, pseudonym: &str) -> String {
        self.sharing.author(subject, pseudonym).await
    }

    /// The outcome the review answers next; `None` for a review that fails.
    pub fn review(&self, outcome: Option<ReviewOutcome>) {
        self.sharing.reviewer.answer(outcome);
    }

    pub async fn comment_on(&self, item: &str, session: &str, body: Value) -> Answer {
        let path = format!("/api/items/{item}/comments");
        with_session(self.library(), "POST", &path, session, Some(body)).await
    }

    /// Comments `text` on the bank of irregular verbs.
    pub async fn comment(&self, session: &str, text: &str) -> Answer {
        self.comment_on(VERBS, session, json!({"text": text})).await
    }

    /// The comments of the verbs' item page.
    pub async fn shown(&self) -> Value {
        let page = get(self.library(), &format!("/api/items/{VERBS}")).await;
        assert_eq!(page.status, axum::http::StatusCode::OK);
        page.json()["comments"].clone()
    }

    /// The session's account's comments, as its owner reads them.
    pub async fn own(&self, session: &str) -> Value {
        let answer = with_session(
            self.library(),
            "GET",
            "/api/account/comments",
            session,
            None,
        )
        .await;
        assert_eq!(answer.status, axum::http::StatusCode::OK);
        answer.json()["comments"].clone()
    }
}
