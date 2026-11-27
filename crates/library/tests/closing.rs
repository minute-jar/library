//! Closing a library account (story S12, `docs/specs/accounts-api.md`): its
//! items stay published under their licence with its pseudonym, which stays
//! reserved; its sessions end at once; its identification data is kept for
//! the legal period, then erased.

mod common;

use axum::http::StatusCode;
use common::identity::*;
use common::*;
use serde_json::json;

async fn close(library: &library::Library, session: &str) -> Answer {
    with_session(library, "DELETE", "/api/account", session, None).await
}

#[tokio::test]
async fn closing_ends_every_session_of_the_account_at_once() {
    let (library, provider, _) = with_accounts().await;
    let other_device = session(&library, &provider, email_code(SUBJECT, signed_in_at())).await;
    let session = session(&library, &provider, passkey(SUBJECT, signed_in_at())).await;
    open_account(&library, &session, "Owl").await;

    let closed = close(&library, &session).await;

    assert_eq!(closed.status, StatusCode::NO_CONTENT);
    assert_eq!(closed.headers["cache-control"], "private, no-store");
    for session in [&session, &other_device] {
        let after = with_session(&library, "GET", "/api/session", session, None).await;
        assert_eq!(after.status, StatusCode::UNAUTHORIZED);
        assert_eq!(after.json(), json!({"error": "signed_out"}));
    }
}

#[tokio::test]
async fn closing_needs_a_recent_strong_sign_in() {
    let (library, provider, clock) = with_accounts().await;
    let strong = session(&library, &provider, passkey(SUBJECT, signed_in_at())).await;
    open_account(&library, &strong, "Owl").await;
    let weak = session(&library, &provider, email_code(SUBJECT, signed_in_at())).await;

    let by_weak = close(&library, &weak).await;
    clock.advance(jiff::SignedDuration::from_mins(16));
    let by_old = close(&library, &strong).await;

    for refused in [by_weak, by_old] {
        assert_eq!(refused.status, StatusCode::FORBIDDEN);
        assert_eq!(refused.json(), json!({"error": "strong_sign_in_required"}));
    }
    let account = with_session(&library, "GET", "/api/account", &weak, None).await;
    assert_eq!(account.json()["pseudonym"], "Owl");
}

#[tokio::test]
async fn without_a_library_account_there_is_nothing_to_close() {
    let (library, provider, _) = with_accounts().await;
    let session = session(&library, &provider, passkey(SUBJECT, signed_in_at())).await;

    let answer = close(&library, &session).await;

    assert_eq!(answer.status, StatusCode::NOT_FOUND);
    assert_eq!(answer.json(), json!({"error": "no_account"}));
}

#[tokio::test]
async fn the_pseudonym_of_a_closed_account_stays_reserved() {
    let (library, provider, _) = with_accounts().await;
    let owl = session(&library, &provider, passkey(SUBJECT, signed_in_at())).await;
    open_account(&library, &owl, "Owl").await;
    let lark = session(&library, &provider, passkey("subject-2", signed_in_at())).await;
    open_account(&library, &lark, "Lark").await;

    close(&library, &owl).await;
    let other = session(&library, &provider, passkey("subject-3", signed_in_at())).await;
    let opening = open_account(&library, &other, "OWL").await;
    let renaming = with_session(
        &library,
        "PUT",
        "/api/account/pseudonym",
        &lark,
        Some(json!({"pseudonym": "owl"})),
    )
    .await;

    for refused in [opening, renaming] {
        assert_eq!(refused.status, StatusCode::CONFLICT);
        assert_eq!(refused.json(), json!({"error": "pseudonym_taken"}));
    }
}

#[tokio::test]
async fn the_next_sign_in_finds_no_account_and_may_open_another() {
    let (library, provider, clock) = with_accounts().await;
    let first = session(&library, &provider, passkey(SUBJECT, signed_in_at())).await;
    open_account(&library, &first, "Owl").await;
    close(&library, &first).await;
    clock.advance(jiff::SignedDuration::from_hours(1));

    let again = session(&library, &provider, passkey(SUBJECT, clock.now())).await;
    let current = with_session(&library, "GET", "/api/session", &again, None).await;
    let reopened = open_account(&library, &again, "Barn owl").await;

    assert_eq!(current.json()["account"], json!(null));
    assert_eq!(reopened.status, StatusCode::CREATED);
    assert_eq!(reopened.json()["pseudonym"], "Barn owl");
}

#[tokio::test]
async fn items_stay_published_under_their_licence_with_the_pseudonym() {
    let (library, provider, _) = with_accounts().await;
    let session = session(&library, &provider, passkey(SUBJECT, signed_in_at())).await;
    open_account(&library, &session, "Owl").await;
    let mut shared = verbs("Irregular verbs");
    shared.author.pseudonym = "Owl".into();
    library
        .publish_version(shared, at("2026-11-27T10:05:00Z"))
        .await
        .unwrap();
    let before = get(&library, &format!("/v1/items/{VERBS}/versions/1")).await;

    close(&library, &session).await;
    library.refresh(at("2026-11-27T11:00:00Z")).await.unwrap();

    let after = get(&library, &format!("/v1/items/{VERBS}/versions/1")).await;
    assert_eq!(after.status, StatusCode::OK);
    assert_eq!(after.body, before.body, "the signed version is unchanged");
    let index = fetch_verified(&library, "/v1/index", library::contract::Kind::Index).await;
    assert_eq!(index["items"][0]["item"], VERBS);
    assert_eq!(index["items"][0]["licence"], "CC-BY-SA-4.0");
    assert_eq!(index["items"][0]["author"]["pseudonym"], "Owl");
    let withdrawals = fetch_verified(
        &library,
        "/v1/withdrawals",
        library::contract::Kind::Withdrawals,
    )
    .await;
    assert_eq!(withdrawals["withdrawals"], json!([]));
}

/// Records a share by the account `pseudonym` at `when`, as sharing
/// (ticket 12) will.
async fn shared(database: &str, pseudonym: &str, when: jiff::Timestamp) {
    let pool = sqlx::PgPool::connect(database).await.unwrap();
    sqlx::query(
        "UPDATE identifications SET last_shared_at = $2
         FROM library_accounts AS account
         WHERE account.id = identifications.account AND account.pseudonym = $1",
    )
    .bind(pseudonym)
    .bind(when.as_second())
    .execute(&pool)
    .await
    .unwrap();
    pool.close().await;
}

#[tokio::test]
async fn a_closed_account_keeps_no_minute_jar_account() {
    let (library, provider, _, database) = with_accounts_and_database().await;
    let session = session(&library, &provider, passkey(SUBJECT, signed_in_at())).await;
    open_account(&library, &session, "Owl").await;

    close(&library, &session).await;

    assert!(tables_holding(&database, SUBJECT).await.is_empty());
    assert_eq!(tables_holding(&database, EMAIL).await, ["identifications"]);
}

#[tokio::test]
async fn identification_data_is_kept_a_year_after_the_last_share_then_erased() {
    let (library, provider, _, database) = with_accounts_and_database().await;
    let session = session(&library, &provider, passkey(SUBJECT, signed_in_at())).await;
    open_account(&library, &session, "Owl").await;
    shared(&database, "Owl", at("2026-11-27T10:05:00Z")).await;
    close(&library, &session).await;

    library
        .erase_identifications(
            at("2027-11-27T10:04:59Z"),
            library::IDENTIFICATION_RETENTION,
        )
        .await
        .unwrap();
    assert_eq!(tables_holding(&database, EMAIL).await, ["identifications"]);

    library
        .erase_identifications(
            at("2027-11-27T10:05:00Z"),
            library::IDENTIFICATION_RETENTION,
        )
        .await
        .unwrap();
    assert!(tables_holding(&database, EMAIL).await.is_empty());
    let other =
        common::identity::session(&library, &provider, passkey("subject-2", signed_in_at())).await;
    assert_eq!(
        open_account(&library, &other, "Owl").await.json(),
        json!({"error": "pseudonym_taken"}),
        "the pseudonym stays reserved once its identification is erased"
    );
}

#[tokio::test]
async fn the_identification_of_an_open_account_is_never_erased() {
    let (library, provider, _, database) = with_accounts_and_database().await;
    let session = session(&library, &provider, passkey(SUBJECT, signed_in_at())).await;
    open_account(&library, &session, "Owl").await;
    shared(&database, "Owl", at("2026-11-27T10:05:00Z")).await;

    library
        .erase_identifications(
            at("2030-01-01T00:00:00Z"),
            library::IDENTIFICATION_RETENTION,
        )
        .await
        .unwrap();

    assert_eq!(tables_holding(&database, EMAIL).await, ["identifications"]);
}

#[tokio::test]
async fn a_closed_account_that_never_shared_has_its_identification_erased_at_once() {
    let (library, provider, _, database) = with_accounts_and_database().await;
    let session = session(&library, &provider, passkey(SUBJECT, signed_in_at())).await;
    open_account(&library, &session, "Owl").await;
    close(&library, &session).await;

    library
        .erase_identifications(
            at("2026-11-27T10:01:00Z"),
            library::IDENTIFICATION_RETENTION,
        )
        .await
        .unwrap();

    assert!(tables_holding(&database, EMAIL).await.is_empty());
}

#[tokio::test]
async fn the_retention_period_is_set_by_the_caller() {
    let (library, provider, _, database) = with_accounts_and_database().await;
    let session = session(&library, &provider, passkey(SUBJECT, signed_in_at())).await;
    open_account(&library, &session, "Owl").await;
    shared(&database, "Owl", at("2026-11-27T10:05:00Z")).await;
    close(&library, &session).await;

    library
        .erase_identifications(
            at("2026-12-27T10:05:00Z"),
            jiff::SignedDuration::from_hours(30 * 24),
        )
        .await
        .unwrap();

    assert!(tables_holding(&database, EMAIL).await.is_empty());
}

#[tokio::test]
async fn closing_ends_a_pending_card_check() {
    let (library, provider, clock, _, database) = common::cards::with_cards().await;
    let session =
        common::cards::account(&library, &provider, &clock, SUBJECT, "Owl", "separate").await;
    let started = common::cards::verify_card(&library, &session).await;
    assert_eq!(started.status, StatusCode::ACCEPTED);

    close(&library, &session).await;

    assert!(tables_holding(&database, "seti_1").await.is_empty());
}

#[tokio::test]
async fn the_card_of_a_closed_account_is_kept_and_erased_with_its_identification() {
    let (library, provider, clock, payments, database) = common::cards::with_cards().await;
    let session =
        common::cards::account(&library, &provider, &clock, SUBJECT, "Owl", "separate").await;
    common::cards::verify_card(&library, &session).await;
    payments.verify("seti_1", common::cards::FINGERPRINT);
    common::cards::finish_check(&library, &session).await;
    shared(&database, "Owl", at("2026-11-27T10:05:00Z")).await;
    close(&library, &session).await;

    library
        .erase_identifications(
            at("2027-11-27T10:04:59Z"),
            library::IDENTIFICATION_RETENTION,
        )
        .await
        .unwrap();
    assert_eq!(
        tables_holding(&database, common::cards::FINGERPRINT).await,
        ["identification_cards"]
    );

    library
        .erase_identifications(
            at("2027-11-27T10:05:00Z"),
            library::IDENTIFICATION_RETENTION,
        )
        .await
        .unwrap();
    assert!(
        tables_holding(&database, common::cards::FINGERPRINT)
            .await
            .is_empty()
    );
    assert!(tables_holding(&database, EMAIL).await.is_empty());
}

#[tokio::test]
async fn the_card_routes_refuse_a_closed_account() {
    let (library, provider, clock, _, _) = common::cards::with_cards().await;
    let first =
        common::cards::account(&library, &provider, &clock, SUBJECT, "Owl", "separate").await;
    close(&library, &first).await;
    let again = session(&library, &provider, passkey(SUBJECT, clock.now())).await;

    for (method, path) in [
        ("GET", "/api/account/card"),
        ("POST", "/api/account/card"),
        ("POST", "/api/account/card/check"),
    ] {
        let by_old = with_session(&library, method, path, &first, None).await;
        assert_eq!(by_old.status, StatusCode::UNAUTHORIZED, "{method} {path}");
        assert_eq!(by_old.json(), json!({"error": "signed_out"}));
        let by_new = with_session(&library, method, path, &again, None).await;
        assert_eq!(by_new.status, StatusCode::NOT_FOUND, "{method} {path}");
        assert_eq!(by_new.json(), json!({"error": "no_account"}));
    }
}
