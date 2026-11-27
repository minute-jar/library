//! Suggestions to authors (story S18, `docs/specs/interface-v1.md`,
//! Suggestions).
//!
//! A family server suggests, under its library identifier, answers to
//! accept for a question of a shared item it imported, at the version it
//! imported. As with imports and ratings, an identifier is kept only with
//! what it suggested, never with a time, an address or an account.
//!
//! The author reads the suggestions of their items as they stand against
//! the latest version: by question, each answer counted by the distinct
//! identifiers that suggested it, never showing an identifier. A question
//! of an earlier version is found again in the latest one by its prompt,
//! and an answer the latest version already accepts is no longer suggested.
//! The author accepts some into a prepared new version, which goes through
//! the share route and its review like any share.

pub(crate) mod http;

use std::collections::{BTreeMap, HashMap, HashSet};

use serde::Serialize;
use serde_json::{Value, json};
use sqlx::PgPool;

use crate::catalogue::{self, has_item, is_item_id, item_path};
use crate::imports::is_library_identifier;
use crate::media::MediaStore;

/// The most answers one suggestion carries.
pub const ANSWERS_PER_SUGGESTION: usize = 10;
/// The longest suggested answer, in characters.
pub const ANSWER_LENGTH: usize = 100;

/// Why a suggestion could not be recorded, or accepted.
#[derive(Debug)]
pub enum SuggestionError {
    /// The library identifier is not 32 lowercase hexadecimal digits.
    BadIdentifier,
    /// No answer, more than [`ANSWERS_PER_SUGGESTION`], or an answer empty,
    /// longer than [`ANSWER_LENGTH`] characters or with control characters.
    BadSuggestion,
    /// No shared item has this identifier.
    UnknownItem,
    /// The item has no such version.
    UnknownVersion,
    /// The version has no such question, or the question takes no answers.
    UnknownQuestion,
    /// The identifier never reported importing this item: suggestions come
    /// from families that used it.
    NotImported,
    /// An accepted answer is not among the suggestions of its question.
    NotSuggested,
    Unavailable,
}

impl From<sqlx::Error> for SuggestionError {
    fn from(_: sqlx::Error) -> Self {
        SuggestionError::Unavailable
    }
}

/// The suggestions of a shared item, against its latest version.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ItemSuggestions {
    pub item: String,
    pub title: String,
    /// The latest version, against which questions are numbered.
    pub version: u32,
    /// The questions with suggestions, in order.
    pub questions: Vec<QuestionSuggestions>,
}

/// The answers suggested for one question.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct QuestionSuggestions {
    /// The question's place in the latest version, from 1.
    pub question: u32,
    pub prompt: Option<String>,
    /// The answers, the most suggested first.
    pub answers: Vec<SuggestedAnswer>,
}

/// An answer and how many families suggested it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SuggestedAnswer {
    pub answer: String,
    pub families: u32,
}

/// The answers an author accepts for one question of the latest version.
#[derive(Debug, Clone)]
pub struct Accepted {
    pub question: u32,
    pub answers: Vec<String>,
}

/// The question at `question` (from 1) of `content`, if it takes answers.
fn question_of(content: &Value, question: u32) -> Option<&Value> {
    let index = usize::try_from(question).ok()?.checked_sub(1)?;
    let found = content.get("questions")?.as_array()?.get(index)?;
    found.get("answers")?.is_array().then_some(found)
}

/// Whether `question` already accepts `answer`.
fn accepts(question: &Value, answer: &str) -> bool {
    question["answers"]
        .as_array()
        .is_some_and(|answers| answers.iter().any(|known| known.as_str() == Some(answer)))
}

async fn content_of(pool: &PgPool, item: &str, version: u32) -> Result<Option<Value>, sqlx::Error> {
    let row: Option<(Value,)> =
        sqlx::query_as("SELECT content FROM item_versions WHERE item = $1 AND version = $2")
            .bind(item)
            .bind(version as i32)
            .fetch_optional(pool)
            .await?;
    Ok(row.map(|(content,)| content))
}

pub(crate) async fn record(
    pool: &PgPool,
    identifier: &str,
    item: &str,
    version: u32,
    question: u32,
    answers: &[&str],
) -> Result<(), SuggestionError> {
    if !is_library_identifier(identifier) {
        return Err(SuggestionError::BadIdentifier);
    }
    let answers: Vec<&str> = answers.iter().map(|answer| answer.trim()).collect();
    let holds = |answer: &&str| {
        (1..=ANSWER_LENGTH).contains(&answer.chars().count())
            && !answer.chars().any(char::is_control)
    };
    if answers.is_empty() || answers.len() > ANSWERS_PER_SUGGESTION || !answers.iter().all(holds) {
        return Err(SuggestionError::BadSuggestion);
    }
    if !is_item_id(item) || !has_item(pool, item).await? {
        return Err(SuggestionError::UnknownItem);
    }
    let (imported,): (bool,) = sqlx::query_as(
        "SELECT EXISTS (SELECT 1 FROM imports WHERE library_identifier = $1 AND item = $2)",
    )
    .bind(identifier)
    .bind(item)
    .fetch_one(pool)
    .await?;
    if !imported {
        return Err(SuggestionError::NotImported);
    }
    let content = content_of(pool, item, version)
        .await?
        .ok_or(SuggestionError::UnknownVersion)?;
    let asked = question_of(&content, question).ok_or(SuggestionError::UnknownQuestion)?;
    for answer in answers {
        // An answer the question already accepts suggests nothing.
        if accepts(asked, answer) {
            continue;
        }
        sqlx::query(
            "INSERT INTO suggestions (library_identifier, item, version, question, answer)
             VALUES ($1, $2, $3, $4, $5) ON CONFLICT DO NOTHING",
        )
        .bind(identifier)
        .bind(item)
        .bind(version as i32)
        .bind(question as i32)
        .bind(answer)
        .execute(pool)
        .await?;
    }
    Ok(())
}

/// The suggestions of `item` as they stand against its latest version, or
/// `None` for an unknown item.
pub(crate) async fn of_item(
    pool: &PgPool,
    item: &str,
) -> Result<Option<ItemSuggestions>, SuggestionError> {
    Ok(standing(pool, item)
        .await?
        .map(|standing| standing.suggestions))
}

/// The latest version of an item and its suggestions.
struct Standing {
    suggestions: ItemSuggestions,
    content: Value,
}

async fn standing(pool: &PgPool, item: &str) -> Result<Option<Standing>, SuggestionError> {
    if !is_item_id(item) {
        return Ok(None);
    }
    let versions: Vec<(i32, String, Value)> = sqlx::query_as(
        "SELECT version, title, content FROM item_versions WHERE item = $1 ORDER BY version",
    )
    .bind(item)
    .fetch_all(pool)
    .await?;
    let Some((latest, title, content)) = versions.last().cloned() else {
        return Ok(None);
    };
    let contents: HashMap<i32, &Value> = versions
        .iter()
        .map(|(version, _, content)| (*version, content))
        .collect();
    let prompt = |question: &Value| question.get("prompt").cloned();
    let latest_questions = content["questions"].as_array().cloned().unwrap_or_default();
    // Where a question of any version stands in the latest one.
    let place = |version: i32, question: u32| -> Option<u32> {
        if version == latest {
            return question_of(&content, question).map(|_| question);
        }
        let asked = prompt(question_of(contents.get(&version)?, question)?)?;
        let index = latest_questions
            .iter()
            .position(|candidate| prompt(candidate).as_ref() == Some(&asked))?;
        let found = u32::try_from(index + 1).ok()?;
        question_of(&content, found).map(|_| found)
    };
    let rows: Vec<(i32, i32, String, String)> = sqlx::query_as(
        "SELECT version, question, answer, library_identifier FROM suggestions WHERE item = $1",
    )
    .bind(item)
    .fetch_all(pool)
    .await?;
    // The distinct identifiers behind each answer of each question: they
    // are counted here and go no further.
    let mut counted: BTreeMap<u32, HashMap<String, HashSet<String>>> = BTreeMap::new();
    for (version, question, answer, identifier) in rows {
        let Some(found) = place(version, question as u32) else {
            continue;
        };
        if accepts(
            question_of(&content, found).expect("a placed question"),
            &answer,
        ) {
            continue;
        }
        counted
            .entry(found)
            .or_default()
            .entry(answer)
            .or_default()
            .insert(identifier);
    }
    let questions = counted
        .into_iter()
        .map(|(question, answers)| {
            let mut answers: Vec<SuggestedAnswer> = answers
                .into_iter()
                .map(|(answer, families)| SuggestedAnswer {
                    answer,
                    families: families.len() as u32,
                })
                .collect();
            answers.sort_by(|a, b| b.families.cmp(&a.families).then(a.answer.cmp(&b.answer)));
            QuestionSuggestions {
                question,
                prompt: question_of(&content, question)
                    .and_then(|asked| asked["prompt"].as_str())
                    .map(str::to_string),
                answers,
            }
        })
        .collect();
    Ok(Some(Standing {
        suggestions: ItemSuggestions {
            item: item.to_string(),
            title,
            version: latest as u32,
            questions,
        },
        content,
    }))
}

/// The new version of `item` its author prepares by accepting `accepted`
/// from the suggestions: the latest version with those answers added, in
/// the body the share route takes (`docs/specs/interface-v1.md`, Shares),
/// its media included.
pub(crate) async fn prepare(
    pool: &PgPool,
    media: &dyn MediaStore,
    item: &str,
    accepted: &[Accepted],
) -> Result<Value, SuggestionError> {
    let standing = standing(pool, item)
        .await?
        .ok_or(SuggestionError::UnknownItem)?;
    if accepted.is_empty() || accepted.iter().any(|accepted| accepted.answers.is_empty()) {
        return Err(SuggestionError::BadSuggestion);
    }
    let mut content = standing.content;
    for accepted in accepted {
        let suggested = standing
            .suggestions
            .questions
            .iter()
            .find(|question| question.question == accepted.question)
            .ok_or(SuggestionError::NotSuggested)?;
        let index = accepted.question as usize - 1;
        let answers = content["questions"][index]["answers"]
            .as_array_mut()
            .expect("a question with suggestions takes answers");
        for answer in &accepted.answers {
            let answer = answer.trim();
            if !suggested.answers.iter().any(|known| known.answer == answer) {
                return Err(SuggestionError::NotSuggested);
            }
            if !answers.iter().any(|known| known.as_str() == Some(answer)) {
                answers.push(json!(answer));
            }
        }
    }
    let version = standing.suggestions.version;
    let (kind, title, metadata): (String, String, Value) = sqlx::query_as(
        "SELECT type, title, metadata FROM item_versions WHERE item = $1 AND version = $2",
    )
    .bind(item)
    .bind(version as i32)
    .fetch_one(pool)
    .await?;
    Ok(json!({
        "item": item,
        "type": kind,
        "title": title,
        "metadata": metadata,
        "content": content,
        "media": media_of(pool, media, item, version).await?,
    }))
}

/// The media of a published version, in base64 as the share route takes
/// them, from its signed document.
async fn media_of(
    pool: &PgPool,
    store: &dyn MediaStore,
    item: &str,
    version: u32,
) -> Result<Vec<Value>, SuggestionError> {
    use base64::Engine;
    let stored = catalogue::document(pool, &item_path(item, version))
        .await?
        .ok_or(SuggestionError::Unavailable)?;
    let envelope: Value =
        serde_json::from_str(&stored.body).map_err(|_| SuggestionError::Unavailable)?;
    let payload: Value = serde_json::from_str(envelope["payload"].as_str().unwrap_or_default())
        .map_err(|_| SuggestionError::Unavailable)?;
    let mut media = Vec::new();
    for medium in payload["media"].as_array().cloned().unwrap_or_default() {
        let digest = medium["digest"].as_str().unwrap_or_default();
        let bytes = store
            .get(digest)
            .await
            .ok()
            .flatten()
            .ok_or(SuggestionError::Unavailable)?;
        media.push(json!({
            "base64": base64::engine::general_purpose::STANDARD.encode(bytes),
        }));
    }
    Ok(media)
}
