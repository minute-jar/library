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

The service reads `acr`, `amr` and `auth_time` from the ID token. A sign-in is **strong** when `acr` is `aal2` (a second factor) or `amr` holds `passkey`, `webauthn` or `oidc` (Google, Apple, Microsoft). An e-mail code or a password alone is not strong. A strong sign-in is **recent** for 15 minutes after `auth_time`. Opening a library account needs a recent strong sign-in; sharing (ticket 12) and linking a subscription will too.

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

A body that is not exactly the expected object (an unknown field, a library identifier among them) answers `400` `{"error": "bad_request"}`.

## What is kept and what is answered

- No answer carries a name, an e-mail address or the Minute Jar account's subject (`tests/account_privacy.rs` scans every route).
- A name is never received. The verified e-mail of a strong sign-in is kept with the session only while the sign-in is recent and the Minute Jar account has no library account; on opening, it moves to the account's identification data (table `identifications`), kept for the legal period (ticket 26) and answered by no route. `Library::forget_sign_ins(now)`, called every five minutes, removes expired sessions, sign-ins that never came back, and the e-mail of sign-ins no longer recent.
- A session is never joined to a library identifier: no table of accounts has one, account routes refuse one, and a family server's call carrying a session is refused (`interface-v1.md`, Library identifier).

## HTTP

- Every answer is `application/json` with `Cache-Control: private, no-store`.
- Until the service is given an identity provider (`Library::with_accounts`), every route here answers `503` `{"error": "unavailable"}`; a database failure too.
