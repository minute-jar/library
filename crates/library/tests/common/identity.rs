//! A stand-in for the identity service (ADR-0006): an OpenID Connect provider
//! whose ID tokens a test writes, and a clock the test moves.

#![allow(dead_code)]

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use axum::body::Body;
use axum::http::{Request, StatusCode};
use jiff::{SignedDuration, Timestamp};
use library::{Authorization, IdToken, IdentityProvider, Library, ProviderError, ProviderFuture};
use serde_json::{Value, json};

use super::{Answer, at, get_with, post_json};

/// What the provider was asked to authorize, and the ID tokens it hands out by code.
#[derive(Default)]
struct State {
    asked: Vec<Authorization>,
    tokens: HashMap<String, (String, IdToken)>,
    unreachable: bool,
}

#[derive(Clone, Default)]
pub struct FakeProvider {
    state: Arc<Mutex<State>>,
}

impl FakeProvider {
    /// The authorization requests the service made, in order.
    pub fn asked(&self) -> Vec<Authorization> {
        self.state.lock().unwrap().asked.clone()
    }

    /// The person signs in at the provider, which hands back `code` for
    /// `token`, to be redeemed with the verifier matching the last request.
    pub fn sign_in(&self, code: &str, token: IdToken) {
        let mut state = self.state.lock().unwrap();
        let challenge = state.asked.last().unwrap().code_challenge.clone();
        state.tokens.insert(code.into(), (challenge, token));
    }

    pub fn set_unreachable(&self) {
        self.state.lock().unwrap().unreachable = true;
    }
}

impl IdentityProvider for FakeProvider {
    fn authorization_url(&self, authorization: &Authorization) -> String {
        self.state.lock().unwrap().asked.push(authorization.clone());
        format!(
            "https://id.example/oauth2/auth?state={}&nonce={}&code_challenge={}&strong={}",
            authorization.state,
            authorization.nonce,
            authorization.code_challenge,
            authorization.strong
        )
    }

    fn redeem<'a>(&'a self, code: &'a str, code_verifier: &'a str) -> ProviderFuture<'a> {
        Box::pin(async move {
            let mut state = self.state.lock().unwrap();
            if state.unreachable {
                return Err(ProviderError::Unreachable);
            }
            let (challenge, token) = state.tokens.remove(code).ok_or(ProviderError::Refused)?;
            if library::code_challenge(code_verifier) != challenge {
                return Err(ProviderError::Refused);
            }
            Ok(token)
        })
    }
}

/// The instant the service reads, which a test moves.
#[derive(Clone)]
pub struct Clock(Arc<Mutex<Timestamp>>);

impl Clock {
    pub fn now(&self) -> Timestamp {
        *self.0.lock().unwrap()
    }

    pub fn advance(&self, by: SignedDuration) {
        let mut now = self.0.lock().unwrap();
        *now = now.checked_add(by).unwrap();
    }
}

/// When tests sign in.
pub fn signed_in_at() -> Timestamp {
    at("2026-11-27T10:00:00Z")
}

/// A library whose accounts sign in through a fake provider, at [`signed_in_at`].
pub async fn with_accounts() -> (Library, FakeProvider, Clock) {
    let (library, provider, clock, _) = with_accounts_and_database().await;
    (library, provider, clock)
}

/// The same, with the address of the library's database.
pub async fn with_accounts_and_database() -> (Library, FakeProvider, Clock, String) {
    let provider = FakeProvider::default();
    let clock = Clock(Arc::new(Mutex::new(signed_in_at())));
    let reading = clock.clone();
    let (library, database) = super::library_and_database().await;
    let library = library.with_accounts(provider.clone(), move || reading.now());
    (library, provider, clock, database)
}

/// Every table of the database whose rows, as text, hold `text`.
pub async fn tables_holding(database: &str, text: &str) -> Vec<String> {
    let pool = sqlx::PgPool::connect(database).await.unwrap();
    let tables: Vec<(String,)> = sqlx::query_as(
        "SELECT table_name::text FROM information_schema.tables
         WHERE table_schema = 'public' ORDER BY table_name",
    )
    .fetch_all(&pool)
    .await
    .unwrap();
    let mut holding = Vec::new();
    for (table,) in tables {
        let (found,): (bool,) = sqlx::query_as(&format!(
            "SELECT EXISTS (SELECT 1 FROM {table} AS row WHERE row::text LIKE '%' || $1 || '%')"
        ))
        .bind(text)
        .fetch_one(&pool)
        .await
        .unwrap();
        if found {
            holding.push(table);
        }
    }
    pool.close().await;
    holding
}

pub const EMAIL: &str = "parent.real.name@example.org";
pub const SUBJECT: &str = "9f1c2a4e-minute-jar-account-1";

/// A sign-in by passkey, just now, with its verified e-mail.
pub fn passkey(subject: &str, now: Timestamp) -> IdToken {
    IdToken {
        subject: subject.into(),
        nonce: None,
        acr: Some("aal1".into()),
        amr: vec!["passkey".into()],
        auth_time: Some(now),
        email: Some(EMAIL.into()),
    }
}

/// A sign-in by a one-time code sent by e-mail: not a strong one.
pub fn email_code(subject: &str, now: Timestamp) -> IdToken {
    IdToken {
        amr: vec!["code".into()],
        ..passkey(subject, now)
    }
}

/// Starts a sign-in as the website does.
pub async fn start(library: &Library, strong: bool) -> Answer {
    post_json(library, "/api/sign-in", &json!({"strong": strong})).await
}

/// Signs in with `token` through the whole flow; answers the callback's answer.
pub async fn sign_in_with(library: &Library, provider: &FakeProvider, token: IdToken) -> Answer {
    let started = start(library, true).await;
    assert_eq!(started.status, StatusCode::OK);
    let state = started.json()["state"].as_str().unwrap().to_string();
    let nonce = provider.asked().last().unwrap().nonce.clone();
    let code = format!("code-{state}");
    provider.sign_in(
        &code,
        IdToken {
            nonce: Some(nonce),
            ..token
        },
    );
    post_json(
        library,
        "/api/sign-in/callback",
        &json!({"state": state, "code": code}),
    )
    .await
}

/// Signs in and answers the session.
pub async fn session(library: &Library, provider: &FakeProvider, token: IdToken) -> String {
    let answer = sign_in_with(library, provider, token).await;
    assert_eq!(
        answer.status,
        StatusCode::OK,
        "{}",
        String::from_utf8_lossy(&answer.body)
    );
    answer.json()["session"].as_str().unwrap().to_string()
}

/// A request carrying the session, as the website's server sends it.
pub async fn with_session(
    library: &Library,
    method: &str,
    path: &str,
    session: &str,
    body: Option<Value>,
) -> Answer {
    let request = Request::builder()
        .method(method)
        .uri(path)
        .header("authorization", format!("Bearer {session}"));
    let request = match body {
        Some(body) => request
            .header("content-type", "application/json")
            .body(Body::from(body.to_string())),
        None => request.body(Body::empty()),
    };
    get_with(library, request.unwrap()).await
}

/// Opens a library account with the usual answers.
pub async fn open_account(library: &Library, session: &str, pseudonym: &str) -> Answer {
    with_session(
        library,
        "POST",
        "/api/account",
        session,
        Some(json!({
            "pseudonym": pseudonym,
            "adult_and_parent": true,
            "minute_jar_account": "separate",
        })),
    )
    .await
}
