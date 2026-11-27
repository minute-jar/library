//! The Minute Jar public library service.
//!
//! [`Library`] is the whole service as callers see it: what is published
//! enters the catalogue, and [`Library::router`] serves the signed documents
//! of the interface with family servers (`docs/specs/interface-v1.md`).

pub mod contract;

mod catalogue;
mod http;
mod imports;
mod withdrawals;

use std::sync::Arc;

use jiff::Timestamp;
use serde::Serialize;
use serde_json::Value;
use sqlx::PgPool;

use crate::contract::SigningKey;

pub use crate::catalogue::PublishError;
pub use crate::imports::ImportError;
pub use crate::withdrawals::{Reason, WithdrawError, Withdrawal};

/// A question bank or a learning path.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ItemType {
    Bank,
    Path,
}

impl ItemType {
    fn name(self) -> &'static str {
        match self {
            ItemType::Bank => "bank",
            ItemType::Path => "path",
        }
    }
}

/// Who answers for a shared item, as others see them.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Author {
    pub pseudonym: String,
    /// The operator's editorial account (ADR-0002).
    pub editorial: bool,
}

/// The shared item a derived work comes from.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Origin {
    pub item: String,
    pub version: u32,
    pub pseudonym: String,
}

/// A curriculum a shared item follows.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Curriculum {
    pub country: String,
    pub year: String,
    pub topic: String,
}

/// What lets families find a shared item.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Metadata {
    /// A BCP 47 language tag.
    pub language: String,
    pub subject: String,
    /// The youngest and oldest ages it is meant for.
    pub ages: (u8, u8),
    pub description: String,
    pub curriculum: Option<Curriculum>,
}

/// One version of a shared item, once reviewed and published.
#[derive(Debug, Clone)]
pub struct NewVersion {
    /// The shared item's identifier: 32 lowercase hexadecimal digits.
    pub item: String,
    pub kind: ItemType,
    pub title: String,
    pub author: Author,
    pub derived_from: Option<Origin>,
    pub metadata: Metadata,
    /// The bank or learning path, in the bank format.
    pub content: Value,
}

/// The library service.
#[derive(Clone)]
pub struct Library {
    pool: PgPool,
    signing: Arc<SigningKey>,
    keys_document: Arc<catalogue::Stored>,
}

impl Library {
    /// The service over its database, which it brings up to date, signing
    /// with `signing`, which `keys_document` certifies; the index is signed
    /// again at `now`.
    pub async fn open(
        pool: PgPool,
        signing: SigningKey,
        keys_document: String,
        now: Timestamp,
    ) -> Result<Self, sqlx::Error> {
        sqlx::migrate!().run(&pool).await?;
        catalogue::refresh(&pool, &signing, now).await?;
        Ok(Library {
            pool,
            signing: Arc::new(signing),
            keys_document: Arc::new(catalogue::Stored::new(keys_document)),
        })
    }

    /// Publishes the next version of a shared item; answers its version number.
    pub async fn publish_version(
        &self,
        version: NewVersion,
        now: Timestamp,
    ) -> Result<u32, PublishError> {
        catalogue::publish(&self.pool, &self.signing, version, now).await
    }

    /// Records that the family server under `library_identifier` imported
    /// the shared item `item`. The index counts each identifier once per item,
    /// from the next time it is signed.
    pub async fn record_import(
        &self,
        library_identifier: &str,
        item: &str,
    ) -> Result<(), ImportError> {
        imports::record(&self.pool, library_identifier, item).await
    }

    /// Withdraws a shared item, or one of its versions when `version` is
    /// given: it enters the withdrawal list and leaves the index.
    pub async fn withdraw(
        &self,
        item: &str,
        version: Option<u32>,
        withdrawal: Withdrawal,
        now: Timestamp,
    ) -> Result<(), WithdrawError> {
        withdrawals::withdraw(&self.pool, &self.signing, item, version, withdrawal, now).await
    }

    /// Ends a switch-off: the item or version leaves the withdrawal list and
    /// comes back to the index.
    pub async fn reactivate(
        &self,
        item: &str,
        version: Option<u32>,
        now: Timestamp,
    ) -> Result<(), WithdrawError> {
        withdrawals::reactivate(&self.pool, &self.signing, item, version, now).await
    }

    /// Signs the index and the withdrawal list again at `now`, before they expire.
    pub async fn refresh(&self, now: Timestamp) -> Result<(), sqlx::Error> {
        catalogue::refresh(&self.pool, &self.signing, now).await
    }

    /// The HTTP interface.
    pub fn router(&self) -> axum::Router {
        http::router(self.clone())
    }
}
