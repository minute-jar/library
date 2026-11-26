---
status: accepted
---

# Minute Jar accounts are one identity service for the library and for hosting

A person signs in with the operator through one kind of account, the Minute Jar account, one per e-mail address, held by an identity service the operator runs: Ory Kratos for the accounts and sign-in, Ory Hydra as the OpenID Connect provider, on PostgreSQL in the EU. The library and the hosting console are OpenID Connect clients of it. A person may use one account for both the library and hosted mode, or sign in to the library with another account (another e-mail address) to keep them apart, or have no account and browse and import anonymously. Sign-in offers, in this order: a passkey, Google, Apple, Microsoft, a one-time code by e-mail, and a password with a TOTP second factor; from Google, Apple and Microsoft only the verified e-mail is asked, never the name or picture. Sharing, linking a subscription, moderating and changing how one signs in need a recent strong sign-in: a passkey, an external provider, or a password with TOTP; moderators use a passkey. We decided this because most parents expect to sign in with what they already have on Android or iOS, in one tap, and because sign-in, recovery and attack protection are code where mistakes are costly and should not be written twice.

## Considered Options

- **Separate account systems in the library and the hosting console**: rejected. Twice the code and two sign-ins for the same parent.
- **Several separate accounts on one e-mail address**: rejected. The operator could join them in one query, and sign-in by e-mail would become ambiguous; a person wanting separation uses an alias or Apple's relay address.
- **Building the identity provider in Rust**: rejected. The Rust OpenID Connect crate is a client only; the provider, passkeys and recovery would be ours to secure.
- **Zitadel**: rejected. Complete, but AGPL-3.0 with a contributor agreement since 2025, and custom sign-in screens go through its session API.
- **Logto**: rejected. MPL-2.0 and it sends magic links, but self-hosted custom server-rendered screens are not documented and it asks for 8 GiB of memory.
- **Keycloak, Authentik, Kanidm**: rejected. Keycloak is heavy and lacks Apple and magic links; Authentik documents no headless mode and keeps part of its code under an enterprise licence; Kanidm has no Google or Apple sign-in and no PostgreSQL.
- **Facebook sign-in**: rejected for now; it sends the wrong signal for a product that protects children's data.

## Consequences

- The sign-in screens are ours, in SvelteKit with shadcn-svelte, driving Kratos's flows; e-mail sign-in is a six-digit code rather than a link, which also works when the mail is opened on another device.
- The identity service, its screens and the hosting console live in a separate repository (`minute-jar/hosting`, to be created with the maintainer's consent). This library only needs an OpenID Connect provider, so another library can use any.
- A library account is a profile opened on a Minute Jar account at the first share or comment, with a pseudonym that can be changed. The library identifier of a family server is never sent with an account's session.
- Using one account for the library and hosted mode joins the pseudonym to the subscription by the person's choice. The main repository's ADR-0027, which speaks only of a later link, must be amended to say so; the choice screen at the first share says what the operator knows and what others see.
- In the main repository, a hosted family server's parent app signs in with the Minute Jar account; a self-hosted family server keeps its local parent accounts, with sign-in by Minute Jar account off by default.
- Sending to Apple's relay addresses needs the operator's sending domain registered with Apple.
