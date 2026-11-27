//! The signed withdrawal list, and what a withdrawal does to the index and
//! the shared items (`docs/specs/interface-v1.md`, Withdrawals).

mod common;

use common::*;
use library::contract::{Kind, Refusal};
use library::{Reason, WithdrawError, Withdrawal};
use serde_json::json;

#[tokio::test]
async fn an_empty_catalogue_serves_a_signed_empty_withdrawal_list() {
    let library = library().await;

    let withdrawals = fetch_verified(&library, "/v1/withdrawals", Kind::Withdrawals).await;

    assert_eq!(
        withdrawals,
        json!({
            "contract": 1,
            "kind": "withdrawals",
            "issued_at": "2026-11-26T00:00:00Z",
            "expires_at": "2026-11-27T00:00:00Z",
            "withdrawals": []
        })
    );
}

const ANIMALS: &str = "00000000000000000000000000000001";

fn animals() -> library::NewVersion {
    let mut version = fractions();
    version.item = ANIMALS.into();
    version.title = "Les animaux".into();
    version
}

#[tokio::test]
async fn the_list_holds_the_three_kinds_with_reason_and_date() {
    let library = library().await;
    library
        .publish_version(verbs("Irregular verbs"), at("2026-11-20T18:00:00Z"))
        .await
        .unwrap();
    library
        .publish_version(verbs("Irregular verbs, part 1"), at("2026-11-21T18:00:00Z"))
        .await
        .unwrap();
    library
        .publish_version(fractions(), at("2026-11-21T08:00:00Z"))
        .await
        .unwrap();
    library
        .publish_version(animals(), at("2026-11-22T08:00:00Z"))
        .await
        .unwrap();

    library
        .withdraw(
            ANIMALS,
            None,
            Withdrawal::Author,
            at("2026-11-26T08:00:00Z"),
        )
        .await
        .unwrap();
    library
        .withdraw(
            VERBS,
            Some(1),
            Withdrawal::Moderation(Reason::Rights),
            at("2026-11-26T08:30:00Z"),
        )
        .await
        .unwrap();
    library
        .withdraw(
            FRACTIONS,
            None,
            Withdrawal::SwitchOff(Reason::PersonalData),
            at("2026-11-26T09:00:00Z"),
        )
        .await
        .unwrap();

    let withdrawals = fetch_verified(&library, "/v1/withdrawals", Kind::Withdrawals).await;
    assert_eq!(
        withdrawals,
        json!({
            "contract": 1,
            "kind": "withdrawals",
            "issued_at": "2026-11-26T09:00:00Z",
            "expires_at": "2026-11-27T09:00:00Z",
            "withdrawals": [
                {"item": ANIMALS, "version": null, "kind": "author", "reason": "author_choice", "withdrawn_at": "2026-11-26T08:00:00Z"},
                {"item": VERBS, "version": 1, "kind": "moderation", "reason": "rights", "withdrawn_at": "2026-11-26T08:30:00Z"},
                {"item": FRACTIONS, "version": null, "kind": "switch_off", "reason": "personal_data", "withdrawn_at": "2026-11-26T09:00:00Z"}
            ]
        })
    );

    // A withdrawn item leaves the index; a withdrawn older version leaves its item there.
    let index = fetch_verified(&library, "/v1/index", Kind::Index).await;
    assert_eq!(index["issued_at"], "2026-11-26T09:00:00Z");
    let listed: Vec<_> = index["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|entry| (entry["item"].clone(), entry["version"].clone()))
        .collect();
    assert_eq!(listed, vec![(json!(VERBS), json!(2))]);
}

fn listed_items(index: &serde_json::Value) -> Vec<serde_json::Value> {
    index["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|entry| entry["item"].clone())
        .collect()
}

#[tokio::test]
async fn a_reactivated_item_leaves_the_list_and_comes_back_to_the_index() {
    let library = library().await;
    library
        .publish_version(fractions(), at("2026-11-21T08:00:00Z"))
        .await
        .unwrap();
    library
        .withdraw(
            FRACTIONS,
            None,
            Withdrawal::SwitchOff(Reason::IllegalContent),
            at("2026-11-26T08:00:00Z"),
        )
        .await
        .unwrap();
    let index = fetch_verified(&library, "/v1/index", Kind::Index).await;
    assert_eq!(listed_items(&index), Vec::<serde_json::Value>::new());

    library
        .reactivate(FRACTIONS, None, at("2026-11-26T10:00:00Z"))
        .await
        .unwrap();

    let withdrawals = fetch_verified(&library, "/v1/withdrawals", Kind::Withdrawals).await;
    assert_eq!(withdrawals["issued_at"], "2026-11-26T10:00:00Z");
    assert_eq!(withdrawals["withdrawals"], json!([]));
    let index = fetch_verified(&library, "/v1/index", Kind::Index).await;
    assert_eq!(index["issued_at"], "2026-11-26T10:00:00Z");
    assert_eq!(listed_items(&index), vec![json!(FRACTIONS)]);
}

#[tokio::test]
async fn only_a_switch_off_can_be_reactivated() {
    let library = library().await;
    library
        .publish_version(fractions(), at("2026-11-21T08:00:00Z"))
        .await
        .unwrap();
    library
        .withdraw(
            FRACTIONS,
            None,
            Withdrawal::Moderation(Reason::WrongAnswer),
            at("2026-11-26T08:00:00Z"),
        )
        .await
        .unwrap();

    let refused = library
        .reactivate(FRACTIONS, None, at("2026-11-26T10:00:00Z"))
        .await;

    assert!(matches!(refused, Err(WithdrawError::NotSwitchedOff)));
    let withdrawals = fetch_verified(&library, "/v1/withdrawals", Kind::Withdrawals).await;
    assert_eq!(withdrawals["withdrawals"][0]["kind"], "moderation");
}

#[tokio::test]
async fn an_unknown_item_or_version_cannot_be_withdrawn() {
    let library = library().await;
    library
        .publish_version(fractions(), at("2026-11-21T08:00:00Z"))
        .await
        .unwrap();

    let unknown_item = library
        .withdraw(
            ANIMALS,
            None,
            Withdrawal::Author,
            at("2026-11-26T08:00:00Z"),
        )
        .await;
    let unknown_version = library
        .withdraw(
            FRACTIONS,
            Some(2),
            Withdrawal::Author,
            at("2026-11-26T08:00:00Z"),
        )
        .await;

    assert!(matches!(unknown_item, Err(WithdrawError::UnknownItem)));
    assert!(matches!(
        unknown_version,
        Err(WithdrawError::UnknownVersion)
    ));
}

#[tokio::test]
async fn a_withdrawal_list_past_its_day_is_refused() {
    let library = library().await;
    let keys = get(&library, "/.well-known/minute-jar-library.json")
        .await
        .keys();
    let list = get(&library, "/v1/withdrawals").await;

    let refused = keys.verify(&list.body, Kind::Withdrawals, at("2026-11-27T00:00:01Z"));

    assert_eq!(refused, Err(Refusal::Expired));
}

#[tokio::test]
async fn a_refresh_signs_the_list_and_the_index_again_as_they_stand() {
    let library = library().await;
    library
        .publish_version(verbs("Irregular verbs"), at("2026-11-20T18:00:00Z"))
        .await
        .unwrap();
    library
        .publish_version(fractions(), at("2026-11-21T08:00:00Z"))
        .await
        .unwrap();
    library
        .withdraw(VERBS, None, Withdrawal::Author, at("2026-11-26T08:00:00Z"))
        .await
        .unwrap();

    library.refresh(at("2026-11-26T20:00:00Z")).await.unwrap();

    let withdrawals = fetch_verified(&library, "/v1/withdrawals", Kind::Withdrawals).await;
    assert_eq!(withdrawals["issued_at"], "2026-11-26T20:00:00Z");
    assert_eq!(withdrawals["expires_at"], "2026-11-27T20:00:00Z");
    assert_eq!(withdrawals["withdrawals"][0]["item"], VERBS);
    assert_eq!(
        withdrawals["withdrawals"][0]["withdrawn_at"],
        "2026-11-26T08:00:00Z"
    );
    let index = fetch_verified(&library, "/v1/index", Kind::Index).await;
    assert_eq!(index["issued_at"], "2026-11-26T20:00:00Z");
    assert_eq!(listed_items(&index), vec![json!(FRACTIONS)]);
}
