---
status: accepted
---

# A Rust service and a SvelteKit website over PostgreSQL, hosted in the European Union

The library is one repository with two deliverables. `crates/library`, in Rust with axum, holds everything the library decides: the interface contract with family servers (signed index, shared items, withdrawals), library accounts, the review, reports and moderation, ratings and comments, and a JSON API. `web/`, in SvelteKit rendered on the server with TypeScript, Tailwind and shadcn-svelte, renders the public website and the administration pages (moderation queue, decisions log) and talks only to that API. Data live in PostgreSQL, reached through sqlx with versioned migrations. Both deliverables ship as OCI images to a host in the European Union that answers only to European law (Scaleway or OVHcloud), with objects in S3-compatible storage. We decided this because the family server is Rust, so the signatures and the bank format are checked by the same kind of code on both sides of the contract, while parents and moderators need a fast, well-indexed website built with the operator's usual web tools.

## Considered Options

- **TypeScript end to end in SvelteKit**: rejected. One language, but signing and the bank format would be written a second time apart from the family server's code.
- **SQLite with continuous replication**: rejected. Simpler to test, but the library is a central service that may need several instances, and moving later costs more than starting on PostgreSQL.
- **A single-page application**: rejected. Search engines must index the catalogue, and anonymous browsing should need no script beyond what a page uses.
- **An AWS, Google Cloud or Azure region in the EU**: rejected. Those providers answer to the US CLOUD Act, which a service about children's learning should avoid.
- **Moderation in an external helpdesk tool**: rejected. Reports would be copied outside the service and the DSA decisions log split in two.

## Consequences

- The code depends on no provider-specific service: PostgreSQL, the S3 API and environment variables, so the host can change, and the same images fit the operator's per-family hosting.
- Tests start a throwaway PostgreSQL with testcontainers, one database per test, or use `DATABASE_URL` when set (for CI). Docker is needed to run the Rust tests locally.
- Front-end tools: pnpm, Paraglide for translations (English and French from the start), Vitest and Playwright, ESLint and Prettier.
- The signed documents are the same for every caller and served as static, cacheable responses; a CDN (for instance Bunny.net, in the EU) can come later without changing the contract, since it cannot alter what is signed.
- The administration pages are built after the parts families need first.
