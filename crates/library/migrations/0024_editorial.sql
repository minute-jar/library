-- The editorial account and the Selection mark (issue #24, ADR-0002).

-- The library account the operator marked as its editorial account, under
-- which it shares the content it makes itself, shown as the software's
-- publisher. At most one.
CREATE TABLE editorial_account (
    account BIGINT PRIMARY KEY REFERENCES library_accounts (id),
    marked_at BIGINT NOT NULL
);

CREATE UNIQUE INDEX editorial_account_is_one ON editorial_account ((true));

-- How a version shared by the editorial account was made and read before
-- its publication: `reader` is the moderator's handle, or NULL when the
-- operator read it.
CREATE TABLE editorial_readings (
    item TEXT NOT NULL,
    version INTEGER NOT NULL,
    ai_generated BOOLEAN NOT NULL,
    reader TEXT,
    read_at BIGINT NOT NULL,
    PRIMARY KEY (item, version)
);

-- The items the operator's team marked Selection, by which moderator
-- (handle) and when.
CREATE TABLE selections (
    item TEXT PRIMARY KEY,
    moderator TEXT NOT NULL,
    selected_at BIGINT NOT NULL
);
