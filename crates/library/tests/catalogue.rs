//! The catalogue JSON API the website browses and searches
//! (`docs/specs/catalogue-api.md`): unsigned, anonymous, read-only.

mod common;

use axum::http::StatusCode;
use common::*;
use library::{Reason, Withdrawal};
use serde_json::json;

#[tokio::test]
async fn the_catalogue_lists_every_shared_item_at_its_latest_version_newest_first() {
    let library = library().await;
    library
        .publish_version(verbs("Irregular verbs"), at("2026-11-20T18:00:00Z"))
        .await
        .unwrap();
    library
        .publish_version(fractions(), at("2026-11-21T08:00:00Z"))
        .await
        .unwrap();
    library
        .publish_version(verbs("Irregular verbs, part 1"), at("2026-11-26T09:00:00Z"))
        .await
        .unwrap();

    let answer = get(&library, "/api/catalogue").await;

    assert_eq!(answer.status, StatusCode::OK);
    assert_eq!(answer.headers["content-type"], "application/json",);
    assert_eq!(
        answer.json()["items"],
        json!([
            {
                "item": VERBS,
                "version": 2,
                "type": "bank",
                "title": "Irregular verbs, part 1",
                "licence": "CC-BY-SA-4.0",
                "author": {"pseudonym": "Owl", "editorial": false},
                "derived_from": null,
                "metadata": {
                    "language": "en-GB",
                    "subject": "english",
                    "ages": {"min": 9, "max": 11},
                    "description": "The fifty most common irregular verbs.",
                    "curriculum": {"country": "GB", "year": "Year 5", "topic": "Verb forms"}
                },
                "selection": false,
                "signals": {"families": 0, "ratings": [0, 0, 0, 0, 0]},
                "published_at": "2026-11-26T09:00:00Z"
            },
            {
                "item": FRACTIONS,
                "version": 1,
                "type": "bank",
                "title": "Les fractions, CM1",
                "licence": "CC-BY-SA-4.0",
                "author": {"pseudonym": "Minute Jar", "editorial": true},
                "derived_from": null,
                "metadata": {
                    "language": "fr-FR",
                    "subject": "mathematics",
                    "ages": {"min": 9, "max": 10},
                    "description": "Comparer et ranger des fractions simples.",
                    "curriculum": null
                },
                "selection": false,
                "signals": {"families": 0, "ratings": [0, 0, 0, 0, 0]},
                "published_at": "2026-11-21T08:00:00Z"
            }
        ])
    );
}

/// The titles the catalogue answers for a query string.
async fn titles(library: &library::Library, path: &str) -> Vec<String> {
    let answer = get(library, path).await;
    assert_eq!(answer.status, StatusCode::OK, "{path}");
    answer.json()["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|item| item["title"].as_str().unwrap().to_string())
        .collect()
}

async fn verbs_and_fractions() -> library::Library {
    let library = library().await;
    library
        .publish_version(verbs("Irregular verbs"), at("2026-11-20T18:00:00Z"))
        .await
        .unwrap();
    library
        .publish_version(fractions(), at("2026-11-21T08:00:00Z"))
        .await
        .unwrap();
    library
}

#[tokio::test]
async fn a_language_filter_matches_every_region_of_that_language() {
    let library = verbs_and_fractions().await;

    assert_eq!(
        titles(&library, "/api/catalogue?language=fr").await,
        ["Les fractions, CM1"]
    );
    assert_eq!(
        titles(&library, "/api/catalogue?language=en-GB").await,
        ["Irregular verbs"]
    );
    assert_eq!(
        titles(&library, "/api/catalogue?language=de").await,
        Vec::<String>::new()
    );
}

#[tokio::test]
async fn a_subject_filter_keeps_the_items_of_that_subject() {
    let library = verbs_and_fractions().await;

    assert_eq!(
        titles(&library, "/api/catalogue?subject=mathematics").await,
        ["Les fractions, CM1"]
    );
}

#[tokio::test]
async fn an_age_filter_keeps_the_items_meant_for_that_age() {
    let library = verbs_and_fractions().await;

    assert_eq!(
        titles(&library, "/api/catalogue?age=11").await,
        ["Irregular verbs"]
    );
    assert_eq!(
        titles(&library, "/api/catalogue?age=9").await,
        ["Les fractions, CM1", "Irregular verbs"]
    );
    assert_eq!(
        titles(&library, "/api/catalogue?age=8").await,
        Vec::<String>::new()
    );
}

#[tokio::test]
async fn a_curriculum_filter_keeps_the_items_of_that_country_year_and_topic() {
    let library = verbs_and_fractions().await;

    assert_eq!(
        titles(
            &library,
            "/api/catalogue?country=GB&year=Year%205&topic=Verb%20forms"
        )
        .await,
        ["Irregular verbs"]
    );
    assert_eq!(
        titles(&library, "/api/catalogue?country=GB&year=Year%206").await,
        Vec::<String>::new()
    );
}

#[tokio::test]
async fn a_search_finds_words_of_the_title_and_description_in_any_form() {
    let library = verbs_and_fractions().await;

    assert_eq!(
        titles(&library, "/api/catalogue?q=verb").await,
        ["Irregular verbs"]
    );
    assert_eq!(
        titles(&library, "/api/catalogue?q=fraction").await,
        ["Les fractions, CM1"]
    );
    assert_eq!(
        titles(&library, "/api/catalogue?q=ranger").await,
        ["Les fractions, CM1"]
    );
    assert_eq!(
        titles(&library, "/api/catalogue?q=volcano").await,
        Vec::<String>::new()
    );
}

#[tokio::test]
async fn filters_and_search_combine() {
    let library = verbs_and_fractions().await;

    assert_eq!(
        titles(&library, "/api/catalogue?q=fraction&language=en").await,
        Vec::<String>::new()
    );
}

#[tokio::test]
async fn a_malformed_filter_is_refused() {
    let library = verbs_and_fractions().await;

    for query in ["age=nine", "age=-1", "language=%25", "language=fr_FR"] {
        let answer = get(&library, &format!("/api/catalogue?{query}")).await;

        assert_eq!(answer.status, StatusCode::BAD_REQUEST, "{query}");
        assert_eq!(answer.json(), json!({"error": "bad_filter"}), "{query}");
    }
}

#[tokio::test]
async fn the_catalogue_is_answered_twenty_items_a_page() {
    let library = library().await;
    for n in 1..=25 {
        let mut bank = verbs(&format!("Verbs {n}"));
        bank.item = format!("{n:032x}");
        library
            .publish_version(bank, at(&format!("2026-11-{n:02}T08:00:00Z")))
            .await
            .unwrap();
    }

    let first = get(&library, "/api/catalogue").await.json();
    let second = get(&library, "/api/catalogue?page=2").await.json();

    assert_eq!(
        (&first["total"], &first["page"], &first["pages"]),
        (&json!(25), &json!(1), &json!(2))
    );
    assert_eq!(first["items"].as_array().unwrap().len(), 20);
    assert_eq!(first["items"][0]["title"], "Verbs 25");
    assert_eq!((&second["page"], &second["pages"]), (&json!(2), &json!(2)));
    assert_eq!(
        titles(&library, "/api/catalogue?page=2").await,
        ["Verbs 5", "Verbs 4", "Verbs 3", "Verbs 2", "Verbs 1"]
    );
    assert_eq!(
        titles(&library, "/api/catalogue?page=3").await,
        Vec::<String>::new()
    );
    assert_eq!(
        get(&library, "/api/catalogue?page=0").await.status,
        StatusCode::BAD_REQUEST
    );
}

#[tokio::test]
async fn the_filters_offer_the_languages_subjects_and_curricula_of_the_catalogue() {
    let library = verbs_and_fractions().await;
    let mut verbs_6 = verbs("Irregular verbs, Year 6");
    verbs_6.item = "00000000000000000000000000000006".into();
    verbs_6.metadata.curriculum.as_mut().unwrap().year = "Year 6".into();
    library
        .publish_version(verbs_6, at("2026-11-22T08:00:00Z"))
        .await
        .unwrap();

    let answer = get(&library, "/api/catalogue/filters").await;

    assert_eq!(answer.status, StatusCode::OK);
    assert_eq!(answer.headers["content-type"], "application/json");
    assert_eq!(
        answer.json(),
        json!({
            "languages": ["en-GB", "fr-FR"],
            "subjects": ["english", "mathematics"],
            "curricula": [
                {"country": "GB", "year": "Year 5", "topic": "Verb forms"},
                {"country": "GB", "year": "Year 6", "topic": "Verb forms"}
            ]
        })
    );
}

#[tokio::test]
async fn a_withdrawn_item_leaves_the_catalogue_and_a_withdrawn_version_gives_way_to_the_one_before()
{
    let library = verbs_and_fractions().await;
    library
        .publish_version(verbs("Irregular verbs, part 1"), at("2026-11-22T08:00:00Z"))
        .await
        .unwrap();

    library
        .withdraw(
            FRACTIONS,
            None,
            Withdrawal::Author,
            at("2026-11-26T08:00:00Z"),
        )
        .await
        .unwrap();
    library
        .withdraw(
            VERBS,
            Some(2),
            Withdrawal::SwitchOff(Reason::PersonalData),
            at("2026-11-26T08:00:00Z"),
        )
        .await
        .unwrap();

    assert_eq!(
        titles(&library, "/api/catalogue").await,
        ["Irregular verbs"]
    );
    let filters = get(&library, "/api/catalogue/filters").await.json();
    assert_eq!(filters["languages"], json!(["en-GB"]));
}

#[tokio::test]
async fn an_entry_carries_the_signals_of_the_index() {
    let library = verbs_and_fractions().await;
    import(&library, FAMILY_A, VERBS).await;
    import(&library, FAMILY_B, VERBS).await;
    post_json(
        &library,
        "/v1/ratings",
        &json!({"library_identifier": FAMILY_A, "item": VERBS, "stars": 4}),
    )
    .await;

    let answer = get(&library, "/api/catalogue?subject=english").await.json();

    assert_eq!(
        answer["items"][0]["signals"],
        json!({"families": 2, "ratings": [0, 0, 0, 1, 0]})
    );
}
