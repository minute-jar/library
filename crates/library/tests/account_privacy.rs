//! What a library account keeps to itself: its real name and e-mail are
//! never answered, kept only as identification data, and its session is
//! never joined to a library identifier (ADR-0006, story S9).

mod common;

use axum::http::StatusCode;
use common::identity::*;
use common::*;
use library::contract::Kind;
use serde_json::{Value, json};

/// Every route of the service, with what it is asked. A new route must be
/// added here.
fn every_route() -> Vec<(&'static str, String, Option<Value>)> {
    vec![
        ("GET", "/.well-known/minute-jar-library.json".into(), None),
        ("GET", "/v1/index".into(), None),
        ("GET", format!("/v1/items/{VERBS}/versions/1"), None),
        ("GET", "/v1/withdrawals".into(), None),
        ("GET", format!("/v1/media/{PIXEL}"), None),
        (
            "POST",
            "/v1/imports".into(),
            Some(json!({"library_identifier": FAMILY_A, "item": VERBS})),
        ),
        (
            "POST",
            "/v1/ratings".into(),
            Some(json!({"library_identifier": FAMILY_A, "item": VERBS, "stars": 5})),
        ),
        (
            "POST",
            "/v1/reports".into(),
            Some(
                json!({"library_identifier": FAMILY_A, "item": VERBS, "reason": "wrong_answer", "details": "Question 1."}),
            ),
        ),
        (
            "POST",
            "/api/reports".into(),
            Some(json!({"item": VERBS, "reason": "wrong_answer", "details": "Question 1."})),
        ),
        (
            "GET",
            "/v1/reports/00000000000000000000000000000000".into(),
            None,
        ),
        ("GET", format!("/api/items/{VERBS}"), None),
        ("GET", format!("/api/items/{VERBS}/versions/1/file"), None),
        ("GET", format!("/api/items/{VERBS}/ratings"), None),
        ("GET", "/api/catalogue".into(), None),
        ("GET", "/api/catalogue?q=Owl".into(), None),
        ("GET", "/api/catalogue/filters".into(), None),
        ("POST", "/api/sign-in".into(), Some(json!({"strong": true}))),
        (
            "POST",
            "/api/sign-in/callback".into(),
            Some(json!({"state": "x", "code": "y"})),
        ),
        ("GET", "/api/session".into(), None),
        ("GET", "/api/account".into(), None),
        (
            "POST",
            "/api/account".into(),
            Some(
                json!({"pseudonym": "Lark", "adult_and_parent": true, "minute_jar_account": "separate"}),
            ),
        ),
        (
            "PUT",
            "/api/account/pseudonym".into(),
            Some(json!({"pseudonym": "Barn owl"})),
        ),
        ("GET", "/api/moderation/queue".into(), None),
        ("GET", format!("/api/moderation/items/{VERBS}"), None),
        ("GET", "/api/moderation/figures/2026".into(), None),
        (
            "POST",
            "/api/moderation/reports/00000000000000000000000000000000/decision".into(),
            Some(json!({"outcome": "rejected", "reasons": "Nothing wrong."})),
        ),
        (
            "POST",
            "/api/moderation/appeals/00000000000000000000000000000000/decision".into(),
            Some(json!({"outcome": "maintained", "reasons": "It stands."})),
        ),
        (
            "POST",
            "/v1/reports/00000000000000000000000000000000/appeal".into(),
            Some(json!({"statement": "Please look again."})),
        ),
        (
            "POST",
            "/api/reports/00000000000000000000000000000000/appeal".into(),
            Some(json!({"statement": "Please look again."})),
        ),
        (
            "GET",
            "/v1/appeals/00000000000000000000000000000000".into(),
            None,
        ),
        // Closing ends the session: last but one.
        ("DELETE", "/api/account".into(), None),
        ("DELETE", "/api/session".into(), None),
    ]
}

/// The parts of the e-mail and the Minute Jar account that must never be answered.
const SECRETS: [&str; 3] = [EMAIL, "real.name", SUBJECT];

fn assert_keeps_secrets(answer: &Answer, what: &str) {
    let mut text = String::from_utf8_lossy(&answer.body).to_lowercase();
    for (name, value) in &answer.headers {
        text.push_str(&format!("\n{name}: {}", value.to_str().unwrap_or("")));
    }
    for secret in SECRETS {
        assert!(
            !text.contains(&secret.to_lowercase()),
            "{what} answers {secret:?}: {text}"
        );
    }
}

#[tokio::test]
async fn no_route_answers_the_e_mail_or_the_minute_jar_account() {
    let (library, provider, _) = with_accounts().await;
    library
        .publish_version(
            library::NewVersion {
                media: vec![pixel()],
                ..verbs("Irregular verbs")
            },
            at("2026-11-20T18:00:00Z"),
        )
        .await
        .unwrap();
    import(&library, FAMILY_A, VERBS).await;
    // A moderator too, so that the moderation routes answer what they hold.
    library
        .grant_moderator(SUBJECT, "Owl", signed_in_at())
        .await
        .unwrap();
    let callback = sign_in_with(&library, &provider, passkey(SUBJECT, signed_in_at())).await;
    assert_keeps_secrets(&callback, "the sign-in's callback");
    let session = callback.json()["session"].as_str().unwrap().to_string();
    assert_eq!(
        open_account(&library, &session, "Owl").await.status,
        StatusCode::CREATED
    );

    for (method, path, body) in every_route() {
        let anonymous = match (method, &body) {
            ("GET", None) => get(&library, &path).await,
            (_, Some(body)) if method == "POST" => post_json(&library, &path, body).await,
            _ => with_session(&library, method, &path, "", body.clone()).await,
        };
        assert_keeps_secrets(&anonymous, &format!("{method} {path}"));
        let signed_in = with_session(&library, method, &path, &session, body).await;
        assert_keeps_secrets(&signed_in, &format!("{method} {path}, signed in"));
    }
}

#[tokio::test]
async fn the_e_mail_is_kept_only_as_the_identification_of_a_library_account() {
    let (library, provider, clock, database) = with_accounts_and_database().await;

    let session = session(&library, &provider, passkey(SUBJECT, signed_in_at())).await;
    open_account(&library, &session, "Owl").await;

    assert_eq!(tables_holding(&database, EMAIL).await, ["identifications"]);
    clock.advance(jiff::SignedDuration::from_hours(1));
    session_with_account_again(&library, &provider, clock.now()).await;
    assert_eq!(tables_holding(&database, EMAIL).await, ["identifications"]);
}

async fn session_with_account_again(
    library: &library::Library,
    provider: &FakeProvider,
    now: jiff::Timestamp,
) {
    let again = session(library, provider, passkey(SUBJECT, now)).await;
    let current = with_session(library, "GET", "/api/session", &again, None).await;
    assert_eq!(current.json()["account"]["pseudonym"], "Owl");
}

#[tokio::test]
async fn without_a_library_account_the_e_mail_is_forgotten_once_the_sign_in_is_not_recent() {
    let (library, provider, clock, database) = with_accounts_and_database().await;
    session(
        &library,
        &provider,
        email_code("subject-weak", signed_in_at()),
    )
    .await;
    session(&library, &provider, passkey(SUBJECT, signed_in_at())).await;
    assert_eq!(
        tables_holding(&database, EMAIL).await,
        ["sessions"],
        "kept, for a strong sign-in only, until the account is opened"
    );

    clock.advance(jiff::SignedDuration::from_mins(16));
    library.forget_sign_ins(clock.now()).await.unwrap();

    assert!(tables_holding(&database, EMAIL).await.is_empty());
}

#[tokio::test]
async fn forgetting_removes_expired_sessions_and_sign_ins_that_never_came_back() {
    let (library, provider, clock, database) = with_accounts_and_database().await;
    let state = start(&library, false).await.json()["state"]
        .as_str()
        .unwrap()
        .to_string();
    let session = session(&library, &provider, passkey(SUBJECT, signed_in_at())).await;

    clock.advance(jiff::SignedDuration::from_hours(30 * 24));
    library.forget_sign_ins(clock.now()).await.unwrap();

    assert!(tables_holding(&database, &state).await.is_empty());
    assert!(tables_holding(&database, SUBJECT).await.is_empty());
    let after = with_session(&library, "GET", "/api/session", &session, None).await;
    assert_eq!(after.status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn a_family_server_call_carrying_a_session_is_refused_and_not_recorded() {
    let (library, provider, _) = with_accounts().await;
    library
        .publish_version(verbs("Irregular verbs"), at("2026-11-20T18:00:00Z"))
        .await
        .unwrap();
    let session = session(&library, &provider, passkey(SUBJECT, signed_in_at())).await;
    open_account(&library, &session, "Owl").await;

    let import = with_session(
        &library,
        "POST",
        "/v1/imports",
        &session,
        Some(json!({"library_identifier": FAMILY_A, "item": VERBS})),
    )
    .await;

    assert_eq!(import.status, StatusCode::BAD_REQUEST);
    assert_eq!(import.json(), json!({"error": "session_not_allowed"}));
    library.refresh(at("2026-11-26T09:00:00Z")).await.unwrap();
    let index = fetch_verified(&library, "/v1/index", Kind::Index).await;
    assert_eq!(index["items"][0]["signals"]["families"], 0);
}

#[tokio::test]
async fn an_account_route_refuses_a_library_identifier() {
    let (library, provider, _) = with_accounts().await;
    let session = session(&library, &provider, passkey(SUBJECT, signed_in_at())).await;

    let opening = with_session(
        &library,
        "POST",
        "/api/account",
        &session,
        Some(json!({
            "pseudonym": "Owl",
            "adult_and_parent": true,
            "minute_jar_account": "separate",
            "library_identifier": FAMILY_A,
        })),
    )
    .await;
    let start = post_json(
        &library,
        "/api/sign-in",
        &json!({"strong": true, "library_identifier": FAMILY_A}),
    )
    .await;

    assert_eq!(opening.json(), json!({"error": "bad_request"}));
    assert_eq!(start.json(), json!({"error": "bad_request"}));
}
