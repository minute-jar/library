-- Reports of shared items. A report keeps no library identifier: a family
-- server's report is known by its random identifier alone. A website
-- reporter's contact address is kept only until the decision is sent.
CREATE TABLE reports (
    id TEXT PRIMARY KEY,
    item TEXT NOT NULL,
    -- The item's latest version when the report arrived.
    version INTEGER NOT NULL,
    reason TEXT NOT NULL,
    details TEXT,
    contact TEXT,
    -- A rights holder's formal notice: who they are and the work they hold.
    notice_name TEXT,
    notice_work TEXT,
    -- with_author, with_moderator, corrected, upheld or rejected.
    status TEXT NOT NULL,
    -- What the report did at once: none, switched_off or hidden.
    effect TEXT NOT NULL,
    findings TEXT,
    received_at BIGINT NOT NULL,
    decided_at BIGINT,
    decision_reasons TEXT
);

CREATE INDEX reports_by_item ON reports (item);

-- The library identifiers that reported each item for each reason, once
-- each, to count distinct families. Like imports, nothing else is kept with
-- an identifier: no time, no report, no account.
CREATE TABLE report_identifiers (
    item TEXT NOT NULL,
    reason TEXT NOT NULL,
    library_identifier TEXT NOT NULL,
    PRIMARY KEY (item, reason, library_identifier)
);

-- Shared items hidden from new imports: out of the index, still served to
-- the families that have them. The cause is urgent or rights.
CREATE TABLE hidden_items (
    item TEXT NOT NULL,
    cause TEXT NOT NULL,
    PRIMARY KEY (item, cause)
);

-- What the author of an item must be told, until their library account
-- (ticket 10) receives it: a report to answer, or a decision.
CREATE TABLE author_notifications (
    id BIGSERIAL PRIMARY KEY,
    item TEXT NOT NULL,
    report TEXT NOT NULL,
    kind TEXT NOT NULL,
    created_at BIGINT NOT NULL
);
