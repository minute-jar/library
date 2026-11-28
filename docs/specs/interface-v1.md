---
title: Interface between a family server and a library, version 1
status: draft
---

# Interface between a family server and a library, version 1

The wire format of the signed documents a library serves to family servers (ADR-0004). It makes concrete the interface contract of the main repository (`docs/specs/public-library.md`, "Interface between a family server and a library") and is proposed there as its version 1. The fixtures in `fixtures/v1/` are signed with public test keys and let a family server's library client and a library check each other.

This version covers the keys document, the index, shared items and their media, item files for family servers without Internet access, the withdrawal list, imports, ratings, reports and shares. Suggestions are added by a later ticket of `docs/tickets/step-1.md`.

## Signed documents

Every signed document is a JSON object of three strings:

```json
{"payload": "<JSON text>", "signature": "<128 hex digits>", "key": "<16 hex digits>"}
```

- `payload` is the document's JSON text. The signature is Ed25519 over its exact UTF-8 bytes; nothing is canonicalised.
- `signature` is the 64-byte signature in lowercase hexadecimal.
- `key` names the key that signed: the first 8 bytes of the SHA-256 of its 32-byte public key, in lowercase hexadecimal.

Every payload is a JSON object that begins with these fields:

| Field | Meaning |
|---|---|
| `contract` | `1` for this version. A family server refuses a version it does not know. |
| `kind` | `keys`, `index`, `item` or `withdrawals`. A family server refuses a document of another kind than the one it asked for. |
| `issued_at` | When the library signed it, RFC 3339 in UTC to the second. |
| `expires_at` | For `keys`, `index` and `withdrawals` only: after this instant the document is no longer current and is refused. |

Times are always RFC 3339 in UTC to the second, such as `2026-11-26T09:00:00Z`.

## Keys

A library has one **root key**, kept offline, and one or more **signing keys** online. A family server is given the root key's public half with the library's address and trusts nothing else.

`GET /.well-known/minute-jar-library.json` answers the keys document, signed by the root key:

```json
{
  "contract": 1,
  "kind": "keys",
  "issued_at": "2026-11-26T09:00:00Z",
  "expires_at": "2026-12-26T09:00:00Z",
  "signing_keys": [
    {"id": "<16 hex>", "public_key": "<64 hex>", "not_after": "2027-02-26T09:00:00Z"}
  ]
}
```

A family server accepts another signed document only if its `key` is listed in a current keys document and the instant of checking is not after that key's `not_after`. The library re-signs every document when it changes signing key.

## Index

`GET /v1/index` answers the index, signed by a signing key, identical for every caller:

```json
{
  "contract": 1,
  "kind": "index",
  "issued_at": "2026-11-26T09:00:00Z",
  "expires_at": "2026-11-29T09:00:00Z",
  "items": [
    {
      "item": "<32 hex>",
      "version": 2,
      "type": "bank",
      "title": "Irregular verbs, part 1",
      "licence": "CC-BY-SA-4.0",
      "author": {"pseudonym": "Owl", "editorial": false},
      "derived_from": null,
      "metadata": {
        "language": "en-GB",
        "subject": "english",
        "ages": {"min": 9, "max": 11},
        "description": "The fifty most common irregular verbs.",
        "curriculum": {"country": "GB", "year": "Year 5", "topic": "Verb forms"}
      },
      "selection": false,
      "signals": {"families": 0, "ratings": [0, 0, 0, 0, 0]},
      "published_at": "2026-11-20T18:00:00Z"
    }
  ]
}
```

- One entry per shared item, at its latest published version that is not withdrawn, ordered by `item`. A withdrawn item leaves the index.
- `type` is `bank` or `path`. `licence` is always `CC-BY-SA-4.0` (ADR-0001).
- `author.editorial` is true for a version the operator's editorial account shared (ADR-0002): the software's publisher, not a parent. It is set at publication and never changes in a signed version.
- `selection` is true for an item the operator's team marked Selection (read and chosen; `moderation-api.md`, Selection). The library signs the index again whenever the mark is set or removed.
- `derived_from` is `null` or `{"item", "version", "pseudonym"}` of the original.
- `metadata.language` is a BCP 47 tag; `curriculum` may be `null`.
- An index lives three days; the library issues a new one at least daily.
- `signals.families` is the number of distinct library identifiers that reported importing the item, at any of its versions, as of when the index was signed (see Imports). A family server whose parent renews its library identifier counts again under the new one.
- `signals.ratings` is five counts: how many library identifiers rated the item 1, 2, 3, 4 and 5 stars, as of when the index was signed (see Ratings). Whole numbers only, so that every client computes the same average.

## Shared items

`GET /v1/items/{item}/versions/{version}` answers one version of a shared item, signed by a signing key:

```json
{
  "contract": 1,
  "kind": "item",
  "issued_at": "2026-11-26T09:00:00Z",
  "item": "<32 hex>",
  "version": 2,
  "type": "bank",
  "title": "Irregular verbs, part 1",
  "licence": "CC-BY-SA-4.0",
  "author": {"pseudonym": "Owl", "editorial": false},
  "derived_from": null,
  "metadata": {"language": "en-GB", "...": "as in the index"},
  "published_at": "2026-11-20T18:00:00Z",
  "content": {"...": "the bank or learning path in the bank format"},
  "media": [
    {"digest": "<64 hex>", "type": "image/png", "size": 70}
  ]
}
```

- `content` is in the bank format of the main repository; this version of the contract does not constrain it further.
- A shared item does not expire: a version, once published, does not change. A withdrawal is announced by the withdrawal list.
- `media` lists the pictures and sounds that `content` names, each once, ordered by `digest`; it is `[]` when there are none.

## Media

Pictures and sounds are not signed one by one (ADR-0004): a signed shared item lists each by its digest, and a family server checks every file it fetches against it.

| Field | Meaning |
|---|---|
| `digest` | The SHA-256 of the file's bytes, 64 lowercase hexadecimal digits, as `content` names it in the bank format. |
| `type` | The file's media type: `image/png`, `image/jpeg`, `image/gif` or `image/webp` for a picture, `audio/mpeg`, `audio/ogg` or `audio/wav` for a sound, as the bank format allows. |
| `size` | The file's length in bytes. |

`GET /v1/media/{digest}` answers the file's bytes, with `Content-Type` its `type`. The library refuses at publication:

- a file over **1048576 bytes** (1 MiB), the bank format's limit for one picture or sound;
- media of one version over **16777216 bytes** (16 MiB) together, the bank format's limit for the pictures and sounds of one bank;
- a file that is none of the types above, recognised by its first bytes.

A family server keeps a file only if its SHA-256 is the digest it asked for.

## Item files

A parent can download a shared item at one version as a file, from the website, so that a family server without Internet access can import it. The file is not signed itself: it carries the signed documents as the library serves them, byte for byte, and the item's media.

```
GET /api/items/{item}/versions/{version}/file
Content-Disposition: attachment; filename="minute-jar-<item>-<version>.json"

{"contract":1,"kind":"item_file","keys":<keys document>,"item":<signed shared item>,"media":[{"digest":"<64 hex>","base64":"<the file's bytes>"}]}
```

| Field | Meaning |
|---|---|
| `contract` | `1` for this version. |
| `kind` | `item_file`. |
| `keys` | The keys document (see Keys), the exact bytes `GET /.well-known/minute-jar-library.json` answered when the file was made. |
| `item` | The signed shared item at that version, the exact bytes `GET /v1/items/{item}/versions/{version}` answers. |
| `media` | One entry per medium the item lists, in its order: `digest`, and the file's bytes in `base64` (RFC 4648, with padding). `[]` when there are none. |

The library writes the file as one line, the two signed documents spliced in unchanged, so that the file holds each of them byte for byte. A family server reads it as JSON; since a signature covers only a payload's exact text (see Signed documents), the documents it reads out check as they would have online. It imports the item only if:

- the keys document is signed by the root key it trusts and lists the item's `key`, and the item's `issued_at` is not after that key's `not_after`. The keys document's own `expires_at` is not checked here: a file may be imported long after it was downloaded, and the root key, kept offline, vouches for the signing key;
- the item is a valid signed shared item of contract `1`;
- every listed medium is in `media`, and the SHA-256 of its bytes is its digest.

A file cannot say whether the item was withdrawn since: a family server checks the withdrawal list the next time it reaches the library, as for any imported item. The library makes files only of versions the catalogue shows: a withdrawn item, or one hidden from new imports, answers `404` with `{"error": "unknown_item"}`, a withdrawn or unknown version `404` with `{"error": "unknown_version"}`. A file is served with `Cache-Control: public, max-age=3600`, as long as the keys document it holds.

## Withdrawals

`GET /v1/withdrawals` answers the withdrawal list, signed by a signing key, identical for every caller:

```json
{
  "contract": 1,
  "kind": "withdrawals",
  "issued_at": "2026-11-26T09:00:00Z",
  "expires_at": "2026-11-27T09:00:00Z",
  "withdrawals": [
    {"item": "<32 hex>", "version": null, "kind": "switch_off", "reason": "personal_data", "withdrawn_at": "2026-11-26T09:00:00Z"}
  ]
}
```

- One entry per withdrawn item or version, ordered by `item` then `version`. `version` is `null` when every version of the item is withdrawn.
- `kind` is `author` (the author's choice), `switch_off` (provisional, until a moderator decides) or `moderation` (a moderation decision).
- `reason` is `author_choice` for the author's kind, otherwise one of `rights`, `personal_data`, `sexual_content`, `illegal_content`, `unsuitable_content`, `wrong_answer`, `other`.
- A family server stops using a listed item or version and tells the parents the reason. An item no longer switched off leaves the list.
- An author's withdrawal is final: it replaces a switch-off or a moderation withdrawal of the same item or version, and nothing replaces or ends it, neither a reactivation nor a moderation decision nor an appeal.
- A withdrawal list lives one day; the library issues a new one whenever the catalogue changes and at least every six hours.

## Library identifier

A family server draws its **library identifier** for one library alone: 128 random bits from a cryptographic source, written as 32 lowercase hexadecimal digits. It is linked to nothing else, and a parent may draw a new one at any time. Every anonymous call a family server makes to the library (imports and ratings here; reports, suggestions and, in step 2, contributions in later versions) carries it the same way:

- as the field `library_identifier` of a JSON request body, `POST`ed with `Content-Type: application/json`;
- never in a path, a query string, a header or a cookie, so that no access log in front of the library records it;
- never in the same request as a library account's session: a share carries no library identifier, and a call carrying a library identifier carries no session.

The library keeps an identifier only with what the call needs for its effect (for an import, the item imported), never with a time, an address or an account, and no route answers anything about an identifier. Technical logs of anonymous calls, if the operator keeps any, hold no request body and are kept briefly.

A malformed identifier answers `400` with `{"error": "bad_library_identifier"}`; a body that is not the expected JSON object answers `400` with `{"error": "bad_request"}`.

A `POST` under `/v1/` that carries an `Authorization` header (where a library account's session travels, `accounts-api.md`) answers `400` with `{"error": "session_not_allowed"}` and has no effect.

## Imports

When a family server imports a shared item into its family library, it tells the library once:

```
POST /v1/imports
Content-Type: application/json

{"library_identifier": "<32 hex>", "item": "<32 hex>"}
```

- The answer is `204 No Content`, with no body and no cookie. Reporting the same item again under the same identifier, at another version or not, changes nothing and answers `204` too.
- An unknown item answers `404` with `{"error": "unknown_item"}`.
- The import counts in `signals.families` of the next index the library signs; the index served until then is unchanged.

## Ratings

Once a family has used an imported item, its family server may rate it, with or without a short review:

```
POST /v1/ratings
Content-Type: application/json

{"library_identifier": "<32 hex>", "item": "<32 hex>", "stars": 4, "review": "Clear and short."}
```

- `stars` is a whole number from 1 to 5. `review` is optional (absent or `null`); it is trimmed, and an empty one counts as none.
- The answer is `204 No Content`, with no body and no cookie.
- One rating per library identifier and item: a new one replaces the earlier rating and its review, whole. The rating counts in `signals.ratings` of the next index the library signs.
- Only an item the identifier reported importing (see Imports) can be rated; otherwise the answer is `403` with `{"error": "not_imported"}`.
- Refusals: `{"error": "bad_rating"}` (`400`) for stars outside 1 to 5; `{"error": "bad_review"}` (`400`) for a review longer than 280 characters (Unicode scalar values, after trimming); `bad_library_identifier`, `bad_request` and `unknown_item` as for imports.
- The library keeps with the identifier only the item, the stars and the review, never a time.

A short review is public text from an anonymous family, so it is held like any other public text: it is stored **pending** and shown nowhere until the review publishes it (or refuses it). A new rating puts its review back to pending. Until the review handles short reviews, every short review stays pending. The stars count at once, whatever becomes of the review.

### For the website

The website's item page reads, from the service's JSON API (not part of the contract with family servers, and not signed):

```
GET /api/items/{item}/ratings

{"item": "<32 hex>", "ratings": [0, 0, 1, 3, 5], "reviews": [{"stars": 5, "text": "My son learnt them in a week."}]}
```

- `ratings` is counted as things stand, not as of the last signed index.
- `reviews` holds only published short reviews, highest rated first, with nothing that tells families apart: no identifier, no time.
- An unknown item answers `404` with `{"error": "unknown_item"}`.

## Suggestions

A family that used an imported item may suggest other answers to accept for one of its questions (story S18), for example a spelling its child typed that the item marked wrong:

```
POST /v1/suggestions
Content-Type: application/json

{"library_identifier": "<32 hex>", "item": "<32 hex>", "version": 1, "question": 1, "answers": ["goed"]}
```

- `version` is the version the family uses; `question` the question's place in its `content.questions`, from 1, as in the review's reasons. The question must take answers (an `answers` array).
- `answers` holds 1 to 10 answers, each trimmed, of 1 to 100 characters (Unicode scalar values) and without control characters. An answer the question already accepts is left out.
- The answer is `204 No Content`, with no body and no cookie. Suggesting the same answer again changes nothing: each identifier counts once per answer.
- Only an item the identifier reported importing can receive suggestions; otherwise `403 {"error": "not_imported"}`.
- Refusals: `400 {"error": "bad_suggestion"}` for the answers; `404 {"error": "unknown_version"}`; `400 {"error": "unknown_question"}`; `bad_library_identifier`, `bad_request` and `unknown_item` as for imports. Like every `POST` under `/v1/`, it is refused with a session.
- The library keeps with the identifier only the item, the version, the question and the answers, never a time, an address or an account. Suggestions are not public: only the item's author reads them, counted.

### For the author

The website's account pages read, with the library account's session (`Cache-Control: private, no-store`):

```
GET /api/account/suggestions

{"items": [{"item": "<32 hex>", "title": "Irregular verbs", "version": 2, "questions": [
  {"question": 1, "prompt": "Past of 'go'", "answers": [{"answer": "goed", "families": 2}]}
]}]}
```

- Only the account's items with suggestions are listed. Questions are numbered in the item's latest `version`; a suggestion for an earlier version follows its question there by its `prompt`, and is left out when no question of the latest version has that prompt. An answer the latest version accepts is no longer suggested.
- `families` counts the distinct library identifiers that suggested the answer; the answers come most suggested first. No identifier is ever shown.

The author accepts some of them into a new version:

```
POST /api/account/items/{item}/suggestions/accept
Authorization: Bearer <session>

{"accepted": [{"question": 1, "answers": ["goed"]}]}
```

- The answer is `200 {"prepared": {...}}`: the latest version with the accepted answers added to their questions, as the body of the share route (`item`, `type`, `title`, `metadata`, `content`, `media` in base64), which the website then sends to `POST /api/shares` with the same session. The new version goes through the review like any share, counts in the quota, and the accepted answers leave the suggestions once it is published.
- Refusals: an item that is not one of the account's `404 {"error": "unknown_item"}`; no answer accepted `400 {"error": "bad_suggestion"}`; an answer that is not among the question's suggestions `409 {"error": "not_suggested"}`; signed out `401 signed_out`.
- The prepared body is a new version of the item (`item`, see Shares, New versions): a derived work keeps its original there without naming it again.

## Reports

A family server reports a shared item for a parent, under its library identifier:

```
POST /v1/reports
Content-Type: application/json

{"library_identifier": "<32 hex>", "item": "<32 hex>", "reason": "wrong_answer", "details": "Question 3: the past of 'go' is 'went'."}
```

The website sends the same report anonymously, through its unsigned API under `/api/` (not part of the family-server contract), with an e-mail address if the reporter wants the acknowledgement and the decision, and a rights holder's formal notice if there is one:

```
POST /api/reports
Content-Type: application/json

{"item": "<32 hex>", "reason": "rights", "details": "...", "contact": "rights@example.org",
 "notice": {"name": "Example Publishing", "work": "English Grammar, Year 5"}}
```

- `reason` is one of `personal_data`, `sexual_content`, `illegal_content`, `rights`, `unsuitable_content`, `wrong_answer`, `other`, the codes of the withdrawal list. `details` (at most 2000 characters), `contact` and `notice` may be left out. A family server's report carries no `contact` and no `notice`; the website's carries no library identifier.
- The answer is `202 Accepted` with `{"report": "<32 hex>"}`, `Cache-Control: no-store` and no cookie: the report's identifier, 128 random bits, is its acknowledgement. Only its reporter knows it, and the library keeps no link between it and the library identifier.
- `GET /v1/reports/{report}` answers where the report stands, with `Cache-Control: no-store`:

  ```json
  {"report": "<32 hex>", "item": "<32 hex>", "reason": "personal_data", "status": "rejected",
   "received_at": "2026-11-26T10:00:00Z",
   "decision": {"outcome": "rejected", "reasons": "...", "decided_at": "2026-11-27T10:00:00Z"}}
  ```

  `status` is `with_author` (a wrong answer or unsuitable content the author has 7 days to correct), `with_moderator`, `corrected` (the author published a new version within 7 days), `upheld` or `rejected`; `decision` is `null` until a moderator decides. A family server polls it to tell the parent the decision; a website reporter who left an address receives the acknowledgement and the decision by e-mail, and the address is erased once the decision is sent.
- What a report does at once: for `personal_data`, `sexual_content` or `illegal_content`, the item is reviewed again on that reason; if the report is plausible, or if two families that imported the item reported it for one of these reasons, the item is switched off (`kind` `switch_off` in the withdrawal list), otherwise it leaves the index while still served to the families that have it (hidden from new imports). For `rights`, the item is hidden from new imports once three distinct library identifiers that imported it reported it, or at once on a formal notice. Other reasons change nothing until a moderator decides.
- A moderator's decision that upholds a report withdraws the item (`kind` `moderation`, with the report's reason); one that rejects it ends the switch-off or the hiding once no other report for the same reasons waits.
- Refusals: a malformed identifier `400 {"error": "bad_library_identifier"}`; an unknown reason `400 {"error": "bad_reason"}`; a contact that is not an e-mail address `400 {"error": "bad_contact"}`; a notice without a name, a work or a contact, or for another reason than `rights`, `400 {"error": "bad_notice"}`; a body that is not the expected object, or details too long, `400 {"error": "bad_request"}`; an unknown item `404 {"error": "unknown_item"}`; an unknown report `404 {"error": "unknown_report"}`.

### Appeals

The reporter of a decided report may appeal the decision within six months, once; another moderator than the one who decided looks again (`moderation-api.md`). Knowing the report's identifier is what makes the reporter.

```
POST /v1/reports/{report}/appeal
Content-Type: application/json

{"statement": "Page 12 of English Grammar, Year 5."}
```

```
POST /api/reports/{report}/appeal
Content-Type: application/json

{"statement": "...", "contact": "reporter@example.org"}
```

- A family server's appeal carries only `statement` (at most 2000 characters); the website's may add `contact`, kept only until the appeal's decision is sent.
- The answer is `202 Accepted` with `{"appeal": "<32 hex>"}` and `Cache-Control: no-store`.
- `GET /v1/appeals/{appeal}` answers where it stands, with `Cache-Control: no-store`:

  ```json
  {"appeal": "<32 hex>", "report": "<32 hex>", "status": "reversed",
   "received_at": "2026-11-27T10:01:00Z",
   "decision": {"outcome": "reversed", "reasons": "...", "decided_at": "2026-11-27T11:00:00Z"}}
  ```

  `status` is `waiting`, `maintained` or `reversed`; `decision` is `null` until decided. A reversal also changes the report: `GET /v1/reports/{report}` then answers the new status (`upheld` or `rejected`) with the appeal's reasons, and the withdrawal list follows it.
- Refusals: an empty statement `400 {"error": "statement_required"}`; a contact that is not an e-mail address `400 {"error": "bad_contact"}`; a body that is not the expected object (a family server's `contact` among others), or a statement too long, `400 {"error": "bad_request"}`; an unknown report `404 {"error": "unknown_report"}`; a report not decided yet, or closed by its author's correction, `409 {"error": "not_decided"}`; more than six months after the decision `409 {"error": "too_late"}`; a second appeal of the same report `409 {"error": "already_appealed"}`; an unknown appeal `404 {"error": "unknown_appeal"}`.
- The author appeals through their library account: `POST /api/account/reports/{report}/appeal` with the session and `{"statement"}` answers `202 {"appeal"}`, with `Cache-Control: private, no-store`, for a report of one of the account's items, with the same refusals; a report of another account's item answers `404 unknown_report`. The author reads what they must be told about reports on their items, and the decisions with their reasons, at `GET /api/account/notifications` (`interface-v1.md`, Shares).

## Shares

A parent shares a bank or a learning path from the parent app, under their library account (stories S10, S13, S16). The family server prepares the item (recorded sound removed, picture metadata erased, the family's names searched for; main repository, sharing preparation), and the parent app sends it with the library account's **session** (`accounts-api.md`), never with the family server's library identifier. It is the one call of this contract that carries a session, so it lives under `/api/`, beside sign-in, and every `POST` under `/v1/` stays anonymous.

```
POST /api/shares
Authorization: Bearer <session>
Content-Type: application/json

{"type": "bank", "title": "Irregular verbs, part 1",
 "metadata": {"language": "en-GB", "subject": "english", "ages": {"min": 9, "max": 11},
              "description": "...", "curriculum": {"country": "GB", "year": "Year 5", "topic": "Verb forms"}},
 "content": {"...": "the bank or learning path in the bank format"},
 "media": [{"base64": "<the file's bytes>"}],
 "sharing_terms": "2026-11"}
```

- `type`, `title` (1 to 120 characters once trimmed), `metadata` (as in the index; `description` at most 2000 characters, `curriculum` may be `null`) and `content` are required; `media` lists the item's pictures and sounds, each file's bytes in base64 (RFC 4648, with padding), and may be left out. The library draws the shared item's identifier at a first share.
- **New versions** (story S14). `"item": "<32 hex>"` shares a new version of one of the account's own published items, with its whole content again; without it the share is a new item. It goes through the same terms, review, quota and outcomes as a first share: the review is asked for the next version number, and a published share answers it as `version`. Only the account that answers for the item may add a version to it (`403 not_the_author`); an item with no published version is `404 unknown_item`, an item withdrawn whole `409 withdrawn_item`. A held new version waits like a held first share; refusing it leaves the item and its published versions to their author.
- **Derived works** (story S15). `"derived_from": {"item": "<32 hex>", "version": n}` names the version of another item the share changes. The library writes the original's attribution, `{"item", "version", "pseudonym"}`, the pseudonym as that version was signed, into the derived work's `derived_from` in the item, the index, the catalogue API and the item page; the parent app does not send the pseudonym. The original must be a version the catalogue shows: not withdrawn, its item not withdrawn or hidden (`404 unknown_original`); an item is not derived from itself (`400 bad_original`). A new version of a derived work keeps its original without naming it again, may name a later version of the same original, and may not name another item (`409 original_changed`): the first original's attribution is never dropped.
- **Editorial account** (ADR-0002, story S5). The operator marks one library account as its editorial account, from the command line, never through a route (`library mark-editorial <subject>`, `library unmark-editorial <subject>`, `Library::mark_editorial`). Its shares go through the same terms, review, quota and outcomes as any other, and are published with `author.editorial` true in the item and the index. Each says how it was made and who read it before it was shared, in the share body only, never signed into the item:

  ```json
  "editorial": {"ai_generated": true, "read": {"by": "moderator", "handle": "Heron", "at": "2026-11-26T08:00:00Z"}}
  ```

  `read.by` is `moderator`, with the `handle` of a moderator the operator named (compared without regard to case), or `operator`, without `handle`, when someone of the operator's team who moderates nothing read it; `at` is when, not after the share. The catalogue API's item page answers it as `editorial` (`catalogue-api.md`, Item page); a held editorial share keeps it until a moderator publishes it. A share from the editorial account without it is `400 reading_required`; a malformed one (an unknown handle, a handle for the operator, an instant to come) `400 bad_reading`; one from any other account `403 not_editorial`. Once the mark is removed, the account shares as any parent, and its versions signed before keep `author.editorial` true.
- **Sharing terms.** `GET /api/sharing-terms` answers `{"version": "2026-11", "shares_per_day": 10}`. An account accepts the terms in force at its first share, by sending their version as `sharing_terms`, or beforehand on the website (`PUT /api/account/sharing-terms` with `{"version"}`, `409 outdated_sharing_terms` for another version). When the terms change, the next share accepts the new version. Without it the share answers `403 {"error": "sharing_terms_required"}`; the parent app shows the terms and sends the share again with their version.
- The share is **reviewed** (ADR-0005) before anything is public. The answer, with `Cache-Control: private, no-store`, is the share:

  ```json
  {"share": "<32 hex>", "item": "<32 hex>", "title": "Irregular verbs, part 1", "outcome": "published",
   "version": 1, "reasons": [], "shared_at": "2026-11-27T10:00:00Z"}
  ```

  - `201`, `outcome` `published`: the version is in the next index under the account's pseudonym, as the account has it at the share, and served at `/v1/items/{item}/versions/{version}`.
  - `202`, `outcome` `held`: held for a moderator, who publishes or refuses it; `version` is `null` until published. What the review found is for the moderator and is not answered.
  - `200`, `outcome` `refused`: nothing is published, `item` is `null`, and `reasons` says why, per question, for the author to correct and share again:

    ```json
    [{"question": 2, "reason": "wrong_answer", "explanation": "The past of 'see' is 'saw'."},
     {"question": null, "reason": "rights", "explanation": "Looks copied from a textbook."}]
    ```

    `question` is the question's or lesson card's place in `content`, from 1, or `null` for the whole item; `reason` one of the codes of the withdrawal list; `explanation` in the item's language.
- **Quota.** An account shares at most 10 items a day (UTC), whatever the outcome: the review costs the operator at every share. Beyond, `429 {"error": "share_quota_reached", "shares_per_day": 10, "next_share_at": "2026-11-28T00:00:00Z"}`. A share the review could not handle (`503`) does not count.
- `GET /api/account/shares`, with the session, answers the author's shares, newest first, with the terms and the quota, for the website:

  ```json
  {"sharing_terms": {"current": "2026-11", "accepted": "2026-11"},
   "quota": {"shares_per_day": 10, "shared_today": 2, "next_share_at": null},
   "shares": [<share>, ...]}
  ```
- Refusals, in this order: a body carrying `library_identifier` `400 {"error": "library_identifier_not_allowed"}` (nothing is read further, nothing kept); `503 unavailable` until the service is given a reviewer; `401 signed_out`; a body that is not the expected object `400 bad_request`; `403 strong_sign_in_required` (a recent strong sign-in, as for opening an account); `404 no_account`; `403 card_required` (the account's card is not verified, `accounts-api.md`, Card); `403 sharing_terms_required`; `400 bad_item` (title or metadata); `400 bad_content` (not in the bank format); `400 medium_too_large`, `media_too_large` or `unsupported_medium` (see Media); `404 unknown_item`, `403 not_the_author`, `409 withdrawn_item` (a new version); `400 bad_original`, `409 original_changed`, `404 unknown_original` (a derived work); `403 not_editorial`, `400 reading_required`, `400 bad_reading` (the editorial account); `429 share_quota_reached`; `503 unavailable` when the review cannot be reached.
- A held share waits in the moderation queue (`moderation-api.md`, `held_review`) until a moderator publishes it, under the account's pseudonym of that moment, or refuses it; `GET /api/account/shares` then answers its new outcome.
- `GET /api/account/notifications`, with the session, answers what the author must be told about reports on their published items, oldest first: `{"notifications": [{"item", "title", "report", "reason", "details", "decision": null | {"outcome": "upheld" | "rejected", "reasons"}, "appeal": null | {"outcome": "maintained" | "reversed", "reasons"}, "at"}]}`. `decision` is `null` for a wrong answer or unsuitable content to correct within 7 days. The author may appeal a decision (see Appeals).
- **The author's withdrawal** (story S17). `GET /api/account/items`, with the session, answers the account's published items, the latest published first, each with its versions, newest first, and what withdrew them: `{"items": [{"item", "title" (of its latest version), "withdrawal": null | {"kind", "reason", "withdrawn_at"}, "versions": [{"version", "published_at", "withdrawal": null | {...}}]}]}`, `kind` and `reason` as in the withdrawal list. `POST /api/account/items/{item}/withdrawal` with `{"version": null}` withdraws every version of the item, with `{"version": 2}` that version only; it answers `200` with the withdrawal list's entry (`"kind": "author", "reason": "author_choice"`). The item or version leaves the index and the catalogue at once and enters the withdrawal list. Refusals, in this order: `503 unavailable` without accounts; `401 signed_out`; `400 bad_request`; `403 strong_sign_in_required` (a recent strong sign-in, as for sharing); `404 no_account`; `404 unknown_item` for an item the account does not answer for, whoever's it is, or never published; `409 already_withdrawn` when the author withdrew it, or the whole item, already; `404 unknown_version`.
- Every share, whatever its outcome, is written to the account's sharing log, identification data kept for the legal period after the last share (`accounts-api.md`, Closing). Each shared item is joined to the account that answers for it, so that its author hears of reports and decisions and may appeal them.

## HTTP

- Every signed document is served as `application/json` with a strong `ETag`; a request whose `If-None-Match` names it answers `304 Not Modified` without a body.
- `Cache-Control`: `public, max-age=3600` for keys, `public, max-age=300` for the index and the withdrawal list, `public, max-age=86400` for an item version.
- A medium is served with `Cache-Control: public, max-age=31536000, immutable`, `X-Content-Type-Options: nosniff` and the strong `ETag` `"<digest>"`, with the same `304 Not Modified`: its address is its digest, so it never changes.
- The bytes served are the same for every caller; nothing in a request changes them.
- An unknown item answers `404` with `{"error": "unknown_item"}`, an unknown version of a known item `404` with `{"error": "unknown_version"}`.
- A digest that no published version lists, or that is not 64 lowercase hexadecimal digits, answers `404` with `{"error": "unknown_media"}`.
