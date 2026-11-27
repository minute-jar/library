//! The verified card of a library account (stories S7 and S8, ADR-0007),
//! over a fake payment provider.

mod common;

use axum::http::StatusCode;
use common::cards::*;
use common::identity::*;
use serde_json::json;

#[tokio::test]
async fn a_parent_without_a_subscription_verifies_a_card_by_a_zero_amount_check() {
    let (library, provider, clock, payments, _) = with_cards().await;
    let session = account(&library, &provider, &clock, SUBJECT, "Owl", "separate").await;

    let started = verify_card(&library, &session).await;

    assert_eq!(started.status, StatusCode::ACCEPTED, "{}", text(&started));
    assert_eq!(
        started.json(),
        json!({"check_url": "https://pay.example/setup/seti_1"})
    );
    assert_eq!(card(&library, &session).await.status, StatusCode::NOT_FOUND);

    payments.verify("seti_1", FINGERPRINT);
    clock.advance(jiff::SignedDuration::from_mins(2));
    let finished = finish_check(&library, &session).await;

    let verified = json!({"verified_by": "check", "verified_at": "2026-11-27T10:02:00Z"});
    assert_eq!(finished.status, StatusCode::OK, "{}", text(&finished));
    assert_eq!(finished.json(), json!({"card": verified}));
    assert_eq!(card(&library, &session).await.json(), verified);
}

#[tokio::test]
async fn a_subscriber_on_the_subscriptions_minute_jar_account_uses_its_card() {
    let (library, provider, clock, payments, _) = with_cards().await;
    payments.subscribe(SUBJECT, FINGERPRINT);
    let session = account(
        &library,
        &provider,
        &clock,
        SUBJECT,
        "Owl",
        "same_as_subscription",
    )
    .await;

    let verified = verify_card(&library, &session).await;

    let card = json!({"verified_by": "subscription", "verified_at": "2026-11-27T10:00:00Z"});
    assert_eq!(verified.status, StatusCode::OK, "{}", text(&verified));
    assert_eq!(verified.json(), json!({"card": card}));
    assert!(payments.started().is_empty(), "no check is needed");
    assert_eq!(card_answer(&library, &session).await, card);
}

#[tokio::test]
async fn without_a_subscription_or_on_a_separate_account_a_check_is_started() {
    let (library, provider, clock, payments, _) = with_cards().await;
    payments.subscribe("subject-separate", FINGERPRINT);
    let unsubscribed = account(
        &library,
        &provider,
        &clock,
        "subject-unsubscribed",
        "Owl",
        "same_as_subscription",
    )
    .await;
    let separate = account(
        &library,
        &provider,
        &clock,
        "subject-separate",
        "Lark",
        "separate",
    )
    .await;

    let first = verify_card(&library, &unsubscribed).await;
    let second = verify_card(&library, &separate).await;

    assert_eq!(first.status, StatusCode::ACCEPTED);
    assert_eq!(
        second.status,
        StatusCode::ACCEPTED,
        "a separate account is never joined to a subscription"
    );
    assert_eq!(payments.started(), ["seti_1", "seti_2"]);
}

#[tokio::test]
async fn one_card_may_serve_several_accounts_and_the_operator_knows_it() {
    let (library, provider, clock, payments, _) = with_cards().await;
    payments.subscribe(SUBJECT, FINGERPRINT);
    let owl = account(
        &library,
        &provider,
        &clock,
        SUBJECT,
        "Owl",
        "same_as_subscription",
    )
    .await;
    let lark = account(&library, &provider, &clock, "subject-2", "Lark", "separate").await;
    let wren = account(&library, &provider, &clock, "subject-3", "Wren", "separate").await;
    verify_card(&library, &owl).await;
    verify_card(&library, &lark).await;
    verify_card(&library, &wren).await;
    payments.verify("seti_1", FINGERPRINT);
    payments.verify("seti_2", "fp_another_card");

    let lark_finished = finish_check(&library, &lark).await;
    let wren_finished = finish_check(&library, &wren).await;

    assert_eq!(
        lark_finished.status,
        StatusCode::OK,
        "{}",
        text(&lark_finished)
    );
    assert_eq!(wren_finished.status, StatusCode::OK);
    assert_eq!(
        library.accounts_sharing_card("Owl").await.unwrap(),
        ["Lark"]
    );
    assert_eq!(
        library.accounts_sharing_card("lark").await.unwrap(),
        ["Owl"]
    );
    assert!(
        library
            .accounts_sharing_card("Wren")
            .await
            .unwrap()
            .is_empty()
    );
    assert!(
        library
            .accounts_sharing_card("Nobody")
            .await
            .unwrap()
            .is_empty()
    );
}

#[tokio::test]
async fn a_check_not_finished_or_refused_verifies_nothing_and_can_be_started_again() {
    let (library, provider, clock, payments, _) = with_cards().await;
    let session = account(&library, &provider, &clock, SUBJECT, "Owl", "separate").await;
    assert_eq!(
        finish_check(&library, &session).await.json(),
        json!({"error": "no_card_check"})
    );
    verify_card(&library, &session).await;

    let pending = finish_check(&library, &session).await;
    payments.refuse("seti_1");
    let refused = finish_check(&library, &session).await;
    let after = finish_check(&library, &session).await;

    assert_eq!(pending.status, StatusCode::CONFLICT);
    assert_eq!(pending.json(), json!({"error": "card_check_pending"}));
    assert_eq!(refused.status, StatusCode::BAD_REQUEST);
    assert_eq!(refused.json(), json!({"error": "card_refused"}));
    assert_eq!(after.status, StatusCode::NOT_FOUND);
    assert_eq!(
        card(&library, &session).await.json(),
        json!({"error": "no_card"})
    );

    let again = verify_card(&library, &session).await;
    payments.verify("seti_2", FINGERPRINT);
    assert_eq!(again.status, StatusCode::ACCEPTED);
    assert_eq!(
        finish_check(&library, &session).await.status,
        StatusCode::OK
    );
}

#[tokio::test]
async fn a_new_check_replaces_the_one_left_unfinished() {
    let (library, provider, clock, payments, _) = with_cards().await;
    let session = account(&library, &provider, &clock, SUBJECT, "Owl", "separate").await;
    verify_card(&library, &session).await;
    verify_card(&library, &session).await;
    payments.verify("seti_1", "fp_abandoned");

    let finished = finish_check(&library, &session).await;

    assert_eq!(finished.json(), json!({"error": "card_check_pending"}));
}

#[tokio::test]
async fn verifying_needs_a_recent_strong_sign_in_and_a_library_account_and_happens_once() {
    let (library, provider, clock, payments, _) = with_cards().await;
    let without_account = session(&library, &provider, passkey("subject-2", clock.now())).await;
    let session = account(&library, &provider, &clock, SUBJECT, "Owl", "separate").await;
    let no_account = verify_card(&library, &without_account).await;
    clock.advance(jiff::SignedDuration::from_mins(16));

    let not_recent = verify_card(&library, &session).await;

    assert_eq!(no_account.status, StatusCode::NOT_FOUND);
    assert_eq!(no_account.json(), json!({"error": "no_account"}));
    assert_eq!(not_recent.status, StatusCode::FORBIDDEN);
    assert_eq!(
        not_recent.json(),
        json!({"error": "strong_sign_in_required"})
    );
    assert!(payments.started().is_empty());

    let again = session_of(&library, &provider, &clock).await;
    verify_card(&library, &again).await;
    payments.verify("seti_1", FINGERPRINT);
    finish_check(&library, &again).await;
    let twice = verify_card(&library, &again).await;
    assert_eq!(twice.status, StatusCode::CONFLICT);
    assert_eq!(twice.json(), json!({"error": "card_verified"}));
    assert_eq!(
        verify_card(&library, "not-a-session").await.status,
        StatusCode::UNAUTHORIZED
    );
}

async fn session_of(library: &library::Library, provider: &FakeProvider, clock: &Clock) -> String {
    session(library, provider, passkey(SUBJECT, clock.now())).await
}

#[tokio::test]
async fn cards_are_unavailable_until_a_verifier_is_given_or_when_it_cannot_be_reached() {
    let (library, provider, clock) = with_accounts().await;
    let session = session(&library, &provider, passkey(SUBJECT, clock.now())).await;
    open_account(&library, &session, "Owl").await;
    for answer in [
        verify_card(&library, &session).await,
        finish_check(&library, &session).await,
        card(&library, &session).await,
    ] {
        assert_eq!(answer.status, StatusCode::SERVICE_UNAVAILABLE);
        assert_eq!(answer.json(), json!({"error": "unavailable"}));
    }

    let (library, provider, clock, payments, _) = with_cards().await;
    let session = account(&library, &provider, &clock, SUBJECT, "Owl", "separate").await;
    payments.set_unreachable();
    let unreachable = verify_card(&library, &session).await;
    assert_eq!(unreachable.status, StatusCode::SERVICE_UNAVAILABLE);
}

#[tokio::test]
async fn the_reference_and_fingerprint_are_kept_as_identification_data_and_never_answered() {
    let (library, provider, clock, payments, database) = with_cards().await;
    payments.subscribe(SUBJECT, FINGERPRINT);
    let owl = account(
        &library,
        &provider,
        &clock,
        SUBJECT,
        "Owl",
        "same_as_subscription",
    )
    .await;
    let lark = account(&library, &provider, &clock, "subject-2", "Lark", "separate").await;
    let mut answers = vec![
        verify_card(&library, &owl).await,
        verify_card(&library, &lark).await,
    ];
    payments.verify("seti_1", FINGERPRINT);
    for session in [&owl, &lark] {
        answers.push(finish_check(&library, session).await);
        answers.push(card(&library, session).await);
        answers.push(with_session(&library, "GET", "/api/session", session, None).await);
        answers.push(with_session(&library, "GET", "/api/account", session, None).await);
    }

    for answer in &answers {
        let body = text(answer);
        for secret in [FINGERPRINT, "pm_of_seti_1", "sub_of_"] {
            assert!(!body.contains(secret), "{body} answers {secret}");
        }
        assert_eq!(answer.headers["cache-control"], "private, no-store");
    }
    assert_eq!(
        tables_holding(&database, FINGERPRINT).await,
        ["identification_cards"]
    );
    assert_eq!(
        tables_holding(&database, "pm_of_seti_1").await,
        ["identification_cards"]
    );
    assert_eq!(tables_holding(&database, EMAIL).await, ["identifications"]);
}
