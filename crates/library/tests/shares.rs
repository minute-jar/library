//! Sharing an item from a library account (stories S10, S13, S16): the
//! sharing terms at the first share, the review, and its outcome.

mod common;

use axum::http::StatusCode;
use common::cards::text;
use common::identity::*;
use common::shares::*;
use common::*;
use jiff::SignedDuration;
use library::contract::Kind;
use library::{Reason, RefusalReason, ReviewOutcome, SHARES_PER_DAY};
use serde_json::json;

#[tokio::test]
async fn a_share_the_review_publishes_enters_the_index_under_the_pseudonym() {
    let sharing = with_sharing().await;
    let session = sharing.author(SUBJECT, "Owl").await;

    let shared = sharing
        .share(&session, prepared("Irregular verbs, part 1"))
        .await;

    assert_eq!(shared.status, StatusCode::CREATED, "{}", text(&shared));
    let answer = shared.json();
    let item = answer["item"].as_str().unwrap().to_string();
    assert_eq!(item.len(), 32);
    assert_eq!(answer["outcome"], "published");
    assert_eq!(answer["version"], 1);
    assert_eq!(answer["reasons"], json!([]));

    let asked = sharing.reviewer.asked();
    assert_eq!(asked.len(), 1);
    assert_eq!(asked[0].item, item);
    assert_eq!(asked[0].version, 1);
    assert_eq!(asked[0].title, "Irregular verbs, part 1");
    assert_eq!(asked[0].language, "en-GB");
    assert_eq!(asked[0].content, prepared("")["content"]);

    let index = fetch_verified(&sharing.library, "/v1/index", Kind::Index).await;
    let entry = &index["items"][0];
    assert_eq!(entry["item"], item.as_str());
    assert_eq!(entry["title"], "Irregular verbs, part 1");
    assert_eq!(
        entry["author"],
        json!({"pseudonym": "Owl", "editorial": false})
    );
    assert_eq!(entry["published_at"], "2026-11-27T10:00:00Z");
    assert_eq!(
        entry["metadata"]["curriculum"],
        json!({"country": "GB", "year": "Year 5", "topic": "Verb forms"})
    );
    let version = fetch_verified(
        &sharing.library,
        &format!("/v1/items/{item}/versions/1"),
        Kind::Item,
    )
    .await;
    assert_eq!(version["content"], prepared("")["content"]);
    assert_eq!(version["licence"], "CC-BY-SA-4.0");
}

#[tokio::test]
async fn a_share_held_for_a_moderator_stays_out_of_the_index() {
    let sharing = with_sharing().await;
    let session = sharing.author(SUBJECT, "Owl").await;
    sharing.reviewer.answer(Some(ReviewOutcome::Held {
        findings: "Question 2 may name a school.".into(),
    }));

    let shared = sharing.share(&session, prepared("Irregular verbs")).await;

    assert_eq!(shared.status, StatusCode::ACCEPTED, "{}", text(&shared));
    let answer = shared.json();
    assert_eq!(answer["outcome"], "held");
    assert_eq!(answer["version"], json!(null));
    assert_eq!(answer["item"].as_str().unwrap().len(), 32);
    assert!(
        !text(&shared).contains("school"),
        "the findings are for the moderator"
    );
    let index = fetch_verified(&sharing.library, "/v1/index", Kind::Index).await;
    assert_eq!(index["items"], json!([]));
}

fn wrong_answer() -> RefusalReason {
    RefusalReason {
        question: Some(2),
        reason: Reason::WrongAnswer,
        explanation: "The past of 'see' is 'saw', not 'seed'.".into(),
    }
}

#[tokio::test]
async fn a_refused_share_answers_its_reasons_per_question_and_publishes_nothing() {
    let sharing = with_sharing().await;
    let session = sharing.author(SUBJECT, "Owl").await;
    sharing.reviewer.answer(Some(ReviewOutcome::Refused {
        reasons: vec![
            wrong_answer(),
            RefusalReason {
                question: None,
                reason: Reason::Rights,
                explanation: "Looks copied from a textbook.".into(),
            },
        ],
    }));

    let shared = sharing.share(&session, prepared("Irregular verbs")).await;

    assert_eq!(shared.status, StatusCode::OK, "{}", text(&shared));
    let answer = shared.json();
    assert_eq!(answer["outcome"], "refused");
    assert_eq!(answer["item"], json!(null));
    assert_eq!(
        answer["reasons"],
        json!([
            {"question": 2, "reason": "wrong_answer", "explanation": "The past of 'see' is 'saw', not 'seed'."},
            {"question": null, "reason": "rights", "explanation": "Looks copied from a textbook."}
        ])
    );
    let index = fetch_verified(&sharing.library, "/v1/index", Kind::Index).await;
    assert_eq!(index["items"], json!([]));
}

#[tokio::test]
async fn the_author_sees_every_share_its_outcome_and_the_days_quota() {
    let sharing = with_sharing().await;
    let session = sharing.author(SUBJECT, "Owl").await;
    let published = sharing.share(&session, prepared("Verbs 1")).await.json();
    sharing.clock.advance(SignedDuration::from_mins(1));
    sharing.reviewer.answer(Some(ReviewOutcome::Refused {
        reasons: vec![wrong_answer()],
    }));
    sharing.share(&session, prepared_again("Verbs 2")).await;

    let shares = sharing.shares(&session).await;

    assert_eq!(shares.status, StatusCode::OK, "{}", text(&shares));
    assert_eq!(shares.headers["cache-control"], "private, no-store");
    let shares = shares.json();
    assert_eq!(
        shares["sharing_terms"],
        json!({"current": TERMS, "accepted": TERMS})
    );
    assert_eq!(
        shares["quota"],
        json!({"shares_per_day": 10, "shared_today": 2, "next_share_at": null})
    );
    let listed = shares["shares"].as_array().unwrap();
    assert_eq!(listed.len(), 2);
    assert_eq!(listed[0]["title"], "Verbs 2");
    assert_eq!(listed[0]["outcome"], "refused");
    assert_eq!(listed[0]["shared_at"], "2026-11-27T10:01:00Z");
    assert_eq!(listed[0]["reasons"][0]["question"], 2);
    assert_eq!(listed[1], published);
}

#[tokio::test]
async fn the_first_share_needs_the_sharing_terms_accepted() {
    let sharing = with_sharing().await;
    let session = sharing.author(SUBJECT, "Owl").await;

    let without = sharing.share(&session, prepared_again("Verbs")).await;
    let mut outdated = prepared("Verbs");
    outdated["sharing_terms"] = json!("2020-01");
    let outdated = sharing.share(&session, outdated).await;

    for refused in [&without, &outdated] {
        assert_eq!(refused.status, StatusCode::FORBIDDEN, "{}", text(refused));
        assert_eq!(refused.json(), json!({"error": "sharing_terms_required"}));
    }
    assert!(sharing.reviewer.asked().is_empty());
    assert_eq!(
        sharing.shares(&session).await.json()["sharing_terms"],
        json!({"current": TERMS, "accepted": null})
    );

    let accepted = sharing.share(&session, prepared("Verbs")).await;
    assert_eq!(accepted.status, StatusCode::CREATED, "{}", text(&accepted));
    let again = sharing.share(&session, prepared_again("Verbs 2")).await;
    assert_eq!(again.status, StatusCode::CREATED, "{}", text(&again));
}

#[tokio::test]
async fn the_sharing_terms_can_be_read_and_accepted_on_the_website() {
    let sharing = with_sharing().await;
    let session = sharing.author(SUBJECT, "Owl").await;

    let terms = get(&sharing.library, "/api/sharing-terms").await;
    assert_eq!(
        terms.json(),
        json!({"version": TERMS, "shares_per_day": 10})
    );

    let outdated = with_session(
        &sharing.library,
        "PUT",
        "/api/account/sharing-terms",
        &session,
        Some(json!({"version": "2020-01"})),
    )
    .await;
    assert_eq!(outdated.status, StatusCode::CONFLICT);
    assert_eq!(outdated.json(), json!({"error": "outdated_sharing_terms"}));

    let accepted = with_session(
        &sharing.library,
        "PUT",
        "/api/account/sharing-terms",
        &session,
        Some(json!({"version": TERMS})),
    )
    .await;
    assert_eq!(accepted.status, StatusCode::OK, "{}", text(&accepted));
    assert_eq!(
        accepted.json(),
        json!({"current": TERMS, "accepted": TERMS})
    );
    let shared = sharing.share(&session, prepared_again("Verbs")).await;
    assert_eq!(shared.status, StatusCode::CREATED, "{}", text(&shared));
}

#[tokio::test]
async fn a_share_carrying_a_library_identifier_is_refused_untouched() {
    let sharing = with_sharing().await;
    let session = sharing.author(SUBJECT, "Owl").await;
    let mut body = prepared("Verbs");
    body["library_identifier"] = json!(FAMILY_A);

    let refused = sharing.share(&session, body).await;

    assert_eq!(refused.status, StatusCode::BAD_REQUEST);
    assert_eq!(
        refused.json(),
        json!({"error": "library_identifier_not_allowed"})
    );
    assert!(sharing.reviewer.asked().is_empty());
    assert!(
        tables_holding(&sharing.database, FAMILY_A).await.is_empty(),
        "the identifier is kept nowhere"
    );
}

#[tokio::test]
async fn sharing_needs_a_recent_strong_sign_in_an_account_and_a_verified_card() {
    let sharing = with_sharing().await;
    let library = &sharing.library;

    let signed_out = sharing.share(&"0".repeat(64), prepared("Verbs")).await;
    assert_eq!(signed_out.status, StatusCode::UNAUTHORIZED);
    assert_eq!(signed_out.json(), json!({"error": "signed_out"}));

    let no_account = session(
        library,
        &sharing.provider,
        passkey("subject-2", sharing.clock.now()),
    )
    .await;
    let refused = sharing.share(&no_account, prepared("Verbs")).await;
    assert_eq!(refused.status, StatusCode::NOT_FOUND);
    assert_eq!(refused.json(), json!({"error": "no_account"}));

    let uncarded = common::cards::account(
        library,
        &sharing.provider,
        &sharing.clock,
        "subject-3",
        "Lark",
        "separate",
    )
    .await;
    let refused = sharing.share(&uncarded, prepared("Verbs")).await;
    assert_eq!(refused.status, StatusCode::FORBIDDEN);
    assert_eq!(refused.json(), json!({"error": "card_required"}));

    let author = sharing.author(SUBJECT, "Owl").await;
    sharing.clock.advance(SignedDuration::from_mins(16));
    let refused = sharing.share(&author, prepared("Verbs")).await;
    assert_eq!(refused.status, StatusCode::FORBIDDEN);
    assert_eq!(refused.json(), json!({"error": "strong_sign_in_required"}));
    assert!(sharing.reviewer.asked().is_empty());
}

#[tokio::test]
async fn without_a_reviewer_sharing_is_unavailable() {
    let (library, provider, clock, payments, _) = common::cards::with_cards().await;
    payments.subscribe(SUBJECT, "fp");
    let session = common::cards::account(
        &library,
        &provider,
        &clock,
        SUBJECT,
        "Owl",
        "same_as_subscription",
    )
    .await;

    let refused = with_session(
        &library,
        "POST",
        "/api/shares",
        &session,
        Some(prepared("Verbs")),
    )
    .await;

    assert_eq!(refused.status, StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(refused.json(), json!({"error": "unavailable"}));
}

#[tokio::test]
async fn a_review_that_fails_shares_nothing_and_counts_nothing() {
    let sharing = with_sharing().await;
    let session = sharing.author(SUBJECT, "Owl").await;
    sharing.reviewer.answer(None);

    let failed = sharing.share(&session, prepared("Verbs")).await;

    assert_eq!(failed.status, StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(failed.json(), json!({"error": "unavailable"}));
    let shares = sharing.shares(&session).await.json();
    assert_eq!(shares["shares"], json!([]));
    assert_eq!(shares["quota"]["shared_today"], 0);
}

#[tokio::test]
async fn the_quota_of_shares_per_day_is_explained_when_reached() {
    let sharing = with_sharing().await;
    let session = sharing.author(SUBJECT, "Owl").await;
    sharing.reviewer.answer(Some(ReviewOutcome::Refused {
        reasons: vec![wrong_answer()],
    }));
    for n in 0..SHARES_PER_DAY {
        let body = if n == 0 {
            prepared("Verbs")
        } else {
            prepared_again("Verbs")
        };
        assert_eq!(sharing.share(&session, body).await.status, StatusCode::OK);
    }

    let refused = sharing.share(&session, prepared_again("Verbs")).await;

    assert_eq!(refused.status, StatusCode::TOO_MANY_REQUESTS);
    assert_eq!(
        refused.json(),
        json!({"error": "share_quota_reached", "shares_per_day": 10, "next_share_at": "2026-11-28T00:00:00Z"})
    );
    assert_eq!(sharing.reviewer.asked().len(), 10);
    assert_eq!(
        sharing.shares(&session).await.json()["quota"],
        json!({"shares_per_day": 10, "shared_today": 10, "next_share_at": "2026-11-28T00:00:00Z"})
    );

    // The next day, after signing in strongly again.
    sharing.clock.advance(SignedDuration::from_hours(14));
    let next_day = common::identity::session(
        &sharing.library,
        &sharing.provider,
        passkey(SUBJECT, sharing.clock.now()),
    )
    .await;
    let shared = sharing.share(&next_day, prepared_again("Verbs")).await;
    assert_eq!(shared.status, StatusCode::OK, "{}", text(&shared));
}

#[tokio::test]
async fn content_outside_the_bank_format_is_refused_before_the_review() {
    let sharing = with_sharing().await;
    let session = sharing.author(SUBJECT, "Owl").await;
    let mut not_a_bank = prepared("Verbs");
    not_a_bank["content"] = json!("a text");
    let no_title = prepared("  ");
    let mut ages = prepared("Verbs");
    ages["metadata"]["ages"] = json!({"min": 12, "max": 9});
    let mut unknown_field = prepared("Verbs");
    unknown_field["author"] = json!({"pseudonym": "Someone else"});

    let refusals = [
        (not_a_bank, "bad_content"),
        (no_title, "bad_item"),
        (ages, "bad_item"),
        (unknown_field, "bad_request"),
    ];
    for (body, error) in refusals {
        let refused = sharing.share(&session, body).await;
        assert_eq!(refused.status, StatusCode::BAD_REQUEST, "{error}");
        assert_eq!(refused.json(), json!({"error": error}));
    }
    assert!(sharing.reviewer.asked().is_empty());
}

#[tokio::test]
async fn a_shared_items_media_are_published_with_it() {
    let sharing = with_sharing().await;
    let session = sharing.author(SUBJECT, "Owl").await;
    let mut body = prepared("Verbs");
    body["media"] = json!([{"base64": base64(&pixel())}]);

    let shared = sharing.share(&session, body).await;

    assert_eq!(shared.status, StatusCode::CREATED, "{}", text(&shared));
    let item = shared.json()["item"].as_str().unwrap().to_string();
    let version = fetch_verified(
        &sharing.library,
        &format!("/v1/items/{item}/versions/1"),
        Kind::Item,
    )
    .await;
    assert_eq!(
        version["media"],
        json!([{"digest": PIXEL, "type": "image/png", "size": 70}])
    );

    let mut text_file = prepared_again("Notes");
    text_file["media"] = json!([{"base64": base64(b"just text")}]);
    let refused = sharing.share(&session, text_file).await;
    assert_eq!(refused.status, StatusCode::BAD_REQUEST);
    assert_eq!(refused.json(), json!({"error": "unsupported_medium"}));
}

fn base64(bytes: &[u8]) -> String {
    use base64::Engine;
    base64::engine::general_purpose::STANDARD.encode(bytes)
}

#[tokio::test]
async fn a_share_is_published_under_the_pseudonym_the_account_has_then() {
    let sharing = with_sharing().await;
    let session = sharing.author(SUBJECT, "Owl").await;
    with_session(
        &sharing.library,
        "PUT",
        "/api/account/pseudonym",
        &session,
        Some(json!({"pseudonym": "Barn owl"})),
    )
    .await;

    sharing.share(&session, prepared("Verbs")).await;

    let index = fetch_verified(&sharing.library, "/v1/index", Kind::Index).await;
    assert_eq!(index["items"][0]["author"]["pseudonym"], "Barn owl");
}

#[tokio::test]
async fn a_closed_accounts_identification_is_kept_for_the_legal_period_after_its_last_share() {
    let sharing = with_sharing().await;
    let session = sharing.author(SUBJECT, "Owl").await;
    sharing.clock.advance(SignedDuration::from_mins(5));
    sharing.share(&session, prepared("Verbs")).await;
    let closed = with_session(&sharing.library, "DELETE", "/api/account", &session, None).await;
    assert_eq!(closed.status, StatusCode::NO_CONTENT);
    let year = library::IDENTIFICATION_RETENTION;
    let legal_end = at("2026-11-27T10:05:00Z").checked_add(year).unwrap();

    let just_before = legal_end.checked_sub(SignedDuration::from_secs(1)).unwrap();
    sharing
        .library
        .erase_identifications(just_before, year)
        .await
        .unwrap();
    assert_eq!(
        tables_holding(&sharing.database, EMAIL).await,
        vec!["identifications"]
    );

    sharing
        .library
        .erase_identifications(legal_end, year)
        .await
        .unwrap();
    assert!(tables_holding(&sharing.database, EMAIL).await.is_empty());
}
