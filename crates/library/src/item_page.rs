//! A shared item as the website's item page shows it
//! (`docs/specs/catalogue-api.md`, Item page), and the item file a parent
//! downloads for a family server without Internet access
//! (`docs/specs/interface-v1.md`, Item files): the same for every caller.

use axum::Router;
use axum::extract::{Path, State};
use axum::http::{StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use base64::Engine;
use base64::engine::general_purpose::STANDARD as BASE64;
use serde_json::{Value, json};
use sqlx::PgPool;

use crate::Library;
use crate::browse::{ENTRY_COLUMNS, Row, entry};
use crate::catalogue::{self, item_path};

const PAGE_CACHE: &str = "public, max-age=60";
/// An item file holds the keys document, cached as long.
const FILE_CACHE: &str = "public, max-age=3600";

/// The versions of `item` that are not withdrawn.
const SHOWN: &str = "SELECT * FROM item_versions AS v
     WHERE item = $1
       AND NOT EXISTS (SELECT 1 FROM withdrawals AS w WHERE w.item = v.item AND w.version IN (0, v.version))";

pub fn router(library: Library) -> Router {
    Router::new()
        .route("/api/items/{item}", get(page))
        .route("/api/items/{item}/versions/{version}/file", get(file))
        .with_state(library)
}

async fn page(State(library): State<Library>, Path(item): Path<String>) -> Response {
    match shown(&library.pool, &item).await {
        Ok(Some(page)) => (
            [
                (header::CONTENT_TYPE, "application/json"),
                (header::CACHE_CONTROL, PAGE_CACHE),
            ],
            page.to_string(),
        )
            .into_response(),
        Ok(None) => refused(StatusCode::NOT_FOUND, "unknown_item"),
        Err(_) => refused(StatusCode::SERVICE_UNAVAILABLE, "unavailable"),
    }
}

/// The item page of `item`: its index entry at the latest version that is
/// not withdrawn, with that version's content and media as signed, and every
/// version that is not withdrawn, newest first. `None` when the catalogue
/// does not show it.
async fn shown(pool: &PgPool, item: &str) -> Result<Option<Value>, sqlx::Error> {
    let latest: Option<Row> = sqlx::query_as(&format!(
        "SELECT {ENTRY_COLUMNS} FROM ({SHOWN} ORDER BY version DESC LIMIT 1) latest"
    ))
    .bind(item)
    .fetch_optional(pool)
    .await?;
    let Some(latest) = latest else {
        return Ok(None);
    };
    let version = latest.1 as u32;
    let Some(signed) = catalogue::document(pool, &item_path(item, version)).await? else {
        return Ok(None);
    };
    let payload = signed_payload(&signed.body);
    let versions: Vec<(i32, i64)> = sqlx::query_as(&format!(
        "SELECT version, published_at FROM ({SHOWN}) shown ORDER BY version DESC"
    ))
    .bind(item)
    .fetch_all(pool)
    .await?;

    let mut page = entry(latest);
    page["content"] = payload["content"].clone();
    page["media"] = payload["media"].clone();
    page["versions"] = versions
        .into_iter()
        .map(|(version, published_at)| {
            json!({"version": version, "published_at": time(published_at)})
        })
        .collect();
    Ok(Some(page))
}

async fn file(
    State(library): State<Library>,
    Path((item, version)): Path<(String, String)>,
) -> Response {
    match item_file(&library, &item, &version).await {
        Ok((version, file)) => (
            [
                (header::CONTENT_TYPE, "application/json".to_string()),
                (header::CACHE_CONTROL, FILE_CACHE.to_string()),
                (
                    header::CONTENT_DISPOSITION,
                    format!("attachment; filename=\"minute-jar-{item}-{version}.json\""),
                ),
            ],
            file,
        )
            .into_response(),
        Err(Missing::Unknown(code)) => refused(StatusCode::NOT_FOUND, code),
        Err(Missing::Unavailable) => refused(StatusCode::SERVICE_UNAVAILABLE, "unavailable"),
    }
}

/// Why there is no item file to answer.
enum Missing {
    /// `unknown_item` or `unknown_version`.
    Unknown(&'static str),
    /// The database or the media store failed.
    Unavailable,
}

impl From<sqlx::Error> for Missing {
    fn from(_: sqlx::Error) -> Self {
        Missing::Unavailable
    }
}

/// The item file of `version` of `item`: the keys document and the signed
/// item, spliced in as served, and the files of its media. Refused with
/// `unknown_item` or `unknown_version` when the catalogue does not show it.
async fn item_file(library: &Library, item: &str, version: &str) -> Result<(u32, String), Missing> {
    let pool = &library.pool;
    let versions: Vec<(i32,)> = sqlx::query_as(&format!("SELECT version FROM ({SHOWN}) shown"))
        .bind(item)
        .fetch_all(pool)
        .await?;
    if versions.is_empty() {
        return Err(Missing::Unknown("unknown_item"));
    }
    let Some(version) = version
        .parse::<u32>()
        .ok()
        .filter(|asked| versions.iter().any(|(shown,)| *shown as u32 == *asked))
    else {
        return Err(Missing::Unknown("unknown_version"));
    };
    let signed = catalogue::document(pool, &item_path(item, version))
        .await?
        .ok_or(Missing::Unavailable)?;
    let mut media = Vec::new();
    for medium in signed_payload(&signed.body)["media"]
        .as_array()
        .expect("a media listing")
    {
        let digest = medium["digest"].as_str().expect("a digest");
        let bytes = library
            .media
            .get(digest)
            .await
            .ok()
            .flatten()
            .ok_or(Missing::Unavailable)?;
        media.push(json!({"digest": digest, "base64": BASE64.encode(bytes)}));
    }
    // The signed documents keep their exact bytes: a family server checks
    // them as it would have received them online.
    let file = format!(
        "{{\"contract\":1,\"kind\":\"item_file\",\"keys\":{},\"item\":{},\"media\":{}}}",
        library.keys_document.body,
        signed.body,
        Value::from(media),
    );
    Ok((version, file))
}

/// The payload of a signed document the library stored itself.
fn signed_payload(body: &str) -> Value {
    let signed: Value = serde_json::from_str(body).expect("a stored signed document");
    serde_json::from_str(signed["payload"].as_str().expect("a payload")).expect("a stored payload")
}

fn time(second: i64) -> String {
    crate::contract::time(jiff::Timestamp::from_second(second).expect("a stored instant"))
}

fn refused(status: StatusCode, code: &'static str) -> Response {
    (
        status,
        [(header::CONTENT_TYPE, "application/json")],
        format!("{{\"error\":\"{code}\"}}"),
    )
        .into_response()
}
