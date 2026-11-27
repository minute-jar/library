//! The library service, configured by its environment:
//!
//! - `DATABASE_URL`: the PostgreSQL database;
//! - `LIBRARY_SIGNING_KEY`: the signing key's 32-byte seed, in hexadecimal;
//! - `LIBRARY_KEYS_DOCUMENT`: the file holding the keys document the root key
//!   signed for that signing key;
//! - `LIBRARY_MEDIA_DIR`: the directory holding the media of shared items,
//!   until the S3-compatible store is wired at deployment;
//! - `LIBRARY_LISTEN`: the address to listen on, `127.0.0.1:8080` by default;
//! - `LIBRARY_IDENTIFICATION_DAYS`: how many days the identification data of
//!   a closed library account is kept after its last share, 365 by default
//!   (ticket 26).
//!
//! Reports answer `503` until a review adapter (ticket 13) and a notifier
//! are wired here with `Library::with_reports`.
//!
//! Sign-in with a Minute Jar account stays off (its routes answer
//! `unavailable`) until the identity service's adapter is wired, with the
//! identity service of `minute-jar/hosting` (ADR-0006).
//!
//! Sharing (`POST /api/shares`) answers `503` until the review adapter
//! (ticket 13) is wired here with `Library::with_sharing`; it needs sign-in
//! and card verification too.
//!
//! Card verification stays off too (its routes answer `unavailable`) until
//! the Stripe adapter is written, once Stripe confirms ADR-0007, and wired
//! with `Library::with_cards`.

use std::time::Duration;

use jiff::Timestamp;
use library::contract::SigningKey;
use library::{IDENTIFICATION_RETENTION, Library, LocalDirectory};

fn required(name: &str) -> String {
    std::env::var(name).unwrap_or_else(|_| panic!("{name} must be set"))
}

#[tokio::main]
async fn main() {
    let seed: [u8; 32] = hex::decode(required("LIBRARY_SIGNING_KEY"))
        .ok()
        .and_then(|bytes| bytes.try_into().ok())
        .expect("LIBRARY_SIGNING_KEY holds 64 hexadecimal digits");
    let keys_document = std::fs::read_to_string(required("LIBRARY_KEYS_DOCUMENT"))
        .expect("the keys document is readable");
    let pool = sqlx::PgPool::connect(&required("DATABASE_URL"))
        .await
        .expect("the database is reachable");
    let library = Library::open(
        pool,
        SigningKey::from_seed(seed),
        keys_document,
        LocalDirectory::new(required("LIBRARY_MEDIA_DIR")),
        Timestamp::now(),
    )
    .await
    .expect("the database is brought up to date");

    // The index lives three days and the withdrawal list one; sign them again well before.
    let refreshing = library.clone();
    tokio::spawn(async move {
        loop {
            tokio::time::sleep(Duration::from_secs(6 * 3600)).await;
            if let Err(error) = refreshing.refresh(Timestamp::now()).await {
                eprintln!("the index and the withdrawal list could not be signed again: {error}");
            }
        }
    });

    // A sign-in's e-mail is kept only while the sign-in is recent.
    let forgetting = library.clone();
    tokio::spawn(async move {
        loop {
            tokio::time::sleep(Duration::from_secs(5 * 60)).await;
            if let Err(error) = forgetting.forget_sign_ins(Timestamp::now()).await {
                eprintln!("expired sessions and sign-ins could not be forgotten: {error}");
            }
        }
    });

    // The identification data of closed accounts is erased once the legal period is over.
    let kept_for = std::env::var("LIBRARY_IDENTIFICATION_DAYS")
        .ok()
        .map(|days| {
            let days: i64 = days
                .parse()
                .expect("LIBRARY_IDENTIFICATION_DAYS is a number of days");
            jiff::SignedDuration::from_hours(days * 24)
        })
        .unwrap_or(IDENTIFICATION_RETENTION);
    let erasing = library.clone();
    tokio::spawn(async move {
        loop {
            if let Err(error) = erasing
                .erase_identifications(Timestamp::now(), kept_for)
                .await
            {
                eprintln!(
                    "the identification data of closed accounts could not be erased: {error}"
                );
            }
            tokio::time::sleep(Duration::from_secs(3600)).await;
        }
    });

    let listen = std::env::var("LIBRARY_LISTEN").unwrap_or_else(|_| "127.0.0.1:8080".into());
    let listener = tokio::net::TcpListener::bind(&listen)
        .await
        .expect("the address is free");
    axum::serve(listener, library.router())
        .await
        .expect("the service runs");
}
