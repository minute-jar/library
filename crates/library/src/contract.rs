//! The signed documents of the interface between a family server and a
//! library (ADR-0004, `docs/specs/interface-v1.md`): signing them on the
//! library's side, checking them on a family server's side.

use ed25519_dalek::Signer;
use jiff::Timestamp;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};
use sha2::{Digest, Sha256};

/// The version of the contract this code speaks.
pub const CONTRACT: u64 = 1;

/// What a signed document is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Keys,
    Index,
    Item,
}

impl Kind {
    fn name(self) -> &'static str {
        match self {
            Kind::Keys => "keys",
            Kind::Index => "index",
            Kind::Item => "item",
        }
    }
}

/// Why a family server refuses a signed document.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Refusal {
    /// Not a signed document, or a payload that is not a JSON object.
    Malformed,
    /// Signed by no key the family server trusts at this instant.
    UnknownKey,
    /// The signature does not match the payload.
    Forged,
    /// A contract version this code does not speak.
    UnknownContract,
    /// Another kind of document than the one asked for.
    WrongKind,
    /// Past its expiry.
    Expired,
}

/// A public key and its identifier, as listed in the keys document.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PublicKey([u8; 32]);

impl PublicKey {
    pub fn from_bytes(bytes: [u8; 32]) -> Self {
        PublicKey(bytes)
    }

    pub fn to_hex(self) -> String {
        hex::encode(self.0)
    }

    /// The first 8 bytes of the SHA-256 of the key, in hexadecimal.
    pub fn id(self) -> String {
        hex::encode(&Sha256::digest(self.0)[..8])
    }

    fn verify(self, payload: &str, signature: &[u8; 64]) -> Result<(), Refusal> {
        let key = ed25519_dalek::VerifyingKey::from_bytes(&self.0).map_err(|_| Refusal::UnknownKey)?;
        key.verify_strict(payload.as_bytes(), &ed25519_dalek::Signature::from_bytes(signature))
            .map_err(|_| Refusal::Forged)
    }
}

fn sign_with(
    key: &ed25519_dalek::SigningKey,
    kind: Kind,
    issued_at: Timestamp,
    expires_at: Option<Timestamp>,
    body: Value,
) -> String {
    let mut payload = Map::new();
    payload.insert("contract".into(), json!(CONTRACT));
    payload.insert("kind".into(), json!(kind.name()));
    payload.insert("issued_at".into(), json!(time(issued_at)));
    if let Some(expires_at) = expires_at {
        payload.insert("expires_at".into(), json!(time(expires_at)));
    }
    if let Value::Object(fields) = body {
        payload.extend(fields);
    }
    let payload = Value::Object(payload).to_string();
    let signature = key.sign(payload.as_bytes()).to_bytes();
    let public = PublicKey(key.verifying_key().to_bytes());
    serde_json::to_string(&Envelope { payload, signature: hex::encode(signature), key: public.id() })
        .expect("an envelope of strings serialises")
}

/// An instant as the contract writes it: RFC 3339, UTC, to the second.
pub fn time(at: Timestamp) -> String {
    at.strftime("%Y-%m-%dT%H:%M:%SZ").to_string()
}

#[derive(Serialize, Deserialize)]
struct Envelope {
    payload: String,
    signature: String,
    key: String,
}

/// The library's offline key, which certifies its signing keys.
pub struct RootKey(ed25519_dalek::SigningKey);

impl RootKey {
    pub fn from_seed(seed: [u8; 32]) -> Self {
        RootKey(ed25519_dalek::SigningKey::from_bytes(&seed))
    }

    pub fn public(&self) -> PublicKey {
        PublicKey(self.0.verifying_key().to_bytes())
    }

    /// The keys document certifying each signing key until its date.
    pub fn certify(&self, signing_keys: &[(PublicKey, Timestamp)], issued_at: Timestamp, expires_at: Timestamp) -> String {
        let listed: Vec<Value> = signing_keys
            .iter()
            .map(|(key, not_after)| json!({"id": key.id(), "public_key": key.to_hex(), "not_after": time(*not_after)}))
            .collect();
        sign_with(&self.0, Kind::Keys, issued_at, Some(expires_at), json!({"signing_keys": listed}))
    }
}

/// A key the library holds online to sign its documents.
pub struct SigningKey(ed25519_dalek::SigningKey);

impl SigningKey {
    pub fn from_seed(seed: [u8; 32]) -> Self {
        SigningKey(ed25519_dalek::SigningKey::from_bytes(&seed))
    }

    pub fn public(&self) -> PublicKey {
        PublicKey(self.0.verifying_key().to_bytes())
    }

    /// A signed document of this kind: the contract's header, then `body`'s fields.
    pub fn sign(&self, kind: Kind, issued_at: Timestamp, expires_at: Option<Timestamp>, body: Value) -> String {
        sign_with(&self.0, kind, issued_at, expires_at, body)
    }
}

/// The signing keys a family server trusts, from a keys document it checked.
#[derive(Debug, Clone)]
pub struct Keys {
    signing_keys: Vec<(PublicKey, Timestamp)>,
}

/// Checks a keys document against the root key's public half.
pub fn verify_keys(document: &[u8], root: &PublicKey, now: Timestamp) -> Result<Keys, Refusal> {
    let payload = open(document, Kind::Keys, now, |id| (id == root.id()).then_some(*root))?;
    let listed = payload.get("signing_keys").and_then(Value::as_array).ok_or(Refusal::Malformed)?;
    let mut signing_keys = Vec::new();
    for entry in listed {
        let public_key = entry.get("public_key").and_then(Value::as_str).ok_or(Refusal::Malformed)?;
        let bytes: [u8; 32] = hex::decode(public_key)
            .ok()
            .and_then(|b| b.try_into().ok())
            .ok_or(Refusal::Malformed)?;
        let not_after = entry
            .get("not_after")
            .and_then(Value::as_str)
            .and_then(|t| t.parse().ok())
            .ok_or(Refusal::Malformed)?;
        signing_keys.push((PublicKey(bytes), not_after));
    }
    Ok(Keys { signing_keys })
}

impl Keys {
    /// Checks a document signed by one of these signing keys, and returns its payload.
    pub fn verify(&self, document: &[u8], kind: Kind, now: Timestamp) -> Result<Value, Refusal> {
        open(document, kind, now, |id| {
            self.signing_keys
                .iter()
                .find(|(key, not_after)| key.id() == id && now <= *not_after)
                .map(|(key, _)| *key)
        })
    }
}

fn open(
    document: &[u8],
    kind: Kind,
    now: Timestamp,
    key_for: impl Fn(&str) -> Option<PublicKey>,
) -> Result<Value, Refusal> {
    let envelope: Envelope = serde_json::from_slice(document).map_err(|_| Refusal::Malformed)?;
    let key = key_for(&envelope.key).ok_or(Refusal::UnknownKey)?;
    let signature: [u8; 64] = hex::decode(&envelope.signature)
        .ok()
        .and_then(|b| b.try_into().ok())
        .ok_or(Refusal::Malformed)?;
    key.verify(&envelope.payload, &signature)?;
    let payload: Value = serde_json::from_str(&envelope.payload).map_err(|_| Refusal::Malformed)?;
    if payload.get("contract").and_then(Value::as_u64) != Some(CONTRACT) {
        return Err(Refusal::UnknownContract);
    }
    if payload.get("kind").and_then(Value::as_str) != Some(kind.name()) {
        return Err(Refusal::WrongKind);
    }
    if let Some(expires_at) = payload.get("expires_at") {
        let expires_at: Timestamp = expires_at.as_str().and_then(|t| t.parse().ok()).ok_or(Refusal::Malformed)?;
        if now > expires_at {
            return Err(Refusal::Expired);
        }
    }
    Ok(payload)
}
