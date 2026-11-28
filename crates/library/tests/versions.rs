//! New versions and derived works (stories S14, S15): an author shares a
//! new version of their own item, reviewed again like a first share; a
//! derived work of someone else's item keeps the original's attribution and
//! links to it.

mod common;

use axum::http::StatusCode;
use common::cards::text;
use common::identity::*;
use common::shares::*;
use common::*;
use library::contract::Kind;
use library::{HeldDecision, HeldOutcome, Reason, ReviewOutcome, Withdrawal};
use serde_json::{Value, json};

/// A prepared new version of `item`, as the parent app sends it.
fn new_version_of(item: &str, title: &str) -> Value {
    let mut body = prepared_again(title);
    body["item"] = json!(item);
    body
}

/// Shares a first version under `session`; answers the item.
async fn first_share(sharing: &Sharing, session: &str, title: &str) -> String {
    let shared = sharing.share(session, prepared(title)).await;
    assert_eq!(shared.status, StatusCode::CREATED, "{}", text(&shared));
    shared.json()["item"].as_str().unwrap().to_string()
}

#[tokio::test]
async fn an_author_shares_a_new_version_of_their_item_reviewed_again() {
    let sharing = with_sharing().await;
    let session = sharing.author(SUBJECT, "Owl").await;
    let item = first_share(&sharing, &session, "Verbs").await;

    let shared = sharing
        .share(&session, new_version_of(&item, "Verbs, corrected"))
        .await;

    assert_eq!(shared.status, StatusCode::CREATED, "{}", text(&shared));
    let answer = shared.json();
    assert_eq!(answer["item"], item.as_str());
    assert_eq!(answer["version"], 2);
    let asked = sharing.review_provider.asked();
    assert_eq!(asked.len(), 2, "every version is reviewed");
    assert_eq!(asked[1].item, item);
    assert_eq!(asked[1].version, 2);
    assert_eq!(asked[1].title, "Verbs, corrected");
    let index = fetch_verified(&sharing.library, "/v1/index", Kind::Index).await;
    assert_eq!(index["items"].as_array().unwrap().len(), 1);
    assert_eq!(index["items"][0]["version"], 2);
    assert_eq!(index["items"][0]["title"], "Verbs, corrected");
    let version = fetch_verified(
        &sharing.library,
        &format!("/v1/items/{item}/versions/2"),
        Kind::Item,
    )
    .await;
    assert_eq!(version["title"], "Verbs, corrected");
}

/// The same body, accepting the sharing terms at a first share.
fn with_terms(mut body: Value) -> Value {
    body["sharing_terms"] = json!(TERMS);
    body
}

/// A prepared first share derived from `version` of `item`.
fn derived_from(item: &str, version: u32, title: &str) -> Value {
    let mut body = prepared_again(title);
    body["derived_from"] = json!({"item": item, "version": version});
    body
}

/// `session`, its account having accepted the sharing terms on the website.
async fn with_terms_accepted(sharing: &Sharing, session: String) -> String {
    let accepted = with_session(
        &sharing.library,
        "PUT",
        "/api/account/sharing-terms",
        &session,
        Some(json!({"version": TERMS})),
    )
    .await;
    assert_eq!(accepted.status, StatusCode::OK, "{}", text(&accepted));
    session
}

const MODERATOR: &str = "moderator-alice-subject";

async fn grant_moderator(sharing: &Sharing) {
    sharing
        .library
        .grant_moderator(MODERATOR, "Alice", signed_in_at())
        .await
        .unwrap();
}

async fn decide(sharing: &Sharing, share: &Value, outcome: HeldOutcome, reasons: &str) {
    sharing
        .library
        .decide_share(
            MODERATOR,
            share["share"].as_str().unwrap(),
            HeldDecision {
                outcome,
                reason: Reason::PersonalData,
                reasons: reasons.into(),
            },
            sharing.clock.now(),
        )
        .await
        .unwrap();
}

fn held_for_a_moderator(sharing: &Sharing) {
    sharing.review_provider.answer(Some(ReviewOutcome::Held {
        findings: "Question 2 may name a school.".into(),
    }));
}

#[tokio::test]
async fn only_the_authors_account_adds_a_version_to_an_item() {
    let sharing = with_sharing().await;
    let owl = sharing.author(SUBJECT, "Owl").await;
    let item = first_share(&sharing, &owl, "Verbs").await;
    let lark = sharing.author("subject-lark", "Lark").await;

    let refused = sharing
        .share(&lark, with_terms(new_version_of(&item, "Verbs, mine now")))
        .await;
    assert_eq!(refused.status, StatusCode::FORBIDDEN, "{}", text(&refused));
    assert_eq!(refused.json(), json!({"error": "not_the_author"}));

    let unknown = sharing
        .share(&owl, new_version_of(&"0".repeat(32), "Verbs"))
        .await;
    assert_eq!(unknown.status, StatusCode::NOT_FOUND, "{}", text(&unknown));
    assert_eq!(unknown.json(), json!({"error": "unknown_item"}));

    assert_eq!(
        sharing.review_provider.asked().len(),
        1,
        "refused before the review"
    );
    let index = fetch_verified(&sharing.library, "/v1/index", Kind::Index).await;
    assert_eq!(index["items"][0]["version"], 1);
    assert_eq!(index["items"][0]["author"]["pseudonym"], "Owl");
}

#[tokio::test]
async fn a_withdrawn_item_takes_no_new_version() {
    let sharing = with_sharing().await;
    let session = sharing.author(SUBJECT, "Owl").await;
    let item = first_share(&sharing, &session, "Verbs").await;
    sharing
        .library
        .withdraw(&item, None, Withdrawal::Author, sharing.clock.now())
        .await
        .unwrap();

    let refused = sharing
        .share(&session, new_version_of(&item, "Verbs"))
        .await;

    assert_eq!(refused.status, StatusCode::CONFLICT, "{}", text(&refused));
    assert_eq!(refused.json(), json!({"error": "withdrawn_item"}));
}

#[tokio::test]
async fn a_new_version_refused_by_a_moderator_leaves_the_item_its_author() {
    let sharing = with_sharing().await;
    grant_moderator(&sharing).await;
    let session = sharing.author(SUBJECT, "Owl").await;
    let item = first_share(&sharing, &session, "Verbs").await;
    held_for_a_moderator(&sharing);
    let held = sharing
        .share(&session, new_version_of(&item, "Verbs, held"))
        .await;
    assert_eq!(held.status, StatusCode::ACCEPTED, "{}", text(&held));
    assert_eq!(held.json()["item"], item.as_str());
    assert_eq!(sharing.review_provider.asked()[1].version, 2);

    decide(
        &sharing,
        &held.json(),
        HeldOutcome::Refused,
        "Names a school.",
    )
    .await;

    sharing
        .review_provider
        .answer(Some(ReviewOutcome::Published));
    let shared = sharing
        .share(&session, new_version_of(&item, "Verbs, corrected"))
        .await;
    assert_eq!(shared.status, StatusCode::CREATED, "{}", text(&shared));
    assert_eq!(shared.json()["version"], 2);
}

#[tokio::test]
async fn a_derived_work_keeps_the_originals_attribution_and_links_to_it() {
    let sharing = with_sharing().await;
    let owl = sharing.author(SUBJECT, "Owl").await;
    let original = first_share(&sharing, &owl, "Verbs").await;
    // The original's author renames later: the attribution stays as the
    // original version was signed.
    with_session(
        &sharing.library,
        "PUT",
        "/api/account/pseudonym",
        &owl,
        Some(json!({"pseudonym": "Barn owl"})),
    )
    .await;
    let lark = sharing.author("subject-lark", "Lark").await;

    let shared = sharing
        .share(&lark, with_terms(derived_from(&original, 1, "More verbs")))
        .await;

    assert_eq!(shared.status, StatusCode::CREATED, "{}", text(&shared));
    let derived = shared.json()["item"].as_str().unwrap().to_string();
    assert_ne!(derived, original, "a derived work is an item of its own");
    let attribution = json!({"item": original, "version": 1, "pseudonym": "Owl"});
    let version = fetch_verified(
        &sharing.library,
        &format!("/v1/items/{derived}/versions/1"),
        Kind::Item,
    )
    .await;
    assert_eq!(version["author"]["pseudonym"], "Lark");
    assert_eq!(version["derived_from"], attribution);
    let index = fetch_verified(&sharing.library, "/v1/index", Kind::Index).await;
    assert_eq!(entry_of(&index, &derived)["derived_from"], attribution);
    let catalogue = get(&sharing.library, "/api/catalogue").await.json();
    assert_eq!(entry_of(&catalogue, &derived)["derived_from"], attribution);
    let page = get(&sharing.library, &format!("/api/items/{derived}")).await;
    assert_eq!(page.json()["derived_from"], attribution);
}

/// The entry of `item` in a list of `items`.
fn entry_of<'a>(listing: &'a Value, item: &str) -> &'a Value {
    listing["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|entry| entry["item"] == item)
        .unwrap()
}

#[tokio::test]
async fn a_new_version_of_a_derived_work_keeps_its_original() {
    let sharing = with_sharing().await;
    let owl = sharing.author(SUBJECT, "Owl").await;
    let original = first_share(&sharing, &owl, "Verbs").await;
    let other = first_share(&sharing, &owl, "Other verbs").await;
    let lark = sharing.author("subject-lark", "Lark").await;
    let derived = sharing
        .share(&lark, with_terms(derived_from(&original, 1, "More verbs")))
        .await
        .json()["item"]
        .as_str()
        .unwrap()
        .to_string();

    let shared = sharing
        .share(&lark, new_version_of(&derived, "More verbs, v2"))
        .await;
    assert_eq!(shared.status, StatusCode::CREATED, "{}", text(&shared));
    let version = fetch_verified(
        &sharing.library,
        &format!("/v1/items/{derived}/versions/2"),
        Kind::Item,
    )
    .await;
    assert_eq!(
        version["derived_from"],
        json!({"item": original, "version": 1, "pseudonym": "Owl"})
    );

    let mut elsewhere = new_version_of(&derived, "More verbs, v3");
    elsewhere["derived_from"] = json!({"item": other, "version": 1});
    let refused = sharing.share(&lark, elsewhere).await;
    assert_eq!(refused.status, StatusCode::CONFLICT, "{}", text(&refused));
    assert_eq!(refused.json(), json!({"error": "original_changed"}));
}

#[tokio::test]
async fn a_held_new_version_a_moderator_publishes_keeps_its_original() {
    let sharing = with_sharing().await;
    grant_moderator(&sharing).await;
    let owl = sharing.author(SUBJECT, "Owl").await;
    let original = first_share(&sharing, &owl, "Verbs").await;
    let lark = sharing.author("subject-lark", "Lark").await;
    let derived = sharing
        .share(&lark, with_terms(derived_from(&original, 1, "More verbs")))
        .await
        .json()["item"]
        .as_str()
        .unwrap()
        .to_string();
    held_for_a_moderator(&sharing);
    let held = sharing
        .share(&lark, new_version_of(&derived, "More verbs, v2"))
        .await
        .json();

    decide(
        &sharing,
        &held,
        HeldOutcome::Published,
        "No school is named.",
    )
    .await;

    let version = fetch_verified(
        &sharing.library,
        &format!("/v1/items/{derived}/versions/2"),
        Kind::Item,
    )
    .await;
    assert_eq!(version["title"], "More verbs, v2");
    assert_eq!(
        version["derived_from"],
        json!({"item": original, "version": 1, "pseudonym": "Owl"})
    );
}

#[tokio::test]
async fn a_derived_work_names_an_original_the_catalogue_shows() {
    let sharing = with_sharing().await;
    let owl = sharing.author(SUBJECT, "Owl").await;
    let original = first_share(&sharing, &owl, "Verbs").await;
    let lark = sharing.author("subject-lark", "Lark").await;
    let lark = with_terms_accepted(&sharing, lark).await;

    for named in [
        derived_from(&"0".repeat(32), 1, "Verbs"),
        derived_from(&original, 2, "Verbs"),
    ] {
        let refused = sharing.share(&lark, named).await;
        assert_eq!(refused.status, StatusCode::NOT_FOUND, "{}", text(&refused));
        assert_eq!(refused.json(), json!({"error": "unknown_original"}));
    }
    sharing
        .library
        .withdraw(&original, None, Withdrawal::Author, sharing.clock.now())
        .await
        .unwrap();
    let refused = sharing
        .share(&lark, derived_from(&original, 1, "Verbs"))
        .await;
    assert_eq!(refused.status, StatusCode::NOT_FOUND, "{}", text(&refused));
    assert_eq!(refused.json(), json!({"error": "unknown_original"}));

    let other = first_share(&sharing, &owl, "Other verbs").await;
    let mut itself = new_version_of(&other, "Other verbs");
    itself["derived_from"] = json!({"item": other, "version": 1});
    let refused = sharing.share(&owl, itself).await;
    assert_eq!(
        refused.status,
        StatusCode::BAD_REQUEST,
        "{}",
        text(&refused)
    );
    assert_eq!(refused.json(), json!({"error": "bad_original"}));
    assert_eq!(
        sharing.review_provider.asked().len(),
        2,
        "refused before the review"
    );
}
