//! The signed index and shared items, served over HTTP as a family server
//! fetches them (`docs/specs/interface-v1.md`).

mod common;

use common::*;
use library::contract::Kind;
use serde_json::json;

#[tokio::test]
async fn a_published_version_is_served_as_a_signed_shared_item() {
    let library = library().await;
    library
        .publish_version(verbs("Irregular verbs, part 1"), at("2026-11-20T18:00:00Z"))
        .await
        .unwrap();

    let item = fetch_verified(
        &library,
        &format!("/v1/items/{VERBS}/versions/1"),
        Kind::Item,
    )
    .await;

    assert_eq!(
        item,
        json!({
            "contract": 1,
            "kind": "item",
            "issued_at": "2026-11-20T18:00:00Z",
            "item": VERBS,
            "version": 1,
            "type": "bank",
            "title": "Irregular verbs, part 1",
            "licence": "CC-BY-SA-4.0",
            "author": {"pseudonym": "Owl", "editorial": false},
            "derived_from": null,
            "metadata": {
                "language": "en-GB",
                "subject": "english",
                "ages": {"min": 9, "max": 11},
                "description": "The fifty most common irregular verbs.",
                "curriculum": {"country": "GB", "year": "Year 5", "topic": "Verb forms"}
            },
            "published_at": "2026-11-20T18:00:00Z",
            "content": {"questions": [{"type": "typed", "prompt": "Past of 'go'", "answers": ["went"]}]},
            "media": []
        })
    );
}

#[tokio::test]
async fn the_index_lists_every_shared_item_at_its_latest_version() {
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
        .publish_version(verbs("Irregular verbs, part 1"), at("2026-11-26T09:00:00Z"))
        .await
        .unwrap();

    let index = fetch_verified(&library, "/v1/index", Kind::Index).await;

    assert_eq!(
        index,
        json!({
            "contract": 1,
            "kind": "index",
            "issued_at": "2026-11-26T09:00:00Z",
            "expires_at": "2026-11-29T09:00:00Z",
            "items": [
                {
                    "item": VERBS,
                    "version": 2,
                    "type": "bank",
                    "title": "Irregular verbs, part 1",
                    "licence": "CC-BY-SA-4.0",
                    "author": {"pseudonym": "Owl", "editorial": false},
                    "derived_from": null,
                    "metadata": {
                        "language": "en-GB",
                        "subject": "english",
                        "ages": {"min": 9, "max": 11},
                        "description": "The fifty most common irregular verbs.",
                        "curriculum": {"country": "GB", "year": "Year 5", "topic": "Verb forms"}
                    },
                    "selection": false,
                    "signals": {"families": 0, "ratings": [0, 0, 0, 0, 0]},
                    "published_at": "2026-11-26T09:00:00Z"
                },
                {
                    "item": FRACTIONS,
                    "version": 1,
                    "type": "bank",
                    "title": "Les fractions, CM1",
                    "licence": "CC-BY-SA-4.0",
                    "author": {"pseudonym": "Minute Jar", "editorial": true},
                    "derived_from": null,
                    "metadata": {
                        "language": "fr-FR",
                        "subject": "mathematics",
                        "ages": {"min": 9, "max": 10},
                        "description": "Comparer et ranger des fractions simples.",
                        "curriculum": null
                    },
                    "selection": true,
                    "signals": {"families": 0, "ratings": [0, 0, 0, 0, 0]},
                    "published_at": "2026-11-21T08:00:00Z"
                }
            ]
        })
    );
}

#[tokio::test]
async fn an_empty_catalogue_still_serves_a_signed_index() {
    let library = library().await;

    let index = fetch_verified(&library, "/v1/index", Kind::Index).await;

    assert_eq!(index["items"], json!([]));
}

#[tokio::test]
async fn an_unknown_item_and_an_unknown_version_answer_fixed_codes() {
    let library = library().await;
    library
        .publish_version(verbs("Irregular verbs"), at("2026-11-20T18:00:00Z"))
        .await
        .unwrap();

    let unknown_item = get(&library, &format!("/v1/items/{FRACTIONS}/versions/1")).await;
    let unknown_version = get(&library, &format!("/v1/items/{VERBS}/versions/2")).await;
    let not_a_version = get(&library, &format!("/v1/items/{VERBS}/versions/latest")).await;

    assert_eq!(
        (unknown_item.status.as_u16(), unknown_item.json()),
        (404, json!({"error": "unknown_item"}))
    );
    assert_eq!(
        (unknown_version.status.as_u16(), unknown_version.json()),
        (404, json!({"error": "unknown_version"}))
    );
    assert_eq!(
        (not_a_version.status.as_u16(), not_a_version.json()),
        (404, json!({"error": "unknown_version"}))
    );
    assert_eq!(
        unknown_item.headers.get("cache-control").unwrap(),
        "private, no-store",
        "no cache keeps a refusal"
    );
}

#[tokio::test]
async fn every_caller_receives_the_same_bytes_whatever_it_sends() {
    let library = library().await;
    library
        .publish_version(verbs("Irregular verbs"), at("2026-11-26T09:00:00Z"))
        .await
        .unwrap();

    for path in [
        "/.well-known/minute-jar-library.json",
        "/v1/index",
        &format!("/v1/items/{VERBS}/versions/1"),
    ] {
        let plain = get(&library, path).await;
        let other = get_with(
            &library,
            axum::http::Request::get(path)
                .header("user-agent", "another family server")
                .header("accept-language", "fr-FR")
                .header("cookie", "session=anything")
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await;
        assert_eq!(plain.body, other.body, "{path}");
        assert_eq!(
            plain.headers.get("etag"),
            other.headers.get("etag"),
            "{path}"
        );
        assert!(plain.headers.get("set-cookie").is_none(), "{path}");
    }
}

#[tokio::test]
async fn a_signed_document_is_served_with_its_etag_and_caching() {
    let library = library().await;
    library
        .publish_version(verbs("Irregular verbs"), at("2026-11-26T09:00:00Z"))
        .await
        .unwrap();

    for (path, cache) in [
        (
            "/.well-known/minute-jar-library.json",
            "public, max-age=3600",
        ),
        ("/v1/index", "public, max-age=300"),
        (
            &format!("/v1/items/{VERBS}/versions/1"),
            "public, max-age=86400",
        ),
    ] {
        let first = get(&library, path).await;
        let etag = first.headers.get("etag").expect("an ETag").clone();
        assert_eq!(first.headers.get("cache-control").unwrap(), cache, "{path}");
        assert_eq!(
            first.headers.get("content-type").unwrap(),
            "application/json",
            "{path}"
        );

        let again = get_with(
            &library,
            axum::http::Request::get(path)
                .header("if-none-match", etag.clone())
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await;
        assert_eq!(again.status.as_u16(), 304, "{path}");
        assert!(again.body.is_empty(), "{path}");
        assert_eq!(again.headers.get("etag"), Some(&etag), "{path}");
    }
}

#[tokio::test]
async fn a_new_version_changes_the_index_etag_but_not_the_older_versions() {
    let library = library().await;
    library
        .publish_version(verbs("Irregular verbs"), at("2026-11-26T09:00:00Z"))
        .await
        .unwrap();
    let index_before = get(&library, "/v1/index").await;
    let version_1_before = get(&library, &format!("/v1/items/{VERBS}/versions/1")).await;

    library
        .publish_version(verbs("Irregular verbs, part 1"), at("2026-11-26T10:00:00Z"))
        .await
        .unwrap();

    let index_after = get(&library, "/v1/index").await;
    let version_1_after = get(&library, &format!("/v1/items/{VERBS}/versions/1")).await;
    assert_ne!(
        index_before.headers.get("etag"),
        index_after.headers.get("etag")
    );
    assert_eq!(version_1_before.body, version_1_after.body);
}

#[tokio::test]
async fn a_version_whose_item_is_not_32_hexadecimal_digits_is_not_published() {
    let library = library().await;
    let mut version = verbs("Irregular verbs");
    version.item = "../../keys".into();

    let refused = library
        .publish_version(version, at("2026-11-26T09:00:00Z"))
        .await;

    assert!(matches!(refused, Err(library::PublishError::BadItem)));
    assert_eq!(
        fetch_verified(&library, "/v1/index", Kind::Index).await["items"],
        json!([])
    );
}
