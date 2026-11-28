//! Sign-in with a Minute Jar account and the library account opened on it
//! (`docs/specs/accounts-api.md`, ADR-0006), over a fake identity provider.

mod common;

use axum::http::StatusCode;
use common::identity::*;
use common::*;
use serde_json::json;

#[tokio::test]
async fn a_sign_in_through_the_provider_opens_a_session_without_a_library_account() {
    let (library, provider, _) = with_accounts().await;

    let started = start(&library, false).await;
    assert_eq!(started.status, StatusCode::OK);
    let url = started.json()["authorization_url"]
        .as_str()
        .unwrap()
        .to_string();
    let asked = provider.asked();
    assert_eq!(asked.len(), 1);
    assert!(url.starts_with("https://id.example/oauth2/auth?"));
    assert_eq!(started.json()["state"], json!(asked[0].state));

    let session = session(&library, &provider, passkey(SUBJECT, signed_in_at())).await;

    let answer = with_session(&library, "GET", "/api/session", &session, None).await;
    assert_eq!(answer.status, StatusCode::OK);
    assert_eq!(answer.json()["account"], json!(null));
    assert_eq!(
        answer.headers["cache-control"], "private, no-store",
        "a session's answers are never cached"
    );
}

#[tokio::test]
async fn a_recent_strong_sign_in_opens_a_library_account_under_a_pseudonym() {
    let (library, provider, _) = with_accounts().await;
    let session = session(&library, &provider, passkey(SUBJECT, signed_in_at())).await;

    let opened = with_session(
        &library,
        "POST",
        "/api/account",
        &session,
        Some(json!({
            "pseudonym": "  Owl  ",
            "adult_and_parent": true,
            "minute_jar_account": "same_as_subscription",
        })),
    )
    .await;

    let account = json!({
        "pseudonym": "Owl",
        "minute_jar_account": "same_as_subscription",
        "opened_at": "2026-11-27T10:00:00Z",
        "show_earlier_pseudonyms": false,
    });
    assert_eq!(opened.status, StatusCode::CREATED);
    assert_eq!(opened.json(), account);
    let current = with_session(&library, "GET", "/api/session", &session, None).await;
    assert_eq!(current.json()["account"], account);
    assert_eq!(
        current.json()["strong_sign_in_until"],
        json!("2026-11-27T10:15:00Z")
    );
    let read = with_session(&library, "GET", "/api/account", &session, None).await;
    assert_eq!(read.json(), account);
}

#[tokio::test]
async fn the_next_sign_in_finds_the_library_account() {
    let (library, provider, clock) = with_accounts().await;
    let first = session(&library, &provider, passkey(SUBJECT, signed_in_at())).await;
    open_account(&library, &first, "Owl").await;
    clock.advance(jiff::SignedDuration::from_hours(48));

    let later = session(&library, &provider, email_code(SUBJECT, clock.now())).await;

    let current = with_session(&library, "GET", "/api/session", &later, None).await;
    assert_eq!(current.json()["account"]["pseudonym"], "Owl");
    assert_eq!(current.json()["strong_sign_in_until"], json!(null));
}

#[tokio::test]
async fn a_sign_in_that_is_not_strong_cannot_open_an_account() {
    let (library, provider, _) = with_accounts().await;
    let session = session(&library, &provider, email_code(SUBJECT, signed_in_at())).await;

    let answer = open_account(&library, &session, "Owl").await;

    assert_eq!(answer.status, StatusCode::FORBIDDEN);
    assert_eq!(answer.json(), json!({"error": "strong_sign_in_required"}));
    let current = with_session(&library, "GET", "/api/session", &session, None).await;
    assert_eq!(current.json()["account"], json!(null));
}

#[tokio::test]
async fn a_password_with_a_second_factor_or_an_external_provider_is_strong() {
    let (library, provider, _) = with_accounts().await;
    let totp = library::IdToken {
        acr: Some("aal2".into()),
        amr: vec!["password".into(), "totp".into()],
        ..passkey("subject-totp", signed_in_at())
    };
    let google = library::IdToken {
        amr: vec!["oidc".into()],
        ..passkey("subject-google", signed_in_at())
    };
    let password = library::IdToken {
        acr: Some("aal1".into()),
        amr: vec!["password".into()],
        ..passkey("subject-password", signed_in_at())
    };

    let totp = session(&library, &provider, totp).await;
    let google = session(&library, &provider, google).await;
    let password = session(&library, &provider, password).await;

    assert_eq!(
        open_account(&library, &totp, "Owl").await.status,
        StatusCode::CREATED
    );
    assert_eq!(
        open_account(&library, &google, "Lark").await.status,
        StatusCode::CREATED
    );
    assert_eq!(
        open_account(&library, &password, "Wren").await.status,
        StatusCode::FORBIDDEN
    );
}

#[tokio::test]
async fn a_strong_sign_in_older_than_fifteen_minutes_must_be_made_again() {
    let (library, provider, clock) = with_accounts().await;
    let old = session(
        &library,
        &provider,
        passkey(
            SUBJECT,
            signed_in_at() - jiff::SignedDuration::from_hours(2),
        ),
    )
    .await;
    let session = session(&library, &provider, passkey(SUBJECT, signed_in_at())).await;
    clock.advance(jiff::SignedDuration::from_mins(16));

    assert_eq!(
        open_account(&library, &old, "Owl").await.status,
        StatusCode::FORBIDDEN
    );
    let answer = open_account(&library, &session, "Owl").await;
    assert_eq!(answer.status, StatusCode::FORBIDDEN);
    assert_eq!(answer.json(), json!({"error": "strong_sign_in_required"}));

    let started = start(&library, true).await;
    assert_eq!(started.status, StatusCode::OK);
    assert!(
        provider.asked().last().unwrap().strong,
        "the provider is asked for a strong sign-in"
    );
}

#[tokio::test]
async fn opening_an_account_needs_the_adult_and_parent_statement_and_a_choice() {
    let (library, provider, _) = with_accounts().await;
    let session = session(&library, &provider, passkey(SUBJECT, signed_in_at())).await;

    let without_statement = with_session(
        &library,
        "POST",
        "/api/account",
        &session,
        Some(json!({"pseudonym": "Owl", "adult_and_parent": false, "minute_jar_account": "separate"})),
    )
    .await;
    let without_choice = with_session(
        &library,
        "POST",
        "/api/account",
        &session,
        Some(json!({"pseudonym": "Owl", "adult_and_parent": true})),
    )
    .await;
    let unknown_choice = with_session(
        &library,
        "POST",
        "/api/account",
        &session,
        Some(json!({"pseudonym": "Owl", "adult_and_parent": true, "minute_jar_account": "both"})),
    )
    .await;

    assert_eq!(without_statement.status, StatusCode::BAD_REQUEST);
    assert_eq!(
        without_statement.json(),
        json!({"error": "statement_required"})
    );
    assert_eq!(without_choice.json(), json!({"error": "bad_request"}));
    assert_eq!(unknown_choice.json(), json!({"error": "bad_request"}));
}

#[tokio::test]
async fn a_pseudonym_is_short_printable_unique_and_not_an_address() {
    let (library, provider, _) = with_accounts().await;
    let first = session(&library, &provider, passkey("subject-1", signed_in_at())).await;
    let second = session(&library, &provider, passkey("subject-2", signed_in_at())).await;
    assert_eq!(
        open_account(&library, &first, "Owl").await.status,
        StatusCode::CREATED
    );

    for bad in [
        "",
        "  ",
        "ab",
        &"x".repeat(33),
        "owl@example.org",
        "Owl\u{7}",
    ] {
        let answer = open_account(&library, &second, bad).await;
        assert_eq!(answer.status, StatusCode::BAD_REQUEST, "{bad:?}");
        assert_eq!(answer.json(), json!({"error": "bad_pseudonym"}), "{bad:?}");
    }
    for taken in ["owl", "OWL", "Minute Jar", "minute jar"] {
        let answer = open_account(&library, &second, taken).await;
        assert_eq!(answer.status, StatusCode::CONFLICT, "{taken:?}");
        assert_eq!(
            answer.json(),
            json!({"error": "pseudonym_taken"}),
            "{taken:?}"
        );
    }
    assert_eq!(
        open_account(&library, &second, "Élan vert").await.status,
        StatusCode::CREATED
    );
}

#[tokio::test]
async fn a_minute_jar_account_opens_one_library_account() {
    let (library, provider, _) = with_accounts().await;
    let session = session(&library, &provider, passkey(SUBJECT, signed_in_at())).await;
    open_account(&library, &session, "Owl").await;

    let again = open_account(&library, &session, "Lark").await;

    assert_eq!(again.status, StatusCode::CONFLICT);
    assert_eq!(again.json(), json!({"error": "account_exists"}));
}

#[tokio::test]
async fn the_pseudonym_can_be_changed() {
    let (library, provider, clock) = with_accounts().await;
    let first = session(&library, &provider, passkey("subject-1", signed_in_at())).await;
    let second = session(&library, &provider, passkey("subject-2", signed_in_at())).await;
    open_account(&library, &first, "Owl").await;
    open_account(&library, &second, "Lark").await;
    clock.advance(jiff::SignedDuration::from_hours(1));

    let changed = with_session(
        &library,
        "PUT",
        "/api/account/pseudonym",
        &first,
        Some(json!({"pseudonym": "Barn owl"})),
    )
    .await;
    let taken = with_session(
        &library,
        "PUT",
        "/api/account/pseudonym",
        &first,
        Some(json!({"pseudonym": "lark"})),
    )
    .await;
    let bad = with_session(
        &library,
        "PUT",
        "/api/account/pseudonym",
        &first,
        Some(json!({"pseudonym": "a@b"})),
    )
    .await;

    assert_eq!(changed.status, StatusCode::OK);
    assert_eq!(changed.json()["pseudonym"], "Barn owl");
    assert_eq!(taken.json(), json!({"error": "pseudonym_taken"}));
    assert_eq!(bad.json(), json!({"error": "bad_pseudonym"}));
    let read = with_session(&library, "GET", "/api/account", &first, None).await;
    assert_eq!(read.json()["pseudonym"], "Barn owl");
    let third = session(&library, &provider, passkey("subject-3", clock.now())).await;
    assert_eq!(
        open_account(&library, &third, "Owl").await.status,
        StatusCode::CREATED,
        "the old pseudonym is free again"
    );
}

#[tokio::test]
async fn without_a_library_account_there_is_no_profile_to_read_or_change() {
    let (library, provider, _) = with_accounts().await;
    let session = session(&library, &provider, passkey(SUBJECT, signed_in_at())).await;

    let read = with_session(&library, "GET", "/api/account", &session, None).await;
    let change = with_session(
        &library,
        "PUT",
        "/api/account/pseudonym",
        &session,
        Some(json!({"pseudonym": "Owl"})),
    )
    .await;

    assert_eq!(read.status, StatusCode::NOT_FOUND);
    assert_eq!(read.json(), json!({"error": "no_account"}));
    assert_eq!(change.status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn signing_out_ends_the_session_and_sessions_expire_after_thirty_days() {
    let (library, provider, clock) = with_accounts().await;
    let ended = session(&library, &provider, passkey(SUBJECT, signed_in_at())).await;
    let kept = session(&library, &provider, passkey(SUBJECT, signed_in_at())).await;

    let out = with_session(&library, "DELETE", "/api/session", &ended, None).await;

    assert_eq!(out.status, StatusCode::NO_CONTENT);
    let after = with_session(&library, "GET", "/api/session", &ended, None).await;
    assert_eq!(after.status, StatusCode::UNAUTHORIZED);
    assert_eq!(after.json(), json!({"error": "signed_out"}));
    let other = with_session(&library, "GET", "/api/session", &kept, None).await;
    assert_eq!(other.status, StatusCode::OK);

    clock.advance(jiff::SignedDuration::from_hours(30 * 24));
    let expired = with_session(&library, "GET", "/api/session", &kept, None).await;
    assert_eq!(expired.status, StatusCode::UNAUTHORIZED);
    assert_eq!(
        get(&library, "/api/session").await.json(),
        json!({"error": "signed_out"})
    );
    assert_eq!(
        with_session(&library, "GET", "/api/account", "not-a-session", None)
            .await
            .status,
        StatusCode::UNAUTHORIZED
    );
}

#[tokio::test]
async fn a_sign_in_that_does_not_match_what_was_started_is_refused() {
    let (library, provider, clock) = with_accounts().await;

    let unknown = post_json(
        &library,
        "/api/sign-in/callback",
        &json!({"state": "made-up", "code": "x"}),
    )
    .await;
    assert_eq!(unknown.status, StatusCode::BAD_REQUEST);
    assert_eq!(unknown.json(), json!({"error": "bad_sign_in"}));

    // Another nonce than the one asked for.
    let state = start(&library, false).await.json()["state"]
        .as_str()
        .unwrap()
        .to_string();
    provider.sign_in(
        "replayed",
        library::IdToken {
            nonce: Some("another".into()),
            ..passkey(SUBJECT, signed_in_at())
        },
    );
    let callback = json!({"state": state, "code": "replayed"});
    let replayed = post_json(&library, "/api/sign-in/callback", &callback).await;
    assert_eq!(replayed.json(), json!({"error": "bad_sign_in"}));

    // A state serves once.
    let answer = sign_in_with(&library, &provider, passkey(SUBJECT, signed_in_at())).await;
    assert_eq!(answer.status, StatusCode::OK);
    let state = provider.asked().last().unwrap().state.clone();
    let again = post_json(
        &library,
        "/api/sign-in/callback",
        &json!({"state": state, "code": format!("code-{state}")}),
    )
    .await;
    assert_eq!(again.json(), json!({"error": "bad_sign_in"}));

    // A sign-in comes back within ten minutes.
    let state = start(&library, false).await.json()["state"]
        .as_str()
        .unwrap()
        .to_string();
    let nonce = provider.asked().last().unwrap().nonce.clone();
    provider.sign_in(
        "late",
        library::IdToken {
            nonce: Some(nonce),
            ..passkey(SUBJECT, signed_in_at())
        },
    );
    clock.advance(jiff::SignedDuration::from_mins(11));
    let late = post_json(
        &library,
        "/api/sign-in/callback",
        &json!({"state": state, "code": "late"}),
    )
    .await;
    assert_eq!(late.json(), json!({"error": "bad_sign_in"}));
}

#[tokio::test]
async fn an_unreachable_provider_makes_sign_in_unavailable() {
    let (library, provider, _) = with_accounts().await;
    provider.set_unreachable();

    let answer = sign_in_with(&library, &provider, passkey(SUBJECT, signed_in_at())).await;

    assert_eq!(answer.status, StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(answer.json(), json!({"error": "unavailable"}));
}

#[tokio::test]
async fn without_an_identity_provider_the_account_routes_are_unavailable() {
    let library = library().await;

    for answer in [
        post_json(&library, "/api/sign-in", &json!({})).await,
        get(&library, "/api/session").await,
        get(&library, "/api/account").await,
    ] {
        assert_eq!(answer.status, StatusCode::SERVICE_UNAVAILABLE);
        assert_eq!(answer.json(), json!({"error": "unavailable"}));
    }
}
