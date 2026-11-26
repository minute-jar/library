---
status: accepted
---

# Signed documents carry their exact JSON text, signed by an online key a root key certifies

Every signed document a library serves (the index, a shared item at one version, the withdrawal list) travels as `{"payload": "<JSON text>", "signature": "<hex>", "key": "<key id>"}`: the signature is Ed25519 over the exact bytes of the payload text, as the family server already does for its signed rules. Each payload names its contract version, its kind and its issue time, and the index and the withdrawal list carry an expiry, so that an old copy cannot be served again as current. The documents are signed by a signing key the library holds online; a root key kept offline certifies the signing keys, each until a date, in a keys document served at `/.well-known/minute-jar-library.json`. A family server trusts only the root key's public half, set with the library's address (built in for the official library). Media are not signed one by one: a shared item lists each by its SHA-256 digest. We decided this because the withdrawal list must be re-signed every hour and within the hour of a switch-off, which needs a key online, while a stolen online key must be replaceable without updating every family server.

## Considered Options

- **JWS or COSE**: rejected. Standard formats, but more options and pitfalls than one envelope, for nothing the family server needs.
- **Detached minisign signatures**: rejected. A second mechanism beside the family server's.
- **Signing a canonical form of the JSON**: rejected. Canonicalisation is a source of mismatches; signing the exact text needs none.
- **One online key built into family servers**: rejected. Simpler, but a compromise would need an update of every family server. No family server uses a library yet, so the two levels cost nothing to adopt now.

## Consequences

- The documents are generated when the catalogue changes and refreshed on a schedule, stored, and served with an ETag; they are byte-identical for every caller.
- The wire format, routes and fixtures are written in `docs/specs/interface-v1.md` and `fixtures/v1/`, and proposed to the main repository as version 1 of its interface contract (`docs/specs/public-library.md` there).
- Test keys in the fixtures are public and marked as such; the official root key is generated and kept offline by the operator, never in this repository.
