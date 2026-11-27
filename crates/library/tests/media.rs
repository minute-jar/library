//! The media of shared items, listed in an item by SHA-256 digest and served
//! by digest (`docs/specs/interface-v1.md`, Media).

mod common;

use common::*;
use library::contract::Kind;
use serde_json::json;
use sha2::{Digest, Sha256};

#[tokio::test]
async fn a_picture_of_a_shared_item_is_listed_by_digest_and_served_by_it() {
    let library = library().await;
    let mut version = verbs("Irregular verbs");
    version.media = vec![pixel()];
    library
        .publish_version(version, at("2026-11-26T09:00:00Z"))
        .await
        .unwrap();

    let item = fetch_verified(
        &library,
        &format!("/v1/items/{VERBS}/versions/1"),
        Kind::Item,
    )
    .await;
    assert_eq!(
        item["media"],
        json!([{"digest": PIXEL, "type": "image/png", "size": 70}])
    );

    let picture = get(&library, &format!("/v1/media/{PIXEL}")).await;
    assert_eq!(picture.status.as_u16(), 200);
    assert_eq!(hex::encode(Sha256::digest(&picture.body)), PIXEL);
    assert_eq!(picture.headers.get("content-type").unwrap(), "image/png");
}

#[tokio::test]
async fn a_medium_is_served_with_immutable_caching_and_its_etag() {
    let library = library().await;
    let mut version = verbs("Irregular verbs");
    version.media = vec![pixel()];
    library
        .publish_version(version, at("2026-11-26T09:00:00Z"))
        .await
        .unwrap();
    let path = format!("/v1/media/{PIXEL}");

    let first = get(&library, &path).await;
    assert_eq!(
        first.headers.get("cache-control").unwrap(),
        "public, max-age=31536000, immutable"
    );
    let etag = first.headers.get("etag").expect("an ETag").clone();
    assert_eq!(etag, format!("\"{PIXEL}\"").as_str());

    let again = get_with(
        &library,
        axum::http::Request::get(&path)
            .header("if-none-match", etag.clone())
            .body(axum::body::Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(again.status.as_u16(), 304);
    assert!(again.body.is_empty());
}

#[tokio::test]
async fn an_unknown_digest_answers_a_fixed_code() {
    let library = library().await;
    library
        .publish_version(verbs("Irregular verbs"), at("2026-11-26T09:00:00Z"))
        .await
        .unwrap();

    for path in [
        format!("/v1/media/{PIXEL}"),
        format!("/v1/media/{}", PIXEL.to_uppercase()),
        "/v1/media/..%2F..%2Fkeys".to_string(),
    ] {
        let answer = get(&library, &path).await;
        assert_eq!(
            (answer.status.as_u16(), answer.json()),
            (404, json!({"error": "unknown_media"})),
            "{path}"
        );
    }
}

/// A file the bank format takes for a PNG picture, of `size` bytes.
fn png_of(size: usize, fill: u8) -> Vec<u8> {
    let mut bytes = pixel();
    bytes.resize(size, fill);
    bytes
}

#[tokio::test]
async fn a_picture_over_a_mebibyte_is_refused_at_publication() {
    let library = library().await;
    let large = png_of(1_048_577, 0);
    let digest = hex::encode(Sha256::digest(&large));
    let mut version = verbs("Irregular verbs");
    version.media = vec![pixel(), large];

    let refused = library
        .publish_version(version, at("2026-11-26T09:00:00Z"))
        .await;

    assert!(
        matches!(&refused, Err(library::PublishError::MediumTooLarge { digest: d }) if *d == digest),
        "{refused:?}"
    );
    assert_eq!(
        fetch_verified(&library, "/v1/index", Kind::Index).await["items"],
        json!([])
    );
    assert_eq!(
        get(&library, &format!("/v1/media/{PIXEL}"))
            .await
            .status
            .as_u16(),
        404
    );
}

#[tokio::test]
async fn a_picture_of_exactly_a_mebibyte_is_published() {
    let library = library().await;
    let mut version = verbs("Irregular verbs");
    version.media = vec![png_of(1_048_576, 0)];

    library
        .publish_version(version, at("2026-11-26T09:00:00Z"))
        .await
        .unwrap();
}

#[tokio::test]
async fn media_over_sixteen_mebibytes_together_are_refused_at_publication() {
    let library = library().await;
    let mut version = verbs("Irregular verbs");
    // Sixteen files of a mebibyte reach the limit; one byte more passes it.
    version.media = (0..16).map(|fill| png_of(1_048_576, fill)).collect();
    version.media.push(png_of(68, 0));

    let refused = library
        .publish_version(version, at("2026-11-26T09:00:00Z"))
        .await;

    assert!(
        matches!(refused, Err(library::PublishError::MediaTooLarge)),
        "{refused:?}"
    );
    assert_eq!(
        fetch_verified(&library, "/v1/index", Kind::Index).await["items"],
        json!([])
    );
}

#[tokio::test]
async fn a_file_that_is_no_picture_or_sound_is_refused_at_publication() {
    let library = library().await;
    let script = b"<script>alert(1)</script>".to_vec();
    let digest = hex::encode(Sha256::digest(&script));
    let mut version = verbs("Irregular verbs");
    version.media = vec![script];

    let refused = library
        .publish_version(version, at("2026-11-26T09:00:00Z"))
        .await;

    assert!(
        matches!(&refused, Err(library::PublishError::UnsupportedMedium { digest: d }) if *d == digest),
        "{refused:?}"
    );
}

#[tokio::test]
async fn the_same_picture_twice_is_listed_once() {
    let library = library().await;
    let mut version = verbs("Irregular verbs");
    version.media = vec![pixel(), pixel()];
    library
        .publish_version(version, at("2026-11-26T09:00:00Z"))
        .await
        .unwrap();

    let item = fetch_verified(
        &library,
        &format!("/v1/items/{VERBS}/versions/1"),
        Kind::Item,
    )
    .await;

    assert_eq!(
        item["media"],
        json!([{"digest": PIXEL, "type": "image/png", "size": 70}])
    );
}
