//! Imports a family server reports under its library identifier, from which
//! the index counts the families using each shared item.
//!
//! An identifier is kept only with the items it imported, never with a time,
//! an address or an account, so that nothing joins it to anyone.

use sqlx::PgPool;

use crate::catalogue::{has_item, is_item_id};

/// Why an import could not be recorded.
#[derive(Debug)]
pub enum ImportError {
    /// The library identifier is not 32 lowercase hexadecimal digits.
    BadIdentifier,
    /// No shared item has this identifier.
    UnknownItem,
    Database(sqlx::Error),
}

impl From<sqlx::Error> for ImportError {
    fn from(error: sqlx::Error) -> Self {
        ImportError::Database(error)
    }
}

/// A library identifier has the shape of an item identifier: 128 random bits
/// in lowercase hexadecimal.
pub fn is_library_identifier(identifier: &str) -> bool {
    is_item_id(identifier)
}

pub async fn record(pool: &PgPool, identifier: &str, item: &str) -> Result<(), ImportError> {
    if !is_library_identifier(identifier) {
        return Err(ImportError::BadIdentifier);
    }
    if !is_item_id(item) || !has_item(pool, item).await? {
        return Err(ImportError::UnknownItem);
    }
    sqlx::query(
        "INSERT INTO imports (library_identifier, item) VALUES ($1, $2) ON CONFLICT DO NOTHING",
    )
    .bind(identifier)
    .bind(item)
    .execute(pool)
    .await?;
    Ok(())
}
