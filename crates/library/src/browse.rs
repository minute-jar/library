//! The catalogue as the website browses and searches it
//! (`docs/specs/catalogue-api.md`): every shared item at its latest version,
//! filtered and searched, unsigned and the same for every caller.

use std::collections::HashMap;

use axum::Router;
use axum::extract::{Query, State};
use axum::http::{StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use jiff::Timestamp;
use serde_json::{Value, json};
use sqlx::{PgPool, Postgres, QueryBuilder};

use crate::Library;
use crate::catalogue::LICENCE;

const CATALOGUE_CACHE: &str = "public, max-age=60";

pub fn router(library: Library) -> Router {
    Router::new()
        .route("/api/catalogue", get(catalogue))
        .route("/api/catalogue/filters", get(filters))
        .with_state(library)
}

async fn catalogue(
    State(library): State<Library>,
    Query(query): Query<HashMap<String, String>>,
) -> Response {
    let Some(filter) = Filter::parse(&query) else {
        return refused(StatusCode::BAD_REQUEST, "bad_filter");
    };
    answered(browse(&library.pool, &filter).await)
}

async fn filters(State(library): State<Library>) -> Response {
    answered(offered(&library.pool).await)
}

fn answered(answer: Result<Value, sqlx::Error>) -> Response {
    match answer {
        Ok(answer) => (
            [
                (header::CONTENT_TYPE, "application/json"),
                (header::CACHE_CONTROL, CATALOGUE_CACHE),
            ],
            answer.to_string(),
        )
            .into_response(),
        Err(_) => refused(StatusCode::SERVICE_UNAVAILABLE, "unavailable"),
    }
}

/// The values the catalogue's items take, for a caller to offer as filters.
async fn offered(pool: &PgPool) -> Result<Value, sqlx::Error> {
    const LATEST: &str =
        "(SELECT DISTINCT ON (item) * FROM item_versions ORDER BY item, version DESC) latest";
    let distinct =
        |field: &str| format!("SELECT DISTINCT metadata->>'{field}' FROM {LATEST} ORDER BY 1");
    let languages: Vec<(String,)> = sqlx::query_as(&distinct("language"))
        .fetch_all(pool)
        .await?;
    let subjects: Vec<(String,)> = sqlx::query_as(&distinct("subject")).fetch_all(pool).await?;
    let curricula: Vec<(String, String, String)> = sqlx::query_as(&format!(
        "SELECT DISTINCT metadata#>>'{{curriculum,country}}', metadata#>>'{{curriculum,year}}', metadata#>>'{{curriculum,topic}}'
         FROM {LATEST} WHERE jsonb_typeof(metadata->'curriculum') = 'object' ORDER BY 1, 2, 3"
    ))
    .fetch_all(pool)
    .await?;
    Ok(json!({
        "languages": languages.into_iter().map(|(language,)| language).collect::<Vec<_>>(),
        "subjects": subjects.into_iter().map(|(subject,)| subject).collect::<Vec<_>>(),
        "curricula": curricula
            .into_iter()
            .map(|(country, year, topic)| json!({"country": country, "year": year, "topic": topic}))
            .collect::<Vec<_>>(),
    }))
}

/// What a caller narrows the catalogue to; every part is optional.
#[derive(Debug)]
struct Filter {
    /// A BCP 47 tag: `fr` matches `fr-FR` and `fr-CA`.
    language: Option<String>,
    subject: Option<String>,
    /// An age the item is meant for.
    age: Option<i32>,
    country: Option<String>,
    year: Option<String>,
    topic: Option<String>,
    /// Words searched in the title, description, subject and topic.
    text: Option<String>,
    /// From 1.
    page: u32,
}

impl Filter {
    /// The filter a query string asks for, or `None` if a value is malformed.
    fn parse(query: &HashMap<String, String>) -> Option<Self> {
        let text = |name: &str| {
            query
                .get(name)
                .map(|value| value.trim().to_string())
                .filter(|value| !value.is_empty())
        };
        let age = match text("age") {
            None => None,
            Some(age) => Some(age.parse::<u8>().ok()? as i32),
        };
        let language = text("language");
        if let Some(language) = &language {
            let tag = language
                .split('-')
                .all(|part| !part.is_empty() && part.bytes().all(|b| b.is_ascii_alphanumeric()));
            if !tag {
                return None;
            }
        }
        let page = match text("page") {
            None => 1,
            Some(page) => page.parse::<u32>().ok().filter(|page| *page >= 1)?,
        };
        Some(Filter {
            language,
            subject: text("subject"),
            age,
            country: text("country"),
            year: text("year"),
            topic: text("topic"),
            text: text("q"),
            page,
        })
    }
}

/// How many items a page of the catalogue holds.
const PAGE_SIZE: i64 = 20;

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
);

/// `SELECT {columns}` over the latest version of every item that `filter`
/// keeps; the searched words, if any, are `asked.words`.
fn kept<'a>(columns: &str, filter: &'a Filter) -> QueryBuilder<'a, Postgres> {
    let mut sql = QueryBuilder::new(format!(
        "SELECT {columns} FROM (SELECT DISTINCT ON (item) * FROM item_versions ORDER BY item, version DESC) latest"
    ));
    if let Some(text) = &filter.text {
        // Stemmed in both languages of the catalogue, and also as typed.
        sql.push(" CROSS JOIN (SELECT websearch_to_tsquery('english', ")
            .push_bind(text)
            .push(") || websearch_to_tsquery('french', ")
            .push_bind(text)
            .push(") || websearch_to_tsquery('simple', ")
            .push_bind(text)
            .push(") AS words) asked");
    }
    sql.push(" WHERE true");
    if filter.text.is_some() {
        sql.push(" AND catalogue_search(title, metadata) @@ asked.words");
    }
    if let Some(language) = &filter.language {
        sql.push(" AND (lower(metadata->>'language') = lower(")
            .push_bind(language)
            .push(") OR lower(metadata->>'language') LIKE lower(")
            .push_bind(language)
            .push(") || '-%')");
    }
    if let Some(subject) = &filter.subject {
        sql.push(" AND lower(metadata->>'subject') = lower(")
            .push_bind(subject)
            .push(")");
    }
    if let Some(age) = filter.age {
        sql.push(" AND (metadata#>>'{ages,min}')::int <= ")
            .push_bind(age)
            .push(" AND (metadata#>>'{ages,max}')::int >= ")
            .push_bind(age);
    }
    for (field, value) in [
        ("country", &filter.country),
        ("year", &filter.year),
        ("topic", &filter.topic),
    ] {
        if let Some(value) = value {
            sql.push(format_args!(
                " AND lower(metadata#>>'{{curriculum,{field}}}') = lower("
            ))
            .push_bind(value)
            .push(")");
        }
    }
    sql
}

async fn browse(pool: &PgPool, filter: &Filter) -> Result<Value, sqlx::Error> {
    let (total,): (i64,) = kept("count(*)", filter)
        .build_query_as()
        .fetch_one(pool)
        .await?;
    let mut sql = kept(
        "item, version, type, title, author_pseudonym, author_editorial, derived_from, metadata, published_at",
        filter,
    );
    sql.push(" ORDER BY published_at DESC, item LIMIT ")
        .push_bind(PAGE_SIZE)
        .push(" OFFSET ")
        .push_bind((i64::from(filter.page) - 1) * PAGE_SIZE);
    let rows: Vec<Row> = sql.build_query_as().fetch_all(pool).await?;
    let items: Vec<Value> = rows.into_iter().map(entry).collect();
    Ok(json!({
        "items": items,
        "total": total,
        "page": filter.page,
        "pages": (total + PAGE_SIZE - 1) / PAGE_SIZE,
    }))
}

fn entry(row: Row) -> Value {
    let (item, version, kind, title, pseudonym, editorial, derived_from, metadata, published_at) =
        row;
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
        "signals": {"families": 0},
        "published_at": crate::contract::time(Timestamp::from_second(published_at).expect("a stored instant")),
    })
}

fn refused(status: StatusCode, code: &'static str) -> Response {
    (
        status,
        [(header::CONTENT_TYPE, "application/json")],
        format!("{{\"error\":\"{code}\"}}"),
    )
        .into_response()
}
