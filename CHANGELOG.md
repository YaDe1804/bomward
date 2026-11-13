# Changelog

All notable changes to this project are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- Fuzzing for the decoder and the policy parser. A deterministic smoke
  fuzz runs in the stable test suite, and libFuzzer targets live in
  `fuzz/` with a committed seed corpus. Boundary tests lock component
  depth 32/33, JSON depth 127/128, dependency cycles, and a 100,000-hash
  component under the file-size limit. See [docs/FUZZING.md](docs/FUZZING.md)
  and [ADR 0018](docs/adr/0018-fuzzing-and-attribute-amplification.md).
- GitHub Action `bomward review`. It diffs the pull-request SBOM against
  the baseline from the base commit, reads the policy from that same
  commit, and posts or updates one pull-request comment. The release
  asset is checked against `SHA256SUMS` before it runs. See
  [docs/ACTION.md](docs/ACTION.md) and
  [ADR 0017](docs/adr/0017-github-action.md).
- Policy version 1: `bomward-core` reads a YAML policy, rejects unknown
  fields, duplicate keys, and aliases, and evaluates a diff into a
  verdict. `mode: warn` turns `deny` into `warn`. Exit code 1 is
  `Verdict::exit_code` when the effective result is `deny`. The CLI
  calls this when `--policy` is set. See [docs/POLICY.md](docs/POLICY.md)
  and ADRs 0013 and 0014. Example files live in `examples/policies/`.
- `bomward diff` reads both CycloneDX documents, optionally applies a
  policy, and writes a terminal, Markdown, or JSON report. `--output`
  writes that report to a file. Exit 0 is a pass or report-only, exit 1
  is an effective denial, and exit 2 is any failure before a verdict.
  See [docs/REPORT.md](docs/REPORT.md) and ADRs 0015 and 0016.
  Example reports live in `examples/reports/`.
- Repository foundation: public docs, ADRs 0001–0007, and the Cargo workspace.
- `bomward` parses `diff <OLD> <NEW>` with `--policy`, `--format`,
  `--output`, and the limit flags.
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

- `license_status: unrecognized` also matches a string that contains
  SPDX ids when the whole string is not one id and not a valid
  expression. Qualifiers in a package pattern must be the canonical
  form the key stores, and `***` is rejected.
- License matching keeps SPDX ids from a string that is not a valid
  expression, and `GPL-3.0+` matches both `GPL-3.0-or-later` and
  `GPL-3.0`. `license_status: [unrecognized]` matches free text.
  `*` and `**` no longer cross `?` or `#`. An unencoded `@` in a
  package pattern's namespace or name is an error. See
  [docs/POLICY.md](docs/POLICY.md).
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
