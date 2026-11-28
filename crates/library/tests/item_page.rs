//! The item page the website shows, and the file a parent downloads for a
//! family server without Internet access (`docs/specs/catalogue-api.md`,
//! Item page; `docs/specs/interface-v1.md`, Item files).

mod common;

use axum::http::StatusCode;
use common::*;
use library::contract::Kind;
use library::{Reason, Withdrawal};
use serde_json::{Value, json};

async fn two_versions() -> library::Library {
    let library = library().await;
    library
        .publish_version(verbs("Irregular verbs"), at("2026-11-20T18:00:00Z"))
        .await
        .unwrap();
    let mut second = verbs("Irregular verbs, part 1");
    second.content = json!({
        "questions": [
            {"type": "typed", "prompt": "Past of 'go'", "answers": ["went"], "picture": PIXEL}
        ],
        "lessons": [{"title": "Strong verbs", "text": "Their vowel changes."}]
    });
    second.media = vec![pixel()];
    library
        .publish_version(second, at("2026-11-22T10:00:00Z"))
        .await
        .unwrap();
    library
}

#[tokio::test]
async fn the_item_page_shows_the_latest_version_with_its_content_and_every_version() {
    let library = two_versions().await;

    let answer = get(&library, &format!("/api/items/{VERBS}")).await;

    assert_eq!(answer.status, StatusCode::OK);
    assert_eq!(
        answer.headers["cache-control"], "public, max-age=60",
        "cached like the catalogue"
    );
    assert_eq!(
        answer.json(),
        json!({
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
            "published_at": "2026-11-22T10:00:00Z",
            "content": {
                "questions": [
                    {"type": "typed", "prompt": "Past of 'go'", "answers": ["went"], "picture": PIXEL}
                ],
                "lessons": [{"title": "Strong verbs", "text": "Their vowel changes."}]
            },
            "media": [{"digest": PIXEL, "type": "image/png", "size": 70}],
            "editorial": null,
            "versions": [
                {"version": 2, "published_at": "2026-11-22T10:00:00Z"},
                {"version": 1, "published_at": "2026-11-20T18:00:00Z"}
            ],
            "comments": [],
        })
    );
}

#[tokio::test]
async fn a_withdrawn_version_gives_way_to_the_one_before_and_leaves_the_versions() {
    let library = two_versions().await;
    library
        .withdraw(
            VERBS,
            Some(2),
            Withdrawal::Moderation(Reason::WrongAnswer),
            at("2026-11-23T00:00:00Z"),
        )
        .await
        .unwrap();

    let page = get(&library, &format!("/api/items/{VERBS}")).await.json();

    assert_eq!(page["version"], 1);
    assert_eq!(page["title"], "Irregular verbs");
    assert_eq!(page["media"], json!([]));
    assert_eq!(
        page["versions"],
        json!([{"version": 1, "published_at": "2026-11-20T18:00:00Z"}])
    );
}

#[tokio::test]
async fn a_withdrawn_or_unknown_item_has_no_page() {
    let library = two_versions().await;
    library
        .withdraw(VERBS, None, Withdrawal::Author, at("2026-11-23T00:00:00Z"))
        .await
        .unwrap();

    for item in [VERBS, FRACTIONS, "not-an-item"] {
        let answer = get(&library, &format!("/api/items/{item}")).await;
        assert_eq!(answer.status, StatusCode::NOT_FOUND, "{item}");
        assert_eq!(answer.json(), json!({"error": "unknown_item"}));
    }
}

/// The signed document a family server would fetch online.
async fn served(library: &library::Library, path: &str) -> String {
    String::from_utf8(get(library, path).await.body).unwrap()
}

#[tokio::test]
async fn the_file_holds_the_signed_item_and_keys_byte_for_byte_and_its_media() {
    let library = two_versions().await;

    let answer = get(&library, &format!("/api/items/{VERBS}/versions/2/file")).await;

    assert_eq!(answer.status, StatusCode::OK);
    assert_eq!(answer.headers["content-type"], "application/json");
    assert_eq!(
        answer.headers["content-disposition"],
        format!("attachment; filename=\"minute-jar-{VERBS}-2.json\"")
    );
    let item = served(&library, &format!("/v1/items/{VERBS}/versions/2")).await;
    let keys = served(&library, "/.well-known/minute-jar-library.json").await;
    let file = String::from_utf8(answer.body.clone()).unwrap();
    assert_eq!(
        file,
        format!(
            "{{\"contract\":1,\"kind\":\"item_file\",\"keys\":{keys},\"item\":{item},\"media\":[{{\"digest\":\"{PIXEL}\",\"base64\":\"{}\"}}]}}",
            // The pixel's bytes in base64, worked out apart from the service.
            "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNkYPhfDwAChwGA60e6kgAAAABJRU5ErkJggg=="
        )
    );
}

#[tokio::test]
async fn a_family_server_checks_the_file_from_the_root_key_alone() {
    let library = two_versions().await;
    let file: Value = get(&library, &format!("/api/items/{VERBS}/versions/1/file"))
        .await
        .json();

    let keys = library::contract::verify_keys(
        file["keys"].to_string().as_bytes(),
        &root().public(),
        checked_at(),
    )
    .unwrap();
    let item = keys
        .verify(
            file["item"].to_string().as_bytes(),
            Kind::Item,
            checked_at(),
        )
        .unwrap();

    assert_eq!(item["item"], VERBS);
    assert_eq!(item["version"], 1);
    assert_eq!(file["media"], json!([]));
}

#[tokio::test]
async fn no_file_for_a_withdrawn_or_unknown_version() {
    let library = two_versions().await;
    library
        .withdraw(
            VERBS,
            Some(2),
            Withdrawal::Author,
            at("2026-11-23T00:00:00Z"),
        )
        .await
        .unwrap();

    for (path, error) in [
        (
            format!("/api/items/{VERBS}/versions/2/file"),
            "unknown_version",
        ),
        (
            format!("/api/items/{VERBS}/versions/3/file"),
            "unknown_version",
        ),
        (
            format!("/api/items/{VERBS}/versions/two/file"),
            "unknown_version",
        ),
        (
            format!("/api/items/{FRACTIONS}/versions/1/file"),
            "unknown_item",
        ),
    ] {
        let answer = get(&library, &path).await;
        assert_eq!(answer.status, StatusCode::NOT_FOUND, "{path}");
        assert_eq!(answer.json(), json!({"error": error}), "{path}");
    }
}

/// A reviewer that finds every report implausible, and a notifier that sends nothing.
struct Unconvinced;

impl library::Reviewer for Unconvinced {
    fn review<'a>(&'a self, _: library::Submission<'a>) -> library::ReviewFuture<'a> {
        Box::pin(async { Ok(library::ReviewOutcome::Published) })
    }
}

struct Silent;

impl library::Notifier for Silent {
    fn send<'a>(&'a self, _: &'a str, _: &'a library::Message) -> library::NotifyFuture<'a> {
        Box::pin(async { Ok(()) })
    }
}

#[tokio::test]
async fn an_item_hidden_from_new_imports_has_no_page_and_no_file() {
    let library = two_versions()
        .await
        .with_reports(Unconvinced, Silent, || at("2026-11-26T10:00:00Z"));
    let reported = post_json(
        &library,
        "/v1/reports",
        &json!({"library_identifier": FAMILY_A, "item": VERBS, "reason": "illegal_content", "details": "Question 1"}),
    )
    .await;
    assert!(reported.status.is_success(), "{}", reported.json());

    let page = get(&library, &format!("/api/items/{VERBS}")).await;
    let file = get(&library, &format!("/api/items/{VERBS}/versions/2/file")).await;

    assert_eq!(page.status, StatusCode::NOT_FOUND);
    assert_eq!(page.json(), json!({"error": "unknown_item"}));
    assert_eq!(file.status, StatusCode::NOT_FOUND);
    assert_eq!(file.json(), json!({"error": "unknown_item"}));
}
