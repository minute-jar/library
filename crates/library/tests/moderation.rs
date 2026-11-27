//! The moderation queue and the decisions log (`docs/specs/moderation-api.md`,
//! stories S22 and S26): moderators, named by the operator and signed in with
//! a recent passkey, decide reports with reasons sent to the author and the
//! reporter; either may appeal, and another moderator decides the appeal;
//! the decisions log gives the yearly transparency report its figures.

mod common;

use std::io;
use std::sync::{Arc, Mutex};

use axum::http::StatusCode;
use common::identity::*;
use common::*;
use jiff::SignedDuration;
use library::contract::Kind;
use library::{
    IdToken, Library, Message, Notifier, NotifyFuture, ReviewFuture, ReviewOutcome, Reviewer,
    Submission,
};
use serde_json::{Value, json};

/// A reviewer that finds nothing.
struct Publishing;

impl Reviewer for Publishing {
    fn review<'a>(&'a self, _: Submission<'a>) -> ReviewFuture<'a> {
        Box::pin(async { Ok::<_, io::Error>(ReviewOutcome::Published) })
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

const ALICE: &str = "moderator-alice-subject";
const BRUNO: &str = "moderator-bruno-subject";

struct Desk {
    library: Library,
    provider: FakeProvider,
    clock: Clock,
    notifier: FakeNotifier,
}

/// A library holding the verbs bank and the fractions bank, taking reports,
/// with two moderators the operator named: Alice and Bruno.
async fn desk() -> Desk {
    let (library, provider, clock) = with_accounts().await;
    let notifier = FakeNotifier::default();
    let reading = clock.clone();
    let library = library.with_reports(Publishing, notifier.clone(), move || reading.now());
    library
        .publish_version(verbs("Irregular verbs"), at("2026-11-20T18:00:00Z"))
        .await
        .unwrap();
    library
        .publish_version(fractions(), at("2026-11-21T08:00:00Z"))
        .await
        .unwrap();
    library.refresh(at("2026-11-26T09:00:00Z")).await.unwrap();
    library
        .grant_moderator(ALICE, "Alice", signed_in_at())
        .await
        .unwrap();
    library
        .grant_moderator(BRUNO, "Bruno", signed_in_at())
        .await
        .unwrap();
    Desk {
        library,
        provider,
        clock,
        notifier,
    }
}

impl Desk {
    /// A session of `subject` signed in just now with `token`.
    async fn session_with(&self, token: IdToken) -> String {
        session(&self.library, &self.provider, token).await
    }

    /// A session of the moderator `subject`, signed in with a passkey just now.
    async fn moderator(&self, subject: &str) -> String {
        let now = self.clock.now();
        self.session_with(passkey(subject, now)).await
    }

    async fn ask(&self, method: &str, path: &str, session: &str, body: Option<Value>) -> Answer {
        with_session(&self.library, method, path, session, body).await
    }

    /// Reports `item` for `reason`, as a family server does.
    async fn report(&self, item: &str, reason: &str) -> String {
        let answer = post_json(
            &self.library,
            "/v1/reports",
            &json!({"library_identifier": FAMILY_A, "item": item, "reason": reason, "details": "Question 1"}),
        )
        .await;
        assert_eq!(answer.status, StatusCode::ACCEPTED);
        answer.json()["report"].as_str().unwrap().to_string()
    }
}

fn text(answer: &Answer) -> String {
    String::from_utf8_lossy(&answer.body).into_owned()
}

#[tokio::test]
async fn a_moderator_signed_in_with_a_passkey_reads_the_queue_of_reports() {
    let desk = desk().await;
    let report = desk.report(VERBS, "other").await;
    let alice = desk.moderator(ALICE).await;

    let answer = desk.ask("GET", "/api/moderation/queue", &alice, None).await;

    assert_eq!(answer.status, StatusCode::OK, "{}", text(&answer));
    assert_eq!(
        answer.headers["cache-control"].to_str().unwrap(),
        "private, no-store"
    );
    assert_eq!(
        answer.json(),
        json!({"cases": [{
            "kind": "report",
            "report": report,
            "item": VERBS,
            "title": "Irregular verbs",
            "version": 1,
            "reason": "other",
            "details": "Question 1",
            "effect": "none",
            "findings": null,
            "notice": null,
            "received_at": "2026-11-27T10:00:00Z",
            "due_at": "2026-11-29T10:00:00Z",
        }]})
    );
}

#[tokio::test]
async fn everyone_but_a_moderator_with_a_recent_passkey_is_refused() {
    let desk = desk().await;
    let now = desk.clock.now();
    let parent = desk.session_with(passkey(SUBJECT, now)).await;
    let by_code = desk.session_with(email_code(ALICE, now)).await;
    let by_second_factor = desk
        .session_with(IdToken {
            acr: Some("aal2".into()),
            amr: vec!["pwd".into(), "totp".into()],
            ..passkey(ALICE, now)
        })
        .await;
    let by_google = desk
        .session_with(IdToken {
            amr: vec!["oidc".into()],
            ..passkey(ALICE, now)
        })
        .await;
    let alice = desk.moderator(ALICE).await;
    let bruno = desk.moderator(BRUNO).await;

    let refusal = |answer: Answer| (answer.status, answer.json()["error"].clone());
    async fn desk_ask(desk: &Desk, session: &str) -> Answer {
        desk.ask("GET", "/api/moderation/queue", session, None)
            .await
    }

    let signed_out = get(&desk.library, "/api/moderation/queue").await;
    assert_eq!(
        refusal(signed_out),
        (StatusCode::UNAUTHORIZED, json!("signed_out"))
    );
    assert_eq!(
        refusal(desk_ask(&desk, &parent).await),
        (StatusCode::FORBIDDEN, json!("not_a_moderator"))
    );
    for session in [&by_code, &by_second_factor, &by_google] {
        assert_eq!(
            refusal(desk_ask(&desk, session).await),
            (StatusCode::FORBIDDEN, json!("passkey_sign_in_required"))
        );
    }

    desk.clock.advance(SignedDuration::from_mins(16));
    assert_eq!(
        refusal(desk_ask(&desk, &alice).await),
        (StatusCode::FORBIDDEN, json!("passkey_sign_in_required")),
        "a passkey sign-in is recent for 15 minutes"
    );

    assert!(desk.library.revoke_moderator(BRUNO).await.unwrap());
    desk.clock.advance(SignedDuration::from_mins(-16));
    assert_eq!(
        refusal(desk_ask(&desk, &bruno).await),
        (StatusCode::FORBIDDEN, json!("not_a_moderator"))
    );
}

/// The withdrawals listed: item, kind and reason.
async fn withdrawals(library: &Library) -> Vec<(String, String, String)> {
    let list = fetch_verified(library, "/v1/withdrawals", Kind::Withdrawals).await;
    list["withdrawals"]
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

async fn in_index(library: &Library, item: &str) -> bool {
    let index = fetch_verified(library, "/v1/index", Kind::Index).await;
    index["items"]
        .as_array()
        .unwrap()
        .iter()
        .any(|entry| entry["item"] == item)
}

/// Reports `item` for `reason` from the website, leaving an address.
async fn website_report(library: &Library, item: &str, reason: &str) -> String {
    let answer = post_json(
        library,
        "/api/reports",
        &json!({"item": item, "reason": reason, "details": "Copied from a workbook", "contact": "reporter@example.org"}),
    )
    .await;
    assert_eq!(answer.status, StatusCode::ACCEPTED, "{}", text(&answer));
    answer.json()["report"].as_str().unwrap().to_string()
}

async fn decide(desk: &Desk, session: &str, report: &str, outcome: &str, reasons: &str) -> Answer {
    desk.ask(
        "POST",
        &format!("/api/moderation/reports/{report}/decision"),
        session,
        Some(json!({"outcome": outcome, "reasons": reasons})),
    )
    .await
}

#[tokio::test]
async fn a_moderator_upholds_a_report_with_reasons_sent_to_the_reporter_and_the_author() {
    let desk = desk().await;
    let report = website_report(&desk.library, VERBS, "rights").await;
    let alice = desk.moderator(ALICE).await;

    let answer = decide(
        &desk,
        &alice,
        &report,
        "upheld",
        "The questions copy a published workbook.",
    )
    .await;

    assert_eq!(answer.status, StatusCode::NO_CONTENT, "{}", text(&answer));
    assert_eq!(
        withdrawals(&desk.library).await,
        vec![(VERBS.to_string(), "moderation".into(), "rights".into())]
    );
    let decision = library::Decision {
        outcome: library::Outcome::Upheld,
        reasons: "The questions copy a published workbook.".into(),
    };
    assert_eq!(
        desk.notifier.sent().last().unwrap(),
        &(
            "reporter@example.org".to_string(),
            Message::Decision {
                report: report.clone(),
                item: VERBS.into(),
                reason: library::Reason::Rights,
                decision: decision.clone(),
            }
        )
    );
    let told = desk.library.author_notifications(VERBS).await.unwrap();
    assert_eq!(told.last().unwrap().decision, Some(decision));
    let state = get(&desk.library, &format!("/v1/reports/{report}")).await;
    assert_eq!(state.json()["status"], "upheld");
    let queue = desk.ask("GET", "/api/moderation/queue", &alice, None).await;
    assert_eq!(queue.json(), json!({"cases": []}));
}

#[tokio::test]
async fn a_decision_needs_reasons_and_is_made_once() {
    let desk = desk().await;
    let report = desk.report(VERBS, "other").await;
    let alice = desk.moderator(ALICE).await;
    let refusal = |answer: Answer| (answer.status, answer.json()["error"].clone());

    assert_eq!(
        refusal(decide(&desk, &alice, &report, "rejected", "  ").await),
        (StatusCode::BAD_REQUEST, json!("reasons_required"))
    );
    assert_eq!(
        refusal(decide(&desk, &alice, &report, "maybe", "Unclear.").await),
        (StatusCode::BAD_REQUEST, json!("bad_request"))
    );
    assert_eq!(
        refusal(
            decide(
                &desk,
                &alice,
                "00000000000000000000000000000000",
                "rejected",
                "Nothing wrong."
            )
            .await
        ),
        (StatusCode::NOT_FOUND, json!("unknown_report"))
    );
    assert_eq!(
        decide(&desk, &alice, &report, "rejected", "Nothing wrong.")
            .await
            .status,
        StatusCode::NO_CONTENT
    );
    assert_eq!(
        refusal(decide(&desk, &alice, &report, "upheld", "On second thought.").await),
        (StatusCode::CONFLICT, json!("already_decided"))
    );
    assert!(in_index(&desk.library, VERBS).await);
}

#[tokio::test]
async fn the_history_of_an_item_shows_its_reports_and_decisions_oldest_first() {
    let desk = desk().await;
    let first = desk.report(VERBS, "other").await;
    desk.clock.advance(SignedDuration::from_mins(1));
    let alice = desk.moderator(ALICE).await;
    decide(
        &desk,
        &alice,
        &first,
        "rejected",
        "Nothing breaks the rules.",
    )
    .await;
    desk.clock.advance(SignedDuration::from_mins(1));
    let second = website_report(&desk.library, VERBS, "rights").await;
    desk.report(FRACTIONS, "other").await;

    let answer = desk
        .ask(
            "GET",
            &format!("/api/moderation/items/{VERBS}"),
            &alice,
            None,
        )
        .await;

    assert_eq!(answer.status, StatusCode::OK, "{}", text(&answer));
    assert_eq!(
        answer.json(),
        json!({
            "item": VERBS,
            "title": "Irregular verbs",
            "history": [
                {"at": "2026-11-27T10:00:00Z", "event": "reported", "report": first,
                 "reason": "other", "details": "Question 1", "effect": "none", "findings": null, "notice": null},
                {"at": "2026-11-27T10:01:00Z", "event": "decided", "report": first,
                 "reason": "other", "outcome": "rejected", "reasons": "Nothing breaks the rules.", "moderator": "Alice"},
                {"at": "2026-11-27T10:02:00Z", "event": "reported", "report": second,
                 "reason": "rights", "details": "Copied from a workbook", "effect": "none", "findings": null, "notice": null},
            ],
        })
    );
    let unknown = desk
        .ask(
            "GET",
            "/api/moderation/items/00000000000000000000000000000000",
            &alice,
            None,
        )
        .await;
    assert_eq!(unknown.status, StatusCode::NOT_FOUND);
    assert_eq!(unknown.json()["error"], "unknown_item");
}

async fn decide_appeal(
    desk: &Desk,
    session: &str,
    appeal: &str,
    outcome: &str,
    reasons: &str,
) -> Answer {
    desk.ask(
        "POST",
        &format!("/api/moderation/appeals/{appeal}/decision"),
        session,
        Some(json!({"outcome": outcome, "reasons": reasons})),
    )
    .await
}

fn appeal_id(answer: &Answer) -> String {
    assert_eq!(answer.status, StatusCode::ACCEPTED, "{}", text(answer));
    assert_eq!(
        answer.headers["cache-control"].to_str().unwrap(),
        "no-store"
    );
    let id = answer.json()["appeal"].as_str().unwrap().to_string();
    assert_eq!(id.len(), 32);
    id
}

#[tokio::test]
async fn the_reporter_appeals_a_rejection_and_another_moderator_reverses_it() {
    let desk = desk().await;
    let report = website_report(&desk.library, VERBS, "rights").await;
    let alice = desk.moderator(ALICE).await;
    decide(&desk, &alice, &report, "rejected", "No workbook was named.").await;
    desk.clock.advance(SignedDuration::from_mins(1));

    let appeal = appeal_id(
        &post_json(
            &desk.library,
            &format!("/api/reports/{report}/appeal"),
            &json!({"statement": "Page 12 of English Grammar, Year 5.", "contact": "reporter@example.org"}),
        )
        .await,
    );

    let bruno = desk.moderator(BRUNO).await;
    let queue = desk.ask("GET", "/api/moderation/queue", &bruno, None).await;
    assert_eq!(
        queue.json(),
        json!({"cases": [{
            "kind": "appeal",
            "appeal": appeal,
            "report": report,
            "item": VERBS,
            "title": "Irregular verbs",
            "reason": "rights",
            "appellant": "reporter",
            "statement": "Page 12 of English Grammar, Year 5.",
            "decision": {"outcome": "rejected", "reasons": "No workbook was named.",
                         "moderator": "Alice", "decided_at": "2026-11-27T10:00:00Z"},
            "received_at": "2026-11-27T10:01:00Z",
            "due_at": "2026-11-29T10:01:00Z",
        }]})
    );
    let same = decide_appeal(&desk, &alice, &appeal, "reversed", "I was wrong.").await;
    assert_eq!(
        (same.status, same.json()["error"].clone()),
        (StatusCode::FORBIDDEN, json!("same_moderator"))
    );

    let answer = decide_appeal(&desk, &bruno, &appeal, "reversed", "Page 12 matches.").await;

    assert_eq!(answer.status, StatusCode::NO_CONTENT, "{}", text(&answer));
    assert_eq!(
        withdrawals(&desk.library).await,
        vec![(VERBS.to_string(), "moderation".into(), "rights".into())]
    );
    let state = get(&desk.library, &format!("/v1/reports/{report}"))
        .await
        .json();
    assert_eq!(state["status"], "upheld");
    assert_eq!(state["decision"]["reasons"], "Page 12 matches.");
    assert_eq!(
        get(&desk.library, &format!("/v1/appeals/{appeal}"))
            .await
            .json(),
        json!({
            "appeal": appeal,
            "report": report,
            "status": "reversed",
            "received_at": "2026-11-27T10:01:00Z",
            "decision": {"outcome": "reversed", "reasons": "Page 12 matches.", "decided_at": "2026-11-27T10:01:00Z"},
        })
    );
    let decision = library::AppealDecision {
        outcome: library::AppealOutcome::Reversed,
        reasons: "Page 12 matches.".into(),
    };
    assert_eq!(
        desk.notifier.sent().last().unwrap(),
        &(
            "reporter@example.org".to_string(),
            Message::AppealDecision {
                appeal: appeal.clone(),
                report: report.clone(),
                item: VERBS.into(),
                reason: library::Reason::Rights,
                decision: decision.clone(),
            }
        )
    );
    let told = desk.library.author_notifications(VERBS).await.unwrap();
    assert_eq!(told.last().unwrap().appeal, Some(decision));
    let history = desk
        .ask(
            "GET",
            &format!("/api/moderation/items/{VERBS}"),
            &bruno,
            None,
        )
        .await
        .json();
    let events: Vec<&str> = history["history"]
        .as_array()
        .unwrap()
        .iter()
        .map(|entry| entry["event"].as_str().unwrap())
        .collect();
    assert_eq!(
        events,
        ["reported", "decided", "appealed", "appeal_decided"]
    );
    assert_eq!(
        history["history"][2],
        json!({"at": "2026-11-27T10:01:00Z", "event": "appealed", "appeal": appeal, "report": report,
               "appellant": "reporter", "statement": "Page 12 of English Grammar, Year 5."})
    );
    assert_eq!(
        history["history"][3],
        json!({"at": "2026-11-27T10:01:00Z", "event": "appeal_decided", "appeal": appeal, "report": report,
               "outcome": "reversed", "reasons": "Page 12 matches.", "moderator": "Bruno"})
    );
}

#[tokio::test]
async fn the_author_appeals_a_withdrawal_and_gets_the_item_back_when_it_is_reversed() {
    let desk = desk().await;
    let report = desk.report(VERBS, "other").await;
    desk.library
        .decide_report(
            ALICE,
            &report,
            library::Decision {
                outcome: library::Outcome::Upheld,
                reasons: "Off topic.".into(),
            },
            desk.clock.now(),
        )
        .await
        .unwrap();
    assert!(!in_index(&desk.library, VERBS).await);

    let appeal = desk
        .library
        .appeal(
            &report,
            library::Appeal {
                appellant: library::Appellant::Author,
                statement: "Every question is about verbs.".into(),
                contact: None,
            },
            desk.clock.now(),
        )
        .await
        .unwrap();
    desk.library
        .decide_appeal(
            BRUNO,
            &appeal,
            library::AppealDecision {
                outcome: library::AppealOutcome::Reversed,
                reasons: "The questions are on topic.".into(),
            },
            desk.clock.now(),
        )
        .await
        .unwrap();

    assert!(withdrawals(&desk.library).await.is_empty());
    assert!(in_index(&desk.library, VERBS).await);
    let state = get(&desk.library, &format!("/v1/reports/{report}"))
        .await
        .json();
    assert_eq!(state["status"], "rejected");
}

/// A family server's appeal of `report`: its status and body.
async fn family_appeal(library: &Library, report: &str, statement: &str) -> (StatusCode, Value) {
    let answer = post_json(
        library,
        &format!("/v1/reports/{report}/appeal"),
        &json!({"statement": statement}),
    )
    .await;
    (answer.status, answer.json())
}

fn error((status, body): (StatusCode, Value)) -> (StatusCode, Value) {
    (status, body["error"].clone())
}

#[tokio::test]
async fn an_appeal_comes_after_a_decision_once_per_party_within_six_months() {
    let desk = desk().await;
    let report = desk.report(VERBS, "other").await;

    assert_eq!(
        error(family_appeal(&desk.library, &report, "Please look again.").await),
        (StatusCode::CONFLICT, json!("not_decided"))
    );
    assert_eq!(
        error(family_appeal(&desk.library, "00000000000000000000000000000000", "Please.").await),
        (StatusCode::NOT_FOUND, json!("unknown_report"))
    );
    let alice = desk.moderator(ALICE).await;
    decide(&desk, &alice, &report, "rejected", "Nothing wrong.").await;
    assert_eq!(
        error(family_appeal(&desk.library, &report, " ").await),
        (StatusCode::BAD_REQUEST, json!("statement_required"))
    );
    let with_contact = post_json(
        &desk.library,
        &format!("/v1/reports/{report}/appeal"),
        &json!({"statement": "Please.", "contact": "parent@example.org"}),
    )
    .await;
    assert_eq!(
        with_contact.status,
        StatusCode::BAD_REQUEST,
        "a family server leaves no address"
    );
    let (status, body) = family_appeal(&desk.library, &report, "Question 1 is wrong.").await;
    assert_eq!(status, StatusCode::ACCEPTED);
    assert_eq!(
        error(family_appeal(&desk.library, &report, "Again.").await),
        (StatusCode::CONFLICT, json!("already_appealed"))
    );
    let bruno = desk.moderator(BRUNO).await;
    let appealed = body["appeal"].as_str().unwrap();
    assert_eq!(
        decide_appeal(
            &desk,
            &bruno,
            appealed,
            "maintained",
            "Question 1 is right."
        )
        .await
        .status,
        StatusCode::NO_CONTENT
    );
    assert_eq!(
        decide_appeal(&desk, &bruno, appealed, "reversed", "Hmm.")
            .await
            .json()["error"],
        "already_decided"
    );
    assert_eq!(
        decide_appeal(
            &desk,
            &bruno,
            "00000000000000000000000000000000",
            "reversed",
            "Hmm."
        )
        .await
        .json()["error"],
        "unknown_appeal"
    );
    assert!(
        in_index(&desk.library, VERBS).await,
        "a maintained rejection changes nothing"
    );
    assert_eq!(
        get(&desk.library, &format!("/v1/appeals/{appealed}"))
            .await
            .json()["decision"]["outcome"],
        "maintained"
    );

    let late = desk.report(FRACTIONS, "other").await;
    decide(&desk, &bruno, &late, "rejected", "Nothing wrong.").await;
    let after = desk
        .clock
        .now()
        .checked_add(SignedDuration::from_hours(184 * 24))
        .unwrap();
    assert!(matches!(
        desk.library
            .appeal(
                &late,
                library::Appeal {
                    appellant: library::Appellant::Reporter,
                    statement: "Too late?".into(),
                    contact: None,
                },
                after,
            )
            .await,
        Err(library::AppealError::TooLate)
    ));
}

#[tokio::test]
async fn the_decisions_log_gives_the_yearly_transparency_figures_without_personal_data() {
    let desk = desk().await;
    let now = desk.clock.now();
    let rights = website_report(&desk.library, VERBS, "rights").await;
    let first = desk.report(FRACTIONS, "other").await;
    let second = desk.report(FRACTIONS, "other").await;
    desk.report(VERBS, "wrong_answer").await;
    let decide_as = |moderator: &'static str, report: String, outcome| {
        let library = desk.library.clone();
        async move {
            library
                .decide_report(
                    moderator,
                    &report,
                    library::Decision {
                        outcome,
                        reasons: "Reasons.".into(),
                    },
                    now,
                )
                .await
                .unwrap()
        }
    };
    decide_as(ALICE, rights.clone(), library::Outcome::Upheld).await;
    decide_as(ALICE, first.clone(), library::Outcome::Rejected).await;
    decide_as(BRUNO, second, library::Outcome::Rejected).await;
    for (report, appellant, outcome) in [
        (
            &first,
            library::Appellant::Reporter,
            library::AppealOutcome::Maintained,
        ),
        (
            &rights,
            library::Appellant::Author,
            library::AppealOutcome::Reversed,
        ),
    ] {
        let appeal = desk
            .library
            .appeal(
                report,
                library::Appeal {
                    appellant,
                    statement: "Please look again.".into(),
                    contact: None,
                },
                now,
            )
            .await
            .unwrap();
        desk.library
            .decide_appeal(
                BRUNO,
                &appeal,
                library::AppealDecision {
                    outcome,
                    reasons: "Looked again.".into(),
                },
                now,
            )
            .await
            .unwrap();
    }
    let next_year = at("2027-01-02T10:00:00Z");
    let later = desk
        .library
        .report(
            library::NewReport {
                item: VERBS.into(),
                reason: library::Reason::Other,
                details: None,
                reporter: library::Reporter::Website {
                    contact: None,
                    notice: None,
                },
            },
            next_year,
        )
        .await
        .unwrap();
    desk.library
        .decide_report(
            BRUNO,
            &later,
            library::Decision {
                outcome: library::Outcome::Rejected,
                reasons: "Reasons.".into(),
            },
            next_year,
        )
        .await
        .unwrap();
    let alice = desk.moderator(ALICE).await;

    let answer = desk
        .ask("GET", "/api/moderation/figures/2026", &alice, None)
        .await;

    assert_eq!(answer.status, StatusCode::OK, "{}", text(&answer));
    assert_eq!(
        answer.json(),
        json!({
            "year": 2026,
            "reports": [
                {"reason": "other", "received": 2},
                {"reason": "rights", "received": 1},
                {"reason": "wrong_answer", "received": 1},
            ],
            "decisions": [
                {"reason": "other", "outcome": "rejected", "count": 2},
                {"reason": "rights", "outcome": "upheld", "count": 1},
            ],
            "appeals": [
                {"appellant": "author", "outcome": "reversed", "count": 1},
                {"appellant": "reporter", "outcome": "maintained", "count": 1},
            ],
        })
    );
    let figures = text(&answer);
    for personal in [
        rights.as_str(),
        "Alice",
        "Bruno",
        "reporter@example.org",
        VERBS,
    ] {
        assert!(!figures.contains(personal), "{personal} in the figures");
    }
    let bad = desk
        .ask("GET", "/api/moderation/figures/last", &alice, None)
        .await;
    assert_eq!(bad.status, StatusCode::BAD_REQUEST);
}
