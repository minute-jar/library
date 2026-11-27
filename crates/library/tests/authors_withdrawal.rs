//! The author's withdrawal (story S17, `docs/specs/interface-v1.md`,
//! Shares): the author of a shared item withdraws it, or one of its
//! versions, from their library account; it leaves the index and the
//! catalogue and enters the withdrawal list with the author's kind.

mod common;

use axum::http::StatusCode;
use common::cards::text;
use common::identity::*;
use common::shares::*;
use common::*;
use library::contract::Kind;
use library::{Reason, WithdrawError, Withdrawal};
use serde_json::{Value, json};

/// Shares a bank the review publishes; answers its item.
async fn shared(sharing: &Sharing, session: &str, title: &str) -> String {
    let shared = sharing.share(session, prepared(title)).await;
    assert_eq!(shared.status, StatusCode::CREATED, "{}", text(&shared));
    shared.json()["item"].as_str().unwrap().to_string()
}

async fn withdraw(sharing: &Sharing, session: &str, item: &str, body: Value) -> Answer {
    with_session(
        &sharing.library,
        "POST",
        &format!("/api/account/items/{item}/withdrawal"),
        session,
        Some(body),
    )
    .await
}

#[tokio::test]
async fn the_author_withdraws_an_item_which_leaves_the_index_for_the_withdrawal_list() {
    let sharing = with_sharing().await;
    let session = sharing.author(SUBJECT, "Owl").await;
    let item = shared(&sharing, &session, "Irregular verbs").await;

    let withdrawn = withdraw(&sharing, &session, &item, json!({"version": null})).await;

    assert_eq!(withdrawn.status, StatusCode::OK, "{}", text(&withdrawn));
    assert_eq!(
        withdrawn.json(),
        json!({"item": item, "version": null, "kind": "author", "reason": "author_choice", "withdrawn_at": "2026-11-27T10:00:00Z"})
    );
    let list = fetch_verified(&sharing.library, "/v1/withdrawals", Kind::Withdrawals).await;
    assert_eq!(
        list["withdrawals"],
        json!([{"item": item, "version": null, "kind": "author", "reason": "author_choice", "withdrawn_at": "2026-11-27T10:00:00Z"}])
    );
    let index = fetch_verified(&sharing.library, "/v1/index", Kind::Index).await;
    assert_eq!(index["items"], json!([]));
    let page = get(&sharing.library, &format!("/api/items/{item}")).await;
    assert_eq!(page.status, StatusCode::NOT_FOUND);
    let catalogue = get(&sharing.library, "/api/catalogue").await.json();
    assert_eq!(catalogue["items"], json!([]));
}

/// Publishes version 2 of `item`, as a new version of the author's would.
async fn second_version(sharing: &Sharing, item: &str) {
    let mut version = verbs("Irregular verbs, part 2");
    version.item = item.into();
    sharing.clock.advance(jiff::SignedDuration::from_mins(5));
    sharing
        .library
        .publish_version(version, sharing.clock.now())
        .await
        .unwrap();
}

#[tokio::test]
async fn the_author_withdraws_one_version_and_the_one_before_takes_its_place() {
    let sharing = with_sharing().await;
    let session = sharing.author(SUBJECT, "Owl").await;
    let item = shared(&sharing, &session, "Irregular verbs").await;
    second_version(&sharing, &item).await;

    let withdrawn = withdraw(&sharing, &session, &item, json!({"version": 2})).await;

    assert_eq!(withdrawn.status, StatusCode::OK, "{}", text(&withdrawn));
    assert_eq!(withdrawn.json()["version"], 2);
    let list = fetch_verified(&sharing.library, "/v1/withdrawals", Kind::Withdrawals).await;
    assert_eq!(
        list["withdrawals"],
        json!([{"item": item, "version": 2, "kind": "author", "reason": "author_choice", "withdrawn_at": "2026-11-27T10:05:00Z"}])
    );
    let index = fetch_verified(&sharing.library, "/v1/index", Kind::Index).await;
    assert_eq!(index["items"][0]["item"], item.as_str());
    assert_eq!(index["items"][0]["version"], 1);
    assert_eq!(index["items"][0]["title"], "Irregular verbs");
}

#[tokio::test]
async fn another_account_cannot_withdraw_the_item() {
    let sharing = with_sharing().await;
    let owl = sharing.author(SUBJECT, "Owl").await;
    let item = shared(&sharing, &owl, "Irregular verbs").await;
    let lark = sharing.author("subject-2", "Lark").await;

    let refused = withdraw(&sharing, &lark, &item, json!({"version": null})).await;

    assert_eq!(refused.status, StatusCode::NOT_FOUND);
    assert_eq!(refused.json(), json!({"error": "unknown_item"}));
    let unknown = withdraw(&sharing, &lark, &"0".repeat(32), json!({"version": null})).await;
    assert_eq!(unknown.json(), json!({"error": "unknown_item"}));
    let list = fetch_verified(&sharing.library, "/v1/withdrawals", Kind::Withdrawals).await;
    assert_eq!(list["withdrawals"], json!([]));
    let index = fetch_verified(&sharing.library, "/v1/index", Kind::Index).await;
    assert_eq!(index["items"][0]["item"], item.as_str());
}

#[tokio::test]
async fn a_withdrawal_needs_a_recent_strong_sign_in() {
    let sharing = with_sharing().await;
    let session = sharing.author(SUBJECT, "Owl").await;
    let item = shared(&sharing, &session, "Irregular verbs").await;

    let signed_out = withdraw(&sharing, &"0".repeat(64), &item, json!({"version": null})).await;
    assert_eq!(signed_out.status, StatusCode::UNAUTHORIZED);
    assert_eq!(signed_out.json(), json!({"error": "signed_out"}));

    sharing.clock.advance(jiff::SignedDuration::from_mins(16));
    let refused = withdraw(&sharing, &session, &item, json!({"version": null})).await;

    assert_eq!(refused.status, StatusCode::FORBIDDEN);
    assert_eq!(refused.json(), json!({"error": "strong_sign_in_required"}));
    let list = fetch_verified(&sharing.library, "/v1/withdrawals", Kind::Withdrawals).await;
    assert_eq!(list["withdrawals"], json!([]));
}

#[tokio::test]
async fn an_unknown_version_or_a_second_withdrawal_is_refused() {
    let sharing = with_sharing().await;
    let session = sharing.author(SUBJECT, "Owl").await;
    let item = shared(&sharing, &session, "Irregular verbs").await;

    let unknown = withdraw(&sharing, &session, &item, json!({"version": 2})).await;
    assert_eq!(unknown.status, StatusCode::NOT_FOUND);
    assert_eq!(unknown.json(), json!({"error": "unknown_version"}));
    let bad = withdraw(&sharing, &session, &item, json!({"versions": 1})).await;
    assert_eq!(bad.status, StatusCode::BAD_REQUEST);

    withdraw(&sharing, &session, &item, json!({"version": null})).await;
    sharing.clock.advance(jiff::SignedDuration::from_mins(1));
    let again = withdraw(&sharing, &session, &item, json!({"version": 1})).await;

    assert_eq!(again.status, StatusCode::CONFLICT);
    assert_eq!(again.json(), json!({"error": "already_withdrawn"}));
    let list = fetch_verified(&sharing.library, "/v1/withdrawals", Kind::Withdrawals).await;
    assert_eq!(
        list["withdrawals"][0]["withdrawn_at"],
        "2026-11-27T10:00:00Z"
    );
    assert_eq!(list["withdrawals"].as_array().unwrap().len(), 1);
}

#[tokio::test]
async fn a_withdrawal_by_the_author_cannot_be_undone_by_a_reactivation() {
    let sharing = with_sharing().await;
    let library = &sharing.library;
    let session = sharing.author(SUBJECT, "Owl").await;
    let item = shared(&sharing, &session, "Irregular verbs").await;
    let other = shared(&sharing, &session, "Fractions").await;
    let now = sharing.clock.now();

    // Withdrawn by the author, then switched off by a report.
    withdraw(&sharing, &session, &item, json!({"version": null})).await;
    library
        .withdraw(
            &item,
            None,
            Withdrawal::SwitchOff(Reason::PersonalData),
            now,
        )
        .await
        .unwrap();
    // Switched off, then withdrawn by the author.
    library
        .withdraw(
            &other,
            None,
            Withdrawal::SwitchOff(Reason::PersonalData),
            now,
        )
        .await
        .unwrap();
    withdraw(&sharing, &session, &other, json!({"version": null})).await;

    for withdrawn in [&item, &other] {
        let reactivated = library.reactivate(withdrawn, None, now).await;
        assert!(
            matches!(reactivated, Err(WithdrawError::NotSwitchedOff)),
            "{reactivated:?}"
        );
    }
    // Nor replaced by a moderation decision, which an appeal could reverse.
    library
        .withdraw(
            &item,
            None,
            Withdrawal::Moderation(Reason::PersonalData),
            now,
        )
        .await
        .unwrap();

    let list = fetch_verified(library, "/v1/withdrawals", Kind::Withdrawals).await;
    let kinds: Vec<_> = list["withdrawals"]
        .as_array()
        .unwrap()
        .iter()
        .map(|entry| (entry["kind"].clone(), entry["reason"].clone()))
        .collect();
    assert_eq!(kinds, vec![(json!("author"), json!("author_choice")); 2]);
    let index = fetch_verified(library, "/v1/index", Kind::Index).await;
    assert_eq!(index["items"], json!([]));
}

#[tokio::test]
async fn the_author_reads_their_items_their_versions_and_what_withdrew_them() {
    let sharing = with_sharing().await;
    let session = sharing.author(SUBJECT, "Owl").await;
    let item = shared(&sharing, &session, "Irregular verbs").await;
    second_version(&sharing, &item).await;
    withdraw(&sharing, &session, &item, json!({"version": 1})).await;
    let lark = sharing.author("subject-2", "Lark").await;
    shared(&sharing, &lark, "Fractions").await;

    let items = with_session(
        &sharing.library,
        "GET",
        "/api/account/items",
        &session,
        None,
    )
    .await;

    assert_eq!(items.status, StatusCode::OK, "{}", text(&items));
    assert_eq!(
        items.json(),
        json!({"items": [{
            "item": item,
            "title": "Irregular verbs, part 2",
            "withdrawal": null,
            "versions": [
                {"version": 2, "published_at": "2026-11-27T10:05:00Z", "withdrawal": null},
                {"version": 1, "published_at": "2026-11-27T10:00:00Z", "withdrawal":
                    {"kind": "author", "reason": "author_choice", "withdrawn_at": "2026-11-27T10:05:00Z"}}
            ]
        }]})
    );
    assert_eq!(
        items.headers.get("cache-control").unwrap(),
        "private, no-store"
    );
}
