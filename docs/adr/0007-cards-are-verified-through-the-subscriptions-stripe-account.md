---
status: accepted, pending Stripe's confirmation
---

# Cards are verified through the subscription's Stripe account

A library account's card (ADR-0001) is verified through Stripe, the payment provider of the subscription: the subscription's card when the account is linked to one, or a zero-amount check with 3-D Secure (a SetupIntent) for a parent without a subscription. The library keeps only Stripe's reference and the card's fingerprint, never the number; the same card may serve several accounts, and the operator knows it. The subscription is sold through Stripe Managed Payments, Stripe's merchant of record, so that taxes on international sales are handled for the operator. The service sees cards only through one interface (verify a card, get a reference and a fingerprint). We decided this because one provider lets the subscription's card stand for the library's check, and Stripe documents both a zero-amount check and a stable card fingerprint.

## Considered Options

- **Paddle**: kept as an alternative for selling the subscription (merchant of record, 5% + $0.50), but its documentation shows no zero-amount card check and no card fingerprint, so the library would need a second provider and the subscription's card could not stand for the check.
- **Polar.sh**: kept as an alternative (merchant of record, from 3.4% + $0.30); it can save a card, but whether that needs a purchase or yields a fingerprint is not documented.
- **Mollie or Adyen**: kept as alternatives for the card check (both document a zero-amount check and a card fingerprint), but neither is a merchant of record, so the operator would handle taxes on every international sale.
- **Lemon Squeezy**: rejected; it accepts no new sellers and closes on 31 March 2027.

## Consequences

- Stripe's documentation does not say whether a plain SetupIntent may run on an account that also sells through Managed Payments. Stripe must confirm it before the real adapter is written; if not, the subscription is sold through Stripe with Stripe Tax, the operator declaring taxes itself.
- Tests use a fake card verifier; no test calls Stripe.
