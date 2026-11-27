//! Suggestions to authors (story S18, `docs/specs/interface-v1.md`,
//! Suggestions): a family server suggests, under its library identifier,
//! answers to accept for a question of an item it imported; the author sees
//! them grouped and counted by family, never who sent them, and accepts some
//! into a prepared new version for the share route's review.

mod common;

use axum::http::StatusCode;
use common::cards::text;
use common::identity::*;
use common::shares::*;
use common::*;
use serde_json::{Value, json};

/// A library with an author, "Owl", who shared the two-question bank of
/// `prepared`; answers the author's session and the item.
async fn shared() -> (Sharing, String, String) {
    let sharing = with_sharing().await;
    let session = sharing.author(SUBJECT, "Owl").await;
    let shared = sharing.share(&session, prepared("Irregular verbs")).await;
    assert_eq!(shared.status, StatusCode::CREATED, "{}", text(&shared));
    let item = shared.json()["item"].as_str().unwrap().to_string();
    (sharing, session, item)
}

async fn suggest(
    library: &library::Library,
    identifier: &str,
    item: &str,
    version: u32,
    question: u32,
    answers: &[&str],
) -> Answer {
    post_json(
        library,
        "/v1/suggestions",
        &json!({
            "library_identifier": identifier,
            "item": item,
            "version": version,
            "question": question,
            "answers": answers,
        }),
    )
    .await
}

async fn suggestions(sharing: &Sharing, session: &str) -> Answer {
    with_session(
        &sharing.library,
        "GET",
        "/api/account/suggestions",
        session,
        None,
    )
    .await
}

#[tokio::test]
async fn the_author_sees_the_suggestions_of_families_grouped_by_question_and_counted_by_family() {
    let (sharing, session, item) = shared().await;
    let library = &sharing.library;
    import(library, FAMILY_A, &item).await;
    import(library, FAMILY_B, &item).await;

    let suggested = suggest(library, FAMILY_A, &item, 1, 1, &["goed", "has gone"]).await;
    assert_eq!(
        suggested.status,
        StatusCode::NO_CONTENT,
        "{}",
        text(&suggested)
    );
    suggest(library, FAMILY_B, &item, 1, 1, &["goed"]).await;
    suggest(library, FAMILY_B, &item, 1, 2, &[" seen "]).await;

    let answer = suggestions(&sharing, &session).await;

    assert_eq!(answer.status, StatusCode::OK, "{}", text(&answer));
    assert_eq!(
        answer.json(),
        json!({"items": [{
            "item": item,
            "title": "Irregular verbs",
            "version": 1,
            "questions": [
                {"question": 1, "prompt": "Past of 'go'", "answers": [
                    {"answer": "goed", "families": 2},
                    {"answer": "has gone", "families": 1}
                ]},
                {"question": 2, "prompt": "Past of 'see'", "answers": [
                    {"answer": "seen", "families": 1}
                ]}
            ]
        }]})
    );
}

#[tokio::test]
async fn a_suggestion_for_an_item_the_identifier_did_not_import_is_refused() {
    let (sharing, session, item) = shared().await;

    let answer = suggest(&sharing.library, FAMILY_A, &item, 1, 1, &["goed"]).await;

    assert_eq!(answer.status, StatusCode::FORBIDDEN);
    assert_eq!(answer.json(), json!({"error": "not_imported"}));
    let seen = suggestions(&sharing, &session).await;
    assert_eq!(seen.json(), json!({"items": []}));
}

#[tokio::test]
async fn a_suggestion_must_name_a_version_and_a_question_that_takes_answers() {
    let (sharing, _, item) = shared().await;
    let library = &sharing.library;
    import(library, FAMILY_A, &item).await;

    let unknown_version = suggest(library, FAMILY_A, &item, 2, 1, &["goed"]).await;
    assert_eq!(unknown_version.status, StatusCode::NOT_FOUND);
    assert_eq!(unknown_version.json(), json!({"error": "unknown_version"}));
    for question in [0, 3] {
        let answer = suggest(library, FAMILY_A, &item, 1, question, &["goed"]).await;
        assert_eq!(answer.status, StatusCode::BAD_REQUEST, "{question}");
        assert_eq!(answer.json(), json!({"error": "unknown_question"}));
    }
    let unknown_item = suggest(
        library,
        FAMILY_A,
        "00000000000000000000000000000000",
        1,
        1,
        &["goed"],
    )
    .await;
    assert_eq!(unknown_item.status, StatusCode::NOT_FOUND);
    assert_eq!(unknown_item.json(), json!({"error": "unknown_item"}));
}

#[tokio::test]
async fn a_suggestion_carries_one_to_ten_short_answers() {
    let (sharing, _, item) = shared().await;
    let library = &sharing.library;
    import(library, FAMILY_A, &item).await;
    let long = "é".repeat(101);
    let eleven: Vec<String> = (0..11).map(|n| format!("answer {n}")).collect();
    let eleven: Vec<&str> = eleven.iter().map(String::as_str).collect();

    for answers in [
        vec![],
        vec!["  "],
        vec![long.as_str()],
        vec!["a\nb"],
        eleven,
    ] {
        let answer = suggest(library, FAMILY_A, &item, 1, 1, &answers).await;
        assert_eq!(answer.status, StatusCode::BAD_REQUEST, "{answers:?}");
        assert_eq!(answer.json(), json!({"error": "bad_suggestion"}));
    }
    let longest = "é".repeat(100);
    let answer = suggest(library, FAMILY_A, &item, 1, 1, &[&longest]).await;
    assert_eq!(answer.status, StatusCode::NO_CONTENT);
}

#[tokio::test]
async fn a_malformed_identifier_or_body_is_refused() {
    let (sharing, _, item) = shared().await;
    let library = &sharing.library;

    let answer = suggest(library, "family-1", &item, 1, 1, &["goed"]).await;
    assert_eq!(answer.status, StatusCode::BAD_REQUEST);
    assert_eq!(answer.json(), json!({"error": "bad_library_identifier"}));

    for body in [
        b"not json".to_vec(),
        json!({"library_identifier": FAMILY_A, "item": item, "version": 1, "question": 1})
            .to_string()
            .into_bytes(),
        json!({"library_identifier": FAMILY_A, "item": item, "version": 1, "question": 1,
               "answers": ["goed"], "account": "Owl"})
        .to_string()
        .into_bytes(),
    ] {
        let answer = post_bytes(library, "/v1/suggestions", body).await;
        assert_eq!(answer.status, StatusCode::BAD_REQUEST);
        assert_eq!(answer.json(), json!({"error": "bad_request"}));
    }
}

#[tokio::test]
async fn a_suggestion_never_travels_with_a_session() {
    let (sharing, session, item) = shared().await;
    import(&sharing.library, FAMILY_A, &item).await;

    let answer = with_session(
        &sharing.library,
        "POST",
        "/v1/suggestions",
        &session,
        Some(
            json!({"library_identifier": FAMILY_A, "item": item, "version": 1,
                    "question": 1, "answers": ["goed"]}),
        ),
    )
    .await;

    assert_eq!(answer.status, StatusCode::BAD_REQUEST, "{}", text(&answer));
    let seen = suggestions(&sharing, &session).await;
    assert_eq!(seen.json(), json!({"items": []}));
}

#[tokio::test]
async fn an_answer_already_accepted_or_suggested_again_counts_nothing_more() {
    let (sharing, session, item) = shared().await;
    let library = &sharing.library;
    import(library, FAMILY_A, &item).await;

    suggest(library, FAMILY_A, &item, 1, 1, &["went", "goed"]).await;
    suggest(library, FAMILY_A, &item, 1, 1, &["goed"]).await;

    let seen = suggestions(&sharing, &session).await;
    assert_eq!(
        seen.json()["items"][0]["questions"],
        json!([{"question": 1, "prompt": "Past of 'go'", "answers": [
            {"answer": "goed", "families": 1}
        ]}])
    );
}

#[tokio::test]
async fn only_the_author_sees_the_suggestions_of_their_items() {
    let (sharing, _, item) = shared().await;
    import(&sharing.library, FAMILY_A, &item).await;
    suggest(&sharing.library, FAMILY_A, &item, 1, 1, &["goed"]).await;
    let other = sharing.author("subject-2", "Heron").await;

    let seen = suggestions(&sharing, &other).await;
    assert_eq!(seen.json(), json!({"items": []}));
    let accepted = accept(
        &sharing,
        &other,
        &item,
        json!([{"question": 1, "answers": ["goed"]}]),
    )
    .await;
    assert_eq!(accepted.status, StatusCode::NOT_FOUND);
    assert_eq!(accepted.json(), json!({"error": "unknown_item"}));
    let signed_out = get(&sharing.library, "/api/account/suggestions").await;
    assert_eq!(signed_out.status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn a_library_identifier_is_kept_only_with_its_imports_and_suggestions() {
    let (sharing, session, item) = shared().await;
    import(&sharing.library, FAMILY_A, &item).await;
    suggest(&sharing.library, FAMILY_A, &item, 1, 1, &["goed"]).await;

    let seen = suggestions(&sharing, &session).await;

    assert!(!text(&seen).contains(FAMILY_A));
    assert_eq!(
        tables_holding(&sharing.database, FAMILY_A).await,
        ["imports", "suggestions"]
    );
}

async fn accept(sharing: &Sharing, session: &str, item: &str, accepted: Value) -> Answer {
    with_session(
        &sharing.library,
        "POST",
        &format!("/api/account/items/{item}/suggestions/accept"),
        session,
        Some(json!({"accepted": accepted})),
    )
    .await
}

#[tokio::test]
async fn the_author_accepts_answers_into_a_prepared_new_version_for_the_share_route() {
    let sharing = with_sharing().await;
    let session = sharing.author(SUBJECT, "Owl").await;
    let mut body = prepared("Irregular verbs");
    use base64::Engine;
    body["media"] = json!([{"base64": base64::engine::general_purpose::STANDARD.encode(pixel())}]);
    let shared = sharing.share(&session, body.clone()).await;
    let item = shared.json()["item"].as_str().unwrap().to_string();
    let library = &sharing.library;
    import(library, FAMILY_A, &item).await;
    suggest(library, FAMILY_A, &item, 1, 1, &["goed", "has gone"]).await;
    suggest(library, FAMILY_A, &item, 1, 2, &["seen"]).await;

    let answer = accept(
        &sharing,
        &session,
        &item,
        json!([{"question": 1, "answers": ["goed"]}]),
    )
    .await;

    assert_eq!(answer.status, StatusCode::OK, "{}", text(&answer));
    let mut expected = prepared_again("Irregular verbs");
    expected["item"] = json!(item);
    expected["content"]["questions"][0]["answers"] = json!(["went", "goed"]);
    expected["media"] = body["media"].clone();
    assert_eq!(answer.json(), json!({"prepared": expected}));
}

#[tokio::test]
async fn only_suggested_answers_may_be_accepted() {
    let (sharing, session, item) = shared().await;
    import(&sharing.library, FAMILY_A, &item).await;
    suggest(&sharing.library, FAMILY_A, &item, 1, 1, &["goed"]).await;

    for accepted in [
        json!([{"question": 1, "answers": ["gone"]}]),
        json!([{"question": 2, "answers": ["goed"]}]),
    ] {
        let answer = accept(&sharing, &session, &item, accepted).await;
        assert_eq!(answer.status, StatusCode::CONFLICT);
        assert_eq!(answer.json(), json!({"error": "not_suggested"}));
    }
    for accepted in [json!([]), json!([{"question": 1, "answers": []}])] {
        let answer = accept(&sharing, &session, &item, accepted).await;
        assert_eq!(answer.status, StatusCode::BAD_REQUEST);
        assert_eq!(answer.json(), json!({"error": "bad_suggestion"}));
    }
}

#[tokio::test]
async fn suggestions_follow_their_question_into_the_latest_version_until_accepted() {
    let (sharing, session, item) = shared().await;
    let library = &sharing.library;
    import(library, FAMILY_A, &item).await;
    import(library, FAMILY_B, &item).await;
    suggest(library, FAMILY_A, &item, 1, 1, &["goed", "has gone"]).await;
    suggest(library, FAMILY_A, &item, 1, 2, &["seen"]).await;

    // Version 2 accepts "goed", asks about 'see' first and no longer about 'go' twice.
    let mut version = verbs("Irregular verbs");
    version.item = item.clone();
    version.content = json!({"questions": [
        {"type": "typed", "prompt": "Past of 'see'", "answers": ["saw"]},
        {"type": "typed", "prompt": "Past of 'go'", "answers": ["went", "goed"]}
    ]});
    library
        .publish_version(version, at("2026-11-28T10:00:00Z"))
        .await
        .unwrap();
    suggest(library, FAMILY_B, &item, 2, 1, &["seen"]).await;

    let seen = suggestions(&sharing, &session).await;

    assert_eq!(
        seen.json()["items"][0],
        json!({
            "item": item,
            "title": "Irregular verbs",
            "version": 2,
            "questions": [
                {"question": 1, "prompt": "Past of 'see'", "answers": [
                    {"answer": "seen", "families": 2}
                ]},
                {"question": 2, "prompt": "Past of 'go'", "answers": [
                    {"answer": "has gone", "families": 1}
                ]}
            ]
        })
    );
}

#[tokio::test]
async fn the_library_methods_record_and_read_suggestions_as_the_routes_do() {
    let (sharing, _, item) = shared().await;
    let library = &sharing.library;
    library.record_import(FAMILY_A, &item).await.unwrap();

    library
        .record_suggestion(FAMILY_A, &item, 1, 1, &["goed"])
        .await
        .unwrap();
    assert!(matches!(
        library
            .record_suggestion(FAMILY_B, &item, 1, 1, &["goed"])
            .await,
        Err(library::SuggestionError::NotImported)
    ));

    let suggestions = library.item_suggestions(&item).await.unwrap().unwrap();
    assert_eq!(suggestions.version, 1);
    assert_eq!(
        suggestions.questions,
        [library::QuestionSuggestions {
            question: 1,
            prompt: Some("Past of 'go'".into()),
            answers: vec![library::SuggestedAnswer {
                answer: "goed".into(),
                families: 1
            }],
        }]
    );
    assert_eq!(library.item_suggestions("y").await.unwrap(), None);
    let prepared = library
        .prepare_from_suggestions(
            &item,
            &[library::Accepted {
                question: 1,
                answers: vec!["goed".into()],
            }],
        )
        .await
        .unwrap();
    assert_eq!(
        prepared["content"]["questions"][0]["answers"],
        json!(["went", "goed"])
    );
}

#[tokio::test]
async fn the_prepared_new_version_goes_through_the_share_route_and_leaves_the_suggestions() {
    let (sharing, session, item) = shared().await;
    let library = &sharing.library;
    import(library, FAMILY_A, &item).await;
    suggest(library, FAMILY_A, &item, 1, 1, &["goed", "has gone"]).await;
    let accepted = accept(
        &sharing,
        &session,
        &item,
        json!([{"question": 1, "answers": ["goed"]}]),
    )
    .await;

    let shared = sharing
        .share(&session, accepted.json()["prepared"].clone())
        .await;

    assert_eq!(shared.status, StatusCode::CREATED, "{}", text(&shared));
    assert_eq!(shared.json()["item"], json!(item));
    assert_eq!(shared.json()["version"], 2);
    let asked = sharing.reviewer.asked();
    assert_eq!(asked.last().unwrap().version, 2);
    assert_eq!(
        asked.last().unwrap().content["questions"][0]["answers"],
        json!(["went", "goed"])
    );
    let seen = suggestions(&sharing, &session).await;
    assert_eq!(
        seen.json()["items"][0]["questions"],
        json!([{"question": 1, "prompt": "Past of 'go'", "answers": [
            {"answer": "has gone", "families": 1}
        ]}])
    );
}
