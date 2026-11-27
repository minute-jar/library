-- The answers to accept that each library identifier suggested for a
-- question of a shared item it imported, at the version it imported, once
-- each. As with imports and ratings, nothing else is kept with an
-- identifier: no time, no address, and no column or table joins it to a
-- Minute Jar account or a library account. The author reads suggestions by
-- item only, counted by distinct identifiers, never the identifiers.
CREATE TABLE suggestions (
    library_identifier TEXT NOT NULL,
    item TEXT NOT NULL,
    version INTEGER NOT NULL,
    question INTEGER NOT NULL CHECK (question >= 1),
    answer TEXT NOT NULL,
    PRIMARY KEY (library_identifier, item, version, question, answer)
);

CREATE INDEX suggestions_by_item ON suggestions (item);
