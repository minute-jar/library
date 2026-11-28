//! The editorial account and the Selection mark (ADR-0002, story S5): the
//! operator marks one library account as its editorial account, whose items
//! are shown as the software's publisher's, AI-generated and read by a
//! person; the operator's team sets and removes the Selection mark.

mod common;

use axum::http::StatusCode;
use common::cards::text;
use common::identity::*;
use common::shares::*;
use common::*;
use library::contract::Kind;
use serde_json::{Value, json};

const EDITORIAL: &str = "editorial-account-subject";

/// A library with sharing, an editorial account marked by the operator, and
/// its strong session.
async fn editorial() -> (Sharing, String) {
    let sharing = with_sharing().await;
    let session = sharing.author(EDITORIAL, "Minute Jar Editions").await;
    sharing
        .library
        .mark_editorial(EDITORIAL, sharing.clock.now())
        .await
        .unwrap();
    (sharing, session)
}

/// Shares `body` under `session`; answers the published item.
async fn published(sharing: &Sharing, session: &str, body: Value) -> String {
    let shared = sharing.share(session, body).await;
    assert_eq!(shared.status, StatusCode::CREATED, "{}", text(&shared));
    shared.json()["item"].as_str().unwrap().to_string()
}

async fn index_entry(sharing: &Sharing, item: &str) -> Value {
    let index = fetch_verified(&sharing.library, "/v1/index", Kind::Index).await;
    index["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|entry| entry["item"] == item)
        .cloned()
        .unwrap_or(Value::Null)
}

async fn catalogue_entry(sharing: &Sharing, query: &str, item: &str) -> Value {
    let answer = get(&sharing.library, &format!("/api/catalogue{query}")).await;
    assert_eq!(answer.status, StatusCode::OK, "{}", text(&answer));
    answer.json()["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|entry| entry["item"] == item)
        .cloned()
        .unwrap_or(Value::Null)
}

async fn item_page(sharing: &Sharing, item: &str) -> Value {
    let answer = get(&sharing.library, &format!("/api/items/{item}")).await;
    assert_eq!(answer.status, StatusCode::OK, "{}", text(&answer));
    answer.json()
}

/// A share from the editorial account, read by a person.
fn read_by(read: Value) -> Value {
    let mut body = prepared("Fractions, Year 4");
    body["editorial"] = json!({"ai_generated": true, "read": read});
    body
}

fn read_by_operator() -> Value {
    read_by(json!({"by": "operator", "at": "2026-11-26T08:00:00Z"}))
}

#[tokio::test]
async fn the_editorial_accounts_items_are_shown_as_the_publishers() {
    let (sharing, session) = editorial().await;

    let item = published(&sharing, &session, read_by_operator()).await;

    let signed = fetch_verified(
        &sharing.library,
        &format!("/v1/items/{item}/versions/1"),
        Kind::Item,
    )
    .await;
    let author = json!({"pseudonym": "Minute Jar Editions", "editorial": true});
    assert_eq!(signed["author"], author);
    assert_eq!(index_entry(&sharing, &item).await["author"], author);
    assert_eq!(catalogue_entry(&sharing, "", &item).await["author"], author);
    assert_eq!(item_page(&sharing, &item).await["author"], author);
}

#[tokio::test]
async fn other_accounts_items_are_not_editorial() {
    let (sharing, _) = editorial().await;
    let parent = sharing.author(SUBJECT, "Owl").await;

    let item = published(&sharing, &parent, prepared("Verbs")).await;

    assert_eq!(
        index_entry(&sharing, &item).await["author"],
        json!({"pseudonym": "Owl", "editorial": false})
    );
    assert_eq!(item_page(&sharing, &item).await["editorial"], Value::Null);
}

#[tokio::test]
async fn the_item_page_says_the_editorial_item_was_generated_by_ai_and_read_by_the_operator() {
    let (sharing, session) = editorial().await;

    let item = published(&sharing, &session, read_by_operator()).await;

    assert_eq!(
        item_page(&sharing, &item).await["editorial"],
        json!({"ai_generated": true, "read": {"by": "operator", "at": "2026-11-26T08:00:00Z"}})
    );
}

#[tokio::test]
async fn a_moderator_who_read_the_item_is_named_by_their_handle() {
    let (sharing, session) = editorial().await;
    sharing
        .library
        .grant_moderator("moderator-heron-subject", "Heron", sharing.clock.now())
        .await
        .unwrap();

    let item = published(
        &sharing,
        &session,
        read_by(json!({"by": "moderator", "handle": "heron", "at": "2026-11-26T08:00:00Z"})),
    )
    .await;

    assert_eq!(
        item_page(&sharing, &item).await["editorial"]["read"],
        json!({"by": "moderator", "handle": "Heron", "at": "2026-11-26T08:00:00Z"})
    );
}

#[tokio::test]
async fn an_editorial_share_names_a_moderator_and_a_past_instant() {
    let (sharing, session) = editorial().await;

    for read in [
        json!({"by": "moderator", "handle": "Nobody", "at": "2026-11-26T08:00:00Z"}),
        json!({"by": "moderator", "at": "2026-11-26T08:00:00Z"}),
        json!({"by": "operator", "handle": "Heron", "at": "2026-11-26T08:00:00Z"}),
        json!({"by": "operator", "at": "2030-01-01T00:00:00Z"}),
        json!({"by": "operator", "at": "yesterday"}),
    ] {
        let shared = sharing.share(&session, read_by(read.clone())).await;
        assert_eq!(shared.status, StatusCode::BAD_REQUEST, "{read}");
        assert_eq!(shared.json(), json!({"error": "bad_reading"}), "{read}");
    }
    assert!(
        sharing.review_provider.asked().is_empty(),
        "nothing was reviewed"
    );
}

#[tokio::test]
async fn the_editorial_account_says_who_read_what_it_shares() {
    let (sharing, session) = editorial().await;

    let shared = sharing.share(&session, prepared("Fractions")).await;

    assert_eq!(shared.status, StatusCode::BAD_REQUEST);
    assert_eq!(shared.json(), json!({"error": "reading_required"}));
}

#[tokio::test]
async fn only_the_editorial_account_carries_its_marks() {
    let (sharing, _) = editorial().await;
    let parent = sharing.author(SUBJECT, "Owl").await;

    let shared = sharing.share(&parent, read_by_operator()).await;

    assert_eq!(shared.status, StatusCode::FORBIDDEN);
    assert_eq!(shared.json(), json!({"error": "not_editorial"}));
}

#[tokio::test]
async fn there_is_one_editorial_account() {
    let (sharing, _) = editorial().await;
    sharing.author(SUBJECT, "Owl").await;

    let marked = sharing
        .library
        .mark_editorial(SUBJECT, sharing.clock.now())
        .await;

    assert!(matches!(
        marked,
        Err(library::EditorialError::AnotherEditorialAccount)
    ));
    let again = sharing
        .library
        .mark_editorial(EDITORIAL, sharing.clock.now())
        .await;
    assert!(again.is_ok(), "marking it again changes nothing");
}

#[tokio::test]
async fn only_a_library_account_is_marked_editorial() {
    let sharing = with_sharing().await;

    let marked = sharing
        .library
        .mark_editorial("no-account-subject", sharing.clock.now())
        .await;

    assert!(matches!(marked, Err(library::EditorialError::NoAccount)));
}

#[tokio::test]
async fn an_unmarked_account_shares_as_any_parent_and_keeps_its_signed_versions() {
    let (sharing, session) = editorial().await;
    let first = published(&sharing, &session, read_by_operator()).await;

    assert!(sharing.library.unmark_editorial(EDITORIAL).await.unwrap());
    let second = published(&sharing, &session, prepared_again("Decimals")).await;

    assert_eq!(
        index_entry(&sharing, &first).await["author"]["editorial"],
        true
    );
    assert_eq!(
        index_entry(&sharing, &second).await["author"]["editorial"],
        false
    );
}

#[tokio::test]
async fn a_held_editorial_share_keeps_its_marks_once_a_moderator_publishes_it() {
    let (sharing, session) = editorial().await;
    sharing
        .library
        .grant_moderator("moderator-heron-subject", "Heron", sharing.clock.now())
        .await
        .unwrap();
    sharing
        .review_provider
        .answer(Some(library::ReviewOutcome::Held {
            findings: "Question 1 may be wrong.".into(),
        }));
    let shared = sharing.share(&session, read_by_operator()).await;
    assert_eq!(shared.status, StatusCode::ACCEPTED, "{}", text(&shared));
    let shared = shared.json();

    sharing
        .library
        .decide_share(
            "moderator-heron-subject",
            shared["share"].as_str().unwrap(),
            library::HeldDecision {
                outcome: library::HeldOutcome::Published,
                reason: library::Reason::WrongAnswer,
                reasons: "Question 1 is right.".into(),
            },
            sharing.clock.now(),
        )
        .await
        .unwrap();

    let item = shared["item"].as_str().unwrap();
    let page = item_page(&sharing, item).await;
    assert_eq!(page["author"]["editorial"], true);
    assert_eq!(page["editorial"]["read"]["by"], "operator");
}

const HERON: &str = "moderator-heron-subject";

/// A library with sharing, a moderator, and a published item; answers the
/// moderator's passkey session and the item.
async fn with_item() -> (Sharing, String, String) {
    let sharing = with_sharing().await;
    sharing
        .library
        .grant_moderator(HERON, "Heron", sharing.clock.now())
        .await
        .unwrap();
    let parent = sharing.author(SUBJECT, "Owl").await;
    let item = published(&sharing, &parent, prepared("Verbs")).await;
    let moderator = session(
        &sharing.library,
        &sharing.provider,
        passkey(HERON, sharing.clock.now()),
    )
    .await;
    (sharing, moderator, item)
}

async fn selection(sharing: &Sharing, method: &str, session: &str, item: &str) -> Answer {
    with_session(
        &sharing.library,
        method,
        &format!("/api/moderation/items/{item}/selection"),
        session,
        None,
    )
    .await
}

#[tokio::test]
async fn a_moderator_marks_an_item_selection_in_the_index_the_catalogue_and_its_page() {
    let (sharing, moderator, item) = with_item().await;
    assert_eq!(index_entry(&sharing, &item).await["selection"], false);

    let marked = selection(&sharing, "PUT", &moderator, &item).await;

    assert_eq!(marked.status, StatusCode::NO_CONTENT, "{}", text(&marked));
    assert_eq!(index_entry(&sharing, &item).await["selection"], true);
    assert_eq!(
        catalogue_entry(&sharing, "", &item).await["selection"],
        true
    );
    assert_eq!(item_page(&sharing, &item).await["selection"], true);
}

#[tokio::test]
async fn a_moderator_removes_the_selection_mark() {
    let (sharing, moderator, item) = with_item().await;
    selection(&sharing, "PUT", &moderator, &item).await;

    let removed = selection(&sharing, "DELETE", &moderator, &item).await;

    assert_eq!(removed.status, StatusCode::NO_CONTENT, "{}", text(&removed));
    assert_eq!(index_entry(&sharing, &item).await["selection"], false);
    assert_eq!(item_page(&sharing, &item).await["selection"], false);
}

#[tokio::test]
async fn the_catalogue_is_filtered_to_the_selection() {
    let (sharing, _, chosen) = with_item().await;
    let parent = sharing.author("parent-lark-subject", "Lark").await;
    let other = published(&sharing, &parent, prepared("Fractions")).await;
    sharing
        .library
        .select(HERON, &chosen, sharing.clock.now())
        .await
        .unwrap();

    let answer = get(&sharing.library, "/api/catalogue?selection=true").await;

    assert_eq!(answer.status, StatusCode::OK, "{}", text(&answer));
    let answer = answer.json();
    let items: Vec<&str> = answer["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|entry| entry["item"].as_str().unwrap())
        .collect();
    assert_eq!(items, vec![chosen.as_str()]);
    assert_eq!(answer["total"], 1);
    assert!(!catalogue_entry(&sharing, "", &other).await.is_null());
    let bad = get(&sharing.library, "/api/catalogue?selection=maybe").await;
    assert_eq!(bad.status, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn only_a_moderator_signed_in_with_a_passkey_sets_the_selection_mark() {
    let (sharing, _, item) = with_item().await;
    let parent = session(
        &sharing.library,
        &sharing.provider,
        passkey("parent-lark-subject", sharing.clock.now()),
    )
    .await;
    let weak = session(
        &sharing.library,
        &sharing.provider,
        email_code(HERON, sharing.clock.now()),
    )
    .await;

    let by_parent = selection(&sharing, "PUT", &parent, &item).await;
    let without_passkey = selection(&sharing, "PUT", &weak, &item).await;
    let signed_out = selection(&sharing, "PUT", "no-such-session", &item).await;

    assert_eq!(by_parent.status, StatusCode::FORBIDDEN);
    assert_eq!(by_parent.json(), json!({"error": "not_a_moderator"}));
    assert_eq!(without_passkey.status, StatusCode::FORBIDDEN);
    assert_eq!(
        without_passkey.json(),
        json!({"error": "passkey_sign_in_required"})
    );
    assert_eq!(signed_out.status, StatusCode::UNAUTHORIZED);
    assert_eq!(index_entry(&sharing, &item).await["selection"], false);
    let by_method = sharing
        .library
        .select("parent-lark-subject", &item, sharing.clock.now())
        .await;
    assert!(matches!(
        by_method,
        Err(library::SelectionError::NotAModerator)
    ));
}

#[tokio::test]
async fn only_an_item_the_catalogue_shows_is_marked_selection() {
    let (sharing, moderator, item) = with_item().await;
    sharing
        .library
        .withdraw(
            &item,
            None,
            library::Withdrawal::Author,
            sharing.clock.now(),
        )
        .await
        .unwrap();

    let withdrawn = selection(&sharing, "PUT", &moderator, &item).await;
    let unknown = selection(
        &sharing,
        "PUT",
        &moderator,
        "0123456789abcdef0123456789abcdee",
    )
    .await;

    for answer in [withdrawn, unknown] {
        assert_eq!(answer.status, StatusCode::NOT_FOUND);
        assert_eq!(answer.json(), json!({"error": "unknown_item"}));
    }
}

#[tokio::test]
async fn moderators_see_who_marked_an_item_selection_in_its_timeline() {
    let (sharing, moderator, item) = with_item().await;
    let path = format!("/api/moderation/items/{item}");
    let before = with_session(&sharing.library, "GET", &path, &moderator, None).await;
    assert_eq!(before.json()["selection"], Value::Null);

    selection(&sharing, "PUT", &moderator, &item).await;

    let after = with_session(&sharing.library, "GET", &path, &moderator, None).await;
    assert_eq!(
        after.json()["selection"],
        json!({"moderator": "Heron", "selected_at": "2026-11-27T10:00:00Z"})
    );
}
