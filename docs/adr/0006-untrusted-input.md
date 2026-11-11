# ADR 0006 — Untrusted input

- Status: accepted
- Date: 2026-11-11

## Context

The files under review are attacker-controlled. They arrive through CI
from pull requests. bomward is a security tool: a parse failure has to
be an error, and a component name has to be data, including when a
pull-request comment renders the report as Markdown.

## Decision

bomward is static.

- No network access, no shell, no execution of foreign code.
- Open only the paths the user passed. Never build a path, URL, or
  command from a string inside a SBOM or a policy file.
- Reject a single input file larger than 32 MiB (33,554,432 bytes).
- Reject a document with more than 100,000 component objects, counting
  nested components.
- Reject nesting deeper than 32. Depth counts nested `components` and
  the dependency-graph walk.
- Reject a cycle in the dependency graph.
- Over-limit and cyclic input is an error (exit 2). The library does
  not panic and does not truncate.
- Do not write a SBOM string into terminal or Markdown output unchanged.
  Encode U+0000–U+001F and U+007F as `\u{XXXX}` (four uppercase hex
  digits). That includes tab, newline, and ESC.
  In Markdown, also escape `` \ ` * _ { } [ ] ( ) # + - . ! | < > ``.
- JSON output uses JSON string encoding and no second escape layer.
- The terminal format emits no ANSI color sequences of its own.
- Fixtures are inert documents written for the tests. No malware
  samples and no copied malicious packages.

These limits are the v0.1 contract. Changing a number is an ADR change.

## Rationale

32 MiB and 100,000 components are enough for a large application SBOM
and small enough that a CI job can refuse a pathological file quickly.
Depth 32 covers real component trees and stops a nested bomb. Cycle
detection stops a graph walk that would otherwise not terminate.

Escaping, rather than stripping, keeps the review honest: the reader can
see that the name contained a control character. Stripping would hide
the payload. Refusing to follow paths and URLs from the document keeps
the tool inside the files the operator named.

## Alternatives

- No size limit, rely on memory. Rejected: one CI variable then holds
  the availability of the runner.
- Truncate long strings and keep going. Rejected: a truncated name can
  hide the difference the policy was meant to judge.
- Shell out to a CycloneDX CLI. Rejected: that is foreign code and a
  process spawn on untrusted paths.
- Allow network "only for license lookup". Rejected: the product rule
  is no network, and a lookup would leak the dependency set.

## Consequences

- Ingest enforces the limits before and during the parse, so a huge
  file is refused without holding the whole payload past the cap.
- The report module owns escaping. Callers pass untrusted strings
  through that module and do not format them themselves.
- `bomward-core` must not gain a HTTP or process dependency. The
  dependency set in `deny.toml` stays on crates.io and on the license
  allow-list; a client such as `reqwest` is a new ADR, not a `cargo add`.
- The numbers above are reviewable. If a real SBOM exceeds them, raise
  the limit in this ADR rather than special-casing one file.
