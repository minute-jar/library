//! What moderation does with shares: a share the review holds waits in the
//! moderation queue until a moderator publishes or refuses it; the author,
//! joined to their items through their library account, reads what they
//! must be told about reports and appeals decisions on them.

mod common;

use axum::http::StatusCode;
use common::cards::text;
use common::identity::*;
use common::shares::*;
use common::*;
use library::contract::Kind;
use library::{Decision, Outcome, ReviewOutcome};
use serde_json::{Value, json};

const ALICE: &str = "moderator-alice-subject";
const BRUNO: &str = "moderator-bruno-subject";

/// A library with sharing, reports and two moderators, and an author whose
/// first share the review holds.
struct Held {
    sharing: Sharing,
    author: String,
    share: String,
    item: String,
}

async fn held() -> Held {
    let sharing = with_sharing().await;
    for (subject, handle) in [(ALICE, "Alice"), (BRUNO, "Bruno")] {
        sharing
            .library
            .grant_moderator(subject, handle, signed_in_at())
            .await
            .unwrap();
    }
    let author = sharing.author(SUBJECT, "Owl").await;
    sharing.reviewer.answer(Some(ReviewOutcome::Held {
        findings: "Question 2 may name a school.".into(),
    }));
    let shared = sharing.share(&author, prepared("Irregular verbs")).await;
    assert_eq!(shared.status, StatusCode::ACCEPTED, "{}", text(&shared));
    let shared = shared.json();
    Held {
        share: shared["share"].as_str().unwrap().into(),
        item: shared["item"].as_str().unwrap().into(),
        sharing,
        author,
    }
}

impl Held {
    async fn moderator(&self, subject: &str) -> String {
        let now = self.sharing.clock.now();
        session(
            &self.sharing.library,
            &self.sharing.provider,
            passkey(subject, now),
        )
        .await
    }

    async fn ask(&self, method: &str, path: &str, session: &str, body: Option<Value>) -> Answer {
        with_session(&self.sharing.library, method, path, session, body).await
    }

    async fn decide(&self, session: &str, body: Value) -> Answer {
        let path = format!("/api/moderation/shares/{}/decision", self.share);
        self.ask("POST", &path, session, Some(body)).await
    }

    async fn in_index(&self) -> bool {
        let index = fetch_verified(&self.sharing.library, "/v1/index", Kind::Index).await;
        index["items"]
            .as_array()
            .unwrap()
            .iter()
            .any(|entry| entry["item"] == self.item.as_str())
    }

    async fn authors_share(&self) -> Value {
        self.sharing.shares(&self.author).await.json()["shares"][0].clone()
    }
}

#[tokio::test]
async fn a_held_share_waits_in_the_moderation_queue_with_the_findings_and_the_content() {
    let held = held().await;
    let alice = held.moderator(ALICE).await;

    let queue = held.ask("GET", "/api/moderation/queue", &alice, None).await;

    assert_eq!(queue.status, StatusCode::OK, "{}", text(&queue));
    assert_eq!(
        queue.json()["cases"],
        json!([{
            "kind": "held_review",
            "share": held.share,
            "item": held.item,
            "title": "Irregular verbs",
            "pseudonym": "Owl",
            "findings": "Question 2 may name a school.",
            "type": "bank",
            "metadata": prepared("")["metadata"],
            "content": prepared("")["content"],
            "received_at": "2026-11-27T10:00:00Z",
            "due_at": "2026-11-29T10:00:00Z",
        }])
    );
}

#[tokio::test]
async fn a_moderator_publishes_a_held_share_under_the_authors_pseudonym() {
    let held = held().await;
    held.sharing
        .clock
        .advance(jiff::SignedDuration::from_hours(2));
    let alice = held.moderator(ALICE).await;

    let decided = held
        .decide(
            &alice,
            json!({"outcome": "published", "reason": "personal_data", "reasons": "The school is made up."}),
        )
        .await;

    assert_eq!(decided.status, StatusCode::NO_CONTENT, "{}", text(&decided));
    assert!(held.in_index().await);
    let version = fetch_verified(
        &held.sharing.library,
        &format!("/v1/items/{}/versions/1", held.item),
        Kind::Item,
    )
    .await;
    assert_eq!(
        version["author"],
        json!({"pseudonym": "Owl", "editorial": false})
    );
    assert_eq!(version["published_at"], "2026-11-27T12:00:00Z");
    let share = held.authors_share().await;
    assert_eq!(share["outcome"], "published");
    assert_eq!(share["version"], 1);
    let queue = held.ask("GET", "/api/moderation/queue", &alice, None).await;
    assert_eq!(queue.json()["cases"], json!([]));
    let figures = held
        .ask("GET", "/api/moderation/figures/2026", &alice, None)
        .await;
    assert_eq!(
        figures.json()["decisions"],
        json!([{"reason": "personal_data", "outcome": "rejected", "count": 1}])
    );
}

#[tokio::test]
async fn a_moderator_refuses_a_held_share_with_reasons_for_its_author() {
    let held = held().await;
    let alice = held.moderator(ALICE).await;

    let decided = held
        .decide(
            &alice,
            json!({"outcome": "refused", "reason": "personal_data", "reasons": "Question 2 names a real school."}),
        )
        .await;

    assert_eq!(decided.status, StatusCode::NO_CONTENT, "{}", text(&decided));
    assert!(!held.in_index().await);
    let share = held.authors_share().await;
    assert_eq!(share["outcome"], "refused");
    assert_eq!(share["item"], json!(null));
    assert_eq!(
        share["reasons"],
        json!([{"question": null, "reason": "personal_data", "explanation": "Question 2 names a real school."}])
    );
    let figures = held
        .ask("GET", "/api/moderation/figures/2026", &alice, None)
        .await;
    assert_eq!(
        figures.json()["decisions"],
        json!([{"reason": "personal_data", "outcome": "upheld", "count": 1}])
    );
}

#[tokio::test]
async fn a_held_share_is_decided_once_with_reasons_by_a_moderator() {
    let held = held().await;
    let alice = held.moderator(ALICE).await;
    let decision = json!({"outcome": "refused", "reason": "rights", "reasons": "Copied."});

    let without_reasons = held
        .decide(
            &alice,
            json!({"outcome": "refused", "reason": "rights", "reasons": " "}),
        )
        .await;
    let bad_reason = held
        .decide(
            &alice,
            json!({"outcome": "refused", "reason": "boring", "reasons": "No."}),
        )
        .await;
    let by_the_author = held.decide(&held.author, decision.clone()).await;
    let first = held.decide(&alice, decision.clone()).await;
    let again = held.decide(&alice, decision).await;
    let unknown = held
        .ask(
            "POST",
            "/api/moderation/shares/00000000000000000000000000000000/decision",
            &alice,
            Some(json!({"outcome": "published", "reason": "rights", "reasons": "Fine."})),
        )
        .await;

    assert_eq!(without_reasons.status, StatusCode::BAD_REQUEST);
    assert_eq!(without_reasons.json(), json!({"error": "reasons_required"}));
    assert_eq!(bad_reason.status, StatusCode::BAD_REQUEST);
    assert_eq!(bad_reason.json(), json!({"error": "bad_request"}));
    assert_eq!(by_the_author.status, StatusCode::FORBIDDEN);
    assert_eq!(by_the_author.json(), json!({"error": "not_a_moderator"}));
    assert_eq!(first.status, StatusCode::NO_CONTENT);
    assert_eq!(again.status, StatusCode::CONFLICT);
    assert_eq!(again.json(), json!({"error": "already_decided"}));
    assert_eq!(unknown.status, StatusCode::NOT_FOUND);
    assert_eq!(unknown.json(), json!({"error": "unknown_share"}));
}

/// An author with one published item, reported by a family server for `reason`.
async fn reported(reason: &str) -> (Held, String) {
    let held = held().await;
    let alice = held.moderator(ALICE).await;
    held.decide(
        &alice,
        json!({"outcome": "published", "reason": "personal_data", "reasons": "Made up."}),
    )
    .await;
    let reported = post_json(
        &held.sharing.library,
        "/v1/reports",
        &json!({"library_identifier": FAMILY_A, "item": held.item, "reason": reason, "details": "Question 1"}),
    )
    .await;
    assert_eq!(reported.status, StatusCode::ACCEPTED, "{}", text(&reported));
    let report = reported.json()["report"].as_str().unwrap().to_string();
    (held, report)
}

#[tokio::test]
async fn the_author_reads_what_they_must_be_told_about_reports_on_their_items() {
    let (held, report) = reported("wrong_answer").await;

    let told = held
        .ask("GET", "/api/account/notifications", &held.author, None)
        .await;

    assert_eq!(told.status, StatusCode::OK, "{}", text(&told));
    assert_eq!(told.headers["cache-control"], "private, no-store");
    assert_eq!(
        told.json(),
        json!({"notifications": [{
            "item": held.item,
            "title": "Irregular verbs",
            "report": report,
            "reason": "wrong_answer",
            "details": "Question 1",
            "decision": null,
            "appeal": null,
            "at": "2026-11-27T10:00:00Z",
        }]})
    );
    let other = held.sharing.author("subject-other", "Lark").await;
    let not_theirs = held
        .ask("GET", "/api/account/notifications", &other, None)
        .await;
    assert_eq!(not_theirs.json(), json!({"notifications": []}));
}

#[tokio::test]
async fn the_author_appeals_a_decision_on_their_item_through_their_account() {
    let (held, report) = reported("other").await;
    held.sharing
        .library
        .decide_report(
            ALICE,
            &report,
            Decision {
                outcome: Outcome::Upheld,
                reasons: "Off topic.".into(),
            },
            held.sharing.clock.now(),
        )
        .await
        .unwrap();
    let path = format!("/api/account/reports/{report}/appeal");
    let statement = json!({"statement": "Every question is about verbs."});

    let other = held.sharing.author("subject-other", "Lark").await;
    let not_theirs = held
        .ask("POST", &path, &other, Some(statement.clone()))
        .await;
    let appealed = held
        .ask("POST", &path, &held.author, Some(statement.clone()))
        .await;
    let again = held.ask("POST", &path, &held.author, Some(statement)).await;

    assert_eq!(not_theirs.status, StatusCode::NOT_FOUND);
    assert_eq!(not_theirs.json(), json!({"error": "unknown_report"}));
    assert_eq!(appealed.status, StatusCode::ACCEPTED, "{}", text(&appealed));
    let appeal = appealed.json()["appeal"].as_str().unwrap().to_string();
    assert_eq!(again.status, StatusCode::CONFLICT);
    assert_eq!(again.json(), json!({"error": "already_appealed"}));
    let bruno = held.moderator(BRUNO).await;
    let queue = held.ask("GET", "/api/moderation/queue", &bruno, None).await;
    let case = &queue.json()["cases"][0];
    assert_eq!(case["kind"], "appeal");
    assert_eq!(case["appeal"], appeal.as_str());
    assert_eq!(case["appellant"], "author");
    assert_eq!(case["statement"], "Every question is about verbs.");
}
