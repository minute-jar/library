-- Closing a library account (story S12, docs/specs/accounts-api.md).
--
-- A closed account loses its Minute Jar account: `subject` is erased, so
-- that no sign-in finds it again, and `closed_at` set. Its row stays, so
-- that its pseudonym stays reserved and its items keep their author.
ALTER TABLE library_accounts ALTER COLUMN subject DROP NOT NULL;
ALTER TABLE library_accounts ADD COLUMN closed_at BIGINT;

-- When the account last shared an item (ticket 12 records it). The
-- identification data of a closed account is kept for the legal period
-- after it (ticket 26), and erased at once if it never shared.
ALTER TABLE identifications ADD COLUMN last_shared_at BIGINT;
