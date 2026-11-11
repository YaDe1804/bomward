# ADR 0004 — Input format: CycloneDX JSON

- Status: accepted
- Date: 2026-11-11

## Context

v0.1 has to name the file it accepts before ingest is written. CycloneDX
is published in JSON, XML, and Protobuf, in several spec versions.
Supporting all of them in the first version would freeze a parser
surface the review gate does not need.

Checked against the current specification on 2026-11-11:

- Current version: CycloneDX 1.7, released 2025-10-21, ECMA-424
  (2nd edition, December 2025).
  <https://cyclonedx.org/specification/overview/>
- JSON reference for 1.7: <https://cyclonedx.org/docs/1.7/json/>
- The specification repository publishes JSON schemas `bom-1.2` through
  `bom-1.7`.
- `sbomdiff` and `sbom-tools` both treat 1.4–1.7 as the CycloneDX
  versions they interchange. 1.5 (2023-06-26), 1.6 (2024-04-09), and
  1.7 (2025-10-21) are the versions current generators emit; 1.4
  (2022-01-12) is still widely produced.

`purl` has been a component field since CycloneDX 1.2, so the identity
rule in ADR 0005 is expressible in every version considered here.

## Decision

v0.1 reads CycloneDX **JSON** with `specVersion` **1.4, 1.5, 1.6, or
1.7**. `bomFormat` must be `CycloneDX`.

Out of scope for v0.1:

- SPDX, in every serialization.
- CycloneDX XML and Protobuf.
- CycloneDX JSON 1.2 and 1.3.
- Any `specVersion` this list does not name, including a future 1.8
  until an ADR adds it.

An unsupported document is an input error (exit 2), not a best-effort
parse. The error names the `specVersion` that was found and the versions
this decision allows. The found value is the word `missing` when the
field is absent. The text is:

```text
unsupported specVersion {found}; supported versions: 1.4, 1.5, 1.6, 1.7
```

## Rationale

JSON is the encoding CI generators write and the encoding the JSON
Schema treats as the reference. 1.4–1.7 covers the documents review
pipelines actually exchange in 2026 without taking on the older 1.2/1.3
component shape. XML and SPDX would each be a second parser for a
non-goal named in the foundation brief.

## Alternatives

- 1.6 and 1.7 only. Rejected: 1.4 and 1.5 are still what many generators
  emit, and dropping them would make the gate unusable on current
  repositories.
- 1.2 through 1.7. Rejected for v0.1: the extra parsers do not change
  the review decision often enough to justify the surface.
- Autodetect SPDX and CycloneDX. Rejected: that is the generic-diff
  scope ADR 0001 declines.

## Consequences

- Ingest rejects XML even when the contents are CycloneDX.
- An unsupported or missing `specVersion` uses the error text above, so
  a CI log shows both the value in the file and the allowed list.
- AP1's parser ADR is limited to JSON for these four versions.
- Adding 1.8, XML, or SPDX requires a new ADR.
- How the bytes are parsed was left open here. [ADR 0008](0008-cyclonedx-parser.md)
  chooses an owned serde model.
