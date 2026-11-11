# ADR 0001 — Scope: review gate, not a generic diff

- Status: accepted
- Date: 2026-11-11

## Context

Several tools already diff two SBOMs. A second generic diff would not
change a review. bomward has to answer whether a change may pass, and
why, with an exit code CI can enforce.

## Decision

bomward is a review gate. It compares two CycloneDX SBOMs, explains the
change, judges it against a policy, and exits 0, 1, or 2.

It is not a general-purpose SBOM diff. `sbomdiff` (Python,
`anthonyharrison/sbomdiff`) already reports added and removed packages
and changes of version, license, and checksum for SPDX and CycloneDX.
`sbom-tools` (Rust, `sbom-tool/sbom-tools`) already normalizes CycloneDX
and SPDX and diffs components, dependencies, licenses, and
vulnerabilities.

bomward does not take SPDX, does not look up vulnerabilities, and does
not try to cover every diff those tools report. The diff kinds v0.1
does report exist to feed the policy, not to be a standalone product.

## Rationale

The review decision is the part the existing diffs leave to a human or
to an ad-hoc script. Putting the policy and the exit code in the tool
makes that decision repeatable. Matching `sbomdiff` or `sbom-tools`
feature for feature would spend the project on ground those tools
already hold.

## Alternatives

- Wrap `sbomdiff` or `sbom-tools` and only add a policy file. Rejected:
  the gate would inherit a foreign parser, a foreign identity rule, and
  a network-capable feature set this tool refuses.
- Build a generic multi-format diff and add policy later. Rejected: the
  format surface (SPDX, XML, Protobuf) is a non-goal for v0.1, and the
  policy would arrive after the model had frozen around the wrong job.

## Consequences

- The CLI is `diff` plus a verdict, not a family of inspection commands.
- Vulnerability data, VEX, and package enrichment stay out of the model.
- A change that only improves diff completeness, without serving the
  verdict, is out of scope unless an ADR says otherwise.
