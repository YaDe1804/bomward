# Contributing

Thanks for considering a contribution.

## Quality gate

Run this before opening a pull request. All of it has to pass:

```powershell
cargo fmt --all --check
cargo clippy --all-targets --all-features --locked -- -D warnings
cargo nextest run --locked
cargo deny check
cargo audit
```

Build and test details are in [docs/DEVELOPMENT.md](docs/DEVELOPMENT.md).

## Commits

Use [Conventional Commits](https://www.conventionalcommits.org/):

- `feat(ingest): …` for a feature in that area
- `fix(diff): …` for a bug fix
- `docs(adr): …` for a decision record
- `chore(ci): …` for CI and tooling

A non-trivial design decision needs an ADR in `docs/adr/` (start from
`docs/adr/0000-template.md`) in the same change.

## Contributor license

bomward is AGPL-3.0-only and also offered under a commercial license.
A contribution can be merged only if you agree to [CLA.md](CLA.md).
That file is a draft pending legal review.

## What not to send

Do not add real malicious packages or malware samples as test data.
Write inert fixtures. Do not commit `_intern/`.
