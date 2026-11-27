//! A stand-in for the payment provider (ADR-0007): zero-amount checks whose
//! outcome a test decides, and the cards of subscriptions.

#![allow(dead_code)]

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use library::{
    CardError, CardFuture, CardVerifier, CheckResult, Library, StartedCheck, SubscriptionCards,
    VerifiedCard,
};
use serde_json::Value;

use super::Answer;
use super::identity::*;

#[derive(Default)]
struct State {
    next: u32,
    checks: HashMap<String, CheckResult>,
    subscriptions: HashMap<String, VerifiedCard>,
    unreachable: bool,
}

/// The provider, as both a card verifier and the subscriptions' cards.
#[derive(Clone, Default)]
pub struct FakePayments {
    state: Arc<Mutex<State>>,
}

impl FakePayments {
    /// The checks started, in order.
    pub fn started(&self) -> Vec<String> {
        let state = self.state.lock().unwrap();
        (1..=state.next).map(|n| format!("seti_{n}")).collect()
    }

    /// The person finishes the check `check` with the card `fingerprint`.
    pub fn verify(&self, check: &str, fingerprint: &str) {
        self.state.lock().unwrap().checks.insert(
            check.into(),
            CheckResult::Verified(VerifiedCard {
                reference: format!("pm_of_{check}"),
                fingerprint: fingerprint.into(),
            }),
        );
    }

    /// The bank refuses the check `check`.
    pub fn refuse(&self, check: &str) {
        self.state
            .lock()
            .unwrap()
            .checks
            .insert(check.into(), CheckResult::Refused);
    }

    /// The Minute Jar account `subject` pays a subscription with the card `fingerprint`.
    pub fn subscribe(&self, subject: &str, fingerprint: &str) {
        self.state.lock().unwrap().subscriptions.insert(
            subject.into(),
            VerifiedCard {
                reference: format!("sub_of_{fingerprint}"),
                fingerprint: fingerprint.into(),
            },
        );
    }

    pub fn set_unreachable(&self) {
        self.state.lock().unwrap().unreachable = true;
    }

    fn reachable(&self) -> Result<std::sync::MutexGuard<'_, State>, CardError> {
        let state = self.state.lock().unwrap();
        if state.unreachable {
            Err(CardError::Unreachable)
        } else {
            Ok(state)
        }
    }
}

impl CardVerifier for FakePayments {
    fn start(&self) -> CardFuture<'_, StartedCheck> {
        Box::pin(async move {
            let mut state = self.reachable()?;
            state.next += 1;
            let check = format!("seti_{}", state.next);
            state.checks.insert(check.clone(), CheckResult::Pending);
            Ok(StartedCheck {
                url: format!("https://pay.example/setup/{check}"),
                check,
            })
        })
    }

    fn result<'a>(&'a self, check: &'a str) -> CardFuture<'a, CheckResult> {
        Box::pin(async move {
            let state = self.reachable()?;
            Ok(state
                .checks
                .get(check)
                .cloned()
                .unwrap_or(CheckResult::Refused))
        })
    }
}

impl SubscriptionCards for FakePayments {
    fn card_of<'a>(&'a self, subject: &'a str) -> CardFuture<'a, Option<VerifiedCard>> {
        Box::pin(async move { Ok(self.reachable()?.subscriptions.get(subject).cloned()) })
    }
}

/// The parts of a card the library keeps that must never be answered.
pub const FINGERPRINT: &str = "fp_card_of_parent_1";

/// A library with accounts and cards, over fakes.
pub async fn with_cards() -> (Library, FakeProvider, Clock, FakePayments, String) {
    let (library, provider, clock, database) = with_accounts_and_database().await;
    let payments = FakePayments::default();
    let library = library.with_cards(payments.clone(), payments.clone());
    (library, provider, clock, payments, database)
}

/// A strong session on `subject` with a library account under `pseudonym`
/// and the choice `minute_jar_account`.
pub async fn account(
    library: &Library,
    provider: &FakeProvider,
    clock: &Clock,
    subject: &str,
    pseudonym: &str,
    minute_jar_account: &str,
) -> String {
    let session = session(library, provider, passkey(subject, clock.now())).await;
    let opened = with_session(
        library,
        "POST",
        "/api/account",
        &session,
        Some(serde_json::json!({
            "pseudonym": pseudonym,
            "adult_and_parent": true,
            "minute_jar_account": minute_jar_account,
        })),
    )
    .await;
    assert_eq!(opened.status, axum::http::StatusCode::CREATED);
    session
}

/// Asks for the card to be verified, as the website does.
pub async fn verify_card(library: &Library, session: &str) -> Answer {
    with_session(library, "POST", "/api/account/card", session, None).await
}

/// Reads the check's result once the browser is back, as the website does.
pub async fn finish_check(library: &Library, session: &str) -> Answer {
    with_session(library, "POST", "/api/account/card/check", session, None).await
}

pub async fn card(library: &Library, session: &str) -> Answer {
    with_session(library, "GET", "/api/account/card", session, None).await
}

/// The card as its owner reads it.
pub async fn card_answer(library: &Library, session: &str) -> Value {
    let answer = card(library, session).await;
    assert_eq!(
        answer.status,
        axum::http::StatusCode::OK,
        "{}",
        text(&answer)
    );
    answer.json()
}

pub fn text(answer: &Answer) -> String {
    String::from_utf8_lossy(&answer.body).into_owned()
}
