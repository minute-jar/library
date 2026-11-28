---
title: Moderation API
status: draft
---

# Moderation API

The JSON API behind the website's administration pages for moderators (stories S22 and S26): one queue of what waits for a decision, the timeline of each item, decisions with reasons, appeals, and the decisions log read as the yearly transparency report's figures. Like the accounts API (`accounts-api.md`), it is the service's own API, read by its website's server, and may change with it. Code: `crates/library/src/moderation/`. The appeals of reporters, which family servers send too, are part of the interface (`interface-v1.md`, Appeals).

## Moderators

The operator names moderators, never a route: `library grant-moderator <subject> <handle>` (or `Library::grant_moderator`) makes the Minute Jar account `subject` a moderator, known to other moderators as `handle` (1 to 32 characters, unique without regard to case); `library revoke-moderator <subject>` ends it. The subject is never answered; decisions name the moderator by handle.

Every route under `/api/moderation/` carries the session as `Authorization: Bearer <session>` (`accounts-api.md`) and is refused, in this order:

- `401` `signed_out`: no valid session;
- `403` `not_a_moderator`: the session's Minute Jar account is not a moderator;
- `403` `passkey_sign_in_required`: the sign-in did not use a passkey (`amr` holding `passkey` or `webauthn`), or is older than 15 minutes. A second factor or an external provider is strong, but not enough to moderate (ADR-0006). The website sends the moderator to `/sign-in?strong&return=moderation`.

## Queue

`GET /api/moderation/queue` answers `{"cases": [...]}`, oldest first. Each case is due 48 hours after it reached the queue (`due_at`). A report waiting for a moderator:

```json
{"kind": "report", "report": "<32 hex>", "item": "<32 hex>", "title": "Irregular verbs",
 "version": 1, "reason": "rights", "details": "...", "effect": "hidden",
 "findings": null, "notice": {"name": "Example Publishing", "work": "English Grammar, Year 5"},
 "received_at": "2026-11-27T10:00:00Z", "due_at": "2026-11-29T10:00:00Z"}
```

`version` is the item's latest version when the report arrived; `effect` is what the report did at once (`none`, `switched_off`, `hidden`); `findings` what the review found. The reporter's address and library identifier are never answered.

An appeal waiting for another moderator, with the decision it is against:

```json
{"kind": "appeal", "appeal": "<32 hex>", "report": "<32 hex>", "item": "<32 hex>",
 "title": "Irregular verbs", "reason": "rights", "appellant": "reporter", "statement": "...",
 "decision": {"outcome": "rejected", "reasons": "...", "moderator": "Alice",
              "decided_at": "2026-11-27T10:00:00Z"},
 "received_at": "2026-11-27T10:01:00Z", "due_at": "2026-11-29T10:01:00Z"}
```

A share the review held (`interface-v1.md`, Shares), not yet published, with the review's findings and the prepared item for the moderator to read:

```json
{"kind": "held_review", "share": "<32 hex>", "item": "<32 hex>", "title": "Irregular verbs",
 "pseudonym": "Owl", "findings": "Question 2 may name a school.", "type": "bank",
 "metadata": {"language": "en-GB", "...": "as the parent app sent it"},
 "content": {"...": "the bank or learning path"},
 "received_at": "2026-11-27T10:00:00Z", "due_at": "2026-11-29T10:00:00Z"}
```

`pseudonym` is the author's pseudonym now, under which a publication would go. A held share has no timeline: its item is not in the catalogue yet.

A comment the review held (`catalogue-api.md`, Comments), with the item it is on:

```json
{"kind": "held_comment", "comment": "<32 hex>", "item": "<32 hex>", "title": "Irregular verbs",
 "pseudonym": "Lark", "text": "Tom loved it.", "findings": "It may name a child.",
 "received_at": "2026-11-27T10:00:00Z", "due_at": "2026-11-29T10:00:00Z"}
```

A report of a published comment (`catalogue-api.md`, Reports of comments); `hidden` is whether the comment is hidden until the decision:

```json
{"kind": "comment_report", "report": "<32 hex>", "comment": "<32 hex>", "item": "<32 hex>",
 "title": "Irregular verbs", "pseudonym": "Lark", "text": "Learnt in a week.",
 "reason": "unsuitable_content", "details": "Rude.", "hidden": false,
 "received_at": "2026-11-27T10:00:00Z", "due_at": "2026-11-29T10:00:00Z"}
```

Comments have no timeline: an item's timeline holds only the reports of the item.

## Timeline

`GET /api/moderation/items/{item}` answers `{"item", "title", "timeline": [...], "selection"}`, the timeline oldest first, or `404` `unknown_item`. `selection` is `null`, or `{"moderator": "<handle>", "selected_at"}` for an item marked Selection. Each entry has `at` and `event`:

- `reported`: `report`, `reason`, `details`, `effect`, `findings`, `notice`;
- `corrected`: `report` (the author published a new version within 7 days);
- `decided`: `report`, `reason`, `outcome` (`upheld` or `rejected`), `reasons`, `moderator`;
- `appealed`: `appeal`, `report`, `appellant` (`author` or `reporter`), `statement`;
- `appeal_decided`: `appeal`, `report`, `outcome` (`maintained` or `reversed`), `reasons`, `moderator`.

## Decisions

- `POST /api/moderation/reports/{report}/decision` with `{"outcome": "upheld" | "rejected", "reasons": "..."}` answers `204`. An upheld report withdraws the item by moderation for the report's reason; a rejected one ends the switch-off or the hiding once no other report for the same reasons waits (`interface-v1.md`, Reports). The reasons go to the reporter (by e-mail through the notifier if they left an address, otherwise in `GET /v1/reports/{report}`) and to the author (`Library::author_notifications`, until accounts deliver them).
- `POST /api/moderation/appeals/{appeal}/decision` with `{"outcome": "maintained" | "reversed", "reasons": "..."}` answers `204`. It must come from another moderator than the one who decided the report. A reversal undoes the decision: an upheld report becomes rejected and the item comes back (unless another upheld report keeps it withdrawn), a rejected one becomes upheld and the item is withdrawn; the report's status and decision then read the appeal's. The reasons go to the author, and to the reporter who appealed and left an address.
- `POST /api/moderation/shares/{share}/decision` with `{"outcome": "published" | "refused", "reason": "<reason code>", "reasons": "..."}` answers `204`. `reason` is the rule the findings were about, one of the codes of the withdrawal list. `published` publishes the held share as version 1 of its item, at the decision's instant, under the author's pseudonym of that moment; `refused` refuses it, and the author reads, in their shares, the reason and the moderator's reasons as a refusal of the whole item (`{"question": null, "reason", "explanation": <reasons>}`). The decisions log counts a refusal as `upheld` (the findings hold) and a publication as `rejected`, so that the transparency figures count every decision that restricts content alike.
- `POST /api/moderation/comments/{comment}/decision` with `{"outcome": "published" | "refused", "reason": "<reason code>", "reasons": "..."}` answers `204`, as for a held share: `published` shows the comment on the item page from the decision's instant under the author's pseudonym of that moment; `refused` refuses it, and the author reads the reason and the reasons in `GET /api/account/comments`. Logged like a held share's decision.
- `POST /api/moderation/comment-reports/{report}/decision` with `{"outcome": "upheld" | "rejected", "reasons": "..."}` answers `204`. Upheld, the comment is removed; rejected, it shows again once no other report of a child's personal data, sexual or illegal content waits.
- Refusals: `400` `bad_request` (not exactly the expected object, an unknown outcome or reason), `400` `reasons_required` (empty reasons), `403` `same_moderator`, `404` `unknown_report`, `unknown_appeal`, `unknown_share` or `unknown_comment`, `409` `already_decided` (decided already, or closed by the author's correction), `503` `unavailable` (reports are not turned on, or the database fails).

Every decision enters the decisions log (table `moderation_decisions`, `case_kind` `report`, `held_review`, `held_comment` or `comment_report`): what it was on, its reason, outcome, reasons, moderator and instant.

## Selection

The operator's team marks Selection the shared items it read and chose (ADR-0002): `PUT /api/moderation/items/{item}/selection` sets the mark, `DELETE` the same path removes it, both with no body, answering `204`. Setting it again, or removing a mark that is not there, changes nothing and answers `204` too. The item must be one the catalogue shows, neither withdrawn nor hidden from new imports, else `404` `unknown_item`; a withdrawn item keeps its mark and has it again if it comes back. `Library::select` and `Library::unselect` do the same.

The library signs the index again at once: the item's entry has `selection` true (`interface-v1.md`, Index), as do its catalogue entry and item page (`catalogue-api.md`), and `GET /api/catalogue?selection=true` keeps only marked items. The mark is not a moderation decision: it does not enter the decisions log, and only who set it, and when, is kept. Items of the editorial account get it only by being chosen, like any other.

## Transparency figures

`GET /api/moderation/figures/{year}` answers the figures of the yearly transparency report for `year` (UTC), which name no item, reporter, author or moderator:

```json
{"year": 2026,
 "reports": [{"reason": "other", "received": 2}],
 "decisions": [{"reason": "other", "outcome": "rejected", "count": 2}],
 "appeals": [{"appellant": "author", "outcome": "reversed", "count": 1}]}
```

`reports` counts the reports of items and comments received in the year by reason; `decisions` the moderators' decisions on reports, held shares and held comments made in the year (not their appeals) by reason and outcome; `appeals` the appeals received in the year by appellant and where they stand (`waiting`, `maintained`, `reversed`). Each list is ordered by its codes. A year that is not a number answers `400` `bad_request`. `Library::transparency_figures` answers the same.

## HTTP

Every answer is `application/json` with `Cache-Control: private, no-store`; no answer carries an e-mail address or a Minute Jar account's subject (`tests/account_privacy.rs`).
