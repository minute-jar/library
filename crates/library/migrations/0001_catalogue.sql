-- Every published version of every shared item.
CREATE TABLE item_versions (
    item TEXT NOT NULL,
    version INTEGER NOT NULL,
    type TEXT NOT NULL,
    title TEXT NOT NULL,
    author_pseudonym TEXT NOT NULL,
    author_editorial BOOLEAN NOT NULL,
    derived_from JSONB,
    metadata JSONB NOT NULL,
    content JSONB NOT NULL,
    published_at BIGINT NOT NULL,
    PRIMARY KEY (item, version)
);

-- The signed documents as served, byte for byte, by path.
CREATE TABLE signed_documents (
    path TEXT PRIMARY KEY,
    body TEXT NOT NULL,
    etag TEXT NOT NULL
);
