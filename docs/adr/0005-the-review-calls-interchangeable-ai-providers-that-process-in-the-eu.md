---
status: accepted
---

# The review calls interchangeable AI providers that process in the European Union

The service knows the review only through one interface: a shared item's version or a comment goes in, and **published**, **held for a moderator** with findings, or **refused** with reasons per question comes out. Behind it, adapters call AI providers chosen by configuration, one or several (for instance a second model answering every question to cross-check wrong answers). Every provider must run the call and process the data in the European Union, and keep and train on nothing. The first adapter is Scaleway's Generative APIs, next to the hosting. We decided this because the review is what lets the library show other people's content to children, models and prices change quickly, and the operator must be able to switch or combine providers without touching the rest of the service.

## Considered Options

- **One provider written into the service**: rejected. A price change, an outage or a better model would mean a rewrite.
- **A provider's API outside the EU**: rejected. Shared content may still hold a child's personal data until the review finds it.

## Consequences

- Tests use a fake review provider; no test calls a provider.
- The models chosen and their detailed instructions are operating configuration, outside this repository (spec).
- Other adapters (Mistral AI, a model reached through an EU region of a cloud provider) can be added as configuration needs them.
