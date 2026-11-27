//! Imports reported under a library identifier, and the number of families
//! the index publishes for each shared item (`docs/specs/interface-v1.md`,
//! Imports).

mod common;

use axum::http::StatusCode;
use common::*;
use library::contract::Kind;
use serde_json::{Value, json};

fn families(index: &Value, item: &str) -> u64 {
    index["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|entry| entry["item"] == item)
        .expect("the item is in the index")["signals"]["families"]
        .as_u64()
        .unwrap()
}

async fn published() -> library::Library {
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
}

#[tokio::test]
async fn the_index_counts_the_families_that_imported_an_item_once_signed_again() {
    let library = published().await;

    assert_eq!(
        import(&library, FAMILY_A, VERBS).await.status,
        StatusCode::NO_CONTENT
    );
    assert_eq!(
        import(&library, FAMILY_B, VERBS).await.status,
        StatusCode::NO_CONTENT
    );
    library.refresh(at("2026-11-26T09:00:00Z")).await.unwrap();

    let index = fetch_verified(&library, "/v1/index", Kind::Index).await;
    assert_eq!(families(&index, VERBS), 2);
    assert_eq!(families(&index, FRACTIONS), 0);
}

#[tokio::test]
async fn an_identifier_counts_once_per_item_whatever_it_reports() {
    let library = published().await;

    import(&library, FAMILY_A, VERBS).await;
    import(&library, FAMILY_A, VERBS).await;
    import(&library, FAMILY_A, FRACTIONS).await;
    library.refresh(at("2026-11-26T09:00:00Z")).await.unwrap();

    let index = fetch_verified(&library, "/v1/index", Kind::Index).await;
    assert_eq!(families(&index, VERBS), 1);
    assert_eq!(families(&index, FRACTIONS), 1);
}

#[tokio::test]
async fn the_index_served_does_not_change_until_it_is_signed_again() {
    let library = published().await;
    let before = get(&library, "/v1/index").await.body;

    import(&library, FAMILY_A, VERBS).await;

    assert_eq!(get(&library, "/v1/index").await.body, before);
}

#[tokio::test]
async fn an_import_answers_nothing_and_sets_no_cookie() {
    let library = published().await;

    let answer = import(&library, FAMILY_A, VERBS).await;

    assert_eq!(answer.status, StatusCode::NO_CONTENT);
    assert!(answer.body.is_empty());
    assert!(answer.headers.get("set-cookie").is_none());
}

#[tokio::test]
async fn an_import_of_an_unknown_item_is_refused() {
    let library = published().await;

    let answer = import(&library, FAMILY_A, "00000000000000000000000000000000").await;

    assert_eq!(answer.status, StatusCode::NOT_FOUND);
    assert_eq!(answer.json(), json!({"error": "unknown_item"}));
}

#[tokio::test]
async fn a_malformed_library_identifier_is_refused_and_not_counted() {
    let library = published().await;

    for identifier in ["", "family-1", "AAAAAAAAAAAAAAAA0000000000000001"] {
        let answer = import(&library, identifier, VERBS).await;
        assert_eq!(answer.status, StatusCode::BAD_REQUEST, "{identifier:?}");
        assert_eq!(answer.json(), json!({"error": "bad_library_identifier"}));
    }
    library.refresh(at("2026-11-26T09:00:00Z")).await.unwrap();

    let index = fetch_verified(&library, "/v1/index", Kind::Index).await;
    assert_eq!(families(&index, VERBS), 0);
}

#[tokio::test]
async fn a_body_that_is_not_an_import_is_refused() {
    let library = published().await;

    for body in [
        b"not json".to_vec(),
        json!({"item": VERBS}).to_string().into_bytes(),
        json!({"library_identifier": FAMILY_A, "item": VERBS, "account": "x"})
            .to_string()
            .into_bytes(),
    ] {
        let answer = post_bytes(&library, "/v1/imports", body).await;
        assert_eq!(answer.status, StatusCode::BAD_REQUEST);
        assert_eq!(answer.json(), json!({"error": "bad_request"}));
    }
}

#[tokio::test]
async fn the_library_method_records_an_import_as_the_route_does() {
    let library = published().await;

    library.record_import(FAMILY_A, VERBS).await.unwrap();
    library.record_import(FAMILY_B, VERBS).await.unwrap();
    assert!(matches!(
        library.record_import("x", VERBS).await,
        Err(library::ImportError::BadIdentifier)
    ));
    assert!(matches!(
        library.record_import(FAMILY_A, "y").await,
        Err(library::ImportError::UnknownItem)
    ));
    library.refresh(at("2026-11-26T09:00:00Z")).await.unwrap();

    let index = fetch_verified(&library, "/v1/index", Kind::Index).await;
    assert_eq!(families(&index, VERBS), 2);
}
