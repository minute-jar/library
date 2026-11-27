---
title: Accounts API
status: draft
---

# Accounts API

The JSON API through which the website signs a person in with their Minute Jar account and opens their library account (ADR-0006). Like the catalogue API (`catalogue-api.md`), it is the service's own API, read by its website's server, and may change with it; the browser never calls it. Code: `crates/library/src/accounts/`.

## Sign-in

The library is an OpenID Connect client of the identity service (Ory Hydra in deployment, behind the `IdentityProvider` interface; a fake in tests). It uses the authorization code flow with PKCE (S256), a `state` and a `nonce`, and asks only for `openid` and the verified e-mail: never a name or a picture.

1. `POST /api/sign-in` with `{"strong": true|false}` (an empty body means `false`) answers `200`:

   ```json
   {"state": "<64 hex>", "authorization_url": "https://.../oauth2/auth?..."}
   ```

   The website keeps `state` in a cookie of the browser and sends the browser to `authorization_url`. `strong` asks the provider for a new strong sign-in (`max_age=0`, a passkey, an external provider or a second factor). A sign-in must come back within 10 minutes.

2. The provider sends the browser back to the website's callback page with `code` and `state`. The website checks `state` against its cookie, then `POST /api/sign-in/callback` with `{"state", "code"}`. The service redeems the code with its PKCE verifier, checks the ID token's `nonce`, and answers `200`:

   ```json
   {"session": "<64 hex>", "expires_at": "2026-12-27T10:00:00Z"}
   ```

   A state unknown, already used or older than 10 minutes, a refused code or a `nonce` that does not match answer `400` `{"error": "bad_sign_in"}`; an unreachable provider `503` `{"error": "unavailable"}`.

The session lasts 30 days. It travels as `Authorization: Bearer <session>` on every request below; the service keeps only its SHA-256. A request without a valid session answers `401` `{"error": "signed_out"}`.

### Strong and recent

The service reads `acr`, `amr` and `auth_time` from the ID token. A sign-in is **strong** when `acr` is `aal2` (a second factor) or `amr` holds `passkey`, `webauthn` or `oidc` (Google, Apple, Microsoft). An e-mail code or a password alone is not strong. A strong sign-in is **recent** for 15 minutes after `auth_time`. Opening and closing a library account, and verifying its card, need a recent strong sign-in; sharing (ticket 12) and linking a subscription will too.

## Session

- `GET /api/session` answers `200` `{"strong_sign_in_until": "<time>" | null, "account": <account> | null}`; `strong_sign_in_until` is set while the sign-in is strong and recent.
- `DELETE /api/session` signs out: `204`.

## Library account

An account, as its owner sees it:

```json
{"pseudonym": "Owl", "minute_jar_account": "separate", "opened_at": "2026-11-27T10:00:00Z"}
```

`minute_jar_account` is the choice made on the opening screen: `same_as_subscription` (one Minute Jar account for the library and hosted mode: the operator can join the pseudonym to the subscription, which lets the library account be linked to it) or `separate` (the person signs in to the library with another Minute Jar account, another e-mail address).

- `POST /api/account` with `{"pseudonym", "adult_and_parent": true, "minute_jar_account"}` opens the library account: `201` with the account. Refusals, in this order:
  - `403` `strong_sign_in_required`: the sign-in is not strong, or older than 15 minutes; sign in again with `strong: true`;
  - `409` `account_exists`: this Minute Jar account already has one;
  - `400` `statement_required`: `adult_and_parent` is not `true` (the person states they are an adult and a parent);
  - `400` `bad_pseudonym`: not 3 to 32 characters once trimmed, a control character, or an `@`;
  - `409` `pseudonym_taken`: another account has it, without regard to case, or it is reserved (`Minute Jar`, the editorial account's);
  - `403` `verified_email_required`: the provider gave no verified e-mail.
- `GET /api/account` answers the account, or `404` `no_account`.
- `PUT /api/account/pseudonym` with `{"pseudonym"}` changes it: `200` with the account; `bad_pseudonym`, `pseudonym_taken`, `no_account` as above.

### Closing

`DELETE /api/account` closes the library account (story S12): `204`. Refusals, in this order: `403` `strong_sign_in_required` (as for opening), `404` `no_account`.

- Every session of the Minute Jar account ends at once, on every device.
- A card check it left unfinished ends at once (table `card_checks`).
- The items it shared stay published under their licence, with its pseudonym: their signed versions do not change.
- Its pseudonym stays reserved: no account may take it again, without regard to case.
- The library account is no longer joined to the Minute Jar account, whose next sign-in finds no account and may open another, under another pseudonym.
- Its identification data, its identification and its verified card (`identification_cards`), is kept for the legal period after its last share, then erased by `Library::erase_identifications(now, kept_for)`, which `main.rs` calls every hour; `kept_for` is `IDENTIFICATION_RETENTION` (one year, ticket 26) unless `LIBRARY_IDENTIFICATION_DAYS` sets it. An account that never shared has nothing to answer for: its identification data is erased at the next call. Sharing (ticket 12) records the last share in `identifications.last_shared_at`.

## Card

A library account needs a verified payment card before its first share (ADR-0001), verified through the subscription's payment provider (ADR-0007, behind the `CardVerifier` and `SubscriptionCards` interfaces; fakes in tests). The card number never reaches the library: it keeps the provider's reference and the card's fingerprint, as identification data (table `identification_cards`, beside `identifications`), answered by no route. The same card may serve several library accounts; the operator sees which (`Library::accounts_sharing_card`).

A card, as its owner sees it:

```json
{"verified_by": "subscription", "verified_at": "2026-11-27T10:00:00Z"}
```

`verified_by` is `subscription` (the card of the subscription of the same Minute Jar account, story S7) or `check` (a zero-amount check with 3-D Secure, story S8).

- `POST /api/account/card` (no body) verifies the card. When the account is on `same_as_subscription` and that Minute Jar account pays a subscription with a card, that card is used: `200` `{"card": <card>}`. Otherwise a zero-amount check starts: `202` `{"check_url": "https://..."}`; the website sends the browser there, and the provider sends it back to the website's card page (`/account/card`). Starting again replaces a check left unfinished. Refusals, in this order:
  - `403` `strong_sign_in_required`: as for opening;
  - `404` `no_account`;
  - `409` `card_verified`: the account's card is already verified.
- `POST /api/account/card/check` (no body), once the browser is back, reads the check's result: `200` `{"card": <card>}` (also when the card was already verified). Refusals: `404` `no_card_check` (none started, or the last one was refused); `409` `card_check_pending` (not finished at the provider yet); `400` `card_refused` (the bank or the person refused it; it can be started again).
- `GET /api/account/card` answers the card, or `404` `no_card` (or `no_account`).

Once the account is closed, its sessions are ended (`401` `signed_out`) and a new sign-in finds no account: the card routes answer `404` `no_account`, as the other account routes do.

Until the service is given a card verifier (`Library::with_cards`), these routes answer `503` `{"error": "unavailable"}`, as when the provider cannot be reached.

A body that is not exactly the expected object (an unknown field, a library identifier among them) answers `400` `{"error": "bad_request"}`.

## What is kept and what is answered

- No answer carries a name, an e-mail address or the Minute Jar account's subject (`tests/account_privacy.rs` scans every route).
- A name is never received. The verified e-mail of a strong sign-in is kept with the session only while the sign-in is recent and the Minute Jar account has no library account; on opening, it moves to the account's identification data (table `identifications`), kept while the account is open, then for the legal period after its last share (see Closing), and answered by no route. `Library::forget_sign_ins(now)`, called every five minutes, removes expired sessions, sign-ins that never came back, and the e-mail of sign-ins no longer recent.
- A session is never joined to a library identifier: no table of accounts has one, account routes refuse one, and a family server's call carrying a session is refused (`interface-v1.md`, Library identifier).

## HTTP

- Every answer is `application/json` with `Cache-Control: private, no-store`.
- Until the service is given an identity provider (`Library::with_accounts`), every route here answers `503` `{"error": "unavailable"}`; a database failure too.
