# Changelog

All notable changes to this project are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- Policy version 1: `bomward-core` reads a YAML policy, rejects unknown
  fields, duplicate keys, and aliases, and evaluates a diff into a
  verdict. `mode: warn` turns `deny` into `warn`. Exit code 1 is
  `Verdict::exit_code` when the effective result is `deny`. The CLI
  does not call this yet. See [docs/POLICY.md](docs/POLICY.md) and
  ADRs 0013 and 0014. Example files live in `examples/policies/`.
- Repository foundation: public docs, ADRs 0001–0007, and the Cargo workspace.
- `bomward` parses `diff <OLD> <NEW>` with `--policy` and `--format`, then
  exits 2 with `not implemented yet`. It does not review SBOMs yet.
- A component `version` that differs from its purl version warns
  `version-mismatch`. The stored version is the purl version.
- A repeated key in a JSON object warns `duplicate-key`. The last value
  is the one that is read.
- Ingest reads only the fields the model keeps. Unknown JSON is skipped
  instead of being built into a value tree. Component count and nesting
  are enforced while reading, so a later syntax error does not hide a
  limit error.
- `bomward-core` reads CycloneDX JSON 1.4–1.7 into the normalized model
  (`read_path`, `read_bytes`), including limits, purl identity, and warnings.
- ADRs 0008 (owned CycloneDX model), 0009 (`packageurl` 0.7.1), and 0010
  (document mapping).

### Changed

- The graph position is a role (`root`, `direct`, `transitive`,
  `unknown`). A missing graph warns `role-unknown`. `scope` is only the
  CycloneDX field. The policy sketch uses `role:`.
- A dependency-graph node is one version of a key. `bom-ref` resolves to
  that pair, and the role is stored on the version.
- A version that stays unknown when the document has a root and a
  `dependencies` field warns `unreachable`, once per component key.
- Hash content is stored as lowercase hex, so `ABCD` and `abcd` are one
  hash. The algorithm name is stored in the CycloneDX spelling, so
  `SHA-256` and `sha-256` are one algorithm. A name outside that
  enumeration warns `unknown-hash-algorithm` and is kept as written.
- A missing CycloneDX `scope` compares as `required`. The model still
  stores the absence.
- `metadata.component` is the root even when the document has no
  `dependencies` field. Other versions stay `unknown`, and the document
  still warns `role-unknown`.
- Roles are compared only when both documents have a graph. The diff
  field is `roles_comparable`.
- A document with no components warns `no-components`.
- ADRs 0011 (change model) and 0012 (version direction).
- `diff` compares two documents into a typed diff. Kinds are `added`,
  `removed`, `version-changed`, `license-changed`, `supplier-changed`,
  `hash-changed`, `hash-coverage-changed`, `scope-changed`, and
  `role-changed`. The root component is a separate entry.
- Version direction uses `semver` 1.0.28 for `npm` and `cargo`. Anything
  else is `unknown`.
- A dependency cycle is stored. `--max-depth` limits nested `components`,
  not JSON container depth and not the length of a dependency chain.
  JSON containers are capped at 127, and that error does not name the
  flag. `--max-depth` above 63 is a usage error.
- GitHub Actions runs push builds on `main` only. Pull requests are unchanged.
