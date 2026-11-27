-- The shared items each library identifier reported importing, once each.
-- Nothing else is kept with an identifier: no time, no version, no address,
-- and no column or table joins it to a Minute Jar account or a library
-- account (ADR-0027 of the main repository).
CREATE TABLE imports (
    library_identifier TEXT NOT NULL,
    item TEXT NOT NULL,
    PRIMARY KEY (library_identifier, item)
);

CREATE INDEX imports_by_item ON imports (item);
