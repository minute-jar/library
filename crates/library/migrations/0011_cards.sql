-- The verified card of a library account (ADR-0001, ADR-0007).
--
-- A card number is never received: the payment provider keeps it. The
-- library keeps the provider's reference and the card's fingerprint.

-- A zero-amount check started at the provider and not yet come back; one per
-- account, the last one started.
CREATE TABLE card_checks (
    account BIGINT PRIMARY KEY REFERENCES library_accounts (id),
    reference TEXT NOT NULL,
    started_at BIGINT NOT NULL
);

-- The verified card of a library account: identification data, alongside
-- `identifications`, answered by no route and kept for the same period
-- (ticket 26). `verified_by` is 'subscription' (the card of the subscription
-- of the same Minute Jar account) or 'check' (a zero-amount check). The same
-- card may serve several accounts.
CREATE TABLE identification_cards (
    account BIGINT PRIMARY KEY REFERENCES library_accounts (id),
    reference TEXT NOT NULL,
    fingerprint TEXT NOT NULL,
    verified_by TEXT NOT NULL,
    verified_at BIGINT NOT NULL
);

CREATE INDEX identification_cards_by_fingerprint ON identification_cards (fingerprint);
