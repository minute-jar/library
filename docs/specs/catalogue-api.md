---
title: Catalogue API
status: draft
---

# Catalogue API

The JSON API through which the website browses and searches the public library (ADR-0003). Unlike the interface with family servers (`interface-v1.md`), nothing here is signed: it is the service's own API, read by its website, and it may change with it. It is anonymous and read-only, and every caller receives the same answer for the same request.

## Browsing and searching

`GET /api/catalogue` answers the shared items at their latest version, newest first, twenty to a page:

```json
{
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
  ],
  "total": 1,
  "page": 1,
  "pages": 1
}
```

Each entry has the fields of an index entry (`interface-v1.md`, Index), its signals counted when asked; a withdrawn item or one hidden from new imports (`interface-v1.md`, Reports) is left out, and a withdrawn version gives way to the latest one before it, as in the index. Every parameter is optional; together they narrow the catalogue to the items that match all of them. Values are compared without regard to case.

| Parameter | Keeps the items |
|---|---|
| `language` | in that language: a BCP 47 tag, where `fr` also matches `fr-FR` and `fr-CA` |
| `subject` | of that subject |
| `age` | meant for that age, from 0 to 255: `ages.min <= age <= ages.max` |
| `country`, `year`, `topic` | whose curriculum has that country, school year or topic |
| `q` | whose title, description, subject or curriculum topic hold the words searched, in any inflected form, English or French (web search syntax: `"exact phrase"`, `or`, `-excluded`) |
| `page` | the page asked for, from 1; a page past the last answers no items |

A malformed `age`, `language` or `page` answers `400` with `{"error": "bad_filter"}`.

## Filters

`GET /api/catalogue/filters` answers the values the catalogue's items take, sorted, for the website to offer as filters:

```json
{
  "languages": ["en-GB", "fr-FR"],
  "subjects": ["english", "mathematics"],
  "curricula": [{"country": "GB", "year": "Year 5", "topic": "Verb forms"}]
}
```

## Item page

`GET /api/items/{item}` answers what the website's item page shows of a shared item:

```json
{
  "item": "<32 hex>",
  "version": 2,
  "...": "every field of a catalogue entry, at this version",
  "content": {"...": "the bank or learning path, as signed"},
  "media": [{"digest": "<64 hex>", "type": "image/png", "size": 70}],
  "versions": [
    {"version": 2, "published_at": "2026-11-22T10:00:00Z"},
    {"version": 1, "published_at": "2026-11-20T18:00:00Z"}
  ]
}
```

- The entry is the item's catalogue entry: its latest version that is not withdrawn, signals counted when asked. An item hidden from new imports (see Reports in `interface-v1.md`) has no page.
- `content` and `media` are those of the signed shared item at that version (`interface-v1.md`, Shared items).
- `versions` lists every version that is not withdrawn, newest first; each can be downloaded as an item file (`interface-v1.md`, Item files) at `GET /api/items/{item}/versions/{version}/file`.
- Its ratings and published short reviews are at `GET /api/items/{item}/ratings` (`interface-v1.md`, Ratings, For the website).
- Comments, and the AI-generated and read-by-a-person marks of editorial content, are added here by the tickets that bring them; until then they are absent, and the website shows them only when present.
- An item withdrawn, hidden, never published or malformed answers `404` with `{"error": "unknown_item"}`.

## HTTP

- Answers are `application/json` with `Cache-Control: public, max-age=60`, except item files (`interface-v1.md`, Item files).
- A database failure answers `503` with `{"error": "unavailable"}`.
