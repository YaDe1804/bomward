# ADR 0003 — Rust workspace

- Status: accepted
- Date: 2026-11-11

## Context

The gate has to run as one CI binary, and the review logic has to be
testable without a process. The language and the crate split need to be
fixed before the modules exist, so later work packages do not invent a
second layout.

## Decision

- Rust, edition 2024, stable toolchain. MSRV is 1.99, the stable release
  used to build this foundation.
- A Cargo workspace with `resolver = "3"`.
- `bomward-core` is the library.
- `bomward-cli` is the binary, named `bomward`.
- `#![forbid(unsafe_code)]` in every crate, and `unsafe_code = "forbid"`
  in the workspace lints.
- Shared package metadata (license, repository, authors, `rust-version`,
  edition) lives in `[workspace.package]`.
- `Cargo.lock` is committed.
- `publish = false` until a release work package says otherwise.

Human-facing CLI text is English, consistent with the public docs.

## Rationale

Rust gives a single static binary and a type system that can keep
`unsafe` out of a tool whose input is hostile. Edition 2024 is the
current edition on the stable toolchain this foundation uses.

The library/binary split keeps parsing and policy free of `clap` and of
process exit. Tests call the library directly. The CLI owns exit codes.

Resolver 3 is the current Cargo resolver and is what the foundation
brief requires.

## Alternatives

- One crate that is both the library and the binary. Rejected: dev
  dependencies and CLI concerns leak into the library, and `anyhow`
  would sit next to the typed errors.
- A `crates/` directory. Rejected in favor of the Lockward layout
  (`bomward-core` and `bomward-cli` at the repository root), so the
  sister projects read the same way.
- German CLI text, as in Lockward. Rejected here: the public spec, the
  reports, and the commit convention for this project are English.
  Lockward's German UI was a decision for that product, not a default
  this brief carried over.

## Consequences

- `thiserror` is the error crate for `bomward-core`. `anyhow` is allowed
  only in `bomward-cli`.
- `clap` is a CLI dependency, not a core dependency.
- A third crate needs a new ADR.
- The MSRV moves only by an ADR or by a documented toolchain bump in
  `docs/DEVELOPMENT.md` when CI's stable channel requires it.
