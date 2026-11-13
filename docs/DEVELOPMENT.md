# Development

Public product contract: [SPEC.md](SPEC.md). Decisions: [adr/](adr/).

## Layout

| Path | Role |
| --- | --- |
| `bomward-core` | Library. `ingest`, `model`, `diff`, `policy`, `report`. |
| `bomward-cli` | Binary `bomward`. |
| `docs/adr` | Architecture decision records. |
| `.github/workflows/ci.yml` | Format, Clippy, tests, MSRV, `cargo-deny`, `cargo-audit`. |
| `deny.toml` | License, advisory, and source policy for this repository. |

Data flow: `ingest` → `model` → `diff` → `policy` → `report`.

## Toolchain

- Edition 2024. `rust-toolchain.toml` pins an exact stable release, currently
  **1.100.0**, with `rustfmt` and `clippy`. It does not say `stable`.
- Local commands and the fmt, clippy, test, deny, and audit jobs follow that
  file. A new Rust release does not change them until the pin moves.
- Dependabot (`package-ecosystem: rust-toolchain` in
  `.github/dependabot.yml`) opens a pull request for a newer versioned
  channel. Merging that pull request is the bump. It does not change the
  MSRV.
- `cargo-nextest`, `cargo-deny`, and `cargo-audit` on the PATH for the gate.

## MSRV

The MSRV is `workspace.package.rust-version` in the root `Cargo.toml`.
It is **1.99**, the stable release this foundation was built with
(`rustc 1.99.0`, 2026-09-28).

- Code and dependencies build and pass tests on that version. Do not use
  newer language features or newer standard-library APIs.
- The `msrv` job reads `rust-version` from `Cargo.toml` and installs that
  toolchain. It does not use the pin in `rust-toolchain.toml`. It runs
  `cargo build --locked --all-targets` and `cargo nextest run --locked`.
  It does not keep a second copy of the number.
- Raising the MSRV is a deliberate change of `rust-version` in the same
  commit as this section: a dependency's floor, or an edition feature the
  project chooses to require. Moving the pinned toolchain is not an MSRV
  bump.
- Do not claim a lower MSRV than the dependency closure allows.

Crate versions are taken from crates.io via `cargo add`, not from memory.
The foundation closure is:

| Crate | Version | Where | Why |
| --- | --- | --- | --- |
| `thiserror` | 2.0.21 | `bomward-core` | Typed library errors. |
| `anyhow` | 1.0.104 | `bomward-cli` | CLI error reporting only. |
| `clap` | 4.6.7 | `bomward-cli` | Command and flag parsing (`derive`). |
| `noyalib` | 0.0.60 | `bomward-core` | Policy YAML. Exact pin `=0.0.60`. See ADR 0014. |
| `spdx` | 0.13.6 | `bomward-core` | SPDX license ids inside policy expressions. Default features. |
| `comrak` | 0.59.0 | `bomward-core` dev | Parses Markdown reports in tests. Default features off. |
| `jsonschema` | 0.61.2 | `bomward-core` dev | Validates report JSON against `docs/report.schema.json`. Default features off. |

`bomward-cli` also depends on `bomward-core` by path. Versions are locked in
`Cargo.lock`. Add a crate with `cargo add` and update this table in the same
change.

## Build

```powershell
cargo build --locked
cargo run -p bomward-cli -- --help
cargo run -p bomward-cli -- diff old.json new.json
```

The last command reads `old.json` and `new.json`. A missing file exits 2
and names which argument failed. A pair of CycloneDX documents exits 0
when no policy is passed.

## Quality gate

Run this before every commit. Continue only when every command exits 0.

```powershell
cargo fmt --all --check
cargo clippy --all-targets --all-features --locked -- -D warnings
cargo nextest run --locked
cargo deny check
cargo audit
```

`cargo nextest run` is locked so CI and local runs use `Cargo.lock`.
`cargo deny check` warns on duplicate crate versions and fails on
advisories, yanked crates, forbidden licenses, wildcard requirements,
and non-crates.io sources.

## Tests

- Unit tests live next to the module they cover.
- Golden files live under that crate's `tests/fixtures/` and are compared
  byte for byte. Ingest fixtures are in `bomward-core/tests/fixtures/`.
  Line endings in the repository are LF (`.gitattributes`).
- CLI tests are integration tests in `bomward-cli/tests`. They spawn the
  `bomward` binary.
- Test helpers return `Result`. They do not call `unwrap` or `expect`.
  `assert!` / `assert_eq!` remain the way a test fails.
- Fixtures are inert documents written for the test. No real malware and
  no copied malicious packages.

## Branches and commits

- `main` holds the initial commit only until a foundation pull request
  lands. Ongoing foundation work is on `chore/foundation`.
- Commit subjects are Conventional Commits: `feat(ingest): …`,
  `docs(adr): …`, `chore(ci): …`.
- A non-trivial design decision is an ADR in `docs/adr/` before the code
  that depends on it.
- Do not push. Do not rewrite published history.
- `_intern/` is gitignored. Do not edit it and do not commit it.

## License of contributions

The project is AGPL-3.0-only with a commercial second license. Contributions
need the agreement in [CLA.md](../CLA.md). That file is a draft for legal
review.
