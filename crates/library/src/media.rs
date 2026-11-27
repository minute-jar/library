//! The media of shared items: pictures and sounds kept apart from the bank,
//! listed in a shared item by SHA-256 digest and served by digest
//! (ADR-0004, `docs/specs/interface-v1.md`, Media).
//!
//! The bytes live in a [`MediaStore`]; the database records which digests
//! were published, with their type and size, so that only media of a
//! published version are served.

use std::future::Future;
use std::io;
use std::path::PathBuf;
use std::pin::Pin;
use std::sync::atomic::{AtomicU64, Ordering};

use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use sqlx::{PgPool, Postgres, Transaction};

use crate::catalogue::PublishError;

/// The largest picture or sound, in bytes, as in the bank format.
pub const MEDIUM_LIMIT: usize = 1_048_576;

/// The largest weight of all the media of one version, in bytes, as in the
/// bank format.
pub const ITEM_MEDIA_LIMIT: usize = 16_777_216;

/// A future a [`MediaStore`] answers.
pub type StoreFuture<'a, T> = Pin<Box<dyn Future<Output = io::Result<T>> + Send + 'a>>;

/// Where the bytes of media are kept, by digest: S3-compatible storage in
/// deployment, a local directory in tests.
pub trait MediaStore: Send + Sync + 'static {
    /// Keeps `bytes` under `digest`, their SHA-256 in lowercase hexadecimal;
    /// keeping the same digest twice is harmless.
    fn put<'a>(&'a self, digest: &'a str, bytes: &'a [u8]) -> StoreFuture<'a, ()>;

    /// The bytes kept under `digest`, if any.
    fn get<'a>(&'a self, digest: &'a str) -> StoreFuture<'a, Option<Vec<u8>>>;
}

/// Media kept as files of a local directory, one per digest.
pub struct LocalDirectory {
    root: PathBuf,
}

impl LocalDirectory {
    /// Media under `root`, which is created when first written to.
    pub fn new(root: impl Into<PathBuf>) -> Self {
        LocalDirectory { root: root.into() }
    }
}

impl MediaStore for LocalDirectory {
    fn put<'a>(&'a self, digest: &'a str, bytes: &'a [u8]) -> StoreFuture<'a, ()> {
        Box::pin(async move {
            let path = self.root.join(digest);
            if tokio::fs::try_exists(&path).await? {
                return Ok(());
            }
            tokio::fs::create_dir_all(&self.root).await?;
            // Written aside then renamed, so that a reader never sees half a file.
            static NEXT: AtomicU64 = AtomicU64::new(0);
            let partial = self.root.join(format!(
                "{digest}.{}.{}.part",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            tokio::fs::write(&partial, bytes).await?;
            if let Err(error) = tokio::fs::rename(&partial, &path).await {
                let _ = tokio::fs::remove_file(&partial).await;
                if !tokio::fs::try_exists(&path).await? {
                    return Err(error);
                }
            }
            Ok(())
        })
    }

    fn get<'a>(&'a self, digest: &'a str) -> StoreFuture<'a, Option<Vec<u8>>> {
        Box::pin(async move {
            match tokio::fs::read(self.root.join(digest)).await {
                Ok(bytes) => Ok(Some(bytes)),
                Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
                Err(error) => Err(error),
            }
        })
    }
}

/// A picture or sound of a version about to be published.
pub struct Medium {
    pub digest: String,
    pub media_type: &'static str,
    pub bytes: Vec<u8>,
}

/// The media of a version, by digest, once checked against the limits and
/// the types of the bank format.
pub fn prepare(media: Vec<Vec<u8>>) -> Result<Vec<Medium>, PublishError> {
    let mut prepared: Vec<Medium> = Vec::with_capacity(media.len());
    for bytes in media {
        let digest = hex::encode(Sha256::digest(&bytes));
        if bytes.len() > MEDIUM_LIMIT {
            return Err(PublishError::MediumTooLarge { digest });
        }
        let Some(media_type) = media_type(&bytes) else {
            return Err(PublishError::UnsupportedMedium { digest });
        };
        prepared.push(Medium {
            digest,
            media_type,
            bytes,
        });
    }
    prepared.sort_by(|a, b| a.digest.cmp(&b.digest));
    prepared.dedup_by(|a, b| a.digest == b.digest);
    let weight: usize = prepared.iter().map(|medium| medium.bytes.len()).sum();
    if weight > ITEM_MEDIA_LIMIT {
        return Err(PublishError::MediaTooLarge);
    }
    Ok(prepared)
}

/// The type of a picture or sound the bank format allows, from its first
/// bytes: PNG, JPEG, GIF or WebP; MP3, Ogg or WAV.
fn media_type(bytes: &[u8]) -> Option<&'static str> {
    let riff = |form: &[u8]| bytes.len() >= 12 && &bytes[..4] == b"RIFF" && &bytes[8..12] == form;
    if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        Some("image/png")
    } else if bytes.starts_with(b"\xff\xd8\xff") {
        Some("image/jpeg")
    } else if bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a") {
        Some("image/gif")
    } else if riff(b"WEBP") {
        Some("image/webp")
    } else if riff(b"WAVE") {
        Some("audio/wav")
    } else if bytes.starts_with(b"OggS") {
        Some("audio/ogg")
    } else if bytes.starts_with(b"ID3")
        || (bytes.len() >= 2 && bytes[0] == 0xff && bytes[1] & 0xe0 == 0xe0)
    {
        Some("audio/mpeg")
    } else {
        None
    }
}

/// Keeps the bytes of every medium in the store.
pub async fn keep(store: &dyn MediaStore, media: &[Medium]) -> Result<(), PublishError> {
    for medium in media {
        store
            .put(&medium.digest, &medium.bytes)
            .await
            .map_err(PublishError::Storage)?;
    }
    Ok(())
}

/// Records the media as published, in the version's transaction.
pub async fn record(
    tx: &mut Transaction<'_, Postgres>,
    media: &[Medium],
) -> Result<(), sqlx::Error> {
    for medium in media {
        sqlx::query(
            "INSERT INTO media (digest, type, size) VALUES ($1, $2, $3) ON CONFLICT (digest) DO NOTHING",
        )
        .bind(&medium.digest)
        .bind(medium.media_type)
        .bind(medium.bytes.len() as i64)
        .execute(&mut **tx)
        .await?;
    }
    Ok(())
}

/// The `media` list of a shared item.
pub fn listing(media: &[Medium]) -> Value {
    media
        .iter()
        .map(|medium| {
            json!({
                "digest": medium.digest,
                "type": medium.media_type,
                "size": medium.bytes.len(),
            })
        })
        .collect()
}

pub fn is_digest(digest: &str) -> bool {
    digest.len() == 64
        && digest
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

/// The type of a published medium, if it was published.
pub async fn published_type(pool: &PgPool, digest: &str) -> Result<Option<String>, sqlx::Error> {
    let row: Option<(String,)> = sqlx::query_as("SELECT type FROM media WHERE digest = $1")
        .bind(digest)
        .fetch_optional(pool)
        .await?;
    Ok(row.map(|(media_type,)| media_type))
}
