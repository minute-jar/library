//! Author profiles (story S19, `docs/specs/catalogue-api.md`, Author
//! profiles): the shared items of the library account that holds a
//! pseudonym, as the catalogue shows them, and how many families use them.
//! The profile is the same for every caller; the author's own view adds
//! the suggestions received, which only the author sees.

use axum::Router;
use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use serde_json::{Value, json};
use sqlx::PgPool;

use crate::accounts::http::signed_in;
use crate::answers::{json, refused};
use crate::browse::{ENTRY_COLUMNS, Row, entry};
use crate::{Library, SuggestionError};

const PROFILE_CACHE: &str = "public, max-age=60";

pub fn router(library: Library) -> Router {
    Router::new()
        .route("/api/authors/{pseudonym}", get(public))
        .route("/api/account/profile", get(own))
        .with_state(library)
}

/// A library account, found by its pseudonym.
struct Author {
    account: i64,
    pseudonym: String,
    /// Whether its profile shows the items published under its earlier
    /// pseudonyms: its owner's choice, off by default.
    earlier_shown: bool,
}

/// The account that holds `pseudonym` now, without regard to case, open or
/// closed (a closed account keeps its pseudonym and its items).
async fn holding(pool: &PgPool, pseudonym: &str) -> Result<Option<Author>, sqlx::Error> {
    let found: Option<(i64, String, bool)> = sqlx::query_as(
        "SELECT id, pseudonym, show_earlier_pseudonyms FROM library_accounts
         WHERE lower(pseudonym) = lower($1)",
    )
    .bind(pseudonym)
    .fetch_optional(pool)
    .await?;
    Ok(found.map(|(account, pseudonym, earlier_shown)| Author {
        account,
        pseudonym,
        earlier_shown,
    }))
}

/// The pseudonym under which the profile of `item`'s author is found now,
/// for the version a page shows, published under `published_as`: the one
/// their library account holds, which may differ. `None` for an item no
/// account shared, and for one published under an earlier pseudonym unless
/// its author chose to show those on their profile.
pub(crate) async fn of_item(
    pool: &PgPool,
    item: &str,
    published_as: &str,
) -> Result<Option<String>, sqlx::Error> {
    let found: Option<(String,)> = sqlx::query_as(
        "SELECT a.pseudonym FROM authored_items AS i
         JOIN library_accounts AS a ON a.id = i.account
         WHERE i.item = $1 AND (a.show_earlier_pseudonyms OR a.pseudonym = $2)",
    )
    .bind(item)
    .bind(published_as)
    .fetch_optional(pool)
    .await?;
    Ok(found.map(|(pseudonym,)| pseudonym))
}

/// The latest version of every item of `$1`'s account that the catalogue
/// shows: not withdrawn, not hidden from new imports; unless `$2`, only
/// those published under `$3`, the account's pseudonym of now.
const SHOWN_OF_ACCOUNT: &str = "(SELECT * FROM (SELECT DISTINCT ON (v.item) v.* FROM item_versions AS v
     JOIN authored_items AS a ON a.item = v.item
     WHERE a.account = $1
       AND NOT EXISTS (SELECT 1 FROM withdrawals AS w WHERE w.item = v.item AND w.version IN (0, v.version))
       AND NOT EXISTS (SELECT 1 FROM hidden_items AS h WHERE h.item = v.item)
     ORDER BY v.item, v.version DESC) AS shown
     WHERE $2 OR shown.author_pseudonym = $3) latest";

/// Which items a profile lists.
#[derive(Clone, Copy)]
enum Listed {
    /// As anyone sees it: those of earlier pseudonyms only when the author
    /// chose to show them.
    Public,
    /// As the author sees it: every item.
    Own,
}

/// The profile of `author`, with no item when the catalogue shows none of
/// theirs that `listed` keeps.
async fn profile(pool: &PgPool, author: &Author, listed: Listed) -> Result<Value, sqlx::Error> {
    let every = matches!(listed, Listed::Own) || author.earlier_shown;
    let rows: Vec<Row> = sqlx::query_as(&format!(
        "SELECT {ENTRY_COLUMNS} FROM {SHOWN_OF_ACCOUNT} ORDER BY latest.published_at DESC, latest.item"
    ))
    .bind(author.account)
    .bind(every)
    .bind(&author.pseudonym)
    .fetch_all(pool)
    .await?;
    // Families are counted by distinct library identifiers, which go no
    // further than this count.
    let (families,): (i64,) = sqlx::query_as(&format!(
        "SELECT count(DISTINCT library_identifier) FROM imports
         WHERE item IN (SELECT item FROM {SHOWN_OF_ACCOUNT})"
    ))
    .bind(author.account)
    .bind(every)
    .bind(&author.pseudonym)
    .fetch_one(pool)
    .await?;
    let (editorial,): (bool,) =
        sqlx::query_as("SELECT EXISTS (SELECT 1 FROM editorial_account WHERE account = $1)")
            .bind(author.account)
            .fetch_one(pool)
            .await?;
    Ok(json!({
        "pseudonym": author.pseudonym,
        "editorial": editorial,
        "families": families,
        "items": rows.into_iter().map(entry).collect::<Vec<_>>(),
    }))
}

/// Anyone's view of an author: refused as `unknown_author` when no account
/// holds the pseudonym, or when the catalogue shows no item of theirs (a
/// library account that shared nothing shown is no author).
async fn public(State(library): State<Library>, Path(pseudonym): Path<String>) -> Response {
    let found = async {
        match holding(&library.pool, &pseudonym).await? {
            Some(author) => profile(&library.pool, &author, Listed::Public)
                .await
                .map(Some),
            None => Ok(None),
        }
    };
    match found.await {
        Ok(Some(profile)) if !is_empty(&profile) => json(StatusCode::OK, PROFILE_CACHE, profile),
        Ok(_) => refused(StatusCode::NOT_FOUND, "unknown_author"),
        Err(_) => refused(StatusCode::SERVICE_UNAVAILABLE, "unavailable"),
    }
}

fn is_empty(profile: &Value) -> bool {
    profile["items"].as_array().is_none_or(Vec::is_empty)
}

/// The author's own view of their profile: each item also counts the
/// suggestions received, which no one else sees: `families`, the distinct
/// library identifiers that ever suggested an answer for it, and `answers`,
/// the suggested answers still standing against its latest version
/// (`docs/specs/interface-v1.md`, Suggestions).
async fn own(State(library): State<Library>, headers: HeaderMap) -> Response {
    let session = match signed_in(&library, &headers).await {
        Ok((session, _)) => session,
        Err(refusal) => return refusal.into_response(),
    };
    let pool = &library.pool;
    let found = async {
        let author: Option<(i64, String)> =
            sqlx::query_as("SELECT id, pseudonym FROM library_accounts WHERE subject = $1")
                .bind(&session.subject)
                .fetch_optional(pool)
                .await?;
        let Some((account, pseudonym)) = author else {
            return Ok(None);
        };
        let author = Author {
            account,
            pseudonym,
            earlier_shown: true,
        };
        let mut profile = profile(pool, &author, Listed::Own).await?;
        for entry in profile["items"].as_array_mut().expect("items") {
            let item = entry["item"].as_str().expect("an item").to_string();
            entry["suggestions"] = received(&library, &item).await?;
        }
        Ok(Some(profile))
    };
    match found.await {
        Ok(Some(profile)) => json(StatusCode::OK, crate::answers::NO_STORE, profile),
        Ok(None) => refused(StatusCode::NOT_FOUND, "no_account"),
        Err(Unavailable) => refused(StatusCode::SERVICE_UNAVAILABLE, "unavailable"),
    }
}

struct Unavailable;

impl From<sqlx::Error> for Unavailable {
    fn from(_: sqlx::Error) -> Self {
        Unavailable
    }
}

impl From<SuggestionError> for Unavailable {
    fn from(_: SuggestionError) -> Self {
        Unavailable
    }
}

/// The suggestions `item` received, counted: no identifier goes further.
async fn received(library: &Library, item: &str) -> Result<Value, Unavailable> {
    let (families,): (i64,) = sqlx::query_as(
        "SELECT count(DISTINCT library_identifier) FROM suggestions WHERE item = $1",
    )
    .bind(item)
    .fetch_one(&library.pool)
    .await?;
    let answers: usize = library
        .item_suggestions(item)
        .await?
        .map(|standing| standing.questions.iter().map(|q| q.answers.len()).sum())
        .unwrap_or(0);
    Ok(json!({"families": families, "answers": answers}))
}
