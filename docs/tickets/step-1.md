# Step 1 of the library service: tickets

Step 1 of `docs/specs/library-service.md`, broken into vertical slices: each one goes from storage to what a caller sees and is tested through the service's HTTP interface (spec, Testing Decisions). **AFK** tickets can be built by an agent alone; **HITL** tickets need the maintainer (an account to open, content to read, a legal or provider answer). Every ticket is mirrored as a GitHub issue of `minute-jar/library`, under the parent issue "Step 1 of the library service", with the same blockers: ticket N is issue #N+1 (ticket 1 is #2), the parent is #1.

User stories are those of the library spec (S1 to S29).

## Contract with family servers

### 1. Signed index and shared items over HTTP (AFK)

The first slice. A Cargo workspace with `crates/library` (axum, sqlx on PostgreSQL, testcontainers in tests), the wire format of `docs/specs/interface-v1.md` and its fixtures in `fixtures/v1/`.

- A published version of a shared item is stored; the service serves `GET /v1/index`, `GET /v1/items/{item}/versions/{version}` and `GET /.well-known/minute-jar-library.json` as signed documents (ADR-0004), the same bytes for every caller, with an ETag and `304 Not Modified`.
- The keys document is signed by the root key and certifies the signing key; the index and items are signed by the signing key; the index carries an expiry.
- An unknown item or version answers `404` with a fixed error code.
- Tests: fixtures catalogue in, every response verified from the root key alone; byte-identical responses for two callers; ETag round trip; unknown item; time passed in as a value.

### 2. Signed withdrawal list (AFK)

Blocked by 1.

- `GET /v1/withdrawals`: every withdrawn item or version with its kind (author's choice, provisional switch-off, moderation decision), reason code and date; an item no longer switched off leaves the list.
- The withdrawal list and index are regenerated when the catalogue changes and re-signed on a schedule before they expire; a withdrawn item leaves the index.
- Tests: the three kinds, reactivation, expiry and refresh at given times.

### 3. Media by digest (AFK)

Blocked by 1.

- Media of a shared item stored in S3-compatible storage (a local directory in tests), listed in the item by SHA-256 digest, served at `GET /v1/media/{digest}` with immutable caching.
- Size limits per file and per item, as in the bank format.
- Tests: a picture fetched and its digest checked; an unknown digest; a file over the limit refused at publication.

### 4. Bank format validation (AFK, blocked outside)

Blocked by 1 and by the bank format's JSON Schema in the main repository.

- Every version is checked against the main repository's JSON Schema before review; origins that cannot be shared (generated from a document, made by a child) are refused with the reason.

### 5. Key ceremony and rotation (HITL)

Blocked by 1.

- A command that generates a root key offline, one that certifies a signing key until a date, and the service loading its signing key and certificate from its secret store.
- The maintainer generates and keeps the official root key.

### 6. Propose the wire format to the main repository (HITL)

Blocked by 2 and 3.

- `docs/specs/interface-v1.md` and the fixtures proposed to `minute-jar/minute-jar` as version 1 of `docs/specs/public-library.md`'s contract, so that the library client is tested against the same fixtures.

## Catalogue and website

### 7. Website skeleton with catalogue browsing and search (AFK)

Blocked by 1. Stories S1, S2, S5.

- `web/` in SvelteKit with server-side rendering, TypeScript, Tailwind, shadcn-svelte and Paraglide (English and French), pnpm, Vitest and Playwright (ADR-0003).
- A catalogue JSON API in the Rust service: filters by language, subject, age range and curriculum, full-text search.
- Catalogue and search pages, anonymous, without trackers.

### 8. Item page and download as a file (AFK)

Blocked by 7. Stories S3, S4, S5.

- The item page: questions and lessons, the author's pseudonym, licence, sources, versions, ratings, Selection mark, editorial and AI-generated marks.
- Download the signed item as a file that a family server without Internet access can import.

## Accounts

### 9. Sign-in with a Minute Jar account and library profile (AFK)

Blocked by 7. Stories S6, S9.

- The service is an OpenID Connect client of the identity service (ADR-0006); tests use a fake provider.
- Opening a library account: pseudonym (changeable), adult and parent statement, the choice screen between the subscription's Minute Jar account and a separate one, saying what the operator knows and what others see; recent strong sign-in required.
- Real name and e-mail never shown; no route answers with them.

### 10. Card verification (AFK, adapter HITL)

Blocked by 9. Stories S7, S8.

- A card verifier interface (reference and fingerprint); a fake in tests; the subscription's card used when the account is linked.
- The Stripe adapter waits for Stripe's confirmation (ADR-0007).

### 11. Close a library account (AFK)

Blocked by 9. Story S12.

- Items stay under their licence with the pseudonym; identification data kept for the legal period, then erased.

## Sharing and review

### 12. Share an item: sharing terms, review, outcome (AFK)

Blocked by 1, 9 and 10. Stories S10, S13, S16.

- Sharing terms accepted at the first share; the parent app sends a prepared item with the account's session, never a library identifier.
- The review interface (ADR-0005) returns published, held for a moderator, or refused with reasons per question; tests use a fake reviewer.
- A published item enters the index; a quota per account and day, explained when reached.

### 13. Review adapter for Scaleway's Generative APIs (HITL)

Blocked by 12.

- The first real reviewer adapter, configured outside the repository; the maintainer opens the account and writes the private instructions.

### 14. New versions and derived works (AFK)

Blocked by 12. Stories S14, S15.

- Every new version reviewed again; a derived work keeps the original's attribution and links to it, in the item and the index.

### 15. Author's withdrawal (AFK)

Blocked by 2 and 12. Story S17.

- An author withdraws an item; it leaves the index and enters the withdrawal list with the author's kind.

## Families' anonymous calls

### 16. Imports under the library identifier and the families signal (AFK)

Blocked by 1.

- A family server reports an import under its library identifier; the index publishes the number of families using each item from distinct identifiers.
- Technical logs of anonymous calls kept briefly and never joined to identifiers; no route joins an identifier to an account.

### 17. Ratings and short reviews (AFK)

Blocked by 16. Story S27.

- One rating per library identifier, only for an item it imported; ratings in the index and on the item page.

### 18. Suggestions to authors (AFK)

Blocked by 12 and 16. Story S18.

- Anonymous suggestions of answers to accept, under the library identifier; the author accepts them into a new version.

## Reports and moderation

### 19. Reports and their three paths (AFK)

Blocked by 2 and 16. Stories S20, S21, S23, S24, S25.

- Reasons: rights, personal data, unsuitable content, wrong answer, other; from the website or under a library identifier.
- A child's personal data or illegal content: immediate review on that reason, switch-off when plausible (withdrawal list), otherwise hidden from new imports; a rejected report reactivates the item by itself.
- Rights: hidden after three distinct reporters or at once on a formal notice form; wrong answer: sent to the author, then to a moderator after 7 days.
- Acknowledgement and decision sent to the reporter.

### 20. Moderation queue and decisions log (AFK)

Blocked by 7, 9 and 19. Stories S22, S26.

- Administration pages for moderators (role given by the operator, passkey sign-in): one queue with review findings, reports and history; decisions with reasons to author and reporter; appeal; the decisions log for the yearly transparency report.

## Comments and profiles

### 21. Comments from library accounts (AFK)

Blocked by 12. Stories S28, S29.

- Comments under the pseudonym, reviewed before they show; no private messages anywhere.

### 22. Author profiles (AFK)

Blocked by 16 and 18. Story S19.

- The author's items, families using them and suggestions received.

## Operator content and operation

### 23. Editorial account and Selection mark (AFK)

Blocked by 12. ADR-0002.

- The editorial account marked as the software's publisher; items marked AI-generated and read by a person; the Selection mark set by the operator's team.

### 24. Seed English and French content (HITL)

Blocked by 23.

- Banks and paths generated after US Common Core, England's National Curriculum and Éducation nationale, reviewed and read by a person per bank.

### 25. Deployment in the EU (HITL)

Blocked by 1 and 5.

- OCI images for the service and the website, configuration by environment, PostgreSQL and S3 storage at the chosen EU host; the maintainer opens the accounts.

### 26. Retention of identification data (HITL)

Blocked by 9.

- Identification data of library accounts kept one year after each share, anonymous technical logs kept briefly; the period confirmed by legal counsel.
