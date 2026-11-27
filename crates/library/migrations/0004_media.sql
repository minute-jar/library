-- Every published picture or sound, by the SHA-256 digest of its bytes,
-- which the media store keeps under that digest.
CREATE TABLE media (
    digest TEXT PRIMARY KEY,
    type TEXT NOT NULL,
    size BIGINT NOT NULL
);
