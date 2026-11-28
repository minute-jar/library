//! A library served in memory over a throwaway PostgreSQL database, signed
//! with the public test keys, driven the way a family server drives it.

#![allow(dead_code)]

use std::sync::OnceLock;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::Duration;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use jiff::Timestamp;
use library::contract::{Keys, Kind, RootKey, SigningKey, verify_keys};
use library::{Author, Curriculum, Library, LocalDirectory, Metadata, NewVersion};
use serde_json::{Value, json};
use sqlx::postgres::PgPoolOptions;
use testcontainers_modules::postgres::Postgres;
use testcontainers_modules::testcontainers::runners::SyncRunner;
use testcontainers_modules::testcontainers::{Container, ImageExt};
use tower::ServiceExt;

pub fn at(text: &str) -> Timestamp {
    text.parse().unwrap()
}

/// The test root key: public, never used outside tests and fixtures.
pub fn root() -> RootKey {
    RootKey::from_seed([1; 32])
}

pub fn signing() -> SigningKey {
    SigningKey::from_seed([2; 32])
}

pub fn keys_document() -> String {
    root().certify(
        &[(signing().public(), at("2027-02-26T09:00:00Z"))],
        at("2026-11-26T09:00:00Z"),
        at("2026-12-26T09:00:00Z"),
    )
}

/// The instant at which tests check what they receive.
pub fn checked_at() -> Timestamp {
    at("2026-11-27T00:00:00Z")
}

fn server_url() -> String {
    if let Ok(url) = std::env::var("DATABASE_URL") {
        return url;
    }
    static CONTAINER: OnceLock<(Container<Postgres>, String)> = OnceLock::new();
    let (_, url) = CONTAINER.get_or_init(|| {
        // The blocking runner keeps its own runtime, so it starts outside the test's.
        std::thread::spawn(|| {
            // A busy Docker can take well over the default minute to start it.
            let container = Postgres::default()
                .with_startup_timeout(Duration::from_secs(180))
                .start()
                .expect("Docker runs a PostgreSQL for the tests");
            let port = container.get_host_port_ipv4(5432).unwrap();
            (
                container,
                format!("postgres://postgres:postgres@127.0.0.1:{port}"),
            )
        })
        .join()
        .unwrap()
    });
    url.clone()
}

/// A library over a database of its own.
pub async fn library() -> Library {
    library_and_database().await.0
}

/// A library over a database of its own, and that database's address.
pub async fn library_and_database() -> (Library, String) {
    static NEXT: AtomicU32 = AtomicU32::new(0);
    let server = server_url();
    let name = format!(
        "test_{}_{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    );
    let admin = PgPoolOptions::new()
        .max_connections(1)
        .connect(&format!("{server}/postgres"))
        .await
        .unwrap();
    sqlx::query(&format!("CREATE DATABASE {name}"))
        .execute(&admin)
        .await
        .unwrap();
    let pool = PgPoolOptions::new()
        .max_connections(4)
        .connect(&format!("{server}/{name}"))
        .await
        .unwrap();
    let media = std::env::temp_dir()
        .join("minute-jar-library-tests")
        .join(&name);
    let _ = std::fs::remove_dir_all(&media);
    let library = Library::open(
        pool,
        signing(),
        keys_document(),
        LocalDirectory::new(media),
        at("2026-11-26T00:00:00Z"),
    )
    .await
    .unwrap();
    (library, format!("{server}/{name}"))
}

pub struct Answer {
    pub status: StatusCode,
    pub headers: axum::http::HeaderMap,
    pub body: Vec<u8>,
}

impl Answer {
    /// The keys a family server trusts after checking this keys document.
    pub fn keys(&self) -> Keys {
        verify_keys(&self.body, &root().public(), checked_at())
            .expect("a keys document certified by the test root")
    }

    pub fn json(&self) -> Value {
        serde_json::from_slice(&self.body).unwrap()
    }
}

pub async fn get(library: &Library, path: &str) -> Answer {
    get_with(library, Request::get(path).body(Body::empty()).unwrap()).await
}

pub async fn get_with(library: &Library, request: Request<Body>) -> Answer {
    let response = library.router().oneshot(request).await.unwrap();
    let status = response.status();
    let headers = response.headers().clone();
    let body = response
        .into_body()
        .collect()
        .await
        .unwrap()
        .to_bytes()
        .to_vec();
    Answer {
        status,
        headers,
        body,
    }
}

/// Fetches a signed document and checks it from the root key alone.
pub async fn fetch_verified(library: &Library, path: &str, kind: Kind) -> Value {
    let keys = get(library, "/.well-known/minute-jar-library.json")
        .await
        .keys();
    let answer = get(library, path).await;
    assert_eq!(
        answer.status,
        StatusCode::OK,
        "{path}: {}",
        String::from_utf8_lossy(&answer.body)
    );
    keys.verify(&answer.body, kind, checked_at())
        .expect("a document signed by a certified key")
}

pub const VERBS: &str = "0123456789abcdef0123456789abcdef";
pub const FRACTIONS: &str = "fedcba9876543210fedcba9876543210";

/// A bank of irregular English verbs, at a given title.
pub fn verbs(title: &str) -> NewVersion {
    NewVersion {
        item: VERBS.into(),
        kind: library::ItemType::Bank,
        title: title.into(),
        author: Author {
            pseudonym: "Owl".into(),
            editorial: false,
        },
        derived_from: None,
        metadata: Metadata {
            language: "en-GB".into(),
            subject: "english".into(),
            ages: (9, 11),
            description: "The fifty most common irregular verbs.".into(),
            curriculum: Some(Curriculum {
                country: "GB".into(),
                year: "Year 5".into(),
                topic: "Verb forms".into(),
            }),
        },
        content: json!({"questions": [{"type": "typed", "prompt": "Past of 'go'", "answers": ["went"]}]}),
        media: vec![],
    }
}

/// A bank on fractions from the editorial account.
pub fn fractions() -> NewVersion {
    NewVersion {
        item: FRACTIONS.into(),
        kind: library::ItemType::Bank,
        title: "Les fractions, CM1".into(),
        author: Author {
            pseudonym: "Minute Jar".into(),
            editorial: true,
        },
        derived_from: None,
        metadata: Metadata {
            language: "fr-FR".into(),
            subject: "mathematics".into(),
            ages: (9, 10),
            description: "Comparer et ranger des fractions simples.".into(),
            curriculum: None,
        },
        content: json!({"questions": []}),
        media: vec![],
    }
}

/// Two library identifiers, as two family servers draw them.
pub const FAMILY_A: &str = "aaaaaaaaaaaaaaaa0000000000000001";
pub const FAMILY_B: &str = "bbbbbbbbbbbbbbbb0000000000000002";

pub async fn post_json(library: &Library, path: &str, body: &Value) -> Answer {
    post_bytes(library, path, body.to_string().into_bytes()).await
}

pub async fn post_bytes(library: &Library, path: &str, body: Vec<u8>) -> Answer {
    get_with(
        library,
        Request::post(path)
            .header("content-type", "application/json")
            .body(Body::from(body))
            .unwrap(),
    )
    .await
}

/// Reports an import of `item` under `identifier`, as a family server does.
pub async fn import(library: &Library, identifier: &str, item: &str) -> Answer {
    post_json(
        library,
        "/v1/imports",
        &json!({"library_identifier": identifier, "item": item}),
    )
    .await
}
/// A picture of one transparent pixel, a valid PNG file of 70 bytes.
pub fn pixel() -> Vec<u8> {
    hex::decode("89504e470d0a1a0a0000000d49484452000000010000000108060000001f15c4890000000d4944415478da636460f85f0f0002870180eb47ba920000000049454e44ae426082").unwrap()
}

/// The SHA-256 digest of [`pixel`], worked out apart from the service.
pub const PIXEL: &str = "497790947d4666760ce38f3c00e852c71fdb66cae849bae8e9ede352719e1581";

pub mod cards;
pub mod comments;
pub mod identity;
pub mod shares;
