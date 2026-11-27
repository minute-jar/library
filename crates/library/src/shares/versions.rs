//! New versions and derived works (stories S14, S15,
//! `docs/specs/interface-v1.md`, Shares).
//!
//! A share names the item it is a new version of, which must be one of the
//! account's own published items, and the original it is derived from, if
//! any. A derived work keeps the original's attribution: the library, not
//! the parent app, writes the original's pseudonym, as that version was
//! signed. A new version keeps the attribution of the versions before it.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::PgPool;

use super::{ShareRefusal, random_id};
use crate::Origin;

/// The original a share says it is derived from, as the parent app names it.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct NamedOrigin {
    pub item: String,
    pub version: u32,
}

/// Where a share stands among the catalogue's items.
pub(crate) struct Lineage {
    /// The shared item: drawn for a first share.
    pub item: String,
    /// The version the share would be published as.
    pub version: u32,
    pub derived_from: Option<Origin>,
}

/// The lineage of a share from the library account `account`: a new
/// version of `item` when named, else a new item, derived from `named` if
/// any.
pub(crate) async fn lineage(
    pool: &PgPool,
    account: i64,
    item: Option<&str>,
    named: Option<&NamedOrigin>,
) -> Result<Lineage, ShareRefusal> {
    let Some(item) = item else {
        return Ok(Lineage {
            item: random_id(),
            version: 1,
            derived_from: derivation(pool, None, named, Shown::Required).await?,
        });
    };
    let (latest, theirs, withdrawn): (Option<i32>, bool, bool) = sqlx::query_as(
        "SELECT (SELECT max(version) FROM item_versions WHERE item = $1),
                EXISTS (SELECT 1 FROM authored_items WHERE item = $1 AND account = $2),
                EXISTS (SELECT 1 FROM withdrawals WHERE item = $1 AND version = 0)",
    )
    .bind(item)
    .bind(account)
    .fetch_one(pool)
    .await?;
    let Some(latest) = latest else {
        return Err(ShareRefusal::UnknownItem);
    };
    if !theirs {
        return Err(ShareRefusal::NotTheAuthor);
    }
    if withdrawn {
        return Err(ShareRefusal::WithdrawnItem);
    }
    Ok(Lineage {
        item: item.to_string(),
        version: latest as u32 + 1,
        derived_from: derivation(pool, Some(item), named, Shown::Required).await?,
    })
}

/// Whether a derived work's original must still be shown by the catalogue.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Shown {
    /// At a share: nothing is derived from a withdrawn or hidden version.
    Required,
    /// At a moderator's decision on a held share: the original was checked
    /// at the share, and its attribution is kept whatever became of it.
    NotRequired,
}

/// The original a version of `item` (none for a first share) is derived
/// from: the one `named`, else the one of the item's latest version.
pub(crate) async fn derivation(
    pool: &PgPool,
    item: Option<&str>,
    named: Option<&NamedOrigin>,
    shown: Shown,
) -> Result<Option<Origin>, ShareRefusal> {
    let inherited = match item {
        Some(item) => inherited(pool, item).await?,
        None => None,
    };
    let Some(named) = named else {
        return Ok(inherited);
    };
    if item == Some(named.item.as_str()) {
        return Err(ShareRefusal::BadOrigin);
    }
    if inherited
        .as_ref()
        .is_some_and(|origin| origin.item != named.item)
    {
        // Another original would drop the attribution of the first.
        return Err(ShareRefusal::OriginChanged);
    }
    let pseudonym: Option<(String,)> = sqlx::query_as(
        "SELECT v.author_pseudonym FROM item_versions AS v
         WHERE v.item = $1 AND v.version = $2
           AND ($3 OR (
               NOT EXISTS (SELECT 1 FROM withdrawals AS w WHERE w.item = v.item AND w.version IN (0, v.version))
               AND NOT EXISTS (SELECT 1 FROM hidden_items AS h WHERE h.item = v.item)))",
    )
    .bind(&named.item)
    .bind(named.version as i32)
    .bind(shown == Shown::NotRequired)
    .fetch_optional(pool)
    .await?;
    let (pseudonym,) = pseudonym.ok_or(ShareRefusal::UnknownOrigin)?;
    Ok(Some(Origin {
        item: named.item.clone(),
        version: named.version,
        pseudonym,
    }))
}

/// The original of the latest version of `item`, if it is a derived work.
async fn inherited(pool: &PgPool, item: &str) -> Result<Option<Origin>, ShareRefusal> {
    let row: Option<(Option<Value>,)> = sqlx::query_as(
        "SELECT derived_from FROM item_versions WHERE item = $1 ORDER BY version DESC LIMIT 1",
    )
    .bind(item)
    .fetch_optional(pool)
    .await?;
    let Some((Some(origin),)) = row else {
        return Ok(None);
    };
    let field = |name: &str| origin.get(name).cloned().unwrap_or(Value::Null);
    match (field("item"), field("version"), field("pseudonym")) {
        (Value::String(item), Value::Number(version), Value::String(pseudonym)) => {
            Ok(Some(Origin {
                item,
                version: version.as_u64().ok_or(ShareRefusal::Unavailable)? as u32,
                pseudonym,
            }))
        }
        _ => Err(ShareRefusal::Unavailable),
    }
}
