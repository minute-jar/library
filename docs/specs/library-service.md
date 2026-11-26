---
title: The public library service
status: draft
---

# The public library service

## Problem Statement

Minute Jar lets a parent build learning content for their children (main repository, `docs/specs/questionnaires-and-learning.md`), but every family starts alone, and a non-technical parent will not write a hundred questions before seeing whether questionnaires work at home. Families need a catalogue of good banks and learning paths, by age, language and school year, that they can trust for their children; parents who made something good need to share it in a few clicks, under their responsibility, without revealing who their family is.

The family server and parent app side, and the interface between a family server and a library, are specified in the main repository (`docs/specs/public-library.md`, ADR-0026, ADR-0027). This spec covers the service the operator runs.

## Solution

One web service, run by ProlificLab in the European Union, that:

- serves a public **website** where anyone browses and searches the catalogue without an account;
- serves family servers the **signed index**, signed **shared items** and the signed **withdrawal list** of the interface contract, the same to every caller, and accepts anonymous imports, ratings, reports, suggestions and, in step 2, contributions;
- manages **library accounts**: verified e-mail and card, adult and parent statement, pseudonym, sharing terms (ADR-0001);
- runs the **AI review** of every new version and every comment before it is public;
- runs **reports and moderation** following the DSA;
- shows **author profiles**, **ratings** and **comments**;
- runs the **Contributor programme** with billing (step 2);
- hosts the operator's **editorial account** and its seeded content (ADR-0002).

The parent sees three levels, each in one sentence, at the moment they need it:

1. **Anonymous by default**: "Browse and import freely. The library does not know who you are."
2. **A pseudonym to share or comment**: "To share or comment, choose a pseudonym. Your real name never appears."
3. **One card, Help the library**: "Share anonymous statistics or useful content, and save 10% on your subscription."

Words such as "random identifier" or "noised totals" appear only behind "Learn more".

## User Stories

### Browsing

1. As a parent, I want to browse and search the catalogue on the web without an account, so that I can see what exists before installing anything.
2. As a parent, I want to filter by language, subject, age range and curriculum (country, school year, topic), so that "fractions, year 4" takes one step.
3. As a parent, I want each item's page to show its questions and lessons, author's pseudonym, licence, sources, versions, ratings, Selection mark and, once enough families contribute, its usage, so that I judge it before importing.
4. As a parent, I want to download an item as a file, so that a family server without Internet access can still import it.
5. As a parent, I want items made by the operator marked as such, AI-generated and read by a person, so that I know where they come from.

### Library accounts

6. As a parent, I want to open a library account only when I first share or comment, with a pseudonym, a verified e-mail and a verified card, so that I stay anonymous until then.
7. As a subscriber, I want my subscription's card to serve as the verified card, so that I do not enter it again.
8. As a parent without a subscription, I want a zero-amount card check, so that I can share without paying.
9. As a parent, I want my real name, e-mail and card never shown, so that sharing does not expose my family.
10. As a parent, I want the sharing terms in plain words before my first share, so that I commit knowingly.
11. As a parent, I want to link my library account to my subscription in one click, with a screen showing what the operator knows and what others see, so that I can earn the Contributor discount as an author and show the badge.
12. As a parent, I want to close my library account, my items staying under their licence with my pseudonym, so that I can leave.

### Sharing and review

13. As a parent, I want to receive within minutes whether my item was published, held for a moderator or refused, with reasons per question, so that I can correct it.
14. As a parent, I want every new version reviewed the same way, so that an update cannot slip something in.
15. As a parent, I want to share a derived work of someone else's item, with its attribution kept, so that improving content is encouraged.
16. As a parent, I want a cap on shares per day explained when I reach it, so that I understand the limit.
17. As an author, I want to withdraw my item, so that I can stop sharing it.
18. As an author, I want to receive anonymous suggestions of answers to accept and turn them into a new version, so that my item improves.
19. As an author, I want a profile with my items, how many families use them and the suggestions received, so that I see my impact.

### Reports and moderation

20. As a parent, I want to report an item or a comment, from the website or the parent app, for rights, personal data, unsuitable content, a wrong answer or another reason, so that problems are fixed.
21. As a parent who reported, I want an acknowledgement and the decision with its reasons, so that I know it was taken seriously.
22. As an author, I want the reasons of any decision on my item and a way to appeal, so that I am treated fairly.
23. As a parent, I want an item reported for a child's personal data or illegal content switched off in every family within the hour if the report holds up, so that harm stops.
24. As an author, I want an item switched off wrongly to come back by itself when a moderator rejects the report, so that one malicious report costs nothing lasting.
25. As a rights holder, I want a formal notice form, so that protected content is hidden at once.
26. As a moderator, I want one queue with the AI review's findings, the reports and their history, so that I decide quickly and consistently.

### Ratings and comments

27. As a parent, I want to rate an item and leave a short review once my children have used it, without an account, so that ratings come from real use.
28. As a parent with a library account, I want to comment under my pseudonym, reviewed before it shows, so that discussion stays safe.
29. As a parent, I want no private messages between accounts, so that strangers cannot reach parents of children through the library.

### Helping the library (step 2)

30. As the operator, I want weekly noised totals from contributing families, published per item above a threshold of families, so that every parent sees usage, pass rates, most-missed questions and co-usage.
31. As a hosted family, I want to become a Contributor for the year through data or through content, and earn a discount, so that helping is worth it.
32. As a parent whose library account is linked, I want a public Contributor badge on my pseudonym, so that my help is recognised where I interact.
33. As the operator, I want a yearly public thank-you with the number of contributing families, naming nobody, so that contribution is visible.

## Implementation Decisions

### Decisions this spec rests on

- Main repository: ADR-0026 (a separate service of the operator, open code here, interface contract there, no bulk export), ADR-0027 (the library learns nothing of a family but the noised totals it chooses to send), ADR-0003 (banks from school courses stay private), ADR-0018 and ADR-0022 (hosted mode and its entitlements).
- This repository: ADR-0001 (CC BY-SA 4.0 under an identified author), ADR-0002 (editorial account).

### Identity and data

- Two identities that the service never joins: the **random library identifier** of a family server (anonymous calls: import, version checks, ratings, reports, suggestions, contributions), and the **library account** (sharing, comments, badges). A share never carries a library identifier.
- Ratings count once per library identifier and only for an item the identifier has imported.
- Identification data of library accounts (e-mail, card check reference, sharing log with addresses) are kept for one year after each share, as French law asks of hosting providers (**to be confirmed by legal counsel**); technical logs of anonymous calls are kept briefly and are not joined to identifiers.
- Hosted in the European Union; the data protection impact assessment of hosted mode (main repository, ADR-0018) covers the library.

### AI review

- Runs on every new version of a shared item and every comment before it is public, on the whole content, in the item's language.
- Checks: personal data (names, addresses, schools, faces in pictures); content that looks like textbook excerpts or other protected works; content unsuitable for the stated age, hateful or illegal; wrong answers, by answering every question independently and comparing.
- Outcome: **published**, **held for a moderator** with the findings, or **refused** with reasons per question.
- Models are called from the European Union. The operator bears the cost, bounded by a sharing quota per account and day.
- The AI model and its detailed instructions are operating configuration, kept out of this repository.

### Reports and moderation

- Reasons: rights, personal data, unsuitable content, wrong answer, other. A child never reports to the library.
- **A child's personal data, sexual or illegal content**: the item is reviewed again by AI at once on that reason. If the report is plausible, the item is **switched off**, listed in the withdrawal list so that every family server stops using it within the hour. Otherwise it is hidden from new imports. A moderator decides within 48 hours; a rejected report puts the item back by itself. A report from a verified library account or from several families goes straight to switch-off.
- **Rights**: hidden from new imports after reports from three distinct library identifiers (each having imported the item, or from library accounts), or at once on a formal notice from a rights holder with their identity.
- **Wrong or unsuitable answer**: sent to the author first, then to a moderator if nothing changes within 7 days; never hidden automatically.
- A moderation decision that removes an item is a **withdrawal** of the moderation kind: family servers switch their copies off and tell the parents the reason.
- DSA process: acknowledgement, decision with reasons to the author and the reporter, internal appeal, yearly transparency report. Reporters whose reports are often rejected weigh less.
- Moderators are the operator's team at first.

### Signals

- Number of families using an item (from distinct library identifiers), ratings and short reviews after use, errors reported and corrected, the Selection mark, the date of the last version; in step 2, usage, pass rates, most-missed questions and co-usage from contributions, published above a threshold (for instance 20 families).
- No data about any child, even aggregated, outside contributions.

### Contributor programme (step 2)

- A hosted family is a Contributor for the year through **data** (the contribution switch on for at least 10 months of 12, read by billing from the hosted family server, never from the library's data) or through **content** (for a library account linked to the subscription: items used by at least 50 families, suggestions accepted by authors, or a Selection mark).
- Reward: a yearly discount on the hosted subscription (for instance 10%, not cumulative) and, for a linked account, a public badge.
- Self-hosted subscribers cannot be verified without linking them; their discount waits for anonymous tokens.
- The scheme needs a legal opinion before it opens: consent must stay free (EDPB guidelines 05/2020), and the data concern children's activity.

### Editorial account and seeding

- Content generated by AI after official curricula, never from textbooks; reviewed, then read by a person per bank; marked AI-generated and Selection.
- English (US Common Core for mathematics and English language arts, England's National Curriculum) and French (Éducation nationale, cycles 2 to 4) first; then Spanish, German, Brazilian Portuguese, Italian, then Japanese, simplified Chinese and Arabic. A language opens to sharing only once it can be moderated: AI review and a moderator who reads it, or a provider.
- The website, the sharing terms, review reasons and moderation messages are multilingual from the start.

### Entitlements

- The service checks nothing about subscriptions for browsing. In hosted mode, the family server enforces the questionnaires entitlement before importing or sharing (main repository, ADR-0022).

## Testing Decisions

- A good test drives the service through its HTTP interface and checks what a caller sees, with time, the AI reviewer and billing passed in as fakes.
- The interface contract's signed fixtures from the main repository run against this service.
- Cases: account creation and its checks; a share published, held and refused; a new version re-reviewed; derived works keeping attribution; the three report paths and their thresholds; switch-off and automatic reactivation; the formal notice; withdrawals in the signed list; ratings once per identifier and only after import; comments held by review; no request joining a library identifier to an account; contributions below and above the publication threshold.

## Out of Scope

- The family server and parent app side (main repository).
- Paying authors; subscription levels and how the discount applies to them.
- Private messages between accounts.
- A bulk export of the catalogue.

## Further Notes

- Step 1: website, index, items, withdrawals, accounts, sharing terms, AI review, moderation, ratings, comments, profiles, editorial account, seeding in English and French. Step 2: contributions and their results, the Contributor programme, further languages.
- The technical stack is to be chosen when development starts; the interface contract and the bank format's JSON Schema come from the main repository.
