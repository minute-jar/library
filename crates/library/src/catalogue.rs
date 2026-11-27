//! The catalogue: published versions, and the signed documents made from
//! them whenever it changes.

use jiff::Timestamp;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use sqlx::{PgPool, Postgres, Transaction};

use crate::contract::{Kind, SigningKey};
use crate::{Metadata, NewVersion};

/// Why a version could not be published.
#[derive(Debug)]
pub enum PublishError {
    /// The item identifier is not 32 lowercase hexadecimal digits.
    BadItem,
    Database(sqlx::Error),
}

impl From<sqlx::Error> for PublishError {
    fn from(error: sqlx::Error) -> Self {
        PublishError::Database(error)
    }
}

pub const LICENCE: &str = "CC-BY-SA-4.0";

pub fn is_item_id(item: &str) -> bool {
    item.len() == 32
        && item
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

pub fn item_path(item: &str, version: u32) -> String {
    format!("/v1/items/{item}/versions/{version}")
}

fn metadata_json(metadata: &Metadata) -> Value {
    json!({
        "language": metadata.language,
        "subject": metadata.subject,
        "ages": {"min": metadata.ages.0, "max": metadata.ages.1},
        "description": metadata.description,
        "curriculum": metadata.curriculum,
    })
}

pub async fn publish(
    pool: &PgPool,
    signing: &SigningKey,
    new: NewVersion,
    now: Timestamp,
) -> Result<u32, PublishError> {
    if !is_item_id(&new.item) {
        return Err(PublishError::BadItem);
    }
    let mut tx = pool.begin().await?;
    // One change of the catalogue at a time, so that documents follow it in order.
    sqlx::query("SELECT pg_advisory_xact_lock(1)")
        .execute(&mut *tx)
        .await?;
    let (latest,): (Option<i32>,) =
        sqlx::query_as("SELECT max(version) FROM item_versions WHERE item = $1")
            .bind(&new.item)
            .fetch_one(&mut *tx)
            .await?;
    let version = latest.unwrap_or(0) as u32 + 1;
    let metadata = metadata_json(&new.metadata);
    let derived_from = new.derived_from.as_ref().map(|origin| json!(origin));
    sqlx::query(
        "INSERT INTO item_versions (item, version, type, title, author_pseudonym, author_editorial, derived_from, metadata, content, published_at)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)",
    )
    .bind(&new.item)
    .bind(version as i32)
    .bind(new.kind.name())
    .bind(&new.title)
    .bind(&new.author.pseudonym)
    .bind(new.author.editorial)
    .bind(&derived_from)
    .bind(&metadata)
    .bind(&new.content)
    .bind(now.as_second())
    .execute(&mut *tx)
    .await?;

    let item = signing.sign(
        Kind::Item,
        now,
        None,
        json!({
            "item": new.item,
            "version": version,
            "type": new.kind.name(),
            "title": new.title,
            "licence": LICENCE,
            "author": new.author,
            "derived_from": derived_from,
            "metadata": metadata,
            "published_at": crate::contract::time(now),
            "content": new.content,
            "media": [],
        }),
    );
    store(&mut tx, &item_path(&new.item, version), &item).await?;
    sign_index(&mut tx, signing, now).await?;
    tx.commit().await?;
    Ok(version)
}

/// How long an index stays current (interface v1: three days).
const INDEX_LIFETIME: jiff::SignedDuration = jiff::SignedDuration::from_hours(72);

pub const INDEX_PATH: &str = "/v1/index";

/// Signs the index again at `now`, as the catalogue stands.
pub async fn refresh(
    pool: &PgPool,
    signing: &SigningKey,
    now: Timestamp,
) -> Result<(), sqlx::Error> {
    let mut tx = pool.begin().await?;
    sqlx::query("SELECT pg_advisory_xact_lock(1)")
        .execute(&mut *tx)
        .await?;
    sign_index(&mut tx, signing, now).await?;
    tx.commit().await
}

async fn sign_index(
    tx: &mut Transaction<'_, Postgres>,
    signing: &SigningKey,
    now: Timestamp,
) -> Result<(), sqlx::Error> {
    type Row = (
        String,
        i32,
        String,
        String,
        String,
        bool,
        Option<Value>,
        Value,
        i64,
        i64,
        Vec<i64>,
    );
    let latest: Vec<Row> = sqlx::query_as(
        "SELECT DISTINCT ON (item) item, version, type, title, author_pseudonym, author_editorial, derived_from, metadata, published_at,
                (SELECT count(*) FROM imports WHERE imports.item = item_versions.item),
                ARRAY(SELECT (SELECT count(*) FROM ratings WHERE ratings.item = item_versions.item AND ratings.stars = s)
                      FROM generate_series(1, 5) AS s ORDER BY s)
         FROM item_versions ORDER BY item, version DESC",
    )
    .fetch_all(&mut **tx)
    .await?;
    let items: Vec<Value> = latest
        .into_iter()
        .map(|(item, version, kind, title, pseudonym, editorial, derived_from, metadata, published_at, families, ratings)| {
            json!({
                "item": item,
                "version": version,
                "type": kind,
                "title": title,
                "licence": LICENCE,
                "author": {"pseudonym": pseudonym, "editorial": editorial},
                "derived_from": derived_from,
                "metadata": metadata,
                "selection": false,
                "signals": {"families": families, "ratings": ratings},
                "published_at": crate::contract::time(Timestamp::from_second(published_at).expect("a stored instant")),
            })
        })
        .collect();
    let expires_at = now
        .checked_add(INDEX_LIFETIME)
        .expect("an index expiry within range");
    let index = signing.sign(Kind::Index, now, Some(expires_at), json!({"items": items}));
    store(tx, INDEX_PATH, &index).await
}

async fn store(
    tx: &mut Transaction<'_, Postgres>,
    path: &str,
    body: &str,
) -> Result<(), sqlx::Error> {
    let etag = Stored::new(body.to_string()).etag;
    sqlx::query(
        "INSERT INTO signed_documents (path, body, etag) VALUES ($1, $2, $3)
         ON CONFLICT (path) DO UPDATE SET body = EXCLUDED.body, etag = EXCLUDED.etag",
    )
    .bind(path)
    .bind(body)
    .bind(etag)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

/// A signed document as stored: its bytes and its ETag.
pub struct Stored {
    pub body: String,
    pub etag: String,
}

impl Stored {
    /// A document and its strong ETag, from the SHA-256 of its bytes.
    pub fn new(body: String) -> Self {
        let etag = format!(
            "\"{}\"",
            hex::encode(&Sha256::digest(body.as_bytes())[..16])
        );
        Stored { body, etag }
    }
}

pub async fn has_item(pool: &PgPool, item: &str) -> Result<bool, sqlx::Error> {
    let (known,): (bool,) =
        sqlx::query_as("SELECT EXISTS (SELECT 1 FROM item_versions WHERE item = $1)")
            .bind(item)
            .fetch_one(pool)
            .await?;
    Ok(known)
}

pub async fn document(pool: &PgPool, path: &str) -> Result<Option<Stored>, sqlx::Error> {
    let row: Option<(String, String)> =
        sqlx::query_as("SELECT body, etag FROM signed_documents WHERE path = $1")
            .bind(path)
            .fetch_optional(pool)
            .await?;
    Ok(row.map(|(body, etag)| Stored { body, etag }))
}
