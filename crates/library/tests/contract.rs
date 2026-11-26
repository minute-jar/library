//! The contract's check, as a family server's library client runs it:
//! a signed document, the root key's public half and the instant of checking.

use jiff::Timestamp;
use library::contract::{Kind, Refusal, RootKey, SigningKey, verify_keys};
use serde_json::json;

fn at(text: &str) -> Timestamp {
    text.parse().unwrap()
}

#[test]
fn a_document_signed_by_a_certified_signing_key_verifies_from_the_root_key_alone() {
    let root = RootKey::from_seed([1; 32]);
    let signing = SigningKey::from_seed([2; 32]);
    let keys_document = root.certify(
        &[(signing.public(), at("2027-02-26T09:00:00Z"))],
        at("2026-11-26T09:00:00Z"),
        at("2026-12-26T09:00:00Z"),
    );
    let item = signing.sign(
        Kind::Item,
        at("2026-11-26T09:00:00Z"),
        None,
        json!({"item": "0123456789abcdef0123456789abcdef", "version": 1}),
    );

    let keys = verify_keys(keys_document.as_bytes(), &root.public(), at("2026-11-27T00:00:00Z")).unwrap();
    let payload = keys.verify(item.as_bytes(), Kind::Item, at("2026-11-27T00:00:00Z")).unwrap();

    assert_eq!(
        payload,
        json!({
            "contract": 1,
            "kind": "item",
            "issued_at": "2026-11-26T09:00:00Z",
            "item": "0123456789abcdef0123456789abcdef",
            "version": 1
        })
    );
}

struct Library {
    root: RootKey,
    signing: SigningKey,
}

fn library() -> Library {
    Library { root: RootKey::from_seed([1; 32]), signing: SigningKey::from_seed([2; 32]) }
}

impl Library {
    fn keys_document(&self) -> String {
        self.root.certify(
            &[(self.signing.public(), at("2027-02-26T09:00:00Z"))],
            at("2026-11-26T09:00:00Z"),
            at("2026-12-26T09:00:00Z"),
        )
    }

    fn keys(&self) -> library::contract::Keys {
        verify_keys(self.keys_document().as_bytes(), &self.root.public(), at("2026-11-27T00:00:00Z")).unwrap()
    }

    fn index(&self) -> String {
        self.signing.sign(Kind::Index, at("2026-11-26T09:00:00Z"), Some(at("2026-11-29T09:00:00Z")), json!({"items": []}))
    }
}

#[test]
fn a_changed_payload_is_refused_as_forged() {
    let library = library();
    let tampered = library.index().replace(r#"\"items\":[]"#, r#"\"items\":[{}]"#);
    assert_ne!(tampered, library.index());

    let refusal = library.keys().verify(tampered.as_bytes(), Kind::Index, at("2026-11-27T00:00:00Z"));

    assert_eq!(refusal, Err(Refusal::Forged));
}

#[test]
fn a_document_signed_by_an_uncertified_key_is_refused() {
    let library = library();
    let stranger = SigningKey::from_seed([3; 32]);
    let index = stranger.sign(Kind::Index, at("2026-11-26T09:00:00Z"), Some(at("2026-11-29T09:00:00Z")), json!({"items": []}));

    let refusal = library.keys().verify(index.as_bytes(), Kind::Index, at("2026-11-27T00:00:00Z"));

    assert_eq!(refusal, Err(Refusal::UnknownKey));
}

#[test]
fn a_signing_key_past_its_certificate_is_refused() {
    let library = library();

    let refusal = library.keys().verify(library.index().as_bytes(), Kind::Index, at("2027-02-26T09:00:01Z"));

    assert_eq!(refusal, Err(Refusal::UnknownKey));
}

#[test]
fn a_keys_document_signed_by_another_root_is_refused() {
    let library = library();
    let other_root = RootKey::from_seed([4; 32]);

    let refusal = verify_keys(library.keys_document().as_bytes(), &other_root.public(), at("2026-11-27T00:00:00Z"));

    assert_eq!(refusal.unwrap_err(), Refusal::UnknownKey);
}

#[test]
fn an_index_past_its_expiry_is_refused() {
    let library = library();

    let refusal = library.keys().verify(library.index().as_bytes(), Kind::Index, at("2026-11-29T09:00:01Z"));

    assert_eq!(refusal, Err(Refusal::Expired));
}

#[test]
fn an_expired_keys_document_is_refused() {
    let library = library();

    let refusal = verify_keys(library.keys_document().as_bytes(), &library.root.public(), at("2026-12-26T09:00:01Z"));

    assert_eq!(refusal.unwrap_err(), Refusal::Expired);
}

#[test]
fn a_document_of_another_kind_than_asked_is_refused() {
    let library = library();

    let refusal = library.keys().verify(library.index().as_bytes(), Kind::Item, at("2026-11-27T00:00:00Z"));

    assert_eq!(refusal, Err(Refusal::WrongKind));
}

#[test]
fn a_contract_version_this_code_does_not_speak_is_refused() {
    let library = library();
    let future = library.signing.sign(Kind::Item, at("2026-11-26T09:00:00Z"), None, json!({"contract": 2}));

    let refusal = library.keys().verify(future.as_bytes(), Kind::Item, at("2026-11-27T00:00:00Z"));

    assert_eq!(refusal, Err(Refusal::UnknownContract));
}

#[test]
fn something_that_is_not_a_signed_document_is_refused_as_malformed() {
    let refusal = library().keys().verify(b"<html>Not found</html>", Kind::Index, at("2026-11-27T00:00:00Z"));

    assert_eq!(refusal, Err(Refusal::Malformed));
}
