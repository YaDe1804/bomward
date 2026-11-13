# Architecture

bomward is a Cargo workspace with two crates. [ADR 0003](adr/0003-rust-workspace.md)
records that split. Ingest, the model, the diff, policy, and the
report are implemented. The CLI calls that pipeline.

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

`bomward-core` exposes five modules. Each one has types and functions.

| Module | Responsibility | Work package |
| --- | --- | --- |
| `ingest` | CycloneDX JSON → model, enforcing the input limits. | AP1, done |
| `model` | Normalized components and identities. | AP1, done |
| `diff` | Two documents → a typed, deterministic diff. | AP2, done |
| `policy` | YAML rules → a verdict with reasons. | AP3, done |
| `report` | Terminal, Markdown, and JSON rendering. | AP4, done |

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

`NotImplemented` remains so the sentence `not implemented yet` stays
stable. The report does not return it. Policy adds `InvalidPolicy`.
Ingest adds I/O, invalid document, unsupported `specVersion`, limit variants, and a
usage error when `--max-depth` is above 63. The limit variant carries
the kind (file size, component count, component nesting, or JSON
nesting), the value that was found, and the limit. Display text matches
ADR 0004 and ADR 0006. The crate is `publish = false` and pre-release,
so the enum is not a stability promise.

`bomward-cli` converts library errors and usage failures into a message on
stderr, prefixed `error: `, and an exit code. It uses `anyhow` for that
message. Exit 0 is a pass or a report-only run. Exit 1 is an effective
denial, and the report is written first. Exit 2 is a usage error, a
file that cannot be read, a rejected SBOM, a limit, a rejected policy,
or a write error. No report is written on exit 2. A closed stdout pipe
exits 0 and prints nothing.
See [ADR 0007](adr/0007-exit-codes-and-determinism.md) and
[ADR 0016](adr/0016-cli-output.md).

## CLI

`clap` defines `diff <OLD> <NEW>` with `--policy`, `--format`, `--output`,
and the three limit flags. The handler reads both documents, diffs them,
optionally evaluates a policy, and renders the report. It contains no
review rules. The contract is [SPEC.md](SPEC.md) and
[REPORT.md](REPORT.md).

## Parser

[ADR 0008](adr/0008-cyclonedx-parser.md) chooses serde visitors over
`cyclonedx-bom` 0.9.0. The visitors read only the fields in
[ADR 0010](adr/0010-document-mapping.md). Unknown JSON is skipped with
`IgnoredAny` and is not built into a value tree. Component count and
nesting are checked while reading. [ADR 0009](adr/0009-purl-normalization.md)
parses package URLs with `packageurl` 0.7.1. Public entry points are
`read_path` and `read_bytes`. The CLI calls `read_path`.

## Diff

[ADR 0011](adr/0011-change-model.md) names the nine change kinds, the
one-to-one version pairing, and the separate root entry.
[ADR 0012](adr/0012-version-ordering.md) gives `version-changed` a
direction. `npm` and `cargo` use `semver` 1.0.28 precedence. Every other
type, a failed parse, and equal precedence are `unknown`. `diff` takes
two `&Document` values, cannot fail, and does not read files. The CLI
calls it after both documents have loaded.

## Policy

[ADR 0013](adr/0013-policy-schema.md) is the schema and the evaluation.
[POLICY.md](POLICY.md) is the page operators read.
[ADR 0014](adr/0014-yaml-parser.md) pins `noyalib` 0.0.60 and rejects
aliases and duplicate keys. License ids are read with `spdx` 0.13.6.
`read_str`, `read_bytes`, and `read_path` validate the file.
`evaluate` takes a policy and a diff, cannot fail, and does no I/O.
The CLI calls both when `--policy` is set. Exit code 1 is
`Verdict::exit_code` when the effective result is `deny`. A file that
fails to load is `Error::InvalidPolicy` and the CLI exits 2.

Each change carries the attributes of the version under review
(`removed` uses the old version, every other kind the new one). That
snapshot is not part of the diff's canonical JSON. In-place rows still
store only the delta there, and a policy matcher needs the version itself.

## Report

[ADR 0015](adr/0015-report-formats-and-escaping.md) is the three formats.
`render` takes a diff, an optional verdict, and a format, and returns
one string. Terminal, Markdown, and JSON share one sentence catalog.
The CLI writes that string and does not assemble a second one.
[REPORT.md](REPORT.md) is the page operators read. The JSON shape is
[report.schema.json](report.schema.json).

## Supply chain of the tool

Runtime code has no network client. Dependencies come from crates.io only,
are locked in `Cargo.lock`, and are checked with `cargo-deny` and
`cargo-audit`. The allow-list is [deny.toml](../deny.toml). Rationale for
the license and the bans is [ADR 0002](adr/0002-license-model.md) and
[ADR 0006](adr/0006-untrusted-input.md).
