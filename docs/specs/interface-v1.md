---
title: Interface between a family server and a library, version 1
status: draft
---

# Interface between a family server and a library, version 1

The wire format of the signed documents a library serves to family servers (ADR-0004). It makes concrete the interface contract of the main repository (`docs/specs/public-library.md`, "Interface between a family server and a library") and is proposed there as its version 1. The fixtures in `fixtures/v1/` are signed with public test keys and let a family server's library client and a library check each other.

This version covers the keys document, the index, shared items, imports and ratings. The withdrawal list, media, reports, suggestions and shares are added by later tickets of `docs/tickets/step-1.md`.

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
| `kind` | `keys`, `index` or `item`. A family server refuses a document of another kind than the one it asked for. |
| `issued_at` | When the library signed it, RFC 3339 in UTC to the second. |
| `expires_at` | For `keys` and `index` only: after this instant the document is no longer current and is refused. |

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

- One entry per shared item, at its latest published version, ordered by `item`.
- `type` is `bank` or `path`. `licence` is always `CC-BY-SA-4.0` (ADR-0001).
- `author.editorial` is true for the operator's editorial account (ADR-0002).
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
  "media": []
}
```

- `content` is in the bank format of the main repository; this version of the contract does not constrain it further.
- A shared item does not expire: a version, once published, does not change. A withdrawal is announced by the withdrawal list.

## Library identifier

A family server draws its **library identifier** for one library alone: 128 random bits from a cryptographic source, written as 32 lowercase hexadecimal digits. It is linked to nothing else, and a parent may draw a new one at any time. Every anonymous call a family server makes to the library (imports and ratings here; reports, suggestions and, in step 2, contributions in later versions) carries it the same way:

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

## HTTP

- Every signed document is served as `application/json` with a strong `ETag`; a request whose `If-None-Match` names it answers `304 Not Modified` without a body.
- `Cache-Control`: `public, max-age=3600` for keys, `public, max-age=300` for the index, `public, max-age=86400` for an item version.
- The bytes served are the same for every caller; nothing in a request changes them.
- An unknown item answers `404` with `{"error": "unknown_item"}`, an unknown version of a known item `404` with `{"error": "unknown_version"}`.
