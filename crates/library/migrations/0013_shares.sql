-- Sharing from a library account (ticket 12, docs/specs/interface-v1.md,
-- Shares): the sharing terms, the author of every shared item, and the
-- sharing log.
--
-- No table here has a library identifier column: a share carries none.

-- The version of the sharing terms an account accepted, and when.
CREATE TABLE sharing_terms (
    account BIGINT PRIMARY KEY REFERENCES library_accounts (id),
    version TEXT NOT NULL,
    accepted_at BIGINT NOT NULL
);

-- The library account that answers for a shared item: the join through
-- which the author hears of reports and decisions, and appeals them.
CREATE TABLE authored_items (
    item TEXT PRIMARY KEY,
    account BIGINT NOT NULL REFERENCES library_accounts (id)
);

CREATE INDEX authored_items_by_account ON authored_items (account);

-- Every share and its outcome: the sharing log, identification data kept
-- with the account's (ticket 26). `outcome` is 'published', 'held' (for a
-- moderator) or 'refused'. `item` is drawn for a published or held share;
-- `version` is set once published. `reasons` is the refusal's reasons per
-- question, a JSON array; `findings` what the review found for a moderator.
-- `submission` keeps a held share's prepared item until a moderator
-- decides, and is erased then.
CREATE TABLE shares (
    id TEXT PRIMARY KEY,
    account BIGINT NOT NULL REFERENCES library_accounts (id),
    item TEXT,
    title TEXT NOT NULL,
    outcome TEXT NOT NULL,
    version INTEGER,
    reasons JSONB NOT NULL DEFAULT '[]',
    findings TEXT,
    submission JSONB,
    shared_at BIGINT NOT NULL,
    decided_at BIGINT
);

CREATE INDEX shares_by_account ON shares (account, shared_at);
