//! Comments from library accounts (stories S28, S29): a library account
//! comments on an item page under its pseudonym, every comment is reviewed
//! before it shows, and no account can address another.

mod common;

use axum::http::StatusCode;
use common::cards::text;
use common::comments::*;
use common::identity::*;
use common::*;
use library::{ReviewOutcome, Withdrawal};
use serde_json::json;

const LARK: &str = "subject-of-lark";

#[tokio::test]
async fn a_comment_the_review_publishes_shows_on_the_item_page_under_the_pseudonym() {
    let commenting = with_commenting().await;
    let lark = commenting.commenter(LARK, "Lark").await;

    let commented = commenting
        .comment(&lark, "My son learnt them in a week.")
        .await;

    assert_eq!(
        commented.status,
        StatusCode::CREATED,
        "{}",
        text(&commented)
    );
    let comment = commented.json();
    let id = comment["comment"].as_str().unwrap();
    assert_eq!(id.len(), 32);
    assert_eq!(
        comment,
        json!({
            "comment": id,
            "item": VERBS,
            "text": "My son learnt them in a week.",
            "outcome": "published",
            "reasons": [],
            "commented_at": "2026-11-27T10:00:00Z",
        })
    );
    assert_eq!(
        commenting.shown().await,
        json!([{
            "comment": id,
            "pseudonym": "Lark",
            "text": "My son learnt them in a week.",
            "published_at": "2026-11-27T10:00:00Z",
        }])
    );
    let asked = commenting.sharing.review_provider.asked();
    assert_eq!(asked.len(), 1);
    assert_eq!(asked[0].item, VERBS);
    assert_eq!(asked[0].version, 1);
    assert_eq!(asked[0].title, "Irregular verbs");
    assert_eq!(asked[0].language, "en-GB");
    assert_eq!(
        asked[0].content,
        json!({"comment": "My son learnt them in a week."})
    );
}

#[tokio::test]
async fn an_item_without_comments_shows_none() {
    let commenting = with_commenting().await;

    assert_eq!(commenting.shown().await, json!([]));
}

#[tokio::test]
async fn a_comment_the_review_refuses_never_shows_and_its_author_reads_the_reasons() {
    let commenting = with_commenting().await;
    let lark = commenting.commenter(LARK, "Lark").await;
    let reasons = json!([{
        "question": null,
        "reason": "personal_data",
        "explanation": "It names a school.",
    }]);
    commenting.review(Some(ReviewOutcome::Refused {
        reasons: serde_json::from_value(reasons.clone()).unwrap(),
    }));

    let commented = commenting
        .comment(&lark, "We use it at Hillside Primary.")
        .await;

    assert_eq!(commented.status, StatusCode::OK, "{}", text(&commented));
    assert_eq!(commented.json()["outcome"], "refused");
    assert_eq!(commented.json()["reasons"], reasons);
    assert_eq!(commenting.shown().await, json!([]));
    let own = commenting.own(&lark).await;
    assert_eq!(own.as_array().unwrap().len(), 1);
    assert_eq!(own[0]["outcome"], "refused");
    assert_eq!(own[0]["text"], "We use it at Hillside Primary.");
    assert_eq!(own[0]["reasons"], reasons);
}

#[tokio::test]
async fn a_comment_the_review_holds_waits_for_a_moderator_and_does_not_show() {
    let commenting = with_commenting().await;
    let lark = commenting.commenter(LARK, "Lark").await;
    commenting.review(Some(ReviewOutcome::Held {
        findings: "It may name a child.".into(),
    }));

    let commented = commenting.comment(&lark, "Tom loved it.").await;

    assert_eq!(
        commented.status,
        StatusCode::ACCEPTED,
        "{}",
        text(&commented)
    );
    assert_eq!(commented.json()["outcome"], "held");
    assert_eq!(commenting.shown().await, json!([]));
    assert_eq!(commenting.own(&lark).await[0]["outcome"], "held");
}

#[tokio::test]
async fn a_comment_needs_a_library_account_with_a_verified_card() {
    let commenting = with_commenting().await;
    let library = commenting.library();
    let signed_out = commenting.comment("", "Nice.").await;
    let no_account = session(
        library,
        &commenting.sharing.provider,
        passkey("subject-without-account", signed_in_at()),
    )
    .await;
    let without_account = commenting.comment(&no_account, "Nice.").await;
    let uncarded = common::cards::account(
        library,
        &commenting.sharing.provider,
        &commenting.sharing.clock,
        "subject-without-card",
        "Wren",
        "separate",
    )
    .await;
    let without_card = commenting.comment(&uncarded, "Nice.").await;

    assert_eq!(signed_out.status, StatusCode::UNAUTHORIZED);
    assert_eq!(without_account.status, StatusCode::NOT_FOUND);
    assert_eq!(without_account.json(), json!({"error": "no_account"}));
    assert_eq!(without_card.status, StatusCode::FORBIDDEN);
    assert_eq!(without_card.json(), json!({"error": "card_required"}));
    assert!(commenting.sharing.review_provider.asked().is_empty());
}

#[tokio::test]
async fn a_comment_is_on_an_item_the_catalogue_shows_and_holds_some_text() {
    let commenting = with_commenting().await;
    let lark = commenting.commenter(LARK, "Lark").await;

    let unknown = commenting
        .comment_on(FRACTIONS, &lark, json!({"text": "Nice."}))
        .await;
    let empty = commenting.comment(&lark, "   ").await;
    let long = commenting
        .comment(&lark, &"a".repeat(library::COMMENT_LENGTH + 1))
        .await;
    let longest = commenting
        .comment(&lark, &"a".repeat(library::COMMENT_LENGTH))
        .await;

    assert_eq!(unknown.status, StatusCode::NOT_FOUND);
    assert_eq!(unknown.json(), json!({"error": "unknown_item"}));
    assert_eq!(empty.status, StatusCode::BAD_REQUEST);
    assert_eq!(empty.json(), json!({"error": "bad_comment"}));
    assert_eq!(long.json(), json!({"error": "bad_comment"}));
    assert_eq!(longest.status, StatusCode::CREATED);
}

#[tokio::test]
async fn a_comment_on_a_withdrawn_item_is_refused() {
    let commenting = with_commenting().await;
    let lark = commenting.commenter(LARK, "Lark").await;
    commenting
        .library()
        .withdraw(VERBS, None, Withdrawal::Author, signed_in_at())
        .await
        .unwrap();

    let commented = commenting.comment(&lark, "Nice.").await;

    assert_eq!(commented.json(), json!({"error": "unknown_item"}));
}

#[tokio::test]
async fn a_comment_the_review_could_not_read_is_not_kept_and_does_not_count() {
    let commenting = with_commenting().await;
    let lark = commenting.commenter(LARK, "Lark").await;
    commenting.review(None);

    let commented = commenting.comment(&lark, "Nice.").await;

    assert_eq!(commented.status, StatusCode::SERVICE_UNAVAILABLE);
    let own = with_session(
        commenting.library(),
        "GET",
        "/api/account/comments",
        &lark,
        None,
    )
    .await
    .json();
    assert_eq!(own["comments"], json!([]));
    assert_eq!(
        own["quota"],
        json!({"comments_per_day": 20, "commented_today": 0, "next_comment_at": null})
    );
}

#[tokio::test]
async fn an_account_writes_at_most_twenty_comments_a_day() {
    let commenting = with_commenting().await;
    let lark = commenting.commenter(LARK, "Lark").await;
    for n in 0..library::COMMENTS_PER_DAY {
        let commented = commenting.comment(&lark, &format!("Comment {n}.")).await;
        assert_eq!(commented.status, StatusCode::CREATED);
    }

    let refused = commenting.comment(&lark, "One more.").await;
    commenting
        .sharing
        .clock
        .advance(jiff::SignedDuration::from_hours(14));
    let tomorrow = commenting.comment(&lark, "One more.").await;

    assert_eq!(refused.status, StatusCode::TOO_MANY_REQUESTS);
    assert_eq!(
        refused.json(),
        json!({
            "error": "comment_quota_reached",
            "comments_per_day": 20,
            "next_comment_at": "2026-11-28T00:00:00Z",
        })
    );
    assert_eq!(tomorrow.status, StatusCode::CREATED);
}

#[tokio::test]
async fn a_comment_shows_under_the_pseudonym_it_was_published_under() {
    let commenting = with_commenting().await;
    let lark = commenting.commenter(LARK, "Lark").await;
    commenting.comment(&lark, "First.").await;

    let renamed = with_session(
        commenting.library(),
        "PUT",
        "/api/account/pseudonym",
        &lark,
        Some(json!({"pseudonym": "Skylark"})),
    )
    .await;
    assert_eq!(renamed.status, StatusCode::OK, "{}", text(&renamed));
    commenting
        .sharing
        .clock
        .advance(jiff::SignedDuration::from_mins(1));
    commenting.comment(&lark, "Second.").await;

    let shown = commenting.shown().await;
    assert_eq!(shown[0]["pseudonym"], "Lark");
    assert_eq!(shown[1]["pseudonym"], "Skylark");
}

#[tokio::test]
async fn a_comment_names_no_one_and_carries_no_library_identifier() {
    let commenting = with_commenting().await;
    let lark = commenting.commenter(LARK, "Lark").await;

    let mut refused = Vec::new();
    for body in [
        json!({"text": "Hello Owl.", "to": "Owl"}),
        json!({"text": "Hello Owl.", "reply_to": "Owl"}),
        json!({"text": "Hello Owl.", "account": 1}),
        json!({"text": "Hello Owl.", "recipient": "Owl"}),
    ] {
        refused.push(commenting.comment_on(VERBS, &lark, body).await);
    }
    let joined = commenting
        .comment_on(
            VERBS,
            &lark,
            json!({"text": "Hello.", "library_identifier": FAMILY_A}),
        )
        .await;

    for answer in refused {
        assert_eq!(answer.status, StatusCode::BAD_REQUEST);
        assert_eq!(answer.json(), json!({"error": "bad_request"}));
    }
    assert_eq!(
        joined.json(),
        json!({"error": "library_identifier_not_allowed"})
    );
    assert!(commenting.sharing.review_provider.asked().is_empty());
}

#[tokio::test]
async fn no_route_addresses_one_account_from_another() {
    let commenting = with_commenting().await;
    let lark = commenting.commenter(LARK, "Lark").await;
    commenting.commenter(SUBJECT, "Owl").await;

    for (method, path) in [
        ("GET", "/api/accounts/Owl"),
        ("POST", "/api/accounts/Owl/messages"),
        ("GET", "/api/messages"),
        ("POST", "/api/messages"),
        ("GET", "/api/account/messages"),
        ("POST", "/api/account/messages"),
        (
            "POST",
            "/api/comments/00000000000000000000000000000000/replies",
        ),
    ] {
        let answer = with_session(
            commenting.library(),
            method,
            path,
            &lark,
            Some(json!({"text": "Hello Owl.", "to": "Owl"})),
        )
        .await;
        assert!(
            answer.status == StatusCode::NOT_FOUND
                || answer.status == StatusCode::METHOD_NOT_ALLOWED,
            "{method} {path} answers {}",
            answer.status
        );
    }
}

#[tokio::test]
async fn without_comments_turned_on_their_routes_are_unavailable() {
    let sharing = common::shares::with_sharing().await;
    let lark = sharing.author(LARK, "Lark").await;

    let commented = with_session(
        &sharing.library,
        "POST",
        &format!("/api/items/{VERBS}/comments"),
        &lark,
        Some(json!({"text": "Nice."})),
    )
    .await;

    assert_eq!(commented.status, StatusCode::SERVICE_UNAVAILABLE);
}
