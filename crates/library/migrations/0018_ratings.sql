-- One rating per library identifier and shared item, with its short review
-- if the family left one. As with imports, nothing else is kept with an
-- identifier: no time, no version, no address, and no column or table joins
-- it to a Minute Jar account or a library account.
--
-- A review is public text from an anonymous family: it is held `pending`
-- until the review decides it (`published` or `refused`), and only a
-- published review is shown.
CREATE TABLE ratings (
    library_identifier TEXT NOT NULL,
    item TEXT NOT NULL,
    stars SMALLINT NOT NULL CHECK (stars BETWEEN 1 AND 5),
    review TEXT,
    review_state TEXT CHECK (review_state IN ('pending', 'published', 'refused')),
    PRIMARY KEY (library_identifier, item),
    CHECK ((review IS NULL) = (review_state IS NULL))
);

CREATE INDEX ratings_by_item ON ratings (item);
