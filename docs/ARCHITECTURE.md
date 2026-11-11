# Architecture

bomward is a Cargo workspace with two crates. [ADR 0003](adr/0003-rust-workspace.md)
records that split. There is no domain logic in the foundation: the modules
and the error type are the seams the later work packages fill.

## Crates

| Crate | Role |
| --- | --- |
| `bomward-core` | Library. Parsing, model, diff, policy, and report rendering. |
| `bomward-cli` | Binary `bomward`. Argument parsing and exit codes. |

`bomward-cli` depends on `bomward-core`. `bomward-core` does not depend on
the CLI, on `anyhow`, or on `clap`.

The workspace forbids `unsafe`. Clippy denies `unwrap`, `expect`, `panic!`,
unchecked indexing, `todo!`, and `dbg!` in every target.

## Modules

`bomward-core` exposes five modules. They are empty apart from their docs
until the matching work package.

| Module | Responsibility | Work package |
| --- | --- | --- |
| `ingest` | CycloneDX JSON → model, enforcing the input limits. | AP1 |
| `model` | Normalized components and identities. | AP1 |
| `diff` | Added, removed, and changed components. | AP2 |
| `policy` | YAML rules → a verdict with reasons. | AP3 |
| `report` | Terminal, Markdown, and JSON rendering. | AP4 |

Data flow, and the only direction of calls:

```text
ingest → model → diff → policy → report
```

A later module may read types from an earlier module. It does not call back.
`report` does not decide. `policy` does not format. `diff` does not read
files. `ingest` is the only module that interprets CycloneDX JSON.

## Errors

`bomward-core` returns `bomward_core::Error`, a `thiserror` enum. Library
functions do not panic and do not exit the process.

The foundation enum has one variant, `NotImplemented`. It exists so the
type, the docs, and the test harness are real. Variants will change before
any release. The crate is `publish = false` and the project is pre-release,
so this skeleton is not a stability promise.

`bomward-cli` converts library errors and usage failures into a message on
stderr and an exit code. It may use `anyhow`. A usage error, a rejected
input, and the current not-implemented path all exit 2. A policy denial
exits 1, once policy exists. See [ADR 0007](adr/0007-exit-codes-and-determinism.md).

## CLI

`clap` defines `diff <OLD> <NEW>` with `--policy` and `--format`. The
handler ignores the parsed values and returns the not-implemented error.
It does not open the paths. The contract those flags will have is in
[SPEC.md](SPEC.md).

## Parser choice

Not decided. AP1 adds an ADR that chooses either a small serde model owned
by this repository or an existing CycloneDX crate. Ingest is not started
before that ADR is accepted.

## Supply chain of the tool

Runtime code has no network client. Dependencies come from crates.io only,
are locked in `Cargo.lock`, and are checked with `cargo-deny` and
`cargo-audit`. The allow-list is [deny.toml](../deny.toml). Rationale for
the license and the bans is [ADR 0002](adr/0002-license-model.md) and
[ADR 0006](adr/0006-untrusted-input.md).
