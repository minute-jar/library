//! Reports of shared items and their three paths (`docs/specs/interface-v1.md`,
//! Reports): a child's personal data, sexual or illegal content reviewed at
//! once and switched off or hidden; rights hidden after three families or
//! a formal notice; wrong answers sent to the author, then to a moderator.

mod common;

use std::io;
use std::sync::{Arc, Mutex};

use axum::http::StatusCode;
use common::*;
use jiff::Timestamp;
use library::contract::Kind;
use library::{
    Decision, Library, Message, Notifier, NotifyFuture, Outcome, Reason, ReviewFuture,
    ReviewOutcome, ReviewProvider, Submission,
};
use serde_json::{Value, json};

/// The Minute Jar account of the moderator who decides.
const MODERATOR: &str = "moderator-subject";

/// The instant every report in these tests arrives at, unless said otherwise.
fn received() -> Timestamp {
    at("2026-11-26T10:00:00Z")
}

/// What the review provider was asked: the item, its version and the reason to check.
type Asked = (String, u32, Option<Reason>);

/// A review provider that answers the same outcome to every submission, and
/// remembers what it was asked.
#[derive(Clone)]
struct FakeReviewProvider {
    outcome: Arc<Mutex<Option<ReviewOutcome>>>,
    asked: Arc<Mutex<Vec<Asked>>>,
}

impl FakeReviewProvider {
    fn answering(outcome: Option<ReviewOutcome>) -> Self {
        FakeReviewProvider {
            outcome: Arc::new(Mutex::new(outcome)),
            asked: Arc::default(),
        }
    }

    fn asked(&self) -> Vec<Asked> {
        self.asked.lock().unwrap().clone()
    }
}

impl ReviewProvider for FakeReviewProvider {
    fn review<'a>(&'a self, submission: Submission<'a>) -> ReviewFuture<'a> {
        self.asked.lock().unwrap().push((
            submission.item.to_string(),
            submission.version,
            submission.focus,
        ));
        let outcome = self.outcome.lock().unwrap().clone();
        Box::pin(async move { outcome.ok_or_else(|| io::Error::other("the provider is down")) })
    }
}

/// A notifier that keeps every message it was given to send.
#[derive(Clone, Default)]
struct FakeNotifier {
    sent: Arc<Mutex<Vec<(String, Message)>>>,
}

impl FakeNotifier {
    fn sent(&self) -> Vec<(String, Message)> {
        self.sent.lock().unwrap().clone()
    }
}

impl Notifier for FakeNotifier {
    fn send<'a>(&'a self, to: &'a str, message: &'a Message) -> NotifyFuture<'a> {
        self.sent
            .lock()
            .unwrap()
            .push((to.to_string(), message.clone()));
        Box::pin(async { Ok(()) })
    }
}

struct Moderated {
    library: Library,
    review_provider: FakeReviewProvider,
    notifier: FakeNotifier,
}

/// A library holding the verbs bank and the fractions bank, whose review provider
/// finds nothing unless told otherwise.
async fn moderated() -> Moderated {
    moderated_with(Some(ReviewOutcome::Published)).await
}

async fn moderated_with(outcome: Option<ReviewOutcome>) -> Moderated {
    let review_provider = FakeReviewProvider::answering(outcome);
    let notifier = FakeNotifier::default();
    let library = library()
        .await
        .with_reports(review_provider.clone(), notifier.clone(), received);
    library
        .publish_version(verbs("Irregular verbs"), at("2026-11-20T18:00:00Z"))
        .await
        .unwrap();
    library
        .publish_version(fractions(), at("2026-11-21T08:00:00Z"))
        .await
        .unwrap();
    library
        .grant_moderator(MODERATOR, "Moderator", received())
        .await
        .unwrap();
    // Signed again before the reports arrive, so that it is current when checked.
    library.refresh(at("2026-11-26T09:00:00Z")).await.unwrap();
    Moderated {
        library,
        review_provider,
        notifier,
    }
}

/// Reports `item` for `reason` under `identifier`, as a family server does.
async fn report(library: &Library, identifier: &str, item: &str, reason: &str) -> Answer {
    post_json(
        library,
        "/v1/reports",
        &json!({"library_identifier": identifier, "item": item, "reason": reason, "details": "Question 1"}),
    )
    .await
}

/// The identifier of the report an answer acknowledges.
fn report_id(answer: &Answer) -> String {
    assert_eq!(
        answer.status,
        StatusCode::ACCEPTED,
        "{}",
        String::from_utf8_lossy(&answer.body)
    );
    let id = answer.json()["report"].as_str().unwrap().to_string();
    assert_eq!(id.len(), 32);
    id
}

async fn status(library: &Library, id: &str) -> Value {
    let answer = get(library, &format!("/v1/reports/{id}")).await;
    assert_eq!(answer.status, StatusCode::OK);
    answer.json()
}

fn in_index(index: &Value, item: &str) -> bool {
    index["items"]
        .as_array()
        .unwrap()
        .iter()
        .any(|entry| entry["item"] == item)
}

#[tokio::test]
async fn a_wrong_answer_is_sent_to_the_author_and_acknowledged_with_the_report() {
    let moderated = moderated().await;

    let answer = report(&moderated.library, FAMILY_A, VERBS, "wrong_answer").await;
    let id = report_id(&answer);

    assert_eq!(
        status(&moderated.library, &id).await,
        json!({
            "report": id,
            "item": VERBS,
            "reason": "wrong_answer",
            "status": "with_author",
            "received_at": "2026-11-26T10:00:00Z",
            "decision": null
        })
    );
    let notifications = moderated.library.author_notifications(VERBS).await.unwrap();
    assert_eq!(notifications.len(), 1);
    assert_eq!(notifications[0].report, id);
    assert_eq!(notifications[0].reason, Reason::WrongAnswer);
    assert_eq!(notifications[0].details.as_deref(), Some("Question 1"));
    assert!(
        moderated
            .library
            .moderation_queue()
            .await
            .unwrap()
            .is_empty()
    );
    assert!(moderated.review_provider.asked().is_empty());
}

#[tokio::test]
async fn a_wrong_answer_goes_to_a_moderator_after_seven_days_without_a_new_version() {
    let moderated = moderated().await;
    let id = report_id(&report(&moderated.library, FAMILY_A, VERBS, "wrong_answer").await);

    moderated
        .library
        .escalate_reports(at("2026-12-03T09:59:59Z"))
        .await
        .unwrap();
    assert!(
        moderated
            .library
            .moderation_queue()
            .await
            .unwrap()
            .is_empty()
    );

    moderated
        .library
        .escalate_reports(at("2026-12-03T10:00:00Z"))
        .await
        .unwrap();
    let queue = moderated.library.moderation_queue().await.unwrap();
    assert_eq!(queue.len(), 1);
    assert_eq!(queue[0].report, id);
    assert_eq!(queue[0].version, 1);
    assert_eq!(queue[0].effect, library::Effect::None);
    assert_eq!(
        status(&moderated.library, &id).await["status"],
        "with_moderator"
    );
    let index = fetch_verified(&moderated.library, "/v1/index", Kind::Index).await;
    assert!(
        in_index(&index, VERBS),
        "a wrong answer never hides an item"
    );
}

#[tokio::test]
async fn a_new_version_within_seven_days_closes_a_wrong_answer_as_corrected() {
    let moderated = moderated().await;
    let id = report_id(
        &post_json(
            &moderated.library,
            "/api/reports",
            &json!({"item": VERBS, "reason": "unsuitable_content", "contact": "parent@example.org"}),
        )
        .await,
    );
    moderated
        .library
        .publish_version(
            verbs("Irregular verbs, corrected"),
            at("2026-11-28T10:00:00Z"),
        )
        .await
        .unwrap();

    moderated
        .library
        .escalate_reports(at("2026-12-04T00:00:00Z"))
        .await
        .unwrap();

    assert!(
        moderated
            .library
            .moderation_queue()
            .await
            .unwrap()
            .is_empty()
    );
    assert_eq!(status(&moderated.library, &id).await["status"], "corrected");
    assert_eq!(
        moderated.notifier.sent().last().unwrap(),
        &(
            "parent@example.org".to_string(),
            Message::Corrected {
                report: id,
                item: VERBS.into(),
                reason: Reason::UnsuitableContent
            }
        )
    );
}

#[tokio::test]
async fn a_website_reporter_who_leaves_an_address_is_acknowledged() {
    let moderated = moderated().await;

    let id = report_id(
        &post_json(
            &moderated.library,
            "/api/reports",
            &json!({"item": FRACTIONS, "reason": "other", "details": "Spam", "contact": "parent@example.org"}),
        )
        .await,
    );
    report_id(
        &post_json(
            &moderated.library,
            "/api/reports",
            &json!({"item": FRACTIONS, "reason": "other"}),
        )
        .await,
    );

    assert_eq!(
        moderated.notifier.sent(),
        vec![(
            "parent@example.org".to_string(),
            Message::Acknowledgement {
                report: id.clone(),
                item: FRACTIONS.into(),
                reason: Reason::Other
            }
        )]
    );
    let queue = moderated.library.moderation_queue().await.unwrap();
    assert_eq!(
        queue.len(),
        2,
        "another reason goes straight to a moderator"
    );
    let spam = queue.iter().find(|queued| queued.report == id).unwrap();
    assert_eq!(spam.details.as_deref(), Some("Spam"));
}

fn listed(withdrawals: &Value) -> Vec<(String, String, String)> {
    withdrawals["withdrawals"]
        .as_array()
        .unwrap()
        .iter()
        .map(|entry| {
            (
                entry["item"].as_str().unwrap().to_string(),
                entry["kind"].as_str().unwrap().to_string(),
                entry["reason"].as_str().unwrap().to_string(),
            )
        })
        .collect()
}

#[tokio::test]
async fn a_plausible_report_of_personal_data_switches_the_item_off_at_once() {
    let moderated = moderated_with(Some(ReviewOutcome::Held {
        findings: "A full name in question 1".into(),
    }))
    .await;

    let id = report_id(&report(&moderated.library, FAMILY_A, VERBS, "personal_data").await);

    assert_eq!(
        moderated.review_provider.asked(),
        vec![(VERBS.to_string(), 1, Some(Reason::PersonalData))]
    );
    let withdrawals =
        fetch_verified(&moderated.library, "/v1/withdrawals", Kind::Withdrawals).await;
    assert_eq!(
        listed(&withdrawals),
        vec![(
            VERBS.to_string(),
            "switch_off".into(),
            "personal_data".into()
        )]
    );
    assert_eq!(withdrawals["issued_at"], "2026-11-26T10:00:00Z");
    let index = fetch_verified(&moderated.library, "/v1/index", Kind::Index).await;
    assert!(!in_index(&index, VERBS));
    let queue = moderated.library.moderation_queue().await.unwrap();
    assert_eq!(queue[0].report, id);
    assert_eq!(queue[0].effect, library::Effect::SwitchedOff);
    assert_eq!(
        queue[0].findings.as_deref(),
        Some("A full name in question 1")
    );
    assert!(
        moderated
            .library
            .author_notifications(VERBS)
            .await
            .unwrap()
            .is_empty()
    );
}

#[tokio::test]
async fn an_implausible_report_hides_the_item_from_new_imports_only() {
    let moderated = moderated().await;

    report_id(&report(&moderated.library, FAMILY_A, VERBS, "illegal_content").await);

    let index = fetch_verified(&moderated.library, "/v1/index", Kind::Index).await;
    assert!(!in_index(&index, VERBS));
    assert!(in_index(&index, FRACTIONS));
    let withdrawals =
        fetch_verified(&moderated.library, "/v1/withdrawals", Kind::Withdrawals).await;
    assert!(listed(&withdrawals).is_empty(), "families keep using it");
    assert_eq!(
        get(&moderated.library, &format!("/v1/items/{VERBS}/versions/1"))
            .await
            .status,
        StatusCode::OK
    );
    assert_eq!(
        moderated.library.moderation_queue().await.unwrap()[0].effect,
        library::Effect::Hidden
    );
}

#[tokio::test]
async fn a_rejected_report_puts_the_switched_off_item_back_by_itself() {
    let moderated = moderated_with(Some(ReviewOutcome::Refused {
        reasons: vec![library::RefusalReason {
            question: Some(1),
            reason: Reason::SexualContent,
            explanation: "An explicit picture".into(),
        }],
    }))
    .await;
    let id = report_id(
        &post_json(
            &moderated.library,
            "/api/reports",
            &json!({"item": VERBS, "reason": "sexual_content", "contact": "parent@example.org"}),
        )
        .await,
    );

    let decision = Decision {
        outcome: Outcome::Rejected,
        reasons: "The picture is a diagram of a flower.".into(),
    };
    moderated
        .library
        .decide_report(MODERATOR, &id, decision.clone(), at("2026-11-27T10:00:00Z"))
        .await
        .unwrap();

    let withdrawals =
        fetch_verified(&moderated.library, "/v1/withdrawals", Kind::Withdrawals).await;
    assert!(listed(&withdrawals).is_empty());
    let index = fetch_verified(&moderated.library, "/v1/index", Kind::Index).await;
    assert!(in_index(&index, VERBS));
    assert_eq!(
        status(&moderated.library, &id).await["decision"],
        json!({"outcome": "rejected", "reasons": "The picture is a diagram of a flower.", "decided_at": "2026-11-27T10:00:00Z"})
    );
    assert_eq!(
        moderated.notifier.sent().last().unwrap(),
        &(
            "parent@example.org".to_string(),
            Message::Decision {
                report: id.clone(),
                item: VERBS.into(),
                reason: Reason::SexualContent,
                decision: decision.clone()
            }
        )
    );
    let told = moderated.library.author_notifications(VERBS).await.unwrap();
    assert_eq!(told.len(), 1);
    assert_eq!(told[0].decision, Some(decision.clone()));
    assert!(matches!(
        moderated
            .library
            .decide_report(MODERATOR, &id, decision, at("2026-11-27T11:00:00Z"))
            .await,
        Err(library::DecideError::AlreadyDecided)
    ));
}

#[tokio::test]
async fn a_rejected_report_shows_a_hidden_item_again() {
    let moderated = moderated().await;
    let id = report_id(&report(&moderated.library, FAMILY_A, VERBS, "personal_data").await);

    moderated
        .library
        .decide_report(
            MODERATOR,
            &id,
            Decision {
                outcome: Outcome::Rejected,
                reasons: "No personal data.".into(),
            },
            at("2026-11-27T10:00:00Z"),
        )
        .await
        .unwrap();

    let index = fetch_verified(&moderated.library, "/v1/index", Kind::Index).await;
    assert!(in_index(&index, VERBS));
    assert!(
        moderated
            .library
            .moderation_queue()
            .await
            .unwrap()
            .is_empty()
    );
}

#[tokio::test]
async fn an_upheld_report_withdraws_the_item_by_moderation() {
    let moderated = moderated_with(Some(ReviewOutcome::Held {
        findings: "An address".into(),
    }))
    .await;
    let id = report_id(&report(&moderated.library, FAMILY_A, VERBS, "personal_data").await);

    moderated
        .library
        .decide_report(
            MODERATOR,
            &id,
            Decision {
                outcome: Outcome::Upheld,
                reasons: "A home address.".into(),
            },
            at("2026-11-27T10:00:00Z"),
        )
        .await
        .unwrap();

    let withdrawals =
        fetch_verified(&moderated.library, "/v1/withdrawals", Kind::Withdrawals).await;
    assert_eq!(
        listed(&withdrawals),
        vec![(
            VERBS.to_string(),
            "moderation".into(),
            "personal_data".into()
        )]
    );
    assert_eq!(status(&moderated.library, &id).await["status"], "upheld");
}

#[tokio::test]
async fn two_families_that_imported_the_item_switch_it_off_without_review() {
    let moderated = moderated().await;
    import(&moderated.library, FAMILY_A, VERBS).await;
    import(&moderated.library, FAMILY_B, VERBS).await;

    report_id(&report(&moderated.library, FAMILY_A, VERBS, "personal_data").await);
    report_id(&report(&moderated.library, FAMILY_B, VERBS, "illegal_content").await);

    let withdrawals =
        fetch_verified(&moderated.library, "/v1/withdrawals", Kind::Withdrawals).await;
    assert_eq!(
        listed(&withdrawals),
        vec![(
            VERBS.to_string(),
            "switch_off".into(),
            "illegal_content".into()
        )]
    );
    assert_eq!(
        moderated.review_provider.asked().len(),
        1,
        "only the first was reviewed"
    );
}

#[tokio::test]
async fn a_review_that_cannot_be_made_switches_the_item_off() {
    let moderated = moderated_with(None).await;

    report_id(&report(&moderated.library, FAMILY_A, VERBS, "personal_data").await);

    let withdrawals =
        fetch_verified(&moderated.library, "/v1/withdrawals", Kind::Withdrawals).await;
    assert_eq!(listed(&withdrawals).len(), 1);
}

#[tokio::test]
async fn a_report_never_turns_the_author_withdrawal_into_a_switch_off() {
    let moderated = moderated_with(Some(ReviewOutcome::Held {
        findings: "A name".into(),
    }))
    .await;
    moderated
        .library
        .withdraw(
            VERBS,
            None,
            library::Withdrawal::Author,
            at("2026-11-26T09:30:00Z"),
        )
        .await
        .unwrap();

    report_id(&report(&moderated.library, FAMILY_A, VERBS, "personal_data").await);

    let withdrawals =
        fetch_verified(&moderated.library, "/v1/withdrawals", Kind::Withdrawals).await;
    assert_eq!(
        listed(&withdrawals),
        vec![(VERBS.to_string(), "author".into(), "author_choice".into())]
    );
    assert_eq!(moderated.library.moderation_queue().await.unwrap().len(), 1);
}

const FAMILY_C: &str = "cccccccccccccccc0000000000000003";
const FAMILY_D: &str = "dddddddddddddddd0000000000000004";

#[tokio::test]
async fn rights_hide_the_item_after_three_distinct_families_that_imported_it() {
    let moderated = moderated().await;
    for family in [FAMILY_A, FAMILY_B, FAMILY_C] {
        import(&moderated.library, family, VERBS).await;
    }

    report_id(&report(&moderated.library, FAMILY_A, VERBS, "rights").await);
    report_id(&report(&moderated.library, FAMILY_A, VERBS, "rights").await);
    report_id(&report(&moderated.library, FAMILY_D, VERBS, "rights").await);
    report_id(&report(&moderated.library, FAMILY_B, VERBS, "rights").await);
    let index = fetch_verified(&moderated.library, "/v1/index", Kind::Index).await;
    assert!(
        in_index(&index, VERBS),
        "one family counts once, and only a family that imported the item"
    );

    report_id(&report(&moderated.library, FAMILY_C, VERBS, "rights").await);
    let index = fetch_verified(&moderated.library, "/v1/index", Kind::Index).await;
    assert!(!in_index(&index, VERBS));
    let withdrawals =
        fetch_verified(&moderated.library, "/v1/withdrawals", Kind::Withdrawals).await;
    assert!(listed(&withdrawals).is_empty());
    assert!(moderated.review_provider.asked().is_empty());
}

#[tokio::test]
async fn a_formal_notice_hides_the_item_at_once() {
    let moderated = moderated().await;

    let id = report_id(
        &post_json(
            &moderated.library,
            "/api/reports",
            &json!({
                "item": VERBS,
                "reason": "rights",
                "details": "Copied from page 12.",
                "contact": "rights@publisher.example",
                "notice": {"name": "Example Publishing", "work": "English Grammar, Year 5"}
            }),
        )
        .await,
    );

    let index = fetch_verified(&moderated.library, "/v1/index", Kind::Index).await;
    assert!(!in_index(&index, VERBS));
    let queue = moderated.library.moderation_queue().await.unwrap();
    assert_eq!(queue[0].report, id);
    assert_eq!(queue[0].effect, library::Effect::Hidden);
    assert_eq!(
        queue[0].notice,
        Some(library::FormalNotice {
            name: "Example Publishing".into(),
            work: "English Grammar, Year 5".into()
        })
    );
    assert_eq!(moderated.notifier.sent()[0].0, "rights@publisher.example");
}

#[tokio::test]
async fn a_website_rights_report_without_a_notice_does_not_hide_the_item() {
    let moderated = moderated().await;

    for _ in 0..3 {
        report_id(
            &post_json(
                &moderated.library,
                "/api/reports",
                &json!({"item": VERBS, "reason": "rights"}),
            )
            .await,
        );
    }

    let index = fetch_verified(&moderated.library, "/v1/index", Kind::Index).await;
    assert!(in_index(&index, VERBS));
}

#[tokio::test]
async fn reports_are_refused_with_fixed_codes() {
    let moderated = moderated().await;
    let library = &moderated.library;
    let refusals = [
        (
            "/v1/reports",
            json!({"library_identifier": "ABC", "item": VERBS, "reason": "rights"}),
            StatusCode::BAD_REQUEST,
            "bad_library_identifier",
        ),
        (
            "/v1/reports",
            json!({"library_identifier": FAMILY_A, "item": VERBS, "reason": "spam"}),
            StatusCode::BAD_REQUEST,
            "bad_reason",
        ),
        (
            "/v1/reports",
            json!({"library_identifier": FAMILY_A, "item": "00000000000000000000000000000009", "reason": "other"}),
            StatusCode::NOT_FOUND,
            "unknown_item",
        ),
        (
            "/v1/reports",
            json!({"library_identifier": FAMILY_A, "item": VERBS, "reason": "other", "contact": "parent@example.org"}),
            StatusCode::BAD_REQUEST,
            "bad_request",
        ),
        (
            "/v1/reports",
            json!({"library_identifier": FAMILY_A, "item": VERBS, "reason": "other", "details": "x".repeat(2001)}),
            StatusCode::BAD_REQUEST,
            "bad_request",
        ),
        (
            "/api/reports",
            json!({"library_identifier": FAMILY_A, "item": VERBS, "reason": "other"}),
            StatusCode::BAD_REQUEST,
            "bad_request",
        ),
        (
            "/api/reports",
            json!({"item": VERBS, "reason": "other", "contact": "not an address"}),
            StatusCode::BAD_REQUEST,
            "bad_contact",
        ),
        (
            "/api/reports",
            json!({"item": VERBS, "reason": "other", "contact": "a@example.org", "notice": {"name": "A", "work": "B"}}),
            StatusCode::BAD_REQUEST,
            "bad_notice",
        ),
        (
            "/api/reports",
            json!({"item": VERBS, "reason": "rights", "notice": {"name": "A", "work": "B"}}),
            StatusCode::BAD_REQUEST,
            "bad_notice",
        ),
    ];
    for (path, body, expected, code) in refusals {
        let answer = post_json(library, path, &body).await;
        assert_eq!(answer.status, expected, "{body}");
        assert_eq!(answer.json(), json!({"error": code}), "{body}");
    }
    let unknown = get(library, "/v1/reports/00000000000000000000000000000000").await;
    assert_eq!(unknown.status, StatusCode::NOT_FOUND);
    assert_eq!(unknown.json(), json!({"error": "unknown_report"}));
    assert!(library.moderation_queue().await.unwrap().is_empty());
}

#[tokio::test]
async fn a_report_answer_carries_nothing_to_keep_and_no_cookie() {
    let moderated = moderated().await;

    let answer = report(&moderated.library, FAMILY_A, VERBS, "other").await;

    report_id(&answer);
    assert_eq!(answer.headers["cache-control"], "no-store");
    assert!(answer.headers.get("set-cookie").is_none());
}

#[tokio::test]
async fn a_library_opened_without_a_review_provider_refuses_reports() {
    let library = library().await;
    library
        .publish_version(verbs("Irregular verbs"), at("2026-11-20T18:00:00Z"))
        .await
        .unwrap();

    let answer = report(&library, FAMILY_A, VERBS, "personal_data").await;

    assert_eq!(answer.status, StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(answer.json(), json!({"error": "unavailable"}));
}

/// A library with accounts, cards and reports whose review finds nothing,
/// the verbs bank published.
async fn with_accounts_and_reports() -> common::shares::Sharing {
    let sharing = common::shares::with_sharing().await;
    sharing
        .library
        .publish_version(verbs("Irregular verbs"), at("2026-11-20T18:00:00Z"))
        .await
        .unwrap();
    sharing
}

/// Reports the verbs bank for `reason` from the website, with `session`.
async fn report_signed_in(library: &Library, session: &str, reason: &str) -> Answer {
    common::identity::with_session(
        library,
        "POST",
        "/api/reports",
        session,
        Some(json!({"item": VERBS, "reason": reason})),
    )
    .await
}

#[tokio::test]
async fn a_verified_library_accounts_report_of_personal_data_switches_the_item_off_at_once() {
    let sharing = with_accounts_and_reports().await;
    let session = sharing.author("reporting-parent-subject", "Lark").await;

    let id = report_id(&report_signed_in(&sharing.library, &session, "personal_data").await);

    let withdrawals = fetch_verified(&sharing.library, "/v1/withdrawals", Kind::Withdrawals).await;
    assert_eq!(
        listed(&withdrawals),
        vec![(
            VERBS.to_string(),
            "switch_off".into(),
            "personal_data".into()
        )],
        "switched off although the review finds nothing"
    );
    let queue = sharing.library.moderation_queue().await.unwrap();
    assert_eq!(queue[0].report, id);
    assert_eq!(queue[0].effect, library::Effect::SwitchedOff);
}

#[tokio::test]
async fn an_account_without_a_verified_card_reports_like_anyone() {
    let sharing = with_accounts_and_reports().await;
    let session = common::cards::account(
        &sharing.library,
        &sharing.provider,
        &sharing.clock,
        "reporting-parent-subject",
        "Lark",
        "separate",
    )
    .await;

    report_id(&report_signed_in(&sharing.library, &session, "personal_data").await);

    let withdrawals = fetch_verified(&sharing.library, "/v1/withdrawals", Kind::Withdrawals).await;
    assert!(listed(&withdrawals).is_empty(), "hidden, not switched off");
    let index = fetch_verified(&sharing.library, "/v1/index", Kind::Index).await;
    assert!(!in_index(&index, VERBS));
}

#[tokio::test]
async fn a_report_with_a_session_that_is_no_longer_valid_is_received_anonymously() {
    let sharing = with_accounts_and_reports().await;

    report_id(&report_signed_in(&sharing.library, "no-such-session", "personal_data").await);

    let withdrawals = fetch_verified(&sharing.library, "/v1/withdrawals", Kind::Withdrawals).await;
    assert!(listed(&withdrawals).is_empty());
}
