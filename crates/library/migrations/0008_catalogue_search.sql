-- Full-text search of the catalogue: the title, description, subject and
-- curriculum topic of a version, stemmed in its language (French for `fr`
-- tags, English otherwise). A function rather than a generated column, so
-- that the expression index works on every supported PostgreSQL.
CREATE FUNCTION catalogue_search(title TEXT, metadata JSONB) RETURNS tsvector
LANGUAGE sql IMMUTABLE PARALLEL SAFE AS $$
    SELECT to_tsvector(
        CASE WHEN lower(metadata->>'language') LIKE 'fr%' THEN 'french'::regconfig ELSE 'english'::regconfig END,
        title || ' ' || coalesce(metadata->>'description', '') || ' ' || coalesce(metadata->>'subject', '')
            || ' ' || coalesce(metadata#>>'{curriculum,topic}', '')
    )
$$;

CREATE INDEX item_versions_search ON item_versions USING gin (catalogue_search(title, metadata));
