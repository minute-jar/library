# Minute Jar public library

This glossary fixes the words used in the public library, its documentation and its code. It extends the glossary of the main Minute Jar repository, whose terms (Family, Parent, Child, Family Server, Question Bank, Learning Path, Parent App, Assistant, Subscription, Hosted Mode, Self-Hosted Mode) keep their meaning here. The French word shown in the product is given next to each term.

## Language

### Content

**Public Library** (FR: Bibliothèque publique):
The catalogue of question banks and learning paths that parents share with every family under a free licence, run by the operator apart from any family server.
_Avoid_: Store, marketplace, hub, catalogue, community library

**Family Library** (FR: Bibliothèque de la famille):
The question banks and learning paths a family keeps on its family server, into which it imports from the public library.
_Avoid_: Private library, my content, collection

**Shared Item** (FR: Élément partagé):
A question bank or a learning path in the public library, at one version.
_Avoid_: Publication, listing, post, item, resource

**Derived Work** (FR: Œuvre dérivée):
A shared item made by changing another one, which keeps the original's attribution and links to it.
_Avoid_: Fork, remix, copy

**Selection** (FR: Sélection):
The mark on a shared item that the operator's team has read and chosen.
_Avoid_: Featured, verified, staff pick, certified

**Editorial Account** (FR: Compte éditorial):
The library account under which the operator shares the content it makes itself, marked as the software's publisher.
_Avoid_: Official author, Minute Jar team, admin account

**Rating** (FR: Note):
From 1 to 5 stars given to a shared item under a library identifier that imported it, one per identifier, the last one counting.
_Avoid_: Score, vote, like

**Short Review** (FR: Avis court):
The few words a family may leave with its rating, held until the review publishes it; not to be confused with the review itself.
_Avoid_: Comment (from a library account), testimonial, feedback

### People

**Minute Jar Account** (FR: Compte Minute Jar):
A person's sign-in with the operator, one per e-mail address, used for the public library, for hosted mode, or for both, as the person chooses.
_Avoid_: User, login, operator account, ProlificLab account

**Library Account** (FR: Compte bibliothèque):
A parent's one public identity in the public library, opened on a Minute Jar account and shown only as a pseudonym, needed to share, comment or carry a badge.
_Avoid_: Author account, profile, user, publisher account

**Library Identifier** (FR: Identifiant de bibliothèque):
The random identifier a family server draws for the public library alone, under which it imports, rates, reports and contributes, never joined to a Minute Jar account.
_Avoid_: Family ID, client ID, device ID, installation ID

**Author** (FR: Auteur):
A parent who has shared an item from their library account and answers for it.
_Avoid_: Publisher, creator, contributor, uploader

**Sharing Terms** (FR: Conditions de partage):
What a parent accepts at their first share: they hold the rights, the content is compliant, they answer for it, and it is licensed CC BY-SA 4.0.
_Avoid_: Terms of service, EULA, publishing agreement

**Contributing Family** (FR: Famille contributrice):
A family whose family server sends the public library noised totals of how its shared items are used, by the parent's choice.
_Avoid_: Contributor, donor, data sharer, panel

**Contributor** (FR: Contributeur):
A hosted family that, over a year, has contributed usage totals or useful content, and so earns a discount and, if its library account is linked, a public badge.
_Avoid_: Partner, ambassador, premium member

### Moderation

**Review** (FR: Examen):
The automatic check, by AI, of every new version of a shared item or comment before it is public, whose outcome is published, held for a moderator, or refused with reasons.
_Avoid_: Scan, validation, approval, audit

**Report** (FR: Signalement):
A parent's notice that a shared item or comment breaks the rules, with a reason.
_Avoid_: Flag, complaint, ticket, abuse report

**Switch-Off** (FR: Désactivation):
The state of a shared item that no child of any family may use, until a moderator's decision, because a report of a child's personal data or illegal content was found plausible.
_Avoid_: Takedown, ban, quarantine, block

**Withdrawal** (FR: Retrait):
The end of a shared item's presence in the catalogue, by its author's choice or by a moderation decision.
_Avoid_: Deletion, removal, unpublish

**Index** (FR: Index):
The signed document listing every shared item with its metadata and signals, the same for every family server, from which each computes its own recommendations.
_Avoid_: Feed, manifest, dump, catalogue export

**Signed Document** (FR: Document signé):
The index, a shared item at one version, or the withdrawal list, as a library serves it: the exact JSON text and its signature by a signing key, the same for every caller.
_Avoid_: Envelope, token, JWS, signed payload

**Root Key** (FR: Clé racine):
The library's long-lived key, kept offline, whose public half a family server trusts and which certifies the signing keys.
_Avoid_: Master key, CA, publisher key

**Signing Key** (FR: Clé de signature):
A key the library holds online to sign its signed documents, valid until a date set in its certificate by the root key.
_Avoid_: Server key, online key, API key
