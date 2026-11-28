//! Author profiles (story S19, `docs/specs/catalogue-api.md`, Author
//! profiles): anyone sees an author's shown items and how many families use
//! them; the author alone also sees the suggestions received.

mod common;

use axum::http::StatusCode;
use common::cards::text;
use common::identity::*;
use common::shares::*;
use common::*;
use serde_json::{Value, json};

/// Shares `body` under `session`; answers the published item.
async fn published(sharing: &Sharing, session: &str, body: Value) -> String {
    let shared = sharing.share(session, body).await;
    assert_eq!(shared.status, StatusCode::CREATED, "{}", text(&shared));
    shared.json()["item"].as_str().unwrap().to_string()
}

#[tokio::test]
async fn anyone_sees_an_authors_items_and_how_many_families_use_them() {
    let sharing = with_sharing().await;
    let library = &sharing.library;
    let owl = sharing.author(SUBJECT, "Owl").await;
    let verbs = published(&sharing, &owl, prepared("Irregular verbs")).await;
    let more = published(&sharing, &owl, prepared_again("More verbs")).await;
    let lark = sharing.author("subject-of-lark", "Lark").await;
    published(&sharing, &lark, prepared("Not Owl's")).await;
    import(library, FAMILY_A, &verbs).await;
    import(library, FAMILY_B, &verbs).await;
    import(library, FAMILY_A, &more).await;

    let answer = get(library, "/api/authors/owl").await;

    assert_eq!(answer.status, StatusCode::OK, "{}", text(&answer));
    assert_eq!(
        answer.headers["cache-control"], "public, max-age=60",
        "the same for every caller"
    );
    let profile = answer.json();
    assert_eq!(profile["pseudonym"], "Owl");
    assert_eq!(profile["editorial"], false);
    assert_eq!(
        profile["families"], 2,
        "two families use the author's items, one of them both"
    );
    let items = profile["items"].as_array().unwrap();
    let listed: Vec<(&str, &str, i64)> = items
        .iter()
        .map(|entry| {
            (
                entry["item"].as_str().unwrap(),
                entry["title"].as_str().unwrap(),
                entry["signals"]["families"].as_i64().unwrap(),
            )
        })
        .collect();
    let mut expected = vec![
        (verbs.as_str(), "Irregular verbs", 2),
        (more.as_str(), "More verbs", 1),
    ];
    expected.sort_by_key(|(item, ..)| *item);
    let mut sorted = listed.clone();
    sorted.sort_by_key(|(item, ..)| *item);
    assert_eq!(sorted, expected);
    assert_eq!(
        items[0]["author"],
        json!({"pseudonym": "Owl", "editorial": false})
    );
    assert!(profile.get("suggestions").is_none());
}

#[tokio::test]
async fn a_pseudonym_without_shown_items_has_no_profile() {
    let sharing = with_sharing().await;
    let library = &sharing.library;
    sharing.author(SUBJECT, "Owl").await;

    for path in ["/api/authors/Owl", "/api/authors/Nobody"] {
        let answer = get(library, path).await;
        assert_eq!(answer.status, StatusCode::NOT_FOUND, "{path}");
        assert_eq!(answer.json(), json!({"error": "unknown_author"}), "{path}");
    }
}

#[tokio::test]
async fn a_withdrawn_item_leaves_the_profile() {
    let sharing = with_sharing().await;
    let library = &sharing.library;
    let owl = sharing.author(SUBJECT, "Owl").await;
    let verbs = published(&sharing, &owl, prepared("Irregular verbs")).await;
    let kept = published(&sharing, &owl, prepared_again("More verbs")).await;
    import(library, FAMILY_A, &verbs).await;
    let withdrawn = with_session(
        library,
        "POST",
        &format!("/api/account/items/{verbs}/withdrawal"),
        &owl,
        Some(json!({})),
    )
    .await;
    assert_eq!(withdrawn.status, StatusCode::OK, "{}", text(&withdrawn));

    let profile = get(library, "/api/authors/Owl").await.json();

    let items: Vec<&str> = profile["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|entry| entry["item"].as_str().unwrap())
        .collect();
    assert_eq!(items, [kept.as_str()]);
    assert_eq!(profile["families"], 0, "families of shown items only");
}

#[tokio::test]
async fn the_item_page_names_the_profile_of_its_author_under_their_pseudonym_of_now() {
    let sharing = with_sharing().await;
    let library = &sharing.library;
    let owl = sharing.author(SUBJECT, "Owl").await;
    let verbs = published(&sharing, &owl, prepared("Irregular verbs")).await;

    assert_eq!(
        get(library, &format!("/api/items/{verbs}")).await.json()["profile"],
        "Owl"
    );
    let renamed = with_session(
        library,
        "PUT",
        "/api/account/pseudonym",
        &owl,
        Some(json!({"pseudonym": "Barn Owl"})),
    )
    .await;
    assert_eq!(renamed.status, StatusCode::OK, "{}", text(&renamed));

    let page = get(library, &format!("/api/items/{verbs}")).await.json();
    assert_eq!(
        page["author"]["pseudonym"], "Owl",
        "the version keeps the pseudonym it was published under"
    );
    assert_eq!(page["profile"], "Barn Owl");
    assert_eq!(
        get(library, "/api/authors/Owl").await.status,
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        get(library, "/api/authors/Barn%20Owl").await.json()["items"][0]["item"],
        verbs
    );
}

#[tokio::test]
async fn an_item_shared_by_no_library_account_names_no_profile() {
    let library = library().await;
    library
        .publish_version(verbs("Irregular verbs"), at("2026-11-20T18:00:00Z"))
        .await
        .unwrap();

    let page = get(&library, &format!("/api/items/{VERBS}")).await.json();

    assert_eq!(page["profile"], Value::Null);
}

async fn own(sharing: &Sharing, session: &str) -> Answer {
    with_session(
        &sharing.library,
        "GET",
        "/api/account/profile",
        session,
        None,
    )
    .await
}

async fn suggest(library: &library::Library, identifier: &str, item: &str, answers: &[&str]) {
    let answer = post_json(
        library,
        "/v1/suggestions",
        &json!({
            "library_identifier": identifier,
            "item": item,
            "version": 1,
            "question": 1,
            "answers": answers,
        }),
    )
    .await;
    assert_eq!(answer.status, StatusCode::NO_CONTENT, "{}", text(&answer));
}

#[tokio::test]
async fn the_author_alone_sees_the_suggestions_received_on_their_profile() {
    let sharing = with_sharing().await;
    let library = &sharing.library;
    let owl = sharing.author(SUBJECT, "Owl").await;
    let verbs = published(&sharing, &owl, prepared("Irregular verbs")).await;
    let quiet = published(&sharing, &owl, prepared_again("More verbs")).await;
    import(library, FAMILY_A, &verbs).await;
    import(library, FAMILY_B, &verbs).await;
    suggest(library, FAMILY_A, &verbs, &["goed", "has gone"]).await;
    suggest(library, FAMILY_B, &verbs, &["goed"]).await;

    let answer = own(&sharing, &owl).await;

    assert_eq!(answer.status, StatusCode::OK, "{}", text(&answer));
    assert_eq!(answer.headers["cache-control"], "private, no-store");
    let profile = answer.json();
    assert_eq!(profile["pseudonym"], "Owl");
    assert_eq!(profile["families"], 2);
    let received: Vec<(&str, Value)> = profile["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|entry| {
            (
                entry["item"].as_str().unwrap(),
                entry["suggestions"].clone(),
            )
        })
        .collect();
    assert!(received.contains(&(verbs.as_str(), json!({"families": 2, "answers": 2}))));
    assert!(received.contains(&(quiet.as_str(), json!({"families": 0, "answers": 0}))));
    let public = get(library, "/api/authors/Owl").await.json();
    assert!(
        public["items"]
            .as_array()
            .unwrap()
            .iter()
            .all(|entry| entry.get("suggestions").is_none()),
        "suggestions are the author's alone"
    );
}

#[tokio::test]
async fn the_authors_own_profile_needs_a_session_and_an_account() {
    let sharing = with_sharing().await;
    let library = &sharing.library;

    let signed_out = get(library, "/api/account/profile").await;
    assert_eq!(signed_out.status, StatusCode::UNAUTHORIZED);
    assert_eq!(signed_out.json(), json!({"error": "signed_out"}));

    let session = common::identity::session(
        library,
        &sharing.provider,
        passkey("subject-without-account", sharing.clock.now()),
    )
    .await;
    let no_account = own(&sharing, &session).await;
    assert_eq!(no_account.status, StatusCode::NOT_FOUND);
    assert_eq!(no_account.json(), json!({"error": "no_account"}));

    let owl = sharing.author(SUBJECT, "Owl").await;
    let empty = own(&sharing, &owl).await;
    assert_eq!(empty.status, StatusCode::OK, "{}", text(&empty));
    assert_eq!(
        empty.json(),
        json!({"pseudonym": "Owl", "editorial": false, "families": 0, "items": []})
    );
}
