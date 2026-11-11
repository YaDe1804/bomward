# Development

Public product contract: [SPEC.md](SPEC.md). Decisions: [adr/](adr/).

## Layout

| Path | Role |
| --- | --- |
| `bomward-core` | Library. `ingest`, `model`, `diff`, `policy`, `report`. |
| `bomward-cli` | Binary `bomward`. |
| `docs/adr` | Architecture decision records. |
| `.github/workflows/ci.yml` | Format, Clippy, tests, `cargo-deny`, `cargo-audit`. |
| `deny.toml` | License, advisory, and source policy for this repository. |

Data flow: `ingest` → `model` → `diff` → `policy` → `report`.

## Toolchain

- Rust stable, edition 2024, with `rustfmt` and `clippy` (`rust-toolchain.toml`).
- MSRV is **1.99**. That is the stable release this foundation was built and
  tested with (`rustc 1.99.0`, 2026-09-28). `rust-version` in the workspace
  `Cargo.toml` is `1.99`. CI follows the `stable` channel. Older compilers
  are not claimed.
- `cargo-nextest`, `cargo-deny`, and `cargo-audit` on the PATH for the gate.

Crate versions are taken from crates.io via `cargo add`, not from memory.
The foundation closure is:

| Crate | Version | Where | Why |
| --- | --- | --- | --- |
| `thiserror` | 2.0.21 | `bomward-core` | Typed library errors. |
| `anyhow` | 1.0.104 | `bomward-cli` | CLI error reporting only. |
| `clap` | 4.6.7 | `bomward-cli` | Command and flag parsing (`derive`). |

`bomward-cli` also depends on `bomward-core` by path. Versions are locked in
`Cargo.lock`. Add a crate with `cargo add` and update this table in the same
change.

## Build

```powershell
cargo build --locked
cargo run -p bomward-cli -- --help
cargo run -p bomward-cli -- diff old.json new.json
```

The last command prints `not implemented yet` and exits 2. It does not
read `old.json` or `new.json`.

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
- Golden files, once parsers and reports exist, live under that crate's
  `tests/fixtures/` and are compared byte for byte. Line endings in the
  repository are LF (`.gitattributes`).
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
