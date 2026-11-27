---
title: Interface between a family server and a library, version 1
status: draft
---

# Interface between a family server and a library, version 1

The wire format of the signed documents a library serves to family servers (ADR-0004). It makes concrete the interface contract of the main repository (`docs/specs/public-library.md`, "Interface between a family server and a library") and is proposed there as its version 1. The fixtures in `fixtures/v1/` are signed with public test keys and let a family server's library client and a library check each other.

This version covers the keys document, the index, shared items and their media, the withdrawal list, imports and reports. Ratings, suggestions and shares are added by later tickets of `docs/tickets/step-1.md`.

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
      "signals": {"families": 0},
      "published_at": "2026-11-20T18:00:00Z"
    }
  ]
}
```

- One entry per shared item, at its latest published version that is not withdrawn, ordered by `item`. A withdrawn item leaves the index.
- `type` is `bank` or `path`. `licence` is always `CC-BY-SA-4.0` (ADR-0001).
- `author.editorial` is true for the operator's editorial account (ADR-0002).
- `derived_from` is `null` or `{"item", "version", "pseudonym"}` of the original.
- `metadata.language` is a BCP 47 tag; `curriculum` may be `null`.
- An index lives three days; the library issues a new one at least daily.
- `signals.families` is the number of distinct library identifiers that reported importing the item, at any of its versions, as of when the index was signed (see Imports). A family server whose parent renews its library identifier counts again under the new one.

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
- A withdrawal list lives one day; the library issues a new one whenever the catalogue changes and at least every six hours.

## Library identifier

A family server draws its **library identifier** for one library alone: 128 random bits from a cryptographic source, written as 32 lowercase hexadecimal digits. It is linked to nothing else, and a parent may draw a new one at any time. Every anonymous call a family server makes to the library (imports here; ratings, reports, suggestions and, in step 2, contributions in later versions) carries it the same way:

- as the field `library_identifier` of a JSON request body, `POST`ed with `Content-Type: application/json`;
- never in a path, a query string, a header or a cookie, so that no access log in front of the library records it;
- never in the same request as a library account's session: a share carries no library identifier, and a call carrying a library identifier carries no session.

The library keeps an identifier only with what the call needs for its effect (for an import, the item imported), never with a time, an address or an account, and no route answers anything about an identifier. Technical logs of anonymous calls, if the operator keeps any, hold no request body and are kept briefly.

A malformed identifier answers `400` with `{"error": "bad_library_identifier"}`; a body that is not the expected JSON object answers `400` with `{"error": "bad_request"}`.

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

## Reports

A family server reports a shared item for a parent, under its library identifier:

```
POST /v1/reports
Content-Type: application/json

{"library_identifier": "<32 hex>", "item": "<32 hex>", "reason": "wrong_answer", "details": "Question 3: the past of 'go' is 'went'."}
```

The website sends the same report anonymously, with an e-mail address if the reporter wants the acknowledgement and the decision, and a rights holder's formal notice if there is one:

```
POST /v1/website/reports
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

## HTTP

- Every signed document is served as `application/json` with a strong `ETag`; a request whose `If-None-Match` names it answers `304 Not Modified` without a body.
- `Cache-Control`: `public, max-age=3600` for keys, `public, max-age=300` for the index and the withdrawal list, `public, max-age=86400` for an item version.
- A medium is served with `Cache-Control: public, max-age=31536000, immutable`, `X-Content-Type-Options: nosniff` and the strong `ETag` `"<digest>"`, with the same `304 Not Modified`: its address is its digest, so it never changes.
- The bytes served are the same for every caller; nothing in a request changes them.
- An unknown item answers `404` with `{"error": "unknown_item"}`, an unknown version of a known item `404` with `{"error": "unknown_version"}`.
- A digest that no published version lists, or that is not 64 lowercase hexadecimal digits, answers `404` with `{"error": "unknown_media"}`.
