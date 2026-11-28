//! What moderation does with comments: a comment the review holds waits in
//! the one moderation queue until a moderator publishes or refuses it; a
//! published comment can be reported like an item, and a moderator who
//! upholds the report removes it.

mod common;

use axum::http::StatusCode;
use common::cards::text;
use common::comments::*;
use common::identity::*;
use common::*;
use library::{CommentReport, Decision, Outcome, Reason, ReviewOutcome};
use serde_json::{Value, json};

const ALICE: &str = "moderator-alice-subject";
const LARK: &str = "subject-of-lark";

/// A library with comments and a moderator, and Lark's session.
async fn moderated() -> (Commenting, String, String) {
    let commenting = with_commenting().await;
    commenting
        .library()
        .grant_moderator(ALICE, "Alice", signed_in_at())
        .await
        .unwrap();
    let lark = commenting.commenter(LARK, "Lark").await;
    let alice = session(
        commenting.library(),
        &commenting.sharing.provider,
        passkey(ALICE, signed_in_at()),
    )
    .await;
    (commenting, lark, alice)
}

async fn ask(commenting: &Commenting, path: &str, session: &str, body: Option<Value>) -> Answer {
    let method = if body.is_some() { "POST" } else { "GET" };
    with_session(commenting.library(), method, path, session, body).await
}

async fn queue(commenting: &Commenting, alice: &str) -> Value {
    let queue = ask(commenting, "/api/moderation/queue", alice, None).await;
    assert_eq!(queue.status, StatusCode::OK, "{}", text(&queue));
    queue.json()["cases"].clone()
}

/// Lark's comment, which the review holds; answers its identifier.
async fn held(commenting: &Commenting, lark: &str) -> String {
    commenting.review(Some(ReviewOutcome::Held {
        findings: "It may name a child.".into(),
    }));
    let commented = commenting.comment(lark, "Tom loved it.").await;
    assert_eq!(commented.status, StatusCode::ACCEPTED);
    commented.json()["comment"].as_str().unwrap().to_string()
}

/// Lark's comment, published; answers its identifier.
async fn published(commenting: &Commenting, lark: &str) -> String {
    commenting.review(Some(ReviewOutcome::Published));
    let commented = commenting.comment(lark, "Learnt in a week.").await;
    assert_eq!(commented.status, StatusCode::CREATED);
    commented.json()["comment"].as_str().unwrap().to_string()
}

async fn report(commenting: &Commenting, comment: &str, body: Value) -> Answer {
    post_json(
        commenting.library(),
        &format!("/api/comments/{comment}/reports"),
        &body,
    )
    .await
}

#[tokio::test]
async fn a_held_comment_waits_in_the_moderation_queue_with_the_findings() {
    let (commenting, lark, alice) = moderated().await;
    let comment = held(&commenting, &lark).await;

    assert_eq!(
        queue(&commenting, &alice).await,
        json!([{
            "kind": "held_comment",
            "comment": comment,
            "item": VERBS,
            "title": "Irregular verbs",
            "pseudonym": "Lark",
            "text": "Tom loved it.",
            "findings": "It may name a child.",
            "received_at": "2026-11-27T10:00:00Z",
            "due_at": "2026-11-29T10:00:00Z",
        }])
    );
}

#[tokio::test]
async fn a_moderator_publishes_a_held_comment() {
    let (commenting, lark, _) = moderated().await;
    let comment = held(&commenting, &lark).await;
    commenting
        .sharing
        .clock
        .advance(jiff::SignedDuration::from_hours(2));
    let alice = session(
        commenting.library(),
        &commenting.sharing.provider,
        passkey(ALICE, commenting.sharing.clock.now()),
    )
    .await;

    let decided = ask(
        &commenting,
        &format!("/api/moderation/comments/{comment}/decision"),
        &alice,
        Some(json!({"outcome": "published", "reason": "personal_data", "reasons": "Tom is no one in particular."})),
    )
    .await;

    assert_eq!(decided.status, StatusCode::NO_CONTENT, "{}", text(&decided));
    assert_eq!(
        commenting.shown().await,
        json!([{
            "comment": comment,
            "pseudonym": "Lark",
            "text": "Tom loved it.",
            "published_at": "2026-11-27T12:00:00Z",
        }])
    );
    assert_eq!(commenting.own(&lark).await[0]["outcome"], "published");
    assert_eq!(queue(&commenting, &alice).await, json!([]));
    let figures = ask(&commenting, "/api/moderation/figures/2026", &alice, None).await;
    assert_eq!(
        figures.json()["decisions"],
        json!([{"reason": "personal_data", "outcome": "rejected", "count": 1}])
    );
}

#[tokio::test]
async fn a_moderator_refuses_a_held_comment_with_reasons_for_its_author() {
    let (commenting, lark, alice) = moderated().await;
    let comment = held(&commenting, &lark).await;

    let decided = commenting
        .library()
        .decide_comment(
            ALICE,
            &comment,
            library::HeldDecision {
                outcome: library::HeldOutcome::Refused,
                reason: Reason::PersonalData,
                reasons: "It names a child.".into(),
            },
            signed_in_at(),
        )
        .await;
    let again = ask(
        &commenting,
        &format!("/api/moderation/comments/{comment}/decision"),
        &alice,
        Some(json!({"outcome": "published", "reason": "personal_data", "reasons": "Fine."})),
    )
    .await;

    assert!(decided.is_ok());
    assert_eq!(again.status, StatusCode::CONFLICT);
    assert_eq!(commenting.shown().await, json!([]));
    let own = commenting.own(&lark).await;
    assert_eq!(own[0]["outcome"], "refused");
    assert_eq!(
        own[0]["reasons"],
        json!([{"question": null, "reason": "personal_data", "explanation": "It names a child."}])
    );
}

#[tokio::test]
async fn only_a_moderator_decides_a_held_comment_with_reasons() {
    let (commenting, lark, alice) = moderated().await;
    let comment = held(&commenting, &lark).await;
    let path = format!("/api/moderation/comments/{comment}/decision");
    let decision = json!({"outcome": "published", "reason": "other", "reasons": "Fine."});

    let by_author = ask(&commenting, &path, &lark, Some(decision.clone())).await;
    let without_reasons = ask(
        &commenting,
        &path,
        &alice,
        Some(json!({"outcome": "published", "reason": "other", "reasons": " "})),
    )
    .await;
    let unknown = ask(
        &commenting,
        "/api/moderation/comments/00000000000000000000000000000000/decision",
        &alice,
        Some(decision),
    )
    .await;

    assert_eq!(by_author.status, StatusCode::FORBIDDEN);
    assert_eq!(by_author.json(), json!({"error": "not_a_moderator"}));
    assert_eq!(without_reasons.json(), json!({"error": "reasons_required"}));
    assert_eq!(unknown.json(), json!({"error": "unknown_comment"}));
    assert_eq!(
        queue(&commenting, &alice).await.as_array().unwrap().len(),
        1
    );
}

#[tokio::test]
async fn a_published_comment_is_reported_like_an_item_and_waits_for_a_moderator() {
    let (commenting, lark, alice) = moderated().await;
    let comment = published(&commenting, &lark).await;

    let reported = report(
        &commenting,
        &comment,
        json!({"reason": "unsuitable_content", "details": "Rude."}),
    )
    .await;

    assert_eq!(reported.status, StatusCode::ACCEPTED, "{}", text(&reported));
    assert_eq!(reported.headers["cache-control"], "no-store");
    let report = reported.json()["report"].as_str().unwrap().to_string();
    assert_eq!(report.len(), 32);
    assert_eq!(commenting.shown().await.as_array().unwrap().len(), 1);
    assert_eq!(
        queue(&commenting, &alice).await,
        json!([{
            "kind": "comment_report",
            "report": report,
            "comment": comment,
            "item": VERBS,
            "title": "Irregular verbs",
            "pseudonym": "Lark",
            "text": "Learnt in a week.",
            "reason": "unsuitable_content",
            "details": "Rude.",
            "hidden": false,
            "received_at": "2026-11-27T10:00:00Z",
            "due_at": "2026-11-29T10:00:00Z",
        }])
    );
    let figures = ask(&commenting, "/api/moderation/figures/2026", &alice, None).await;
    assert_eq!(
        figures.json()["reports"],
        json!([{"reason": "unsuitable_content", "received": 1}])
    );
}

#[tokio::test]
async fn a_comment_reported_for_a_childs_personal_data_is_hidden_until_a_moderator_decides() {
    let (commenting, lark, alice) = moderated().await;
    let comment = published(&commenting, &lark).await;

    let report = commenting
        .library()
        .report_comment(
            &comment,
            CommentReport {
                reason: Reason::PersonalData,
                details: None,
            },
            signed_in_at(),
        )
        .await
        .unwrap();

    assert_eq!(commenting.shown().await, json!([]));
    assert_eq!(queue(&commenting, &alice).await[0]["hidden"], true);
    let decided = ask(
        &commenting,
        &format!("/api/moderation/comment-reports/{report}/decision"),
        &alice,
        Some(json!({"outcome": "rejected", "reasons": "No child is named."})),
    )
    .await;
    assert_eq!(decided.status, StatusCode::NO_CONTENT, "{}", text(&decided));
    assert_eq!(commenting.shown().await.as_array().unwrap().len(), 1);
    assert_eq!(queue(&commenting, &alice).await, json!([]));
}

#[tokio::test]
async fn a_moderator_who_upholds_a_report_of_a_comment_removes_it() {
    let (commenting, lark, alice) = moderated().await;
    let comment = published(&commenting, &lark).await;
    let report = report(&commenting, &comment, json!({"reason": "other"}))
        .await
        .json()["report"]
        .as_str()
        .unwrap()
        .to_string();

    commenting
        .library()
        .decide_comment_report(
            ALICE,
            &report,
            Decision {
                outcome: Outcome::Upheld,
                reasons: "Off topic and rude.".into(),
            },
            signed_in_at(),
        )
        .await
        .unwrap();

    assert_eq!(commenting.shown().await, json!([]));
    assert_eq!(commenting.own(&lark).await[0]["outcome"], "removed");
    let again = report_again(&commenting, &comment).await;
    assert_eq!(again.json(), json!({"error": "unknown_comment"}));
    let figures = ask(&commenting, "/api/moderation/figures/2026", &alice, None).await;
    assert_eq!(
        figures.json()["decisions"],
        json!([{"reason": "other", "outcome": "upheld", "count": 1}])
    );
    let decided_again = ask(
        &commenting,
        &format!("/api/moderation/comment-reports/{report}/decision"),
        &alice,
        Some(json!({"outcome": "rejected", "reasons": "Fine."})),
    )
    .await;
    assert_eq!(decided_again.status, StatusCode::CONFLICT);
}

async fn report_again(commenting: &Commenting, comment: &str) -> Answer {
    report(commenting, comment, json!({"reason": "other"})).await
}

#[tokio::test]
async fn a_report_of_a_comment_names_a_reason_and_a_published_comment() {
    let (commenting, lark, _) = moderated().await;
    let comment = published(&commenting, &lark).await;
    let held = held(&commenting, &lark).await;

    let bad_reason = report(&commenting, &comment, json!({"reason": "boring"})).await;
    let long = report(
        &commenting,
        &comment,
        json!({"reason": "other", "details": "a".repeat(library::DETAILS_LIMIT + 1)}),
    )
    .await;
    let unknown = report(
        &commenting,
        "00000000000000000000000000000000",
        json!({"reason": "other"}),
    )
    .await;
    let not_shown = report(&commenting, &held, json!({"reason": "other"})).await;
    let with_identifier = report(
        &commenting,
        &comment,
        json!({"reason": "other", "library_identifier": FAMILY_A}),
    )
    .await;

    assert_eq!(bad_reason.json(), json!({"error": "bad_reason"}));
    assert_eq!(long.json(), json!({"error": "bad_request"}));
    assert_eq!(unknown.status, StatusCode::NOT_FOUND);
    assert_eq!(unknown.json(), json!({"error": "unknown_comment"}));
    assert_eq!(not_shown.json(), json!({"error": "unknown_comment"}));
    assert_eq!(with_identifier.json(), json!({"error": "bad_request"}));
}

#[tokio::test]
async fn the_timeline_of_an_item_leaves_out_its_comments() {
    let (commenting, lark, alice) = moderated().await;
    let comment = published(&commenting, &lark).await;
    report(&commenting, &comment, json!({"reason": "other"})).await;

    let timeline = ask(
        &commenting,
        &format!("/api/moderation/items/{VERBS}"),
        &alice,
        None,
    )
    .await;

    assert_eq!(timeline.json()["timeline"], json!([]));
}
