//! Sign-in with a Minute Jar account and the library account opened on it
//! (ADR-0006, `docs/specs/accounts-api.md`).
//!
//! The service is an OpenID Connect client of the identity service, which it
//! reaches through [`IdentityProvider`]. A session is a random token the
//! service hands the website once a sign-in comes back; it is kept as its
//! SHA-256 only. Nothing here touches a library identifier: no table of
//! accounts has one, and the routes of family servers refuse a session.

pub(crate) mod http;

use std::future::Future;
use std::pin::Pin;

use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use jiff::{SignedDuration, Timestamp};
use sha2::{Digest, Sha256};
use sqlx::PgPool;

/// How long a started sign-in may take to come back.
pub const SIGN_IN_LIFETIME: SignedDuration = SignedDuration::from_mins(10);
/// How long a session lasts.
pub const SESSION_LIFETIME: SignedDuration = SignedDuration::from_hours(30 * 24);
/// How long a strong sign-in counts as recent, for opening a library account.
pub const RECENT: SignedDuration = SignedDuration::from_mins(15);

/// What the service asks the identity provider to authorize.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Authorization {
    /// Binds the provider's answer to this sign-in.
    pub state: String,
    /// Must come back in the ID token.
    pub nonce: String,
    /// The PKCE challenge (S256) of the verifier the service keeps.
    pub code_challenge: String,
    /// Whether the person must sign in again, strongly: a passkey, an
    /// external provider, or a password with a second factor.
    pub strong: bool,
}

/// The claims of a verified ID token that the service reads. The provider
/// adapter checks the signature, issuer, audience and expiry. There is no
/// field for a name: the service never asks for one, and drops it if given.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IdToken {
    /// `sub`: the Minute Jar account.
    pub subject: String,
    pub nonce: Option<String>,
    /// `acr`: `aal2` when a second factor was used.
    pub acr: Option<String>,
    /// `amr`: how the person signed in (`passkey`, `webauthn`, `oidc`,
    /// `password`, `totp`, `code`...).
    pub amr: Vec<String>,
    /// `auth_time`: when the person last signed in at the provider.
    pub auth_time: Option<Timestamp>,
    /// The e-mail address, only when `email_verified` is true.
    pub email: Option<String>,
}

impl IdToken {
    /// A passkey, an external provider, or a second factor (ADR-0006).
    pub fn is_strong(&self) -> bool {
        self.acr.as_deref() == Some("aal2")
            || self
                .amr
                .iter()
                .any(|method| matches!(method.as_str(), "passkey" | "webauthn" | "oidc"))
    }

    /// A passkey, which moderators must use (ADR-0006).
    pub fn is_passkey(&self) -> bool {
        self.amr
            .iter()
            .any(|method| matches!(method.as_str(), "passkey" | "webauthn"))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProviderError {
    /// The provider refused the code or the token did not verify.
    Refused,
    Unreachable,
}

pub type ProviderFuture<'a> =
    Pin<Box<dyn Future<Output = Result<IdToken, ProviderError>> + Send + 'a>>;

/// The identity service, as an OpenID Connect provider: Ory Hydra in
/// deployment (ADR-0006), a fake in tests.
pub trait IdentityProvider: Send + Sync + 'static {
    /// Where to send the person's browser to sign in, with the provider's
    /// redirect address set to the website's callback page.
    fn authorization_url(&self, authorization: &Authorization) -> String;

    /// Redeems an authorization code with its PKCE verifier; answers the
    /// verified ID token.
    fn redeem<'a>(&'a self, code: &'a str, code_verifier: &'a str) -> ProviderFuture<'a>;
}

/// The PKCE S256 challenge of a verifier.
pub fn code_challenge(code_verifier: &str) -> String {
    URL_SAFE_NO_PAD.encode(Sha256::digest(code_verifier.as_bytes()))
}

/// The accounts' part of the service, once turned on.
pub(crate) struct Desk {
    pub provider: Box<dyn IdentityProvider>,
    pub clock: Box<dyn Fn() -> Timestamp + Send + Sync>,
}

/// 32 random bytes in lowercase hexadecimal.
fn random_token() -> String {
    let mut bytes = [0u8; 32];
    getrandom::fill(&mut bytes).expect("the system's randomness");
    hex::encode(bytes)
}

fn digest(token: &str) -> String {
    hex::encode(Sha256::digest(token.as_bytes()))
}

#[derive(Debug)]
pub enum SignInError {
    /// Unknown or expired state, refused code, or a nonce that does not match.
    BadSignIn,
    Unavailable,
}

impl From<sqlx::Error> for SignInError {
    fn from(_: sqlx::Error) -> Self {
        SignInError::Unavailable
    }
}

/// Starts a sign-in: answers its state and where to send the browser.
pub(crate) async fn start(
    pool: &PgPool,
    desk: &Desk,
    strong: bool,
) -> Result<(String, String), sqlx::Error> {
    let now = (desk.clock)();
    let state = random_token();
    let nonce = random_token();
    let code_verifier = random_token();
    sqlx::query(
        "INSERT INTO sign_ins (state, nonce, code_verifier, started_at) VALUES ($1, $2, $3, $4)",
    )
    .bind(&state)
    .bind(&nonce)
    .bind(&code_verifier)
    .bind(sqlx_time(now))
    .execute(pool)
    .await?;
    let url = desk.provider.authorization_url(&Authorization {
        state: state.clone(),
        nonce,
        code_challenge: code_challenge(&code_verifier),
        strong,
    });
    Ok((state, url))
}

/// A session handed to the website.
pub(crate) struct NewSession {
    pub token: String,
    pub expires_at: Timestamp,
}

/// Finishes a sign-in that came back with `code`: opens a session.
pub(crate) async fn finish(
    pool: &PgPool,
    desk: &Desk,
    state: &str,
    code: &str,
) -> Result<NewSession, SignInError> {
    let now = (desk.clock)();
    let started: Option<(String, String, Timestamp)> =
        sqlx::query_as::<_, (String, String, SqlTime)>(
            "DELETE FROM sign_ins WHERE state = $1 RETURNING nonce, code_verifier, started_at",
        )
        .bind(state)
        .fetch_optional(pool)
        .await?
        .map(|(nonce, verifier, started_at)| (nonce, verifier, from_sqlx(started_at)));
    let Some((nonce, code_verifier, started_at)) = started else {
        return Err(SignInError::BadSignIn);
    };
    if now.duration_since(started_at) > SIGN_IN_LIFETIME {
        return Err(SignInError::BadSignIn);
    }
    let token = match desk.provider.redeem(code, &code_verifier).await {
        Ok(token) => token,
        Err(ProviderError::Refused) => return Err(SignInError::BadSignIn),
        Err(ProviderError::Unreachable) => return Err(SignInError::Unavailable),
    };
    if token.nonce.as_deref() != Some(nonce.as_str()) || token.subject.is_empty() {
        return Err(SignInError::BadSignIn);
    }
    let strong = token.is_strong();
    let passkey = token.is_passkey();
    let has_account = account_of(pool, &token.subject).await?.is_some();
    // The e-mail serves only to open a library account, which needs a
    // recent strong sign-in.
    let email = (strong && !has_account).then_some(token.email).flatten();
    let session = random_token();
    let expires_at = now.checked_add(SESSION_LIFETIME).expect("a session's end");
    sqlx::query(
        "INSERT INTO sessions (token_digest, subject, strong, passkey, auth_time, email, expires_at)
         VALUES ($1, $2, $3, $4, $5, $6, $7)",
    )
    .bind(digest(&session))
    .bind(&token.subject)
    .bind(strong)
    .bind(passkey)
    .bind(token.auth_time.map(sqlx_time))
    .bind(email)
    .bind(sqlx_time(expires_at))
    .execute(pool)
    .await?;
    Ok(NewSession {
        token: session,
        expires_at,
    })
}

/// A session, as a request carries it.
#[derive(Debug, Clone)]
pub(crate) struct Session {
    pub digest: String,
    pub subject: String,
    /// Until when the sign-in counts as recent and strong, if it was strong.
    pub strong_until: Option<Timestamp>,
    /// Whether the sign-in used a passkey.
    pub passkey: bool,
    pub email: Option<String>,
}

impl Session {
    pub fn recently_strong(&self, now: Timestamp) -> bool {
        self.strong_until.is_some_and(|until| now <= until)
    }

    /// A recent sign-in with a passkey, as moderating needs.
    pub fn recently_passkey(&self, now: Timestamp) -> bool {
        self.passkey && self.recently_strong(now)
    }
}

/// The session `token` names, if it has not expired.
pub(crate) async fn session(
    pool: &PgPool,
    token: &str,
    now: Timestamp,
) -> Result<Option<Session>, sqlx::Error> {
    type Row = (String, bool, bool, Option<SqlTime>, Option<String>);
    let row: Option<Row> = sqlx::query_as(
        "SELECT subject, strong, passkey, auth_time, email FROM sessions
         WHERE token_digest = $1 AND expires_at > $2",
    )
    .bind(digest(token))
    .bind(sqlx_time(now))
    .fetch_optional(pool)
    .await?;
    Ok(
        row.map(|(subject, strong, passkey, auth_time, email)| Session {
            digest: digest(token),
            subject,
            strong_until: auth_time.filter(|_| strong).map(|time| {
                from_sqlx(time)
                    .checked_add(RECENT)
                    .expect("a recent sign-in's end")
            }),
            passkey,
            email,
        }),
    )
}

/// Ends a session.
pub(crate) async fn end(pool: &PgPool, session: &Session) -> Result<(), sqlx::Error> {
    sqlx::query("DELETE FROM sessions WHERE token_digest = $1")
        .bind(&session.digest)
        .execute(pool)
        .await?;
    Ok(())
}

/// Forgets what is no longer needed at `now`: expired sessions, sign-ins
/// that never came back, and the e-mail of sign-ins no longer recent.
pub(crate) async fn forget(pool: &PgPool, now: Timestamp) -> Result<(), sqlx::Error> {
    let mut transaction = pool.begin().await?;
    sqlx::query("DELETE FROM sessions WHERE expires_at <= $1")
        .bind(sqlx_time(now))
        .execute(&mut *transaction)
        .await?;
    sqlx::query("DELETE FROM sign_ins WHERE started_at < $1")
        .bind(sqlx_time(
            now.checked_sub(SIGN_IN_LIFETIME).expect("a past instant"),
        ))
        .execute(&mut *transaction)
        .await?;
    sqlx::query(
        "UPDATE sessions SET email = NULL
         WHERE email IS NOT NULL AND (auth_time IS NULL OR auth_time < $1)",
    )
    .bind(sqlx_time(now.checked_sub(RECENT).expect("a past instant")))
    .execute(&mut *transaction)
    .await?;
    transaction.commit().await
}

/// Which Minute Jar account a library account uses, as its owner chose on
/// the choice screen.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MinuteJarAccount {
    /// The same as the subscription's: the pseudonym is joined to the
    /// subscription, which lets the account be linked later.
    SameAsSubscription,
    /// Another Minute Jar account than the subscription's, if any.
    Separate,
}

impl MinuteJarAccount {
    fn name(self) -> &'static str {
        match self {
            MinuteJarAccount::SameAsSubscription => "same_as_subscription",
            MinuteJarAccount::Separate => "separate",
        }
    }

    fn from_name(name: &str) -> Self {
        if name == "same_as_subscription" {
            MinuteJarAccount::SameAsSubscription
        } else {
            MinuteJarAccount::Separate
        }
    }
}

/// A library account, as its owner sees it.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct Account {
    pub pseudonym: String,
    pub minute_jar_account: MinuteJarAccount,
    pub opened_at: Timestamp,
}

/// The library account opened on the Minute Jar account `subject`, if any.
pub(crate) async fn account_of(
    pool: &PgPool,
    subject: &str,
) -> Result<Option<Account>, sqlx::Error> {
    let row: Option<(String, String, SqlTime)> = sqlx::query_as(
        "SELECT pseudonym, minute_jar_account, opened_at FROM library_accounts WHERE subject = $1",
    )
    .bind(subject)
    .fetch_optional(pool)
    .await?;
    Ok(row.map(|(pseudonym, choice, opened_at)| Account {
        pseudonym,
        minute_jar_account: MinuteJarAccount::from_name(&choice),
        opened_at: from_sqlx(opened_at),
    }))
}

type SqlTime = i64;

fn sqlx_time(time: Timestamp) -> SqlTime {
    time.as_second()
}

fn from_sqlx(time: SqlTime) -> Timestamp {
    Timestamp::from_second(time).expect("a stored instant")
}

/// The longest pseudonym, in characters.
pub const PSEUDONYM_LENGTH: usize = 32;

/// Pseudonyms no library account may take, compared without regard to case:
/// the editorial account's (ADR-0002).
const RESERVED: &[&str] = &["minute jar"];

#[derive(Debug)]
pub enum AccountError {
    /// The sign-in is not strong, or no longer recent.
    StrongSignInRequired,
    /// The adult and parent statement was not made.
    StatementRequired,
    /// Not 3 to 32 printable characters, or holding an `@`.
    BadPseudonym,
    PseudonymTaken,
    /// This Minute Jar account already has a library account.
    AccountExists,
    NoAccount,
    /// The identity provider gave no verified e-mail.
    VerifiedEmailRequired,
    Database(sqlx::Error),
}

impl From<sqlx::Error> for AccountError {
    fn from(error: sqlx::Error) -> Self {
        match &error {
            sqlx::Error::Database(database)
                if database.constraint() == Some("library_accounts_by_pseudonym") =>
            {
                AccountError::PseudonymTaken
            }
            _ => AccountError::Database(error),
        }
    }
}

/// The pseudonym as kept: trimmed, checked.
fn pseudonym(asked: &str) -> Result<String, AccountError> {
    let pseudonym = asked.trim();
    let length = pseudonym.chars().count();
    if !(3..=PSEUDONYM_LENGTH).contains(&length)
        || pseudonym.chars().any(|c| c.is_control() || c == '@')
    {
        return Err(AccountError::BadPseudonym);
    }
    if RESERVED.contains(&pseudonym.to_lowercase().as_str()) {
        return Err(AccountError::PseudonymTaken);
    }
    Ok(pseudonym.to_string())
}

/// What a person answers on the opening screen.
pub(crate) struct Opening<'a> {
    pub pseudonym: &'a str,
    pub adult_and_parent: bool,
    pub minute_jar_account: MinuteJarAccount,
}

/// Opens a library account on the session's Minute Jar account at `now`,
/// keeping its verified e-mail as identification data only.
pub(crate) async fn open(
    pool: &PgPool,
    session: &Session,
    opening: Opening<'_>,
    now: Timestamp,
) -> Result<Account, AccountError> {
    if !session.recently_strong(now) {
        return Err(AccountError::StrongSignInRequired);
    }
    if account_of(pool, &session.subject).await?.is_some() {
        return Err(AccountError::AccountExists);
    }
    if !opening.adult_and_parent {
        return Err(AccountError::StatementRequired);
    }
    let pseudonym = pseudonym(opening.pseudonym)?;
    let Some(email) = &session.email else {
        return Err(AccountError::VerifiedEmailRequired);
    };
    let mut transaction = pool.begin().await?;
    let (id,): (i64,) = sqlx::query_as(
        "INSERT INTO library_accounts
             (subject, pseudonym, minute_jar_account, adult_and_parent_at, opened_at)
         VALUES ($1, $2, $3, $4, $4)
         ON CONFLICT (subject) DO NOTHING
         RETURNING id",
    )
    .bind(&session.subject)
    .bind(&pseudonym)
    .bind(opening.minute_jar_account.name())
    .bind(sqlx_time(now))
    .fetch_optional(&mut *transaction)
    .await?
    .ok_or(AccountError::AccountExists)?;
    sqlx::query("INSERT INTO identifications (account, email, recorded_at) VALUES ($1, $2, $3)")
        .bind(id)
        .bind(email)
        .bind(sqlx_time(now))
        .execute(&mut *transaction)
        .await?;
    // The e-mail has served: no session keeps it any longer.
    sqlx::query("UPDATE sessions SET email = NULL WHERE subject = $1")
        .bind(&session.subject)
        .execute(&mut *transaction)
        .await?;
    transaction.commit().await?;
    Ok(Account {
        pseudonym,
        minute_jar_account: opening.minute_jar_account,
        opened_at: now,
    })
}

/// Changes the pseudonym of the session's library account.
pub(crate) async fn rename(
    pool: &PgPool,
    session: &Session,
    asked: &str,
) -> Result<Account, AccountError> {
    let pseudonym = pseudonym(asked)?;
    let changed = sqlx::query("UPDATE library_accounts SET pseudonym = $2 WHERE subject = $1")
        .bind(&session.subject)
        .bind(&pseudonym)
        .execute(pool)
        .await?;
    if changed.rows_affected() == 0 {
        return Err(AccountError::NoAccount);
    }
    account_of(pool, &session.subject)
        .await?
        .ok_or(AccountError::NoAccount)
}
