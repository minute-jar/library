-- Every shared item or version withdrawn from the catalogue, at most one
-- withdrawal each; version 0 withdraws every version of the item.
CREATE TABLE withdrawals (
    item TEXT NOT NULL,
    version INTEGER NOT NULL,
    kind TEXT NOT NULL,
    reason TEXT NOT NULL,
    withdrawn_at BIGINT NOT NULL,
    PRIMARY KEY (item, version)
);
