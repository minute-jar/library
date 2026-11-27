//! Ratings a family server leaves under its library identifier, from which
//! the index publishes how each shared item is rated.
//!
//! As with imports, an identifier is kept only with the item it rated and
//! the rating, never with a time, an address or an account.
//!
//! A short review is public text from an anonymous family: it is held
//! pending until the review publishes it, and only published reviews are
//! shown, never with anything that tells families apart.

use serde::Serialize;
use sqlx::PgPool;

use crate::catalogue::{has_item, is_item_id};
use crate::imports::is_library_identifier;

/// Why a rating could not be recorded.
#[derive(Debug)]
pub enum RatingError {
    /// The library identifier is not 32 lowercase hexadecimal digits.
    BadIdentifier,
    /// The rating is not a whole number of stars from 1 to 5.
    BadRating,
    /// The review is longer than [`REVIEW_LENGTH`] characters.
    BadReview,
    /// No shared item has this identifier.
    UnknownItem,
    /// The identifier never reported importing this item: ratings come from
    /// families that used it.
    NotImported,
    Database(sqlx::Error),
}

impl From<sqlx::Error> for RatingError {
    fn from(error: sqlx::Error) -> Self {
        RatingError::Database(error)
    }
}

/// The longest short review, in characters.
pub const REVIEW_LENGTH: usize = 280;

/// How a shared item is rated, as the website shows it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ItemRatings {
    /// The number of ratings of 1 to 5 stars.
    pub counts: [i64; 5],
    /// The published reviews, highest rated first.
    pub reviews: Vec<Review>,
}

/// A published short review and the rating it came with.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Review {
    pub stars: u8,
    pub text: String,
}

pub async fn record(
    pool: &PgPool,
    identifier: &str,
    item: &str,
    stars: u8,
    review: Option<&str>,
) -> Result<(), RatingError> {
    if !is_library_identifier(identifier) {
        return Err(RatingError::BadIdentifier);
    }
    if !(1..=5).contains(&stars) {
        return Err(RatingError::BadRating);
    }
    let review = review.map(str::trim).filter(|text| !text.is_empty());
    if review.is_some_and(|text| text.chars().count() > REVIEW_LENGTH) {
        return Err(RatingError::BadReview);
    }
    if !is_item_id(item) || !has_item(pool, item).await? {
        return Err(RatingError::UnknownItem);
    }
    // A new rating replaces the earlier one with its review, which is held
    // again until reviewed.
    let recorded = sqlx::query(
        "INSERT INTO ratings (library_identifier, item, stars, review, review_state)
         SELECT $1, $2, $3, $4, CASE WHEN $4::TEXT IS NULL THEN NULL ELSE 'pending' END
         WHERE EXISTS (SELECT 1 FROM imports WHERE library_identifier = $1 AND item = $2)
         ON CONFLICT (library_identifier, item) DO UPDATE
         SET stars = EXCLUDED.stars, review = EXCLUDED.review, review_state = EXCLUDED.review_state",
    )
    .bind(identifier)
    .bind(item)
    .bind(i16::from(stars))
    .bind(review)
    .execute(pool)
    .await?
    .rows_affected();
    if recorded == 0 {
        return Err(RatingError::NotImported);
    }
    Ok(())
}

pub async fn of_item(pool: &PgPool, item: &str) -> Result<ItemRatings, RatingError> {
    if !is_item_id(item) || !has_item(pool, item).await? {
        return Err(RatingError::UnknownItem);
    }
    let counted: Vec<(i16, i64)> =
        sqlx::query_as("SELECT stars, count(*) FROM ratings WHERE item = $1 GROUP BY stars")
            .bind(item)
            .fetch_all(pool)
            .await?;
    let mut counts = [0; 5];
    for (stars, count) in counted {
        counts[usize::try_from(stars - 1).expect("stars from 1 to 5")] = count;
    }
    let reviews: Vec<(i16, String)> = sqlx::query_as(
        "SELECT stars, review FROM ratings WHERE item = $1 AND review_state = 'published'
         ORDER BY stars DESC, review",
    )
    .bind(item)
    .fetch_all(pool)
    .await?;
    Ok(ItemRatings {
        counts,
        reviews: reviews
            .into_iter()
            .map(|(stars, text)| Review {
                stars: u8::try_from(stars).expect("stars from 1 to 5"),
                text,
            })
            .collect(),
    })
}
