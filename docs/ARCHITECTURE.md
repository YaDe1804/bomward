# Architecture

bomward is a Cargo workspace with two crates. [ADR 0003](adr/0003-rust-workspace.md)
records that split. Ingest and the model are implemented. Diff, policy, and
report are still seams.

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

`bomward-core` exposes five modules. Ingest and the model have types and
functions. The other three are still empty apart from their docs.

| Module | Responsibility | Work package |
| --- | --- | --- |
| `ingest` | CycloneDX JSON → model, enforcing the input limits. | AP1, done |
| `model` | Normalized components and identities. | AP1, done |
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

`NotImplemented` remains for diff, policy, and report. Ingest adds I/O,
invalid document, unsupported `specVersion`, limit variants, and a
usage error when `--max-depth` is above 63. The limit variant carries
the kind (file size, component count, component nesting, or JSON
nesting), the value that was found, and the limit. Display text matches
ADR 0004 and ADR 0006. The crate is `publish = false` and pre-release,
so the enum is not a stability promise.

`bomward-cli` converts library errors and usage failures into a message on
stderr and an exit code. It may use `anyhow`. A usage error, a rejected
input, and the current not-implemented path all exit 2. A policy denial
exits 1, once policy exists. See [ADR 0007](adr/0007-exit-codes-and-determinism.md).

## CLI

`clap` defines `diff <OLD> <NEW>` with `--policy` and `--format`. The
handler ignores the parsed values and returns the not-implemented error.
It does not open the paths. The contract those flags will have is in
[SPEC.md](SPEC.md).

## Parser

[ADR 0008](adr/0008-cyclonedx-parser.md) chooses serde visitors over
`cyclonedx-bom` 0.9.0. The visitors read only the fields in
[ADR 0010](adr/0010-document-mapping.md). Unknown JSON is skipped with
`IgnoredAny` and is not built into a value tree. Component count and
nesting are checked while reading. [ADR 0009](adr/0009-purl-normalization.md)
parses package URLs with `packageurl` 0.7.1. Public entry points are
`read_path` and `read_bytes`. The CLI does not call them yet.

## Supply chain of the tool

Runtime code has no network client. Dependencies come from crates.io only,
are locked in `Cargo.lock`, and are checked with `cargo-deny` and
`cargo-audit`. The allow-list is [deny.toml](../deny.toml). Rationale for
the license and the bans is [ADR 0002](adr/0002-license-model.md) and
[ADR 0006](adr/0006-untrusted-input.md).
