# Minute Jar public library

Instructions for coding agents working in this repository.

- Read `GLOSSARY.md` and `docs/adr/` before changing anything; the family server side and the interface contract live in the main repository `minute-jar/minute-jar` (`docs/specs/public-library.md`).
- Commit with `git commit --signoff`.
- Nothing is created on GitHub (repository, issue, pull request) without the maintainer's explicit consent.
- Never name a child, a family device or a private host anywhere.
- The maintainer writes in French; repository documents are in English.

## Agent skills

### Issue tracker

Issues live in GitHub Issues of `minute-jar/library`, managed with `gh`. See `docs/agents/issue-tracker.md`.

### Triage labels

The five default labels: `needs-triage`, `needs-info`, `ready-for-agent`, `ready-for-human`, `wontfix`. See `docs/agents/triage-labels.md`.

### Domain docs

Single-context: one `GLOSSARY.md` and `docs/adr/` at the repo root. See `docs/agents/domain.md`.
