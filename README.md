# Minute Jar public library

The public library is where parents share question banks and learning paths for [Minute Jar](https://github.com/minute-jar/minute-jar) with other families, under a free licence. ProlificLab runs the official library; this repository holds its code, open under MPL-2.0 so that parents and contributors can see what it does with what a family sends it.

What lives here:

- the website and catalogue, library accounts and sharing terms;
- the AI review of every shared version, reports and moderation;
- ratings, comments, author profiles and the Contributor programme;
- the signed index, shared items and withdrawal list that family servers fetch.

What does not live here:

- The family server and parent app side, and the **interface** between a family server and a library: they are in the main repository (`docs/specs/public-library.md` and ADR-0026, ADR-0027 there). This service implements that interface; any other library may too.
- The catalogue itself, the operator's keys, the choice of AI model and its detailed moderation instructions, which ProlificLab keeps to itself.

Start with [GLOSSARY.md](GLOSSARY.md), the [decision records](docs/adr/) and the [service spec](docs/specs/library-service.md).

## Development

The service is in Rust (`crates/library`, ADR-0003). Its tests start a throwaway PostgreSQL with Docker, or use the database named by `DATABASE_URL`:

```
cargo test
cargo clippy --all-targets
```

The interface with family servers is `docs/specs/interface-v1.md`, with its fixtures in `fixtures/v1/`. Work is tracked in [GitHub issues](https://github.com/minute-jar/library/issues), broken down in `docs/tickets/step-1.md`.

## Licence

Code: [MPL-2.0](LICENSE), contributions signed off under the [DCO](CONTRIBUTING.md). Shared content: CC BY-SA 4.0 (ADR-0001).
