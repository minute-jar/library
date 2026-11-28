-- Author profiles (issue #23, docs/specs/catalogue-api.md, Author
-- profiles).
--
-- Whether the account's public profile shows the items it published under
-- its earlier pseudonyms, and their pages lead to its profile of now. Off
-- unless its owner chooses it: a new pseudonym leaves the old one behind.
ALTER TABLE library_accounts
    ADD COLUMN show_earlier_pseudonyms BOOLEAN NOT NULL DEFAULT FALSE;
