# Fixtures of interface v1

Signed documents of `docs/specs/interface-v1.md`, for a family server's library client and for any library implementing the interface.

- `served/`: what this service serves for a small catalogue, byte for byte, at the path given in `manifest.json`. Each signed document verifies from the root public key in `manifest.json` at its `checked_at` instant; each file of `served/media/` has the SHA-256 that names it, as version 2 of the verbs bank lists it.
- `refused/`: documents a family server must refuse when asked for the kind given, with the refusal named in `manifest.json` (`Forged`, `Expired`, `UnknownKey`, `WrongKind`, `UnknownContract`).

The keys are **public test keys**, never used outside tests: the root key's Ed25519 seed is 32 bytes of `0x01`, the signing key's 32 bytes of `0x02`, an uncertified key's 32 bytes of `0x03`.

`crates/library/tests/fixtures.rs` checks that the service still serves these bytes; `UPDATE_FIXTURES=1 cargo test --test fixtures` writes them again after a deliberate change of the contract.
