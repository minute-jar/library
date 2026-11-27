//! The library service, configured by its environment:
//!
//! - `DATABASE_URL`: the PostgreSQL database;
//! - `LIBRARY_SIGNING_KEY`: the signing key's 32-byte seed, in hexadecimal;
//! - `LIBRARY_KEYS_DOCUMENT`: the file holding the keys document the root key
//!   signed for that signing key;
//! - `LIBRARY_MEDIA_DIR`: the directory holding the media of shared items,
//!   until the S3-compatible store is wired at deployment;
//! - `LIBRARY_LISTEN`: the address to listen on, `127.0.0.1:8080` by default.

use std::time::Duration;

use jiff::Timestamp;
use library::contract::SigningKey;
use library::{Library, LocalDirectory};

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

    let listen = std::env::var("LIBRARY_LISTEN").unwrap_or_else(|_| "127.0.0.1:8080".into());
    let listener = tokio::net::TcpListener::bind(&listen)
        .await
        .expect("the address is free");
    axum::serve(listener, library.router())
        .await
        .expect("the service runs");
}
