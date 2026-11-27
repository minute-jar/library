-- Sign-in with a Minute Jar account and the library accounts opened on them
-- (docs/specs/accounts-api.md, ADR-0006).
--
-- No table here has a library identifier column, and no table holding a
-- library identifier has an account column: the two are never joined.
-- A real name is never received. The e-mail is kept only where the
-- identification of an account needs it.

-- Instants are seconds since the Unix epoch, as elsewhere.

-- A sign-in started and not yet come back from the identity provider.
CREATE TABLE sign_ins (
    state TEXT PRIMARY KEY,
    nonce TEXT NOT NULL,
    code_verifier TEXT NOT NULL,
    started_at BIGINT NOT NULL
);

-- A session, known by the SHA-256 of its token only. `email` is the
-- verified e-mail of a strong sign-in of a Minute Jar account that has no
-- library account yet, kept while the sign-in is recent, so that the account
-- can be opened; it is erased when the account is opened or the sign-in is
-- no longer recent.
CREATE TABLE sessions (
    token_digest TEXT PRIMARY KEY,
    subject TEXT NOT NULL,
    strong BOOLEAN NOT NULL,
    auth_time BIGINT,
    email TEXT,
    expires_at BIGINT NOT NULL
);

CREATE INDEX sessions_by_expiry ON sessions (expires_at);

-- A library account, on the Minute Jar account `subject`.
CREATE TABLE library_accounts (
    id BIGSERIAL PRIMARY KEY,
    subject TEXT NOT NULL UNIQUE,
    pseudonym TEXT NOT NULL,
    -- `minute_jar_account`: 'same_as_subscription' or 'separate'.
    minute_jar_account TEXT NOT NULL,
    adult_and_parent_at BIGINT NOT NULL,
    opened_at BIGINT NOT NULL
);

-- Pseudonyms are unique without regard to case.
CREATE UNIQUE INDEX library_accounts_by_pseudonym ON library_accounts (lower(pseudonym));

-- The identification data of a library account, which no route answers,
-- kept for the period the law asks of hosting providers (ticket 26).
CREATE TABLE identifications (
    account BIGINT PRIMARY KEY REFERENCES library_accounts (id),
    email TEXT NOT NULL,
    recorded_at BIGINT NOT NULL
);
