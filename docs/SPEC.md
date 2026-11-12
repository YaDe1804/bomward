# bomward specification

Status of this document: contract for v0.1. The library can read a CycloneDX
JSON document into the model and diff two models. The CLI parses `diff`
and still exits with code 2. See [ARCHITECTURE.md](ARCHITECTURE.md).

## Purpose

bomward is an SBOM review gate for pull requests and releases. It compares two
CycloneDX SBOMs (old to new), explains the change in plain language, judges
that change against a policy, and returns an exit code for CI.

The review question is: may this change pass, and why?

## What v0.1 does

Given two CycloneDX JSON documents and an optional policy file, v0.1 will:

1. Read only the paths the user passed.
2. Normalize each document into the model from [ADR 0005](adr/0005-component-identity.md).
3. Diff the two models.
4. If `--policy` is present, judge the diff against the policy. If it is
   omitted, stay in report-only mode.
5. Write one report and exit with the code from [ADR 0007](adr/0007-exit-codes-and-determinism.md).

Data flow: `ingest` → `model` → `diff` → `policy` → `report`.

Each component key maps to a set of versions, defined in
[ADR 0005](adr/0005-component-identity.md). The diff compares those sets.
[ADR 0011](adr/0011-change-model.md) is the change model.

Diff kinds in v0.1, in sort order:

- `added` — a version exists only in the new document
- `removed` — a version exists only in the old document
- `version-changed` — the key has exactly one version on each side, and they differ
- `license-changed` — the license set differs for a version in both documents
- `supplier-changed` — the supplier set differs for a version in both documents
- `hash-changed` — a shared algorithm has different content
- `hash-coverage-changed` — an algorithm was added or removed, with no contradiction on a shared algorithm
- `scope-changed` — the CycloneDX `scope` set differs
- `role-changed` — the graph role differs

`version-changed` carries both attribute sets and one flag per attribute
kind. It does not also emit those kinds. Its direction is `upgrade`,
`downgrade`, or `unknown` ([ADR 0012](adr/0012-version-ordering.md)).
Only `npm` and `cargo` use SemVer precedence, and only when both
versions parse. Anything else, including equal precedence, is
`unknown`. The root component is a separate entry, not one of these
kinds. A component that loses or gains its purl changes key and is
reported as `removed` plus `added`.

License, hash, and supplier changes apply to a version that is in both
sets. Hash content is compared as lowercase hex, so letter case is not a
hash change. The algorithm name is the CycloneDX spelling, so `SHA-256`
and `sha-256` are one algorithm. A name outside that enumeration is
kept and warns `unknown-hash-algorithm`. An invalid `purl` does
not reject the file. The key falls back to `group` plus `name`, and the
report counts and lists a warning. When a component `version` and its
purl version differ, the purl version is stored and the document warns
`version-mismatch`. A JSON object that repeats a key keeps the last
value and warns `duplicate-key`.

Each dependency change carries `old_role` and `new_role`. The role a
policy sees is the one in the new document, and for `removed` the one
in the old document. A version whose role is `root` is not a dependency
change. The graph node is one key together with one version. Two
versions of one package can have two roles.
[ADR 0010](adr/0010-document-mapping.md) defines the four roles.
`metadata.component` is `root` even when `dependencies` is absent. A
document with no `metadata.component`, or with no `dependencies` field,
counts one warning, `role-unknown`. When the graph is present, a version
that stays unknown also counts one warning, `unreachable`, once per
component key. Roles are compared only when neither document has
`role-unknown`. The diff records that as `roles_comparable`. A document
with no components warns `no-components`. A document that only names
`metadata.component` is not empty. `scope` in a component is only the
CycloneDX field. A missing scope is stored as absent and compares as
`required`. `bom-ref` is never compared across the two documents.

## Non-goals for v0.1

- A general-purpose SBOM diff. See the boundary below.
- SPDX, CycloneDX XML, and CycloneDX Protobuf.
- CycloneDX JSON `specVersion` 1.2 and 1.3.
- Looking up vulnerabilities, fetching packages, or executing package code.
- Any network access.
- Generating SBOMs.
- A GitHub Action or a release pipeline. Those are later work packages.

## Boundary

`sbomdiff` (Python, [anthonyharrison/sbomdiff](https://github.com/anthonyharrison/sbomdiff))
reports added and removed packages and changes of version, license, and
checksum for SPDX and CycloneDX.

`sbom-tools` (Rust, [sbom-tool/sbom-tools](https://github.com/sbom-tool/sbom-tools))
normalizes CycloneDX and SPDX and diffs components, dependencies, licenses,
and vulnerabilities.

bomward does not replace either tool. It consumes two CycloneDX JSON SBOMs
and produces a policy verdict a CI job can block on.

## CLI contract

```text
bomward --version
bomward --help
bomward diff <OLD> <NEW> [--policy <FILE>] [--format <terminal|markdown|json>] [--max-file-bytes N] [--max-components N] [--max-depth N]
```

| Argument | Meaning |
| --- | --- |
| `OLD` | Baseline CycloneDX JSON file. Required. |
| `NEW` | Candidate CycloneDX JSON file. Required. |
| `--policy <FILE>` | Policy file. Optional. Omit it for report-only mode. |
| `--format` | `terminal` (default), `markdown`, or `json`. |
| `--max-file-bytes` | Max size of one input file. Default `33554432`. |
| `--max-components` | Max component objects, including nested ones. Default `100000`. |
| `--max-depth` | Max nesting of `components` inside `components`. Does not limit dependency-graph path length or JSON container depth. Default `32`. Maximum `63`. |

`--version` and `--help` exit 0.

Until the pipeline exists, `diff` prints `not implemented yet` and exits 2.
It does not read the files. A missing argument is a usage error and also
exits 2.

When the pipeline exists:

- If `--policy` is omitted, the run is report-only and exits 0. Terminal,
  Markdown, and JSON include the sentence `No policy was applied.` JSON
  also sets `"policy_applied": false`. The report still lists the changes.
- If `--policy` is present, that sentence is absent and a denial exits 1.
- `--format terminal` writes plain text. It does not emit ANSI color.
- `markdown` is the text a pull-request comment can host.
- `json` is the machine-readable report.

Human-facing CLI text is English.

## Exit codes

| Code | Meaning |
| --- | --- |
| 0 | The inputs are valid, and either no policy was applied or the policy allows the change. |
| 1 | The inputs are valid and the policy denies the change. |
| 2 | Usage error, rejected input, or a command that is not implemented yet. |

No other code is used. Details are in [ADR 0007](adr/0007-exit-codes-and-determinism.md).

## Input format

v0.1 accepts CycloneDX JSON only, `specVersion` 1.4, 1.5, 1.6, and 1.7.
The current CycloneDX specification is 1.7 (released 2025-10-21, ECMA-424).
The decision and the sources are in [ADR 0004](adr/0004-cyclonedx-json.md).

Anything else — XML, SPDX, a missing `bomFormat`, an unsupported
`specVersion` — is an input error (exit 2). An unsupported or missing
`specVersion` reports:

```text
unsupported specVersion {found}; supported versions: 1.4, 1.5, 1.6, 1.7
```

`{found}` is `missing` when the field is absent.

Limits for hostile files are defaults from [ADR 0006](adr/0006-untrusted-input.md):
32 MiB per file (`--max-file-bytes`), 100,000 components
(`--max-components`), and component nesting 32 (`--max-depth`, maximum
63). That flag covers `components` nested inside `components`. It does
not cover JSON container depth, and it does not cover how long a
dependency chain is. JSON arrays and objects are capped at 127. That
error names the depth that was found and the cap, and it does not name
`--max-depth`. A `--max-depth` above 63 is a usage error and names the
requested value and the maximum. A cycle in `dependencies` is stored;
it is not an error. A file-size, component-count, or component-nesting
violation names the limit, the value that was found, and the flag.

## Policy

The policy file, the matchers, and the verdict are
[ADR 0013](adr/0013-policy-schema.md). Operators read
[POLICY.md](POLICY.md). The page is added with the implementation.

An omitted `--policy` does not evaluate rules. The run is report-only.

## Determinism

The same SBOM bytes, policy bytes, format flag, and limit flags produce
byte-identical stdout. Reports use LF. They contain no timestamps and no
local paths. Warnings are sorted.
The rule is [ADR 0007](adr/0007-exit-codes-and-determinism.md).

## Deferred

The parser is the owned model in [ADR 0008](adr/0008-cyclonedx-parser.md).
Package URL parsing is [ADR 0009](adr/0009-purl-normalization.md). The field
mapping is [ADR 0010](adr/0010-document-mapping.md). The change model is
[ADR 0011](adr/0011-change-model.md). Version direction is
[ADR 0012](adr/0012-version-ordering.md). The comparison is
`bomward_core::diff::diff`. Policy and the report are later work
packages. The CLI does not call ingest or the diff yet.
