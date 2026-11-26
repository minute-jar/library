//! The fixtures of `fixtures/v1/`, which a family server's library client
//! runs against itself: documents this service serves, byte for byte, and
//! documents a family server must refuse, with the reason.
//!
//! `UPDATE_FIXTURES=1 cargo test --test fixtures` writes them again; without
//! it, the test fails if the service no longer serves what they hold.

mod common;

use std::path::PathBuf;

use common::*;
use library::contract::{Kind, Refusal, SigningKey, verify_keys};
use serde_json::{Value, json};

fn fixtures() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/v1")
}

fn check_or_write(name: &str, body: &str, mismatches: &mut Vec<String>) {
    let path = fixtures().join(name);
    if std::env::var("UPDATE_FIXTURES").is_ok() {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, body).unwrap();
    } else if std::fs::read_to_string(&path).ok().as_deref() != Some(body) {
        mismatches.push(name.to_string());
    }
}

#[tokio::test]
async fn the_service_serves_the_fixtures_and_family_servers_refuse_the_refused_ones() {
    let library = library().await;
    library
        .publish_version(verbs("Irregular verbs"), at("2026-11-20T18:00:00Z"))
        .await
        .unwrap();
    library
        .publish_version(fractions(), at("2026-11-21T08:00:00Z"))
        .await
        .unwrap();
    library
        .publish_version(verbs("Irregular verbs, part 1"), at("2026-11-26T09:00:00Z"))
        .await
        .unwrap();
    let mut mismatches = Vec::new();

    let served = [
        (
            "keys.json",
            "/.well-known/minute-jar-library.json".to_string(),
        ),
        ("index.json", "/v1/index".to_string()),
        (
            &format!("items/{VERBS}/1.json"),
            format!("/v1/items/{VERBS}/versions/1"),
        ),
        (
            &format!("items/{VERBS}/2.json"),
            format!("/v1/items/{VERBS}/versions/2"),
        ),
        (
            &format!("items/{FRACTIONS}/1.json"),
            format!("/v1/items/{FRACTIONS}/versions/1"),
        ),
    ];
    for (name, path) in &served {
        let answer = get(&library, path).await;
        check_or_write(
            &format!("served/{name}"),
            std::str::from_utf8(&answer.body).unwrap(),
            &mut mismatches,
        );
    }

    let index = get(&library, "/v1/index").await.body;
    let index = String::from_utf8(index).unwrap();
    let stranger = SigningKey::from_seed([3; 32]);
    let refused: [(&str, String, Kind, Refusal); 5] = [
        (
            "forged-index.json",
            index.replace("Irregular verbs, part 1", "Irregular verbs, part 2"),
            Kind::Index,
            Refusal::Forged,
        ),
        (
            "expired-index.json",
            signing().sign(
                Kind::Index,
                at("2026-11-20T09:00:00Z"),
                Some(at("2026-11-23T09:00:00Z")),
                json!({"items": []}),
            ),
            Kind::Index,
            Refusal::Expired,
        ),
        (
            "uncertified-index.json",
            stranger.sign(
                Kind::Index,
                at("2026-11-26T09:00:00Z"),
                Some(at("2026-11-29T09:00:00Z")),
                json!({"items": []}),
            ),
            Kind::Index,
            Refusal::UnknownKey,
        ),
        (
            "index-asked-as-item.json",
            index.clone(),
            Kind::Item,
            Refusal::WrongKind,
        ),
        (
            "contract-2-item.json",
            signing().sign(
                Kind::Item,
                at("2026-11-26T09:00:00Z"),
                None,
                json!({"contract": 2}),
            ),
            Kind::Item,
            Refusal::UnknownContract,
        ),
    ];
    let keys = verify_keys(keys_document().as_bytes(), &root().public(), checked_at()).unwrap();
    let mut manifest = Vec::new();
    for (name, body, kind, refusal) in &refused {
        assert_eq!(
            keys.verify(body.as_bytes(), *kind, checked_at()),
            Err(refusal.clone()),
            "{name}"
        );
        check_or_write(&format!("refused/{name}"), body, &mut mismatches);
        manifest.push(json!({
            "file": format!("refused/{name}"),
            "asked_as": format!("{kind:?}").to_lowercase(),
            "refusal": format!("{refusal:?}"),
        }));
    }

    let manifest: Value = json!({
        "root_public_key": root().public().to_hex(),
        "checked_at": library::contract::time(checked_at()),
        "served": served.iter().map(|(name, path)| json!({"file": format!("served/{name}"), "path": path})).collect::<Vec<_>>(),
        "refused": manifest,
    });
    check_or_write(
        "manifest.json",
        &(serde_json::to_string_pretty(&manifest).unwrap() + "\n"),
        &mut mismatches,
    );

    assert!(
        mismatches.is_empty(),
        "fixtures out of date (UPDATE_FIXTURES=1 writes them): {mismatches:?}"
    );
}
