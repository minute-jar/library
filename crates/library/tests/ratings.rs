//! Ratings and short reviews left under a library identifier, once per
//! identifier and only for an item it imported (`docs/specs/interface-v1.md`,
//! Ratings).

mod common;

use axum::http::StatusCode;
use common::*;
use library::contract::Kind;
use serde_json::{Value, json};

/// The counts of ratings of 1 to 5 stars the index publishes for `item`.
fn ratings(index: &Value, item: &str) -> Value {
    index["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|entry| entry["item"] == item)
        .expect("the item is in the index")["signals"]["ratings"]
        .clone()
}

async fn rate(library: &library::Library, identifier: &str, item: &str, stars: u8) -> Answer {
    post_json(
        library,
        "/v1/ratings",
        &json!({"library_identifier": identifier, "item": item, "stars": stars}),
    )
    .await
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
async fn the_index_counts_the_ratings_of_families_that_imported_an_item() {
    let library = published().await;
    import(&library, FAMILY_A, VERBS).await;
    import(&library, FAMILY_B, VERBS).await;

    assert_eq!(
        rate(&library, FAMILY_A, VERBS, 5).await.status,
        StatusCode::NO_CONTENT
    );
    assert_eq!(
        rate(&library, FAMILY_B, VERBS, 3).await.status,
        StatusCode::NO_CONTENT
    );
    library.refresh(at("2026-11-26T09:00:00Z")).await.unwrap();

    let index = fetch_verified(&library, "/v1/index", Kind::Index).await;
    assert_eq!(ratings(&index, VERBS), json!([0, 0, 1, 0, 1]));
    assert_eq!(ratings(&index, FRACTIONS), json!([0, 0, 0, 0, 0]));
}

#[tokio::test]
async fn a_rating_for_an_item_the_identifier_did_not_import_is_refused_and_not_counted() {
    let library = published().await;
    import(&library, FAMILY_A, FRACTIONS).await;

    let answer = rate(&library, FAMILY_A, VERBS, 1).await;

    assert_eq!(answer.status, StatusCode::FORBIDDEN);
    assert_eq!(answer.json(), json!({"error": "not_imported"}));
    library.refresh(at("2026-11-26T09:00:00Z")).await.unwrap();
    let index = fetch_verified(&library, "/v1/index", Kind::Index).await;
    assert_eq!(ratings(&index, VERBS), json!([0, 0, 0, 0, 0]));
}

#[tokio::test]
async fn an_identifier_counts_once_per_item_and_its_last_rating_replaces_the_first() {
    let library = published().await;
    import(&library, FAMILY_A, VERBS).await;

    rate(&library, FAMILY_A, VERBS, 2).await;
    rate(&library, FAMILY_A, VERBS, 4).await;
    library.refresh(at("2026-11-26T09:00:00Z")).await.unwrap();

    let index = fetch_verified(&library, "/v1/index", Kind::Index).await;
    assert_eq!(ratings(&index, VERBS), json!([0, 0, 0, 1, 0]));
}

#[tokio::test]
async fn a_rating_outside_one_to_five_stars_is_refused() {
    let library = published().await;
    import(&library, FAMILY_A, VERBS).await;

    for stars in [0, 6] {
        let answer = rate(&library, FAMILY_A, VERBS, stars).await;
        assert_eq!(answer.status, StatusCode::BAD_REQUEST, "{stars}");
        assert_eq!(answer.json(), json!({"error": "bad_rating"}));
    }
}

#[tokio::test]
async fn a_malformed_library_identifier_is_refused() {
    let library = published().await;

    for identifier in ["", "family-1", "AAAAAAAAAAAAAAAA0000000000000001"] {
        let answer = rate(&library, identifier, VERBS, 5).await;
        assert_eq!(answer.status, StatusCode::BAD_REQUEST, "{identifier:?}");
        assert_eq!(answer.json(), json!({"error": "bad_library_identifier"}));
    }
}

#[tokio::test]
async fn a_rating_of_an_unknown_item_is_refused() {
    let library = published().await;

    let answer = rate(&library, FAMILY_A, "00000000000000000000000000000000", 5).await;

    assert_eq!(answer.status, StatusCode::NOT_FOUND);
    assert_eq!(answer.json(), json!({"error": "unknown_item"}));
}

#[tokio::test]
async fn a_body_that_is_not_a_rating_is_refused() {
    let library = published().await;

    for body in [
        b"not json".to_vec(),
        json!({"library_identifier": FAMILY_A, "item": VERBS})
            .to_string()
            .into_bytes(),
        json!({"library_identifier": FAMILY_A, "item": VERBS, "stars": "five"})
            .to_string()
            .into_bytes(),
        json!({"library_identifier": FAMILY_A, "item": VERBS, "stars": 5, "account": "x"})
            .to_string()
            .into_bytes(),
    ] {
        let answer = post_bytes(&library, "/v1/ratings", body).await;
        assert_eq!(answer.status, StatusCode::BAD_REQUEST);
        assert_eq!(answer.json(), json!({"error": "bad_request"}));
    }
}

async fn review(
    library: &library::Library,
    identifier: &str,
    item: &str,
    stars: u8,
    text: &str,
) -> Answer {
    post_json(
        library,
        "/v1/ratings",
        &json!({"library_identifier": identifier, "item": item, "stars": stars, "review": text}),
    )
    .await
}

#[tokio::test]
async fn the_website_reads_the_ratings_of_an_item_as_they_stand() {
    let library = published().await;
    import(&library, FAMILY_A, VERBS).await;
    import(&library, FAMILY_B, VERBS).await;
    rate(&library, FAMILY_A, VERBS, 4).await;
    rate(&library, FAMILY_B, VERBS, 4).await;

    let answer = get(&library, &format!("/api/items/{VERBS}/ratings")).await;

    assert_eq!(answer.status, StatusCode::OK);
    assert_eq!(
        answer.json(),
        json!({"item": VERBS, "ratings": [0, 0, 0, 2, 0], "reviews": []})
    );
}

#[tokio::test]
async fn a_short_review_is_held_for_review_and_not_shown_while_its_rating_counts() {
    let library = published().await;
    import(&library, FAMILY_A, VERBS).await;

    let answer = review(
        &library,
        FAMILY_A,
        VERBS,
        5,
        "My son learnt them in a week.",
    )
    .await;

    assert_eq!(answer.status, StatusCode::NO_CONTENT);
    let shown = get(&library, &format!("/api/items/{VERBS}/ratings")).await;
    assert_eq!(
        shown.json(),
        json!({"item": VERBS, "ratings": [0, 0, 0, 0, 1], "reviews": []})
    );
}

#[tokio::test]
async fn a_review_longer_than_280_characters_is_refused() {
    let library = published().await;
    import(&library, FAMILY_A, VERBS).await;

    let answer = review(&library, FAMILY_A, VERBS, 5, &"é".repeat(281)).await;
    assert_eq!(answer.status, StatusCode::BAD_REQUEST);
    assert_eq!(answer.json(), json!({"error": "bad_review"}));

    let answer = review(&library, FAMILY_A, VERBS, 5, &"é".repeat(280)).await;
    assert_eq!(answer.status, StatusCode::NO_CONTENT);
}

#[tokio::test]
async fn the_website_is_told_an_item_is_unknown() {
    let library = published().await;

    let answer = get(
        &library,
        "/api/items/00000000000000000000000000000000/ratings",
    )
    .await;

    assert_eq!(answer.status, StatusCode::NOT_FOUND);
    assert_eq!(answer.json(), json!({"error": "unknown_item"}));
}

#[tokio::test]
async fn the_library_methods_record_and_read_ratings_as_the_routes_do() {
    let library = published().await;
    library.record_import(FAMILY_A, VERBS).await.unwrap();

    library
        .record_rating(FAMILY_A, VERBS, 3, Some("Clear and short."))
        .await
        .unwrap();
    assert!(matches!(
        library.record_rating(FAMILY_B, VERBS, 3, None).await,
        Err(library::RatingError::NotImported)
    ));
    assert!(matches!(
        library.record_rating(FAMILY_A, VERBS, 9, None).await,
        Err(library::RatingError::BadRating)
    ));

    let ratings = library.item_ratings(VERBS).await.unwrap();
    assert_eq!(ratings.counts, [0, 0, 1, 0, 0]);
    assert!(ratings.reviews.is_empty());
    assert!(matches!(
        library.item_ratings("y").await,
        Err(library::RatingError::UnknownItem)
    ));
}
