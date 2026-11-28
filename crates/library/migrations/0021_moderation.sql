-- Moderators, appeals and the decisions log (docs/specs/moderation-api.md).

-- Whether a session's sign-in used a passkey (`amr` holding `passkey` or
-- `webauthn`): moderators must use one (ADR-0006).
ALTER TABLE sessions ADD COLUMN passkey BOOLEAN NOT NULL DEFAULT FALSE;

-- The moderators the operator named, by their Minute Jar account. `handle`
-- is how decisions name the moderator to other moderators; no answer
-- carries the subject.
CREATE TABLE moderators (
    subject TEXT PRIMARY KEY,
    handle TEXT NOT NULL,
    granted_at BIGINT NOT NULL
);

CREATE UNIQUE INDEX moderators_by_handle ON moderators (lower(handle));

-- An appeal against a moderator's decision on a report, by its author or
-- its reporter. A website reporter's contact is kept only until the
-- appeal's decision is sent.
CREATE TABLE appeals (
    id TEXT PRIMARY KEY,
    report TEXT NOT NULL REFERENCES reports (id),
    -- author or reporter.
    appellant TEXT NOT NULL,
    statement TEXT NOT NULL,
    contact TEXT,
    -- waiting, maintained or reversed.
    status TEXT NOT NULL,
    received_at BIGINT NOT NULL,
    UNIQUE (report, appellant)
);

-- The decisions log: every moderator's decision, on a report or on an
-- appeal, with its reasons. The transparency report counts it by reason and
-- outcome. `case_kind` is `report` (later also `held_review`, issue #13);
-- `appeal` names the appeal an `appeal` decision answers.
CREATE TABLE moderation_decisions (
    id BIGSERIAL PRIMARY KEY,
    case_kind TEXT NOT NULL,
    case_id TEXT NOT NULL,
    appeal TEXT REFERENCES appeals (id),
    item TEXT NOT NULL,
    reason TEXT NOT NULL,
    -- upheld or rejected for a report; maintained or reversed for an appeal.
    outcome TEXT NOT NULL,
    moderator TEXT NOT NULL,
    reasons TEXT NOT NULL,
    decided_at BIGINT NOT NULL
);

CREATE INDEX moderation_decisions_by_item ON moderation_decisions (item);
CREATE INDEX moderation_decisions_by_time ON moderation_decisions (decided_at);

-- An author's notification about an appeal's decision names the appeal.
ALTER TABLE author_notifications ADD COLUMN appeal TEXT REFERENCES appeals (id);
