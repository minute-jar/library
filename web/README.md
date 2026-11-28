# Website

The public library's website (ADR-0003): SvelteKit rendered on the server, reading the library service's catalogue API (`docs/specs/catalogue-api.md`) from `LIBRARY_API` (`http://127.0.0.1:8080` by default). The browser never calls the API, loads nothing from another host and runs no tracker. English and French through Paraglide (`messages/`), the language in the address (`/fr/...`).

```sh
corepack pnpm install
corepack pnpm dev            # http://localhost:5173, against a running service
corepack pnpm lint           # Prettier and ESLint
corepack pnpm check          # svelte-check
corepack pnpm test:unit      # Vitest: the server load functions against a fake API
corepack pnpm test:e2e       # Playwright smoke test, built site over e2e/fake-api.ts
```

`corepack pnpm exec playwright install chromium` once before the smoke test.
