---
title: Interface between a family server and a library, version 1
status: draft
---

# Interface between a family server and a library, version 1

The wire format of the signed documents a library serves to family servers (ADR-0004). It makes concrete the interface contract of the main repository (`docs/specs/public-library.md`, "Interface between a family server and a library") and is proposed there as its version 1. The fixtures in `fixtures/v1/` are signed with public test keys and let a family server's library client and a library check each other.

This version covers the keys document, the index, shared items and the withdrawal list. Media, imports, ratings, reports, suggestions and shares are added by later tickets of `docs/tickets/step-1.md`.

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
  "media": []
}
```

- `content` is in the bank format of the main repository; this version of the contract does not constrain it further.
- A shared item does not expire: a version, once published, does not change. A withdrawal is announced by the withdrawal list.

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

## HTTP

- Every signed document is served as `application/json` with a strong `ETag`; a request whose `If-None-Match` names it answers `304 Not Modified` without a body.
- `Cache-Control`: `public, max-age=3600` for keys, `public, max-age=300` for the index and the withdrawal list, `public, max-age=86400` for an item version.
- The bytes served are the same for every caller; nothing in a request changes them.
- An unknown item answers `404` with `{"error": "unknown_item"}`, an unknown version of a known item `404` with `{"error": "unknown_version"}`.
