-- Comments from library accounts (ticket 21, docs/specs/catalogue-api.md,
-- Comments): every comment is reviewed before it shows, and can be
-- reported. A comment is addressed to an item, never to an account: no
-- table here joins one account to another.

-- Every comment and its outcome. `outcome` is 'reviewing', 'published',
-- 'held' (for a moderator), 'deciding' (a moderator is deciding it),
-- 'refused' or 'removed' (by a moderator upholding a report). `pseudonym`
-- is the account's at publication. `reasons` is a refusal's reasons, a JSON
-- array; `findings` what the review found for a moderator. `hidden` is set
-- while a report of a child's personal data, sexual or illegal content
-- waits for a moderator.
CREATE TABLE comments (
    id TEXT PRIMARY KEY,
    item TEXT NOT NULL,
    account BIGINT NOT NULL REFERENCES library_accounts (id),
    text TEXT NOT NULL,
    outcome TEXT NOT NULL,
    pseudonym TEXT,
    reasons JSONB NOT NULL DEFAULT '[]',
    findings TEXT,
    hidden BOOLEAN NOT NULL DEFAULT FALSE,
    commented_at BIGINT NOT NULL,
    published_at BIGINT,
    decided_at BIGINT
);

CREATE INDEX comments_by_item ON comments (item, published_at);
CREATE INDEX comments_by_account ON comments (account, commented_at);

-- Reports of published comments, from the website, anonymous: no library
-- identifier, no contact. `status` is 'with_moderator', 'deciding',
-- 'upheld' or 'rejected'.
CREATE TABLE comment_reports (
    id TEXT PRIMARY KEY,
    comment TEXT NOT NULL REFERENCES comments (id),
    reason TEXT NOT NULL,
    details TEXT,
    status TEXT NOT NULL,
    received_at BIGINT NOT NULL,
    decided_at BIGINT
);

CREATE INDEX comment_reports_by_comment ON comment_reports (comment);
